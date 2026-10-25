//! Mistral's speech (`mistral-speech-model.ts`, HST-046) and transcription
//! (`mistral-transcription-model.ts`, HST-047). Speech sends `POST
//! <base>/audio/speech` with the `mistral` options, answered with base64
//! audio in one of five formats. The settings the TypeScript model drops
//! with a warning, an unknown output format, the prompt's instructions, a
//! speed and a language, are left out with the same warnings in
//! `Item.warnings`; an option outside its schema, or a key the entry does not
//! take, fails with `Error::Validation` before any request.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Number, Value, json};

use super::images::warning;
use crate::{
    Error, Model, Provider,
    providers::{
        http::{self, ApiProfile},
        multipart::Multipart,
        openai::{audio_extension, media_bytes},
        operations::{
            AudioParams, AudioPrompt, Item, MediaInput, TranscribeInput, TranscribeParams,
            Transcription, TranscriptionSegment,
        },
    },
    types::{messages::FilePart, usage::Usage},
};

/// The output formats Mistral takes, with their media types; the first is
/// the default.
const FORMATS: &[(&str, &str)] = &[
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("pcm", "audio/pcm"),
    ("flac", "audio/flac"),
    ("opus", "audio/opus"),
];

fn invalid(message: String) -> Error {
    Error::Validation(message)
}

/// The `mistral` speech options (`mistralSpeechModelOptions`).
#[derive(Default)]
struct SpeechOptions {
    ref_audio: Option<String>,
    expressiveness: Option<Number>,
    phoneme_hints: Option<Map<String, Value>>,
}

impl SpeechOptions {
    fn parse(params: &AudioParams) -> Result<Self, Error> {
        let Some(value) = params.provider_options.get("mistral") else {
            return Ok(Self::default());
        };
        let Value::Object(entry) = value else {
            return Err(invalid("mistral speech options must be an object".into()));
        };
        let mut options = Self::default();
        for (option, value) in entry {
            match option.as_str() {
                "refAudio" => {
                    let audio = value.as_str().filter(|audio| !audio.is_empty()).ok_or_else(|| {
                        invalid(format!(
                            "mistral speech option refAudio must be a non-empty string, not {value}"
                        ))
                    })?;
                    options.ref_audio = Some(audio.to_owned());
                }
                "expressiveness" => {
                    options.expressiveness = Some(match value {
                        Value::Number(level)
                            if level.as_f64().is_some_and(|l| (1.0..=5.0).contains(&l)) =>
                        {
                            level.clone()
                        }
                        other => {
                            return Err(invalid(format!(
                                "mistral speech option expressiveness must be a number from 1 to 5, not {other}"
                            )));
                        }
                    });
                }
                "phonemeHints" => {
                    let bad = || {
                        invalid(
                            "mistral speech option phonemeHints must map non-empty words to non-empty lists of non-empty phonemes".into(),
                        )
                    };
                    let Value::Object(hints) = value else {
                        return Err(bad());
                    };
                    for (word, phonemes) in hints {
                        let list = phonemes.as_array().filter(|list| !list.is_empty());
                        let Some(list) = list else {
                            return Err(bad());
                        };
                        if word.is_empty()
                            || list
                                .iter()
                                .any(|phoneme| phoneme.as_str().is_none_or(str::is_empty))
                        {
                            return Err(bad());
                        }
                    }
                    options.phoneme_hints = Some(hints.clone());
                }
                other => {
                    return Err(invalid(format!(
                        "mistral speech option {other} is not known"
                    )));
                }
            }
        }
        Ok(options)
    }
}

