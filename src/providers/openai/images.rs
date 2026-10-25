//! Image generation and edits with OpenAI (`image/openai-image-model.ts`),
//! OAI-017.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{errors, params::invalid, protocol::http_request};
use crate::{
    Error, Model, Provider,
    models::files::{self, DEFAULT_MAX_BYTES},
    providers::{
        http,
        multipart::Multipart,
        operations::{CallLimits, ImageParams, ImagePrompt, Item, MediaInput},
    },
    types::{
        media,
        messages::{FileData, FilePart},
        usage::Usage,
    },
};

/// The images one call returns: 10 for `dall-e-2`, `chatgpt-image-latest`
/// and `gpt-image-` models, 1 otherwise (`getMaxImagesPerCall`).
pub(super) fn limits(model: &Model) -> CallLimits {
    let id = model.id.as_str();
    let ten = id == "dall-e-2" || id == "chatgpt-image-latest" || id.starts_with("gpt-image-");
    CallLimits {
        max_per_call: Some(if ten { 10 } else { 1 }),
        parallel: true,
    }
}

/// The options of a generation (`openaiImageModelGenerationOptions`) and
/// of an edit (`openaiImageModelEditOptions`).
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageOptions {
    quality: Option<String>,
    background: Option<String>,
    output_format: Option<String>,
    output_compression: Option<i64>,
    user: Option<String>,
    style: Option<String>,
    moderation: Option<String>,
    input_fidelity: Option<String>,
}

fn one_of(name: &str, value: Option<&str>, allowed: &[&str]) -> Result<(), Error> {
    match value {
        Some(value) if !allowed.contains(&value) => Err(invalid(format!(
            "openai image option {name} must be one of {}, not {value:?}",
            allowed.join(", ")
        ))),
        _ => Ok(()),
    }
}

impl ImageOptions {
    fn parse(params: &ImageParams, edit: bool) -> Result<Self, Error> {
        let options: Self = match params.provider_options.get("openai") {
            None => Self::default(),
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| invalid(format!("invalid openai image options: {e}")))?,
        };
        one_of(
            "quality",
            options.quality.as_deref(),
            &[
                "standard", "hd", "low", "medium", "high", "xhigh", "max", "auto",
            ],
        )?;
        one_of(
            "background",
            options.background.as_deref(),
            &["transparent", "opaque", "auto"],
        )?;
        one_of(
            "outputFormat",
            options.output_format.as_deref(),
            &["png", "jpeg", "webp"],
        )?;
        if options
            .output_compression
            .is_some_and(|c| !(0..=100).contains(&c))
        {
            return Err(invalid(
                "openai image option outputCompression must be from 0 to 100",
            ));
        }
        let (generation_only, edit_only) = (
            options.style.is_some() || options.moderation.is_some(),
            options.input_fidelity.is_some(),
        );
        if edit && generation_only {
            return Err(invalid(
                "openai image options style and moderation apply to generations, not edits",
            ));
        }
        if !edit && edit_only {
            return Err(invalid(
                "openai image option inputFidelity applies to edits, not generations",
            ));
        }
        one_of("style", options.style.as_deref(), &["vivid", "natural"])?;
        one_of(
            "moderation",
            options.moderation.as_deref(),
            &["auto", "low"],
        )?;
        one_of(
            "inputFidelity",
            options.input_fidelity.as_deref(),
            &["high", "low"],
        )?;
        Ok(options)
    }
}

