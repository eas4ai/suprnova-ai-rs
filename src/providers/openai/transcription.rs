//! Transcription of a file with OpenAI
//! (`transcription/openai-transcription-model.ts`), OAI-019.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{errors, images::media_bytes, params::invalid, protocol::http_request};
use crate::{
    Error, Model, Provider,
    providers::{
        http,
        multipart::Multipart,
        operations::{
            Item, MediaInput, TranscribeInput, TranscribeParams, Transcription,
            TranscriptionSegment,
        },
    },
    types::usage::Usage,
};

/// The transcription options (`openAITranscriptionModelOptions`).
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TranscriptionOptions {
    pub include: Option<Vec<String>>,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub temperature: Option<f64>,
    pub timestamp_granularities: Option<Vec<String>>,
    pub response_format: Option<String>,
    pub chunking_strategy: Option<Value>,
    pub streaming: Option<StreamingOptions>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamingOptions {
    pub delay: Option<String>,
    pub include: Option<Vec<String>>,
}

impl TranscriptionOptions {
    pub(super) fn parse(params: &TranscribeParams) -> Result<Self, Error> {
        let options: Self = match params.provider_options.get("openai") {
            None => Self::default(),
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| invalid(format!("invalid openai transcription options: {e}")))?,
        };
        if options
            .temperature
            .is_some_and(|t| !(0.0..=1.0).contains(&t))
        {
            return Err(invalid(
                "openai transcription option temperature must be from 0 to 1",
            ));
        }
        for granularity in options.timestamp_granularities.iter().flatten() {
            if !matches!(granularity.as_str(), "word" | "segment") {
                return Err(invalid(format!(
                    "openai transcription option timestampGranularities takes word and segment, not {granularity:?}"
                )));
            }
        }
        if let Some(format) = &options.response_format
            && !matches!(format.as_str(), "json" | "verbose_json" | "diarized_json")
        {
            return Err(invalid(format!(
                "openai transcription option responseFormat must be json, verbose_json or diarized_json, not {format:?}"
            )));
        }
        if let Some(delay) = options.streaming.as_ref().and_then(|s| s.delay.as_deref())
            && !matches!(delay, "minimal" | "low" | "medium" | "high" | "xhigh")
        {
            return Err(invalid(format!(
                "openai transcription option streaming.delay must be minimal, low, medium, high or xhigh, not {delay:?}"
            )));
        }
        Ok(options)
    }
}

/// `gpt-realtime-whisper` and its dated versions transcribe only streams.
pub(super) fn realtime_model(model: &str) -> bool {
    model == "gpt-realtime-whisper" || model.starts_with("gpt-realtime-whisper-")
}

/// The extension OpenAI and Mistral read a file's audio format from
/// (`mediaTypeToExtension`).
pub(crate) fn extension(media_type: &str) -> String {
    let lower = media_type.to_ascii_lowercase();
    let subtype = lower.split_once('/').map_or("", |(_, sub)| sub);
    match subtype {
        "mpeg" => "mp3",
        "x-wav" => "wav",
        "opus" => "ogg",
        "mp4" | "x-m4a" => "m4a",
        other => other,
    }
    .to_owned()
}

/// The chunking strategy as the form takes it: `auto`, or the server VAD
/// settings as JSON with the wire's snake_case names.
fn chunking(strategy: &Value) -> Result<String, Error> {
    if strategy == "auto" {
        return Ok("auto".into());
    }
    let bad = || {
        invalid(
            "openai transcription option chunkingStrategy must be \"auto\" or {\"type\": \"server_vad\", ...}",
        )
    };
    let object = strategy.as_object().ok_or_else(bad)?;
    if object.get("type").and_then(Value::as_str) != Some("server_vad") {
        return Err(bad());
    }
    let mut out = Map::new();
    out.insert("type".into(), json!("server_vad"));
    for (key, wire) in [
        ("threshold", "threshold"),
        ("prefixPaddingMs", "prefix_padding_ms"),
        ("silenceDurationMs", "silence_duration_ms"),
    ] {
        if let Some(value) = object.get(key).filter(|v| !v.is_null()) {
            if !value.is_number() {
                return Err(bad());
            }
            out.insert(wire.into(), value.clone());
        }
    }
    if object.keys().any(|key| {
        !["type", "threshold", "prefixPaddingMs", "silenceDurationMs"].contains(&key.as_str())
    }) {
        return Err(bad());
    }
    Ok(Value::Object(out).to_string())
}