/// Generate speech (`doGenerate`): the `audio_data` of the response, decoded,
/// as one file part of the format's media type.
pub(super) async fn mistral_speech(
    api: &'static ApiProfile,
    model: &Model,
    prompt: AudioPrompt,
    params: AudioParams,
    provider: &dyn Provider,
) -> Result<Item<Vec<FilePart>>, Error> {
    let options = SpeechOptions::parse(&params)?;
    let mut warnings = Vec::new();
    let requested = params.output_format.as_deref().unwrap_or(FORMATS[0].0);
    let (format, media_type) = match FORMATS.iter().find(|(name, _)| *name == requested) {
        Some(known) => *known,
        None => {
            warnings.push(warning(
                "unsupported",
                Some("outputFormat"),
                &format!("Unsupported output format: {requested}. Using mp3 instead."),
            ));
            FORMATS[0]
        }
    };
    if prompt.instructions.is_some() {
        warnings.push(warning(
            "unsupported",
            Some("instructions"),
            "Mistral speech models do not support the `instructions` option. Use a reference audio clip to guide delivery.",
        ));
    }
    if params.speed.is_some() {
        warnings.push(warning(
            "unsupported",
            Some("speed"),
            "Mistral speech models do not support the `speed` option. It was ignored.",
        ));
    }
    if params.language.is_some() {
        warnings.push(warning(
            "unsupported",
            Some("language"),
            "Mistral speech models do not support the `language` option. Language is inferred from the input text and voice.",
        ));
    }
    let mut body = json!({
        "model": model.id,
        "input": prompt.text,
        "response_format": format,
        "stream": false,
    });
    match &options.ref_audio {
        Some(audio) => body["ref_audio"] = json!(audio),
        None => {
            if let Some(voice) = &params.voice {
                body["voice_id"] = json!(voice);
            }
        }
    }
    if let Some(level) = options.expressiveness {
        body["expressiveness"] = Value::Number(level);
    }
    if let Some(hints) = options.phoneme_hints {
        body["phoneme_hints"] = Value::Object(hints);
    }
    let request = http::request(
        api,
        provider.config(),
        "POST",
        &["audio", "speech"],
        Some(body),
        Default::default(),
        Default::default(),
    )?;
    let transport = provider.transport().ok_or_else(|| {
        Error::Configuration(format!(
            "provider {:?} has no transport for speech",
            provider.name()
        ))
    })?;
    let response = http::json(api, transport, request, provider.name(), Some(&model.id)).await?;
    let encoded = response["audio_data"].as_str().ok_or_else(|| {
        http::response_error(provider.name(), "Mistral speech response has no audio_data")
    })?;
    let bytes = STANDARD.decode(encoded).map_err(|_| {
        http::response_error(
            provider.name(),
            "Mistral speech response holds invalid base64 audio",
        )
    })?;
    let audio = FilePart::from_bytes(bytes, Some(media_type), None)
        .map_err(|e| http::response_error(provider.name(), e.to_string()))?;
    let mut item = Item::new(vec![audio]);
    item.warnings = warnings;
    Ok(item)
}

/// The transcription models that cannot take a language with timestamps
/// (`mistral-transcription-model.ts`).
const LEGACY_TRANSCRIBERS: [&str; 3] = [
    "voxtral-mini-2507",
    "voxtral-mini-2602",
    "voxtral-mini-latest",
];

/// The `mistral` transcription options
/// (`mistralTranscriptionModelOptions`).
#[derive(Default)]
struct TranscribeOptions {
    language: Option<String>,
    return_language: Option<bool>,
    temperature: Option<Number>,
    timestamp_granularities: Option<Vec<&'static str>>,
    diarize: Option<bool>,
    context_bias: Option<Vec<String>>,
}

impl TranscribeOptions {
    fn parse(model: &str, params: &TranscribeParams) -> Result<Self, Error> {
        let Some(value) = params.provider_options.get("mistral") else {
            return Ok(Self::default());
        };
        let Value::Object(entry) = value else {
            return Err(invalid(
                "mistral transcription options must be an object".into(),
            ));
        };
        let flag = |option: &str, value: &Value| {
            value.as_bool().ok_or_else(|| {
                invalid(format!(
                    "mistral transcription option {option} must be a boolean, not {value}"
                ))
            })
        };
        let mut options = Self::default();
        for (option, value) in entry {
            match option.as_str() {
                "language" => {
                    let language = value.as_str().filter(|l| !l.is_empty()).ok_or_else(|| {
                        invalid(format!(
                            "mistral transcription option language must be a non-empty string, not {value}"
                        ))
                    })?;
                    options.language = Some(language.to_owned());
                }
                "returnLanguage" => options.return_language = Some(flag(option, value)?),
                "temperature" => {
                    options.temperature = Some(match value {
                        Value::Number(temperature) => temperature.clone(),
                        other => {
                            return Err(invalid(format!(
                                "mistral transcription option temperature must be a number, not {other}"
                            )));
                        }
                    });
                }
                "timestampGranularities" => {
                    let bad = || {
                        invalid(format!(
                            "mistral transcription option timestampGranularities must be a non-empty list of segment and word, not {value}"
                        ))
                    };
                    let list = value
                        .as_array()
                        .filter(|list| !list.is_empty())
                        .ok_or_else(bad)?;
                    let mut granularities = Vec::with_capacity(list.len());
                    for granularity in list {
                        granularities.push(match granularity.as_str() {
                            Some("segment") => "segment",
                            Some("word") => "word",
                            _ => return Err(bad()),
                        });
                    }
                    options.timestamp_granularities = Some(granularities);
                }
                "diarize" => options.diarize = Some(flag(option, value)?),
                "contextBias" => {
                    let bad = || {
                        invalid(format!(
                            "mistral transcription option contextBias must be a list of non-empty strings, not {value}"
                        ))
                    };
                    let list = value.as_array().ok_or_else(bad)?;
                    let mut bias = Vec::with_capacity(list.len());
                    for entry in list {
                        let text = entry
                            .as_str()
                            .filter(|text| !text.is_empty())
                            .ok_or_else(bad)?;
                        bias.push(text.to_owned());
                    }
                    options.context_bias = Some(bias);
                }
                other => {
                    return Err(invalid(format!(
                        "mistral transcription option {other} is not known"
                    )));
                }
            }
        }
        // The two combinations TypeScript refuses.
        if options.language.is_some() && options.return_language == Some(true) {
            return Err(invalid(
                "mistral transcription option language cannot be combined with returnLanguage"
                    .into(),
            ));
        }
        if options.language.is_some()
            && options.timestamp_granularities.is_some()
            && LEGACY_TRANSCRIBERS.contains(&model)
        {
            return Err(invalid(format!(
                "mistral transcription option language cannot be combined with timestampGranularities on {model}"
            )));
        }
        Ok(options)
    }
}