/// The bytes and media type of an input, fetching a URL through the
/// provider's download transport.
pub(crate) async fn media_bytes(
    input: &MediaInput,
    provider: &dyn Provider,
    what: &str,
) -> Result<(Vec<u8>, String), Error> {
    let download = |url: String| async move {
        let transport = provider.download_transport().ok_or_else(|| {
            invalid(format!(
                "OpenAI takes {what} as bytes; build the provider with with_download_transport to fetch {url}"
            ))
        })?;
        let (bytes, media_type) = files::download(transport.as_ref(), &url, DEFAULT_MAX_BYTES)
            .await
            .map_err(|e| invalid(format!("could not fetch {what} {url}: {e}")))?;
        Ok::<_, Error>((bytes.to_vec(), media_type))
    };
    let detect = |bytes: &[u8]| {
        media::detect_image_media_type(bytes)
            .or_else(|| media::detect_audio_media_type(bytes))
            .unwrap_or("application/octet-stream")
            .to_owned()
    };
    match input {
        MediaInput::Bytes(bytes) => Ok((bytes.clone(), detect(bytes))),
        MediaInput::Url(url) => {
            let (bytes, media_type) = download(url.clone()).await?;
            let media_type = media_type.unwrap_or_else(|| detect(&bytes));
            Ok((bytes, media_type))
        }
        MediaInput::File(file) => match &file.data {
            FileData::Bytes(bytes) => Ok((bytes.clone(), file.media_type.clone())),
            FileData::Text(text) if media::is_downloadable_url(text) => {
                let (bytes, _) = download(text.clone()).await?;
                Ok((bytes, file.media_type.clone()))
            }
            FileData::Text(text) => {
                let encoded = match media::split_data_url(text) {
                    (_, Some(data)) => data,
                    _ => text.as_str(),
                };
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|_| invalid(format!("{what} is neither a URL nor base64 data")))?;
                Ok((bytes, file.media_type.clone()))
            }
        },
    }
}

/// The extension of a media type, for a form filename.
pub(super) fn extension(media_type: &str) -> &str {
    match media_type {
        "image/jpeg" => "jpg",
        "audio/mpeg" => "mp3",
        other => other.split_once('/').map_or("bin", |(_, sub)| {
            sub.split(['+', ';']).next().unwrap_or("bin")
        }),
    }
}

/// The share of `tokens` of image `index` of `total`, the last taking the
/// remainder (`distributeTokenDetails`).
fn share(tokens: u64, index: usize, total: usize) -> u64 {
    let base = tokens / total as u64;
    if index + 1 == total {
        tokens - base * (total as u64 - 1)
    } else {
        base
    }
}