/// A language as a two-letter code, by OpenAI's name for it when it is not
/// one (`languageMap`).
fn language_code(language: &str) -> Option<String> {
    const NAMES: &[(&str, &str)] = &[
        ("afrikaans", "af"),
        ("arabic", "ar"),
        ("armenian", "hy"),
        ("azerbaijani", "az"),
        ("belarusian", "be"),
        ("bosnian", "bs"),
        ("bulgarian", "bg"),
        ("catalan", "ca"),
        ("chinese", "zh"),
        ("croatian", "hr"),
        ("czech", "cs"),
        ("danish", "da"),
        ("dutch", "nl"),
        ("english", "en"),
        ("estonian", "et"),
        ("finnish", "fi"),
        ("french", "fr"),
        ("galician", "gl"),
        ("german", "de"),
        ("greek", "el"),
        ("hebrew", "he"),
        ("hindi", "hi"),
        ("hungarian", "hu"),
        ("icelandic", "is"),
        ("indonesian", "id"),
        ("italian", "it"),
        ("japanese", "ja"),
        ("kannada", "kn"),
        ("kazakh", "kk"),
        ("korean", "ko"),
        ("latvian", "lv"),
        ("lithuanian", "lt"),
        ("macedonian", "mk"),
        ("malay", "ms"),
        ("marathi", "mr"),
        ("maori", "mi"),
        ("nepali", "ne"),
        ("norwegian", "no"),
        ("persian", "fa"),
        ("polish", "pl"),
        ("portuguese", "pt"),
        ("romanian", "ro"),
        ("russian", "ru"),
        ("serbian", "sr"),
        ("slovak", "sk"),
        ("slovenian", "sl"),
        ("spanish", "es"),
        ("swahili", "sw"),
        ("swedish", "sv"),
        ("tagalog", "tl"),
        ("tamil", "ta"),
        ("thai", "th"),
        ("turkish", "tr"),
        ("ukrainian", "uk"),
        ("urdu", "ur"),
        ("vietnamese", "vi"),
        ("welsh", "cy"),
    ];
    let is_code = |text: &str| text.len() == 2 && text.bytes().all(|b| b.is_ascii_lowercase());
    if is_code(language) {
        return Some(language.to_owned());
    }
    NAMES
        .iter()
        .find(|(name, _)| *name == language)
        .map(|(_, code)| (*code).to_owned())
}

