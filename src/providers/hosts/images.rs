//! The image models of Together AI (`togetherai-image-model.ts`, HST-029),
//! DeepInfra (`deepinfra-image-model.ts`, HST-030) and Fireworks
//! (`fireworks-image-model.ts`, HST-031). Each call makes one image; `ops::generate_image` makes one call per
//! image (OPS-003). Warnings that TypeScript returns come back in
//! `Item.warnings`.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

use crate::{
    Error, Model, Provider,
    models::files::{self, DEFAULT_MAX_BYTES, DownloadTransport},
    providers::{
        http::{self, ApiProfile},
        multipart::Multipart,
        operations::{ImageParams, ImagePrompt, Item, MediaInput, Warning},
    },
    types::{
        media,
        messages::{FileData, FilePart},
    },
};

/// A host's image model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Images {
    TogetherAI,
    DeepInfra,
    Fireworks,
}

/// Generate the images of one call, fetching input image URLs through
/// `downloads` where the host takes images as bytes.
pub(crate) async fn generate(
    images: Images,
    api: &'static ApiProfile,
    downloads: Option<&dyn DownloadTransport>,
    model: &Model,
    prompt: ImagePrompt,
    params: ImageParams,
    provider: &dyn Provider,
) -> Result<Item<Vec<FilePart>>, Error> {
    match images {
        Images::TogetherAI => together(api, model, prompt, params, provider).await,
        Images::DeepInfra => deepinfra(api, downloads, model, prompt, params, provider).await,
        Images::Fireworks => fireworks(api, downloads, model, prompt, params, provider).await,
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

/// The host's entry of image options, as an object.
fn entry(params: &ImageParams, key: &str) -> Result<Map<String, Value>, Error> {
    match params.provider_options.get(key) {
        None => Ok(Map::new()),
        Some(Value::Object(entry)) => Ok(entry.clone()),
        Some(_) => Err(invalid(format!("{key} image options must be an object"))),
    }
}

/// Fail on an entry value of the wrong type. Together AI's and DeepInfra's
/// schemas (`nullish`) take `null` for these options too.
fn typed(
    entry: &Map<String, Value>,
    key: &str,
    option: &str,
    expected: &str,
    ok: fn(&Value) -> bool,
) -> Result<(), Error> {
    match entry.get(option) {
        Some(value) if !value.is_null() && !ok(value) => Err(invalid(format!(
            "{key} image option {option} must be {expected}, not {value}"
        ))),
        _ => Ok(()),
    }
}

/// [`typed`] for a schema whose options are `optional`, as Fireworks' are,
/// which takes no `null`.
fn typed_present(
    entry: &Map<String, Value>,
    key: &str,
    option: &str,
    expected: &str,
    ok: fn(&Value) -> bool,
) -> Result<(), Error> {
    match entry.get(option) {
        Some(value) if !ok(value) => Err(invalid(format!(
            "{key} image option {option} must be {expected}, not {value}"
        ))),
        _ => Ok(()),
    }
}

/// A size `<width>x<height>`, as its two numbers.
fn size(size: &str) -> Result<(u64, u64), Error> {
    size.split_once('x')
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
        .ok_or_else(|| invalid(format!("image size must be <width>x<height>, not {size:?}")))
}

/// An input image as a URL: a URL as given, bytes or a file as a data URL
/// (`convertImageModelFileToDataUri`).
fn data_url(input: &MediaInput) -> String {
    let encoded = |media_type: &str, bytes: &[u8]| {
        format!("data:{media_type};base64,{}", STANDARD.encode(bytes))
    };
    match input {
        MediaInput::Url(url) => url.clone(),
        MediaInput::Bytes(bytes) => encoded(
            media::detect_image_media_type(bytes.as_slice()).unwrap_or("application/octet-stream"),
            bytes,
        ),
        MediaInput::File(file) => match &file.data {
            FileData::Text(text) if media::is_url(text) => text.clone(),
            FileData::Text(text) => format!("data:{};base64,{text}", file.media_type),
            FileData::Bytes(bytes) => encoded(&file.media_type, bytes),
        },
    }
}

pub(super) fn warning(kind: &str, feature: Option<&str>, message: &str) -> Warning {
    let (details, message) = match feature {
        Some(_) => (Some(message.to_owned()), None),
        None => (None, Some(message.to_owned())),
    };
    Warning {
        kind: kind.into(),
        feature: feature.map(str::to_owned),
        details,
        message,
        ..Warning::default()
    }
}

/// An image the response holds as bytes, with the media type its bytes
/// show, PNG when they show none.
fn image(provider: &dyn Provider, bytes: Vec<u8>) -> Result<FilePart, Error> {
    let media_type = media::detect_image_media_type(bytes.as_slice()).unwrap_or("image/png");
    FilePart::from_bytes(bytes, Some(media_type), None)
        .map_err(|e| http::response_error(provider.name(), format!("image response: {e}")))
}

/// An image the response holds as base64 text.
fn base64_image(provider: &dyn Provider, text: &str) -> Result<FilePart, Error> {
    let bytes = STANDARD.decode(text).map_err(|_| {
        http::response_error(provider.name(), "image response holds invalid base64")
    })?;
    image(provider, bytes)
}

/// `POST <base>/images/generations`, as `togetherai-image-model.ts` sends it.
async fn together(
    api: &'static ApiProfile,
    model: &Model,
    prompt: ImagePrompt,
    params: ImageParams,
    provider: &dyn Provider,
) -> Result<Item<Vec<FilePart>>, Error> {
    if prompt.mask.is_some() {
        return Err(invalid(
            "Together AI does not support mask-based image editing; use a FLUX Kontext model, \
             such as black-forest-labs/FLUX.1-kontext-pro, with a reference image and a prompt",
        ));
    }
    let mut options = entry(&params, "togetherai")?;
    typed(
        &options,
        "togetherai",
        "steps",
        "a number",
        Value::is_number,
    )?;
    typed(
        &options,
        "togetherai",
        "guidance",
        "a number",
        Value::is_number,
    )?;
    typed(
        &options,
        "togetherai",
        "negative_prompt",
        "a string",
        Value::is_string,
    )?;
    typed(
        &options,
        "togetherai",
        "disable_safety_checker",
        "a boolean",
        Value::is_boolean,
    )?;
    let mut warnings = Vec::new();
    if params.aspect_ratio.is_some() {
        warnings.push(warning(
            "unsupported",
            Some("aspectRatio"),
            "This model does not support the `aspectRatio` option. Use `size` instead.",
        ));
    }
    // A model that does not diffuse takes no seed or diffusion settings.
    let diffusion = model.id != "google/gemini-3-pro-image";
    if !diffusion {
        for option in [
            "steps",
            "guidance",
            "negative_prompt",
            "disable_safety_checker",
        ] {
            options.remove(option);
        }
        if params.seed.is_some() {
            warnings.push(warning(
                "unsupported",
                Some("seed"),
                &format!("The {} model does not support the `seed` option.", model.id),
            ));
        }
    }
    if prompt.images.len() > 1 {
        warnings.push(warning(
            "other",
            None,
            "Together AI only supports a single input image. Additional images are ignored.",
        ));
    }
    let mut body = json!({"model": model.id});
    if let Some(text) = &prompt.text {
        body["prompt"] = json!(text);
    }
    if let Some(seed) = params.seed.filter(|_| diffusion) {
        body["seed"] = json!(seed);
    }
    if params.n > 1 {
        body["n"] = json!(params.n);
    }
    if let Some(value) = &params.size {
        let (width, height) = size(value)?;
        body["width"] = json!(width);
        body["height"] = json!(height);
    }
    if let Some(first) = prompt.images.first() {
        body["image_url"] = json!(data_url(first));
    }
    body["response_format"] = json!("base64");
    if let Value::Object(fields) = &mut body {
        fields.extend(options);
    }
    let request = http::request(
        api,
        provider.config(),
        "POST",
        &["images", "generations"],
        Some(body),
        Default::default(),
        Default::default(),
    )?;
    // The images come inside the JSON, so it is read up to the media limit.
    let (_, response) = http::json_with_limit(
        api,
        transport(provider)?,
        request,
        provider.name(),
        Some(&model.id),
        DEFAULT_MAX_BYTES,
    )
    .await?;
    let images = response["data"]
        .as_array()
        .ok_or_else(|| {
            http::response_error(provider.name(), "Together AI image response has no data")
        })?
        .iter()
        .map(|item| {
            let text = item["b64_json"].as_str().ok_or_else(|| {
                http::response_error(
                    provider.name(),
                    "Together AI image response has no b64_json",
                )
            })?;
            base64_image(provider, text)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut item = Item::new(images);
    item.warnings = warnings;
    Ok(item)
}

fn transport(provider: &dyn Provider) -> Result<&dyn crate::transport::HttpTransport, Error> {
    provider.transport().ok_or_else(|| {
        Error::Configuration(format!(
            "provider {:?} has no transport for images",
            provider.name()
        ))
    })
}

/// A model id as URL path segments. An id with an empty, `.` or `..`
/// segment, or a control character, would name another path.
fn model_path(id: &str) -> Result<Vec<&str>, Error> {
    let segments: Vec<&str> = id.split('/').collect();
    if segments
        .iter()
        .any(|s| s.is_empty() || *s == "." || *s == ".." || s.chars().any(char::is_control))
    {
        return Err(invalid(format!(
            "{id:?} is not a model id that can go in a URL path"
        )));
    }
    Ok(segments)
}

/// The bytes and media type of an input image, fetching a URL through the
/// download transport.
async fn image_bytes(
    input: &MediaInput,
    downloads: Option<&dyn DownloadTransport>,
    host: &str,
) -> Result<(Vec<u8>, String), Error> {
    let download = |url: &str| {
        let url = url.to_owned();
        async move {
            let downloads = downloads.ok_or_else(|| {
                invalid(format!(
                    "{host} takes input images as bytes; build the provider with \
                     with_download_transport to fetch {url}"
                ))
            })?;
            let (bytes, media_type) = files::download(downloads, &url, DEFAULT_MAX_BYTES)
                .await
                .map_err(|e| invalid(format!("could not fetch the image {url}: {e}")))?;
            Ok::<_, Error>((bytes.to_vec(), media_type))
        }
    };
    let detected = |bytes: &[u8]| {
        media::detect_image_media_type(bytes)
            .unwrap_or("application/octet-stream")
            .to_owned()
    };
    match input {
        MediaInput::Bytes(bytes) => Ok((bytes.clone(), detected(bytes))),
        MediaInput::Url(url) => {
            let (bytes, media_type) = download(url).await?;
            let media_type = media_type.unwrap_or_else(|| detected(&bytes));
            Ok((bytes, media_type))
        }
        MediaInput::File(file) => match &file.data {
            FileData::Bytes(bytes) => Ok((bytes.clone(), file.media_type.clone())),
            FileData::Text(text) if media::is_downloadable_url(text) => {
                let (bytes, _) = download(text).await?;
                Ok((bytes, file.media_type.clone()))
            }
            FileData::Text(text) => {
                let encoded = match media::split_data_url(text) {
                    (_, Some(data)) => data,
                    _ => text.as_str(),
                };
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|_| invalid("an input image is neither a URL nor base64 data"))?;
                Ok((bytes, file.media_type.clone()))
            }
        },
    }
}

/// The extension of an image media type, for a form filename.
fn extension(media_type: &str) -> &str {
    match media_type {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    }
}

/// An entry value as a form field's text: arrays repeat the field, as
/// `convertToFormData` does, and `null` sends nothing.
fn form_values(value: &Value) -> Vec<String> {
    match value {
        Value::Null => vec![],
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => items.iter().flat_map(form_values).collect(),
        other => vec![other.to_string()],
    }
}

/// DeepInfra's images: `POST <root>/inference/<model>`, or with input
/// images `POST <base>/images/edits` as a form (`deepinfra-image-model.ts`).
async fn deepinfra(
    api: &'static ApiProfile,
    downloads: Option<&dyn DownloadTransport>,
    model: &Model,
    prompt: ImagePrompt,
    params: ImageParams,
    provider: &dyn Provider,
) -> Result<Item<Vec<FilePart>>, Error> {
    let options = entry(&params, "deepinfra")?;
    for (option, expected, ok) in [
        (
            "negative_prompt",
            "a string",
            Value::is_string as fn(&Value) -> bool,
        ),
        ("num_inference_steps", "a number", Value::is_number),
        ("guidance_scale", "a number", Value::is_number),
        ("guidance", "a number", Value::is_number),
        ("quality", "a string", Value::is_string),
        ("style", "a string", Value::is_string),
        ("user", "a string", Value::is_string),
    ] {
        typed(&options, "deepinfra", option, expected, ok)?;
    }
    typed(&options, "deepinfra", "response_format", "b64_json", |v| {
        v == "b64_json"
    })?;
    let config = provider.config();
    let (request, edit) = if prompt.images.is_empty() {
        let base = config.base_url();
        let root = base
            .trim_end_matches('/')
            .strip_suffix("/openai")
            .ok_or_else(|| {
                Error::Configuration(format!(
                    "DeepInfra images go to the root of the base URL, which is the base URL \
                     without its last segment openai; {base} does not end in /openai"
                ))
            })?;
        let mut path = vec!["inference"];
        path.extend(model_path(&model.id)?);
        let mut body = json!({});
        if let Some(text) = &prompt.text {
            body["prompt"] = json!(text);
        }
        body["num_images"] = json!(params.n);
        if let Some(ratio) = &params.aspect_ratio {
            body["aspect_ratio"] = json!(ratio);
        }
        if let Some(value) = &params.size {
            let (width, height) = size(value)?;
            body["width"] = json!(width.to_string());
            body["height"] = json!(height.to_string());
        }
        if let Some(seed) = params.seed {
            body["seed"] = json!(seed);
        }
        if let Value::Object(fields) = &mut body {
            fields.extend(options);
        }
        let request = http::request_to(
            api,
            root,
            config,
            "POST",
            &path,
            Some(body),
            Default::default(),
            Default::default(),
        )?;
        (request, false)
    } else {
        let mut form = Multipart::new();
        form.text("model", &model.id);
        if let Some(text) = &prompt.text {
            form.text("prompt", text);
        }
        for (index, input) in prompt.images.iter().enumerate() {
            let (bytes, media_type) = image_bytes(input, downloads, "DeepInfra").await?;
            let name = format!("image-{index}.{}", extension(&media_type));
            form.file("image", &name, &media_type, &bytes);
        }
        if let Some(mask) = &prompt.mask {
            let (bytes, media_type) = image_bytes(mask, downloads, "DeepInfra").await?;
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
        for (option, value) in &options {
            for text in form_values(value) {
                form.text(option, &text);
            }
        }
        let request = http::request(
            api,
            config,
            "POST",
            &["images", "edits"],
            None,
            Default::default(),
            Default::default(),
        )?;
        (form.into_request(request), true)
    };
    // The images come inside the JSON, so it is read up to the media limit.
    let (_, response) = http::json_with_limit(
        api,
        transport(provider)?,
        request,
        provider.name(),
        Some(&model.id),
        DEFAULT_MAX_BYTES,
    )
    .await?;
    let bad = |what: &str| {
        http::response_error(
            provider.name(),
            format!("DeepInfra image response has no {what}"),
        )
    };
    let images = if edit {
        response["data"]
            .as_array()
            .ok_or_else(|| bad("data"))?
            .iter()
            .map(|item| {
                let text = item["b64_json"].as_str().ok_or_else(|| bad("b64_json"))?;
                base64_image(provider, text)
            })
            .collect::<Result<Vec<_>, Error>>()?
    } else {
        response["images"]
            .as_array()
            .ok_or_else(|| bad("images"))?
            .iter()
            .map(|item| {
                let text = item.as_str().ok_or_else(|| bad("image text"))?;
                base64_image(provider, without_data_prefix(text))
            })
            .collect::<Result<Vec<_>, Error>>()?
    };
    Ok(Item::new(images))
}

/// An image without a leading `data:image/<type>;base64,`.
fn without_data_prefix(text: &str) -> &str {
    text.strip_prefix("data:image/")
        .and_then(|rest| rest.split_once(";base64,"))
        .filter(|(kind, _)| {
            !kind.is_empty() && kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map_or(text, |(_, data)| data)
}

/// The Fireworks models that take a size, at `image_generation`.
const FIREWORKS_SIZE_MODELS: [&str; 5] = [
    "accounts/fireworks/models/playground-v2-5-1024px-aesthetic",
    "accounts/fireworks/models/japanese-stable-diffusion-xl",
    "accounts/fireworks/models/playground-v2-1024px-aesthetic",
    "accounts/fireworks/models/stable-diffusion-xl-1024-v1-0",
    "accounts/fireworks/models/SSD-1B",
];
/// The Fireworks Kontext models, whose workflows run as jobs to poll.
const FIREWORKS_KONTEXT_MODELS: [&str; 2] = [
    "accounts/fireworks/models/flux-kontext-pro",
    "accounts/fireworks/models/flux-kontext-max",
];
const FIREWORKS_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
const FIREWORKS_POLL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Fireworks' images, at the URL of the model's backend
/// (`fireworks-image-model.ts`).
async fn fireworks(
    api: &'static ApiProfile,
    downloads: Option<&dyn DownloadTransport>,
    model: &Model,
    prompt: ImagePrompt,
    params: ImageParams,
    provider: &dyn Provider,
) -> Result<Item<Vec<FilePart>>, Error> {
    let options = entry(&params, "fireworks")?;
    let integer = |v: &Value| v.is_i64() || v.is_u64();
    for (option, expected, ok) in [
        (
            "guidance_scale",
            "a number",
            Value::is_number as fn(&Value) -> bool,
        ),
        ("num_inference_steps", "an integer", integer),
        ("webhook_url", "a string", Value::is_string),
        ("webhook_secret", "a string", Value::is_string),
        ("prompt_upsampling", "a boolean", Value::is_boolean),
        ("cfg_scale", "a number", Value::is_number),
        ("steps", "an integer", integer),
    ] {
        typed_present(&options, "fireworks", option, expected, ok)?;
    }
    typed_present(&options, "fireworks", "output_format", "jpeg or png", |v| {
        v == "jpeg" || v == "png"
    })?;
    typed_present(
        &options,
        "fireworks",
        "safety_tolerance",
        "an integer from 0 to 6",
        |v| v.as_u64().is_some_and(|t| t <= 6),
    )?;
    let id = model.id.as_str();
    let size_model = FIREWORKS_SIZE_MODELS.contains(&id);
    let kontext = FIREWORKS_KONTEXT_MODELS.contains(&id);
    let mut warnings = Vec::new();
    if !size_model && params.size.is_some() {
        warnings.push(warning(
            "unsupported",
            Some("size"),
            "This model does not support the `size` option. Use `aspectRatio` instead.",
        ));
    }
    if size_model && params.aspect_ratio.is_some() {
        warnings.push(warning(
            "unsupported",
            Some("aspectRatio"),
            "This model does not support the `aspectRatio` option.",
        ));
    }
    if prompt.images.len() > 1 {
        warnings.push(warning(
            "other",
            None,
            "Fireworks only supports a single input image. Additional images are ignored.",
        ));
    }
    if prompt.mask.is_some() {
        warnings.push(warning(
            "unsupported",
            Some("mask"),
            "Fireworks Kontext models do not support explicit masks. Use the prompt to describe \
             the areas to edit.",
        ));
    }
    let mut body = json!({});
    if let Some(text) = &prompt.text {
        body["prompt"] = json!(text);
    }
    if let Some(ratio) = params.aspect_ratio.as_ref().filter(|_| !size_model) {
        body["aspect_ratio"] = json!(ratio);
    }
    if let Some(seed) = params.seed {
        body["seed"] = json!(seed);
    }
    body["samples"] = json!(params.n);
    if let Some(first) = prompt.images.first() {
        body["input_image"] = json!(data_url(first));
    }
    if let Some(value) = params.size.as_ref().filter(|_| size_model) {
        let (width, height) = size(value)?;
        body["width"] = json!(width.to_string());
        body["height"] = json!(height.to_string());
    }
    if let Value::Object(fields) = &mut body {
        fields.extend(options);
    }
    let model_segments = model_path(id)?;
    let path = |first: &'static str, last: Option<&'static str>| {
        let mut path = vec![first];
        path.extend(model_segments.iter().copied());
        path.extend(last);
        path
    };
    let url_path = if size_model {
        path("image_generation", None)
    } else if kontext {
        path("workflows", None)
    } else {
        path("workflows", Some("text_to_image"))
    };
    let config = provider.config();
    let transport = transport(provider)?;
    let post = |path: &[&str], body: Value| {
        http::request(
            api,
            config,
            "POST",
            path,
            Some(body),
            Default::default(),
            Default::default(),
        )
    };
    let (bytes, declared) = if kontext {
        // The sample, usually on a CDN, needs the download transport, and
        // the poll a timer; check both before a job is started and billed,
        // the transport that HST-031 names first.
        let downloads = downloads.ok_or_else(|| {
            invalid(
                "a Fireworks Kontext call needs a download transport, since its result is \
                 usually on another origin; build the provider with with_download_transport or \
                 register it with register_with_downloads",
            )
        })?;
        crate::models::call::require_timer()?;
        let submitted = http::json(
            api,
            transport,
            post(&url_path, body)?,
            provider.name(),
            Some(id),
        )
        .await?;
        let request_id = submitted["request_id"].as_str().ok_or_else(|| {
            http::response_error(
                provider.name(),
                "Fireworks image response has no request_id",
            )
        })?;
        let poll = path("workflows", Some("get_result"));
        let polled = tokio::time::timeout(FIREWORKS_POLL_TIMEOUT, async {
            loop {
                let status = http::json(
                    api,
                    transport,
                    post(&poll, json!({"id": request_id}))?,
                    provider.name(),
                    Some(id),
                )
                .await?;
                match status["status"].as_str() {
                    Some("Ready") => {
                        return status["result"]["sample"]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| {
                                http::response_error(
                                    provider.name(),
                                    "Fireworks poll response is Ready but missing result.sample",
                                )
                            });
                    }
                    Some(failed @ ("Error" | "Failed")) => {
                        return Err(http::response_error(
                            provider.name(),
                            format!("Fireworks image generation failed with status: {failed}"),
                        ));
                    }
                    _ => tokio::time::sleep(FIREWORKS_POLL_INTERVAL).await,
                }
            }
        })
        .await;
        let sample = match polled {
            Ok(sample) => sample?,
            Err(_) => {
                // A retry would start and bill a new job, so it must not
                // retry, as other poll timeouts do not.
                return Err(crate::errors::ProviderError::new(
                    crate::errors::ProviderErrorKind::Timeout,
                    format!(
                        "Fireworks image generation timed out after {} ms",
                        FIREWORKS_POLL_TIMEOUT.as_millis()
                    ),
                )
                .with_provider(provider.name())
                .with_retryable(false)
                .into());
            }
        };
        download_sample(api, downloads, &sample, provider, id).await?
    } else {
        let request = http::request(
            api,
            config,
            "POST",
            &url_path,
            Some(body),
            any_type(),
            Default::default(),
        )?;
        let response = http::send(api, transport, request, provider.name(), Some(id)).await?;
        let (headers, bytes) =
            http::read_body_with_limit(api, provider.name(), response, DEFAULT_MAX_BYTES).await?;
        (bytes.to_vec(), content_type(&headers))
    };
    // The bytes must be an image, by their content or the type the response
    // declares; a JSON body is not one.
    let media_type = media::detect_image_media_type(bytes.as_slice())
        .map(str::to_owned)
        .or(declared.filter(|declared| declared.starts_with("image/")))
        .ok_or_else(|| {
            http::response_error(
                provider.name(),
                "Fireworks image response holds no image it can read",
            )
        })?;
    let file = FilePart::from_bytes(bytes, Some(&media_type), None)
        .map_err(|e| http::response_error(provider.name(), format!("image response: {e}")))?;
    let mut item = Item::new(vec![file]);
    item.warnings = warnings;
    Ok(item)
}

/// The headers of a request whose answer is image bytes: any media type, as
/// TypeScript's fetch asks, not the JSON the other requests ask for.
fn any_type() -> std::collections::BTreeMap<String, String> {
    [("accept".to_owned(), "*/*".to_owned())].into()
}

/// Download a Kontext job's sample: with the key from the base URL's
/// origin, through the download transport without it from another.
async fn download_sample(
    api: &'static ApiProfile,
    downloads: &dyn DownloadTransport,
    sample: &str,
    provider: &dyn Provider,
    model_id: &str,
) -> Result<(Vec<u8>, Option<String>), Error> {
    let config = provider.config();
    let origin = |url: &str| url::Url::parse(url).ok().map(|url| url.origin());
    let sample_origin = origin(sample)
        .filter(|origin| origin.is_tuple())
        .ok_or_else(|| {
            http::response_error(provider.name(), "Fireworks sample is not an HTTP URL")
        })?;
    if Some(sample_origin) == origin(&config.base_url()) {
        let request = http::request_to(
            api,
            sample,
            config,
            "GET",
            &[],
            None,
            any_type(),
            Default::default(),
        )?;
        let response = http::send(
            api,
            transport(provider)?,
            request,
            provider.name(),
            Some(model_id),
        )
        .await?;
        let (headers, bytes) =
            http::read_body_with_limit(api, provider.name(), response, DEFAULT_MAX_BYTES).await?;
        return Ok((bytes.to_vec(), content_type(&headers)));
    }
    let (bytes, media_type) = files::download(downloads, sample, DEFAULT_MAX_BYTES)
        .await
        .map_err(|e| {
            http::response_error(
                provider.name(),
                format!("could not fetch the Fireworks sample: {e}"),
            )
        })?;
    Ok((bytes.to_vec(), media_type))
}

/// A response's `content-type`, without its parameters.
fn content_type(headers: &[(String, String)]) -> Option<String> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
}