pub(super) async fn generate(
    model: &Model,
    prompt: ImagePrompt,
    params: ImageParams,
    provider: &dyn Provider,
) -> Result<Item<Vec<FilePart>>, Error> {
    if params.aspect_ratio.is_some() {
        return Err(invalid(
            "OpenAI image models take a size, not an aspect ratio",
        ));
    }
    if params.seed.is_some() {
        return Err(invalid("OpenAI image models do not take a seed"));
    }
    let edit = !prompt.images.is_empty() || prompt.mask.is_some();
    if edit && model.id == "dall-e-3" {
        return Err(invalid("dall-e-3 does not edit images"));
    }
    let options = ImageOptions::parse(&params, edit)?;
    let transport = provider.transport().ok_or_else(|| {
        Error::Configuration(format!(
            "provider {:?} has no transport for images",
            provider.name()
        ))
    })?;
    let request = if edit {
        let mut form = Multipart::new();
        form.text("model", &model.id);
        if let Some(text) = &prompt.text {
            form.text("prompt", text);
        }
        let field = if prompt.images.len() == 1 {
            "image"
        } else {
            "image[]"
        };
        for (index, image) in prompt.images.iter().enumerate() {
            let (bytes, media_type) = media_bytes(image, provider, "an image").await?;
            let name = format!("image-{index}.{}", extension(&media_type));
            form.file(field, &name, &media_type, &bytes);
        }
        if let Some(mask) = &prompt.mask {
            let (bytes, media_type) = media_bytes(mask, provider, "a mask").await?;
            form.file(
                "mask",
                &format!("mask.{}", extension(&media_type)),
                &media_type,
                &bytes,
            );
        }
        form.text("n", &params.n.to_string());
        if let Some(size) = &params.size {
            form.text("size", size);
        }
        for (name, value) in [
            ("quality", &options.quality),
            ("background", &options.background),
            ("output_format", &options.output_format),
            ("input_fidelity", &options.input_fidelity),
            ("user", &options.user),
        ] {
            if let Some(value) = value {
                form.text(name, value);
            }
        }
        if let Some(compression) = options.output_compression {
            form.text("output_compression", &compression.to_string());
        }
        let request = http_request(
            &errors::API,
            "POST",
            provider.config(),
            &["images", "edits"],
            None,
            None,
        )?;
        form.into_request(request)
    } else {
        let mut body = json!({"model": model.id, "prompt": prompt.text.as_deref().unwrap_or(""), "n": params.n});
        if let Some(size) = &params.size {
            body["size"] = json!(size);
        }
        for (name, value) in [
            ("quality", &options.quality),
            ("style", &options.style),
            ("background", &options.background),
            ("moderation", &options.moderation),
            ("output_format", &options.output_format),
            ("user", &options.user),
        ] {
            if let Some(value) = value {
                body[name] = json!(value);
            }
        }
        if let Some(compression) = options.output_compression {
            body["output_compression"] = json!(compression);
        }
        let default_format =
            model.id.starts_with("gpt-image-") || model.id.starts_with("chatgpt-image-");
        if !default_format {
            body["response_format"] = json!("b64_json");
        }
        http_request(
            &errors::API,
            "POST",
            provider.config(),
            &["images", "generations"],
            Some(body),
            None,
        )?
    };
    let response = http::json(
        &errors::API,
        transport,
        request,
        provider.name(),
        Some(&model.id),
    )
    .await?;
    let data = response["data"].as_array().cloned().unwrap_or_default();
    let format = response["output_format"]
        .as_str()
        .or(options.output_format.as_deref())
        .unwrap_or("png")
        .to_owned();
    let mut images = Vec::with_capacity(data.len());
    let mut metadata = Vec::with_capacity(data.len());
    let details = &response["usage"]["input_tokens_details"];
    for (index, item) in data.iter().enumerate() {
        let b64 = item["b64_json"].as_str().ok_or_else(|| {
            http::response_error(
                provider.name(),
                "OpenAI image response has no b64_json image",
            )
        })?;
        let bytes = STANDARD.decode(b64).map_err(|_| {
            http::response_error(provider.name(), "OpenAI image response has invalid base64")
        })?;
        images.push(
            FilePart::from_bytes(bytes, Some(&format!("image/{format}")), None)
                .map_err(|e| http::response_error(provider.name(), e.to_string()))?,
        );
        let mut entry = Map::new();
        if let Some(revised) = item["revised_prompt"].as_str().filter(|r| !r.is_empty()) {
            entry.insert("revisedPrompt".into(), json!(revised));
        }
        for (src, dst) in [
            ("created", "created"),
            ("size", "size"),
            ("quality", "quality"),
            ("background", "background"),
            ("output_format", "outputFormat"),
        ] {
            if !response[src].is_null() {
                entry.insert(dst.into(), response[src].clone());
            }
        }
        for (src, dst) in [
            ("image_tokens", "imageTokens"),
            ("text_tokens", "textTokens"),
        ] {
            if let Some(tokens) = details[src].as_u64() {
                entry.insert(dst.into(), json!(share(tokens, index, data.len())));
            }
        }
        metadata.push(Value::Object(entry));
    }
    let mut item = Item::new(images);
    if response["usage"].is_object() {
        item.usage = Some(Usage {
            input_tokens: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: response["usage"]["output_tokens"].as_u64().unwrap_or(0),
            raw: Some(response["usage"].clone()),
            ..Default::default()
        });
    }
    item.provider_metadata = Some([("openai".to_owned(), json!({"images": metadata}))].into());
    Ok(item)
}
