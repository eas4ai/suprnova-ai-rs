//! Mistral's embeddings (`mistral-embedding-model.ts`, HST-045): `POST
//! <base>/embeddings` with the `mistral` options, 32 values per call and one
//! call at a time. An option outside its schema, or a key the entry does not
//! take, fails with `Error::Validation` before any request; TypeScript's
//! schema strips an unknown key.
use serde_json::{Map, Value, json};

use crate::{
    Error, Model, Provider,
    providers::{
        http::{self, ApiProfile},
        operations::{CallLimits, EmbedParams, Item},
    },
    types::usage::Usage,
};

/// The most values one call takes (`maxEmbeddingsPerCall`).
const MAX_VALUES: usize = 32;

/// 32 values per call, one call at a time (`supportsParallelCalls` is
/// false).
pub(super) const MISTRAL_LIMITS: CallLimits = CallLimits {
    max_per_call: Some(MAX_VALUES),
    parallel: false,
};

/// The `mistral` embedding options (`mistralEmbeddingModelOptions`).
#[derive(Default)]
struct Options {
    metadata: Option<Map<String, Value>>,
    output_dimension: Option<u64>,
    output_dtype: Option<&'static str>,
}

fn invalid(message: String) -> Error {
    Error::Validation(message)
}

impl Options {
    fn parse(params: &EmbedParams) -> Result<Self, Error> {
        let Some(value) = params.provider_options.get("mistral") else {
            return Ok(Self::default());
        };
        let Value::Object(entry) = value else {
            return Err(invalid(
                "mistral embedding options must be an object".into(),
            ));
        };
        let mut options = Self::default();
        for (option, value) in entry {
            match option.as_str() {
                "metadata" => match value {
                    Value::Object(metadata) => options.metadata = Some(metadata.clone()),
                    other => {
                        return Err(invalid(format!(
                            "mistral embedding option metadata must be an object, not {other}"
                        )));
                    }
                },
                "outputDimension" => {
                    let dimension = value.as_u64().filter(|d| *d > 0).ok_or_else(|| {
                        invalid(format!(
                            "mistral embedding option outputDimension must be a positive integer, not {value}"
                        ))
                    })?;
                    options.output_dimension = Some(dimension);
                }
                "outputDtype" => {
                    options.output_dtype = Some(match value.as_str() {
                        Some("float") => "float",
                        Some("int8") => "int8",
                        Some("uint8") => "uint8",
                        Some("binary") => "binary",
                        Some("ubinary") => "ubinary",
                        _ => {
                            return Err(invalid(format!(
                                "mistral embedding option outputDtype must be float, int8, uint8, binary or ubinary, not {value}"
                            )));
                        }
                    });
                }
                other => {
                    return Err(invalid(format!(
                        "mistral embedding option {other} is not known"
                    )));
                }
            }
        }
        Ok(options)
    }
}

/// Embed up to 32 values (`doEmbed`): each `data[].embedding`, with
/// `usage.prompt_tokens` as the input tokens.
pub(super) async fn mistral(
    api: &'static ApiProfile,
    model: &Model,
    values: Vec<String>,
    params: EmbedParams,
    provider: &dyn Provider,
) -> Result<Item<Vec<Vec<f64>>>, Error> {
    if values.len() > MAX_VALUES {
        return Err(invalid(format!(
            "{} takes at most {MAX_VALUES} values per call, not {}",
            model.id,
            values.len()
        )));
    }
    let options = Options::parse(&params)?;
    let mut body = json!({"model": model.id, "input": values, "encoding_format": "float"});
    if let Some(metadata) = options.metadata {
        body["metadata"] = Value::Object(metadata);
    }
    if let Some(dimension) = options.output_dimension {
        body["output_dimension"] = json!(dimension);
    }
    if let Some(dtype) = options.output_dtype {
        body["output_dtype"] = json!(dtype);
    }
    let request = http::request(
        api,
        provider.config(),
        "POST",
        &["embeddings"],
        Some(body),
        Default::default(),
        Default::default(),
    )?;
    let transport = provider.transport().ok_or_else(|| {
        Error::Configuration(format!(
            "provider {:?} has no transport for embeddings",
            provider.name()
        ))
    })?;
    let response = http::json(api, transport, request, provider.name(), Some(&model.id)).await?;
    let bad = || {
        http::response_error(
            provider.name(),
            "Mistral embedding response has no embeddings",
        )
    };
    let embeddings = response["data"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|item| {
            item["embedding"]
                .as_array()
                .and_then(|values| {
                    values
                        .iter()
                        .map(Value::as_f64)
                        .collect::<Option<Vec<f64>>>()
                })
                .ok_or_else(bad)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut item = Item::new(embeddings);
    if let Some(tokens) = response["usage"]["prompt_tokens"].as_u64() {
        item.usage = Some(Usage {
            input_tokens: tokens,
            raw: Some(response["usage"].clone()),
            ..Default::default()
        });
    }
    Ok(item)
}