/// Transcribe audio (`doGenerate`): a multipart form with the model, the
/// file and the options, answered with the text, its segments, its language
/// and its usage.
pub(super) async fn mistral_transcribe(
    api: &'static ApiProfile,
    model: &Model,
    audio: TranscribeInput,
    params: TranscribeParams,
    provider: &dyn Provider,
) -> Result<Item<Transcription>, Error> {
    let options = TranscribeOptions::parse(&model.id, &params)?;
    let input = match audio {
        TranscribeInput::File(file) => MediaInput::File(file),
        TranscribeInput::Bytes(bytes) => MediaInput::Bytes(bytes),
    };
    let (bytes, media_type) = media_bytes(&input, provider, "audio").await?;
    let mut form = Multipart::new();
    form.text("model", &model.id);
    form.file(
        "file",
        &format!("audio.{}", audio_extension(&media_type)),
        &media_type,
        &bytes,
    );
    if let Some(language) = &options.language {
        form.text("language", language);
    }
    if let Some(return_language) = options.return_language {
        form.text("return_language", &return_language.to_string());
    }
    if let Some(temperature) = &options.temperature {
        form.text("temperature", &temperature.to_string());
    }
    for granularity in options.timestamp_granularities.iter().flatten() {
        form.text("timestamp_granularities", granularity);
    }
    if let Some(diarize) = options.diarize {
        form.text("diarize", &diarize.to_string());
    }
    for bias in options.context_bias.iter().flatten() {
        form.text("context_bias", bias);
    }
    let request = http::request(
        api,
        provider.config(),
        "POST",
        &["audio", "transcriptions"],
        None,
        Default::default(),
        Default::default(),
    )?;
    let transport = provider.transport().ok_or_else(|| {
        Error::Configuration(format!(
            "provider {:?} has no transport for transcription",
            provider.name()
        ))
    })?;
    let response = http::json(
        api,
        transport,
        form.into_request(request),
        provider.name(),
        Some(&model.id),
    )
    .await?;
    let text = response["text"].as_str().ok_or_else(|| {
        http::response_error(
            provider.name(),
            "Mistral transcription response has no text",
        )
    })?;
    let raw_segments = response["segments"].as_array();
    let segments: Vec<TranscriptionSegment> = raw_segments
        .into_iter()
        .flatten()
        .filter_map(|segment| {
            Some(TranscriptionSegment {
                text: segment["text"].as_str()?.to_owned(),
                start_second: segment["start"].as_f64()?,
                end_second: segment["end"].as_f64()?,
            })
        })
        .collect();
    let usage = response.get("usage").filter(|usage| usage.is_object());
    let duration_seconds = usage
        .and_then(|usage| usage["prompt_audio_seconds"].as_f64())
        .or_else(|| segments.last().map(|segment| segment.end_second));
    let mut item = Item::new(Transcription {
        text: text.to_owned(),
        segments,
        language: response["language"].as_str().map(str::to_owned),
        duration_seconds,
    });
    let mut mistral = Map::new();
    if let Some(usage) = usage {
        item.usage = Some(Usage {
            input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
            raw: Some(usage.clone()),
            ..Default::default()
        });
        let mut camel = Map::new();
        for (key, name) in [
            ("prompt_tokens", "promptTokens"),
            ("completion_tokens", "completionTokens"),
            ("total_tokens", "totalTokens"),
            ("prompt_audio_seconds", "promptAudioSeconds"),
            ("request_count", "requestCount"),
        ] {
            if let Some(value) = usage.get(key).filter(|value| !value.is_null()) {
                camel.insert(name.into(), value.clone());
            }
        }
        mistral.insert("usage".into(), Value::Object(camel));
    }
    // The segments with a type, score or speaker, as TypeScript keeps them.
    let detailed: Vec<Value> = raw_segments
        .into_iter()
        .flatten()
        .filter(|segment| {
            ["type", "score", "speaker_id"]
                .iter()
                .any(|key| segment.get(*key).is_some_and(|value| !value.is_null()))
        })
        .map(|segment| {
            let mut out = json!({
                "text": segment["text"],
                "startSecond": segment["start"],
                "endSecond": segment["end"],
            });
            for (key, name) in [
                ("type", "type"),
                ("score", "score"),
                ("speaker_id", "speakerId"),
            ] {
                if let Some(value) = segment.get(key).filter(|value| !value.is_null()) {
                    out[name] = value.clone();
                }
            }
            out
        })
        .collect();
    if !detailed.is_empty() {
        mistral.insert("segments".into(), Value::Array(detailed));
    }
    if !mistral.is_empty() {
        item.provider_metadata = Some([("mistral".to_owned(), Value::Object(mistral))].into());
    }
    Ok(item)
}