pub(super) async fn transcribe(
    model: &Model,
    audio: TranscribeInput,
    params: TranscribeParams,
    provider: &dyn Provider,
) -> Result<Item<Transcription>, Error> {
    if realtime_model(&model.id) {
        return Err(invalid(format!(
            "{} transcribes only streams; use stream_transcription",
            model.id
        )));
    }
    let options = TranscriptionOptions::parse(&params)?;
    if options.streaming.is_some() {
        return Err(invalid(
            "openai transcription option streaming applies to stream_transcription",
        ));
    }
    let input = match audio {
        TranscribeInput::File(file) => MediaInput::File(file),
        TranscribeInput::Bytes(bytes) => MediaInput::Bytes(bytes),
    };
    let (bytes, media_type) = media_bytes(&input, provider, "audio").await?;
    let mut form = Multipart::new();
    form.text("model", &model.id);
    form.file(
        "file",
        &format!("audio.{}", extension(&media_type)),
        &media_type,
        &bytes,
    );
    let diarize = model.id == "gpt-4o-transcribe-diarize";
    let format = if model.id == "whisper-1" {
        "verbose_json".to_owned()
    } else {
        options.response_format.clone().unwrap_or_else(|| {
            match model.id.as_str() {
                _ if diarize => "diarized_json",
                "gpt-4o-transcribe" | "gpt-4o-mini-transcribe" => "json",
                _ => "verbose_json",
            }
            .to_owned()
        })
    };
    form.text("response_format", &format);
    for value in options.include.iter().flatten() {
        form.text("include[]", value);
    }
    if let Some(language) = &options.language {
        form.text("language", language);
    }
    if let Some(prompt) = &options.prompt {
        form.text("prompt", prompt);
    }
    if let Some(temperature) = options.temperature {
        form.text("temperature", &temperature.to_string());
    }
    for granularity in options.timestamp_granularities.iter().flatten() {
        form.text("timestamp_granularities[]", granularity);
    }
    let strategy = match &options.chunking_strategy {
        Some(strategy) => Some(chunking(strategy)?),
        None if diarize => Some("auto".to_owned()),
        None => None,
    };
    if let Some(strategy) = strategy {
        form.text("chunking_strategy", &strategy);
    }
    let request = http_request(
        &errors::API,
        "POST",
        provider.config(),
        &["audio", "transcriptions"],
        None,
        None,
    )?;
    let transport = provider.transport().ok_or_else(|| {
        Error::Configuration(format!(
            "provider {:?} has no transport for transcription",
            provider.name()
        ))
    })?;
    let response = http::json(
        &errors::API,
        transport,
        form.into_request(request),
        provider.name(),
        Some(&model.id),
    )
    .await?;
    let text = response["text"].as_str().ok_or_else(|| {
        http::response_error(provider.name(), "OpenAI transcription response has no text")
    })?;
    let segment = |item: &Value, key: &str| {
        Some(TranscriptionSegment {
            text: item[key].as_str()?.to_owned(),
            start_second: item["start"].as_f64()?,
            end_second: item["end"].as_f64()?,
        })
    };
    let segments = match (
        response["segments"].as_array(),
        response["words"].as_array(),
    ) {
        (Some(segments), _) => segments.iter().filter_map(|s| segment(s, "text")).collect(),
        (None, Some(words)) => words.iter().filter_map(|w| segment(w, "word")).collect(),
        (None, None) => Vec::new(),
    };
    let languages = response["languages"].as_array();
    let language = response["language"]
        .as_str()
        .and_then(language_code)
        .or_else(|| match languages {
            Some(list) if list.len() == 1 => list[0]["code"]
                .as_str()
                .filter(|code| code.len() == 2 && code.bytes().all(|b| b.is_ascii_lowercase()))
                .map(str::to_owned),
            _ => None,
        });
    let mut item = Item::new(Transcription {
        text: text.to_owned(),
        segments,
        language,
        duration_seconds: response["duration"].as_f64(),
    });
    if response["usage"]["type"] == "tokens" {
        item.usage = Some(Usage {
            input_tokens: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: response["usage"]["output_tokens"].as_u64().unwrap_or(0),
            raw: Some(response["usage"].clone()),
            ..Default::default()
        });
    }
    let diarized: Vec<Value> = response["segments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s.get("speaker").is_some())
        .map(|s| json!({"text": s["text"], "startSecond": s["start"], "endSecond": s["end"], "speaker": s["speaker"]}))
        .collect();
    let mut openai = Map::new();
    if !diarized.is_empty() {
        openai.insert("segments".into(), json!(diarized));
    }
    if let Some(languages) = languages {
        openai.insert("languages".into(), json!(languages));
    }
    if !openai.is_empty() {
        item.provider_metadata = Some([("openai".to_owned(), Value::Object(openai))].into());
    }
    Ok(item)
}
