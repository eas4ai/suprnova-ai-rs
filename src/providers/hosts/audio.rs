//! Mistral's speech (`mistral-speech-model.ts`, HST-046): `POST
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
        operations::{AudioParams, AudioPrompt, Item},
    },
    types::messages::FilePart,
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
