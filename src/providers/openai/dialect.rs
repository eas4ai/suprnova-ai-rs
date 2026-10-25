//! The request rules of the Chat Completions hosts whose API differs from
//! OpenAI's in more than field names: DeepSeek, Moonshot AI and Mistral, as
//! their AI SDK for TypeScript 7.0.148 packages send them. A setting that the
//! TypeScript package drops with a warning fails with `Error::Validation`
//! here, before any request.
use super::{
    params::invalid,
    profile::{ChatProfile, Dialect, ResponsesDialect, ResponsesProfile, Setting},
};
use crate::{
    Error, StreamOptions,
    errors::{ProviderError, ProviderErrorKind, is_retryable_status},
    models::params::{InferenceRequestParams, ProviderValue, Seed},
    providers::http::{self, ApiProfile},
    types::ToolChoice,
};
use serde_json::{Map, Value, json};

/// The tool choice a call makes: its own, or the one in its parameters.
pub(super) fn tool_choice<'a>(
    p: &'a InferenceRequestParams,
    options: &'a StreamOptions,
) -> Option<&'a ToolChoice> {
    options
        .tool_choice
        .as_ref()
        .or(p.tool_calling.as_ref().map(|t| &t.tool_choice))
}

/// Fail on a setting the host does not take, before any request.
pub(super) fn check(
    profile: &ChatProfile,
    model: &str,
    p: &InferenceRequestParams,
    options: &StreamOptions,
) -> Result<(), Error> {
    check_settings(profile.api.label, profile.unsupported, p, options)?;
    match profile.dialect {
        Dialect::OpenAI | Dialect::VertexMaas | Dialect::VertexXai => Ok(()),
        Dialect::DeepSeek => check_deepseek(model, p),
        Dialect::Moonshot => check_moonshot(model, p, options),
        Dialect::Mistral => check_mistral(model, p),
    }
}

/// Fail on a setting a Responses host does not take, before any request.
pub(super) fn check_responses(
    profile: &ResponsesProfile,
    model: &str,
    p: &InferenceRequestParams,
    options: &StreamOptions,
) -> Result<(), Error> {
    check_settings(profile.api.label, profile.unsupported, p, options)?;
    if profile.dialect == ResponsesDialect::Xai
        && xai_without_reasoning(model)
        && effort(p).is_some()
    {
        return Err(invalid(format!(
            "xAI model {model} does not take reasoning settings"
        )));
    }
    Ok(())
}

/// Change a Responses body built from the parameters to the host's fields.
pub(super) fn responses_request(
    profile: &ResponsesProfile,
    model: &str,
    body: &mut Map<String, Value>,
) {
    match profile.dialect {
        ResponsesDialect::Xai => xai_request(model, body),
        // OpenAI's own rules are in `responses.rs`.
        ResponsesDialect::OpenAI | ResponsesDialect::HuggingFace => {}
    }
}

fn xai_request(model: &str, body: &mut Map<String, Value>) {
    if let Some(reasoning) = body.get_mut("reasoning").and_then(Value::as_object_mut)
        && let Some(effort) = reasoning.get("effort").and_then(Value::as_str)
    {
        let sent = match effort {
            "minimal" => "low",
            "xhigh" | "max" if model == "grok-4.6" => "xhigh",
            "xhigh" | "max" => "high",
            other => other,
        }
        .to_owned();
        reasoning.insert("effort".into(), json!(sent));
    }
    // xAI stores responses unless told not to; an unstored response needs its
    // reasoning back as encrypted content.
    match body.get("store") {
        Some(Value::Bool(true)) => {
            body.remove("store");
        }
        Some(Value::Bool(false)) => super::responses::include(body, "reasoning.encrypted_content"),
        _ => {}
    }
}

/// An xAI model that takes no reasoning settings: `grok-4.20`, an optional
/// `-` and four digits, then `-reasoning` or `-non-reasoning`
/// (`supports-reasoning-effort.ts`).
fn xai_without_reasoning(model: &str) -> bool {
    let Some(rest) = model.strip_prefix("grok-4.20") else {
        return false;
    };
    let rest = match rest.strip_prefix('-') {
        Some(tail) if tail.len() > 4 && tail.as_bytes()[..4].iter().all(u8::is_ascii_digit) => {
            &rest[5..]
        }
        _ => rest,
    };
    matches!(rest, "-reasoning" | "-non-reasoning")
}

pub(super) fn check_settings(
    label: &str,
    unsupported: &[Setting],
    p: &InferenceRequestParams,
    options: &StreamOptions,
) -> Result<(), Error> {
    for setting in unsupported {
        if sets(*setting, p, options) {
            return Err(invalid(format!(
                "{label} does not support {}",
                setting_name(*setting)
            )));
        }
    }
    Ok(())
}

/// The open models on Vertex AI that get an output limit of 8192 tokens
/// when the call sets none.
const VERTEX_MAAS_LIMITS: [&str; 2] = [
    "meta/llama-4-maverick-17b-128e-instruct-maas",
    "meta/llama-4-scout-17b-16e-instruct-maas",
];

/// Change the request body built from the parameters to the host's fields.
pub(super) fn request(
    profile: &ChatProfile,
    model: &str,
    p: &InferenceRequestParams,
    body: &mut Map<String, Value>,
) -> Result<(), Error> {
    match profile.dialect {
        Dialect::OpenAI => {}
        Dialect::VertexMaas => {
            // Llama 4 on Vertex AI needs an output limit
            // (`google-vertex-maas-provider.ts`).
            if VERTEX_MAAS_LIMITS.contains(&model) && !body.contains_key(profile.max_tokens_field) {
                body.insert(profile.max_tokens_field.into(), json!(8192));
            }
        }
        // xAI on Vertex AI takes no reasoning effort
        // (`google-vertex-xai-provider.ts`).
        Dialect::VertexXai => {
            body.remove("reasoning_effort");
        }
        Dialect::DeepSeek => super::deepseek::Options::parse(p)?.apply(p, body),
        Dialect::Moonshot => {
            body.remove("reasoning_effort");
            match (moonshot_family(model), effort(p)) {
                (Family::K3 | Family::Unknown, Some(Effort::Level(level))) => {
                    body.insert(
                        "reasoning_effort".into(),
                        json!(moonshot_effort(level.as_str())),
                    );
                }
                (Family::K26 | Family::K25, Some(effort)) => {
                    let kind = match effort {
                        Effort::Disabled => "disabled",
                        Effort::Level(_) => "enabled",
                    };
                    body.insert("thinking".into(), json!({"type":kind}));
                }
                _ => {}
            }
        }
        Dialect::Mistral => {
            if let Some(seed) = body.remove("seed") {
                body.insert("random_seed".into(), seed);
            }
            // Mistral reasons at one level, whatever the effort's name.
            if let Some(Effort::Level(_)) = effort(p) {
                body.insert("reasoning_effort".into(), json!("high"));
            }
            // The `mistral` entry replaces the core settings it names
            // (HST-042).
            super::mistral::Options::parse(p)?.apply(body);
        }
    }
    Ok(())
}

/// A tool's parameter schema as the host takes it.
pub(super) fn tool_parameters(profile: &ChatProfile, schema: Value) -> Result<Value, Error> {
    match profile.dialect {
        Dialect::Moonshot => mfjs_root(schema, "tool parameters"),
        _ => Ok(schema),
    }
}

/// The `response_format` for an output schema, for the hosts whose format
/// differs from OpenAI's strict JSON schema or JSON mode.
pub(super) fn response_format(
    profile: &ChatProfile,
    name: &str,
    schema: &schemars::Schema,
) -> Result<Option<Value>, Error> {
    Ok(match profile.dialect {
        Dialect::Moonshot => {
            let schema = mfjs_root(serde_json::to_value(schema)?, "an output schema")?;
            Some(
                json!({"type":"json_schema","json_schema":{"name":name,"strict":true,"schema":schema}}),
            )
        }
        Dialect::Mistral => Some(
            json!({"type":"json_schema","json_schema":{"name":name,"strict":false,"schema":schema}}),
        ),
        Dialect::OpenAI | Dialect::DeepSeek | Dialect::VertexMaas | Dialect::VertexXai => None,
    })
}

/// The system message DeepSeek puts first under an output schema, because it
/// has only JSON mode.
pub(super) fn schema_message(
    profile: &ChatProfile,
    schema: &schemars::Schema,
) -> Result<Option<Value>, Error> {
    if profile.dialect != Dialect::DeepSeek {
        return Ok(None);
    }
    let text = format!(
        "Return JSON that conforms to the following schema: {}",
        serde_json::to_string(schema)?
    );
    Ok(Some(json!({"role":"system","content":text})))
}

fn sets(setting: Setting, p: &InferenceRequestParams, options: &StreamOptions) -> bool {
    match setting {
        Setting::ServiceTier => p
            .provider_service
            .as_ref()
            .is_some_and(|s| matches!(s.service_tier, ProviderValue::Value(_))),
        Setting::SafetyIdentifier => p.safety_identifier.is_some(),
        Setting::Metadata => p.metadata.is_some(),
        Setting::CacheKey => p.cache.as_ref().is_some_and(|c| c.key.is_some()),
        Setting::CacheRetention => p
            .cache
            .as_ref()
            .is_some_and(|c| matches!(c.retention, ProviderValue::Value(_))),
        Setting::ParallelToolCalls => p
            .tool_calling
            .as_ref()
            .is_some_and(|t| matches!(t.parallel_tool_calls, ProviderValue::Value(_))),
        Setting::ToolSelection => {
            matches!(tool_choice(p, options), Some(ToolChoice::Selection(_)))
        }
        Setting::TopK => sampler(p, |s| {
            s.top_k
                .as_ref()
                .is_some_and(|t| matches!(t.top_k, ProviderValue::Value(Some(_))))
        }),
        Setting::MinP => sampler(p, |s| {
            s.min_p
                .as_ref()
                .is_some_and(|t| matches!(t.min_p, ProviderValue::Value(Some(_))))
        }),
        Setting::Seed => sets_seed(p),
        Setting::FrequencyPenalty => sampling(p).contains(&"frequency_penalty"),
        Setting::PresencePenalty => sampling(p).contains(&"presence_penalty"),
        Setting::ReasoningSummary => p
            .output
            .as_ref()
            .is_some_and(|o| matches!(o.reasoning_summary, ProviderValue::Value(Some(_)))),
        Setting::TextVerbosity => p
            .output
            .as_ref()
            .is_some_and(|o| matches!(o.text_verbosity, ProviderValue::Value(Some(_)))),
        Setting::OutputInclude => p.output.as_ref().is_some_and(|o| o.include.is_some()),
        Setting::MaxToolCalls => p
            .tool_calling
            .as_ref()
            .is_some_and(|t| matches!(t.max_tool_calls, ProviderValue::Value(Some(_)))),
        Setting::ContextManagement => p
            .context_management
            .as_ref()
            .is_some_and(|m| m.compaction.is_some()),
    }
}

/// Whether the call's sampling settings satisfy `test`.
fn sampler(
    p: &InferenceRequestParams,
    test: impl FnOnce(&crate::models::params::SamplerParams) -> bool,
) -> bool {
    matches!(&p.sampling, ProviderValue::Value(s) if test(s))
}

fn setting_name(setting: Setting) -> &'static str {
    match setting {
        Setting::ServiceTier => "a service tier",
        Setting::SafetyIdentifier => "a safety identifier",
        Setting::Metadata => "metadata",
        Setting::CacheKey => "a cache key",
        Setting::CacheRetention => "a cache retention",
        Setting::ParallelToolCalls => "parallel tool calls",
        Setting::ToolSelection => "a tool selection",
        Setting::TopK => "top_k",
        Setting::MinP => "min_p",
        Setting::Seed => "seed",
        Setting::FrequencyPenalty => "frequency_penalty",
        Setting::PresencePenalty => "presence_penalty",
        Setting::ReasoningSummary => "a reasoning summary",
        Setting::TextVerbosity => "text verbosity",
        Setting::OutputInclude => "an output include list",
        Setting::MaxToolCalls => "max_tool_calls",
        Setting::ContextManagement => "context management",
    }
}

/// The reasoning a call asks for.
pub(super) enum Effort {
    Disabled,
    Level(String),
}
pub(super) fn effort(p: &InferenceRequestParams) -> Option<Effort> {
    let ProviderValue::Value(reasoning) = &p.reasoning else {
        return None;
    };
    let ProviderValue::Value(effort) = &reasoning.effort else {
        return None;
    };
    Some(match effort.as_deref() {
        None | Some("none") => Effort::Disabled,
        Some(level) => Effort::Level(level.to_owned()),
    })
}

/// The sampling settings a call sets, by their names in errors.
fn sampling(p: &InferenceRequestParams) -> Vec<&'static str> {
    let ProviderValue::Value(s) = &p.sampling else {
        return Vec::new();
    };
    let mut set = Vec::new();
    if s.temperature
        .as_ref()
        .is_some_and(|t| matches!(t.temperature, ProviderValue::Value(_)))
    {
        set.push("temperature");
    }
    if s.top_p
        .as_ref()
        .is_some_and(|t| matches!(t.top_p, ProviderValue::Value(Some(_))))
    {
        set.push("top_p");
    }
    if let Some(r) = &s.repetition_penalty {
        if matches!(r.frequency_penalty, ProviderValue::Value(Some(_))) {
            set.push("frequency_penalty");
        }
        if matches!(r.presence_penalty, ProviderValue::Value(Some(_))) {
            set.push("presence_penalty");
        }
    }
    set
}

/// Whether a call sets a seed, as the request builder decides it.
fn sets_seed(p: &InferenceRequestParams) -> bool {
    let ProviderValue::Value(s) = &p.sampling else {
        return false;
    };
    s.seed.as_ref().is_some_and(
        |t| matches!(&t.seed, ProviderValue::Value(Some(Seed::Value(seed))) if *seed != -1),
    )
}

/// A DeepSeek model before V4: `deepseek-chat`, `deepseek-reasoner`, or
/// `deepseek-v1` to `deepseek-v3` with an optional minor version, at the
/// start of the id or after `/` or `.`, followed by the end or by `-`, `/`,
/// `:` or `.` (`is-deepseek-v4-model.ts`). The `.` of a minor version is
/// itself such a follower, so the version digit alone decides.
pub(super) fn deepseek_legacy(model: &str) -> bool {
    let bytes = model.as_bytes();
    model.match_indices("deepseek-").any(|(start, _)| {
        if start > 0 && !matches!(bytes[start - 1], b'/' | b'.') {
            return false;
        }
        let rest = &model[start + "deepseek-".len()..];
        let tail = if let Some(tail) = rest.strip_prefix("chat") {
            tail
        } else if let Some(tail) = rest.strip_prefix("reasoner") {
            tail
        } else if let Some(tail) = rest
            .strip_prefix('v')
            .and_then(|r| r.strip_prefix(['1', '2', '3']))
        {
            tail
        } else {
            return false;
        };
        tail.is_empty() || tail.starts_with(['-', '/', ':', '.'])
    })
}

fn check_deepseek(model: &str, p: &InferenceRequestParams) -> Result<(), Error> {
    let options = super::deepseek::Options::parse(p)?;
    if options.thinking_on(model, p)
        && let Some(name) = sampling(p)
            .into_iter()
            .find(|name| matches!(*name, "temperature" | "top_p"))
    {
        return Err(invalid(format!(
            "DeepSeek ignores {name} while the model reasons; disable reasoning to set {name}"
        )));
    }
    Ok(())
}

/// A Moonshot AI model family (`moonshotai-chat-options.ts`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    K3,
    K27,
    K26,
    K25,
    V1,
    Unknown,
}
fn moonshot_family(model: &str) -> Family {
    match model {
        "kimi-k3" => Family::K3,
        "kimi-k2.7-code" | "kimi-k2.7-code-highspeed" => Family::K27,
        "kimi-k2.6" => Family::K26,
        "kimi-k2.5" => Family::K25,
        _ if model.starts_with("moonshot-v1-") => Family::V1,
        _ => Family::Unknown,
    }
}
fn moonshot_effort(level: &str) -> &str {
    match level {
        "minimal" | "low" => "low",
        "medium" | "high" => "high",
        "xhigh" | "max" => "max",
        other => other,
    }
}

fn check_moonshot(
    model: &str,
    p: &InferenceRequestParams,
    options: &StreamOptions,
) -> Result<(), Error> {
    let family = moonshot_family(model);
    if sets_seed(p) {
        return Err(invalid("Moonshot AI does not support seed"));
    }
    if !matches!(family, Family::V1 | Family::Unknown)
        && let Some(name) = sampling(p).first()
    {
        return Err(invalid(format!(
            "Moonshot AI model {model} has a fixed {name}"
        )));
    }
    if matches!(
        model,
        "kimi-k2.6" | "kimi-k2.7-code" | "kimi-k2.7-code-highspeed"
    ) && matches!(tool_choice(p, options), Some(ToolChoice::Required))
    {
        return Err(invalid(format!(
            "Moonshot AI model {model} does not support required tool choice"
        )));
    }
    match (family, effort(p)) {
        (Family::K3 | Family::K27 | Family::Unknown, Some(Effort::Disabled)) => Err(invalid(
            format!("Moonshot AI model {model} cannot disable reasoning"),
        )),
        (Family::V1, Some(Effort::Level(_))) => Err(invalid(format!(
            "Moonshot AI model {model} does not reason"
        ))),
        _ => Ok(()),
    }
}

/// The Mistral models that take no reasoning settings
/// (`mistral-chat-language-model.ts`).
const MISTRAL_NO_REASONING: [&str; 14] = [
    "codestral-2508",
    "codestral-latest",
    "ministral-14b-2512",
    "ministral-14b-latest",
    "ministral-3b-2512",
    "ministral-3b-latest",
    "ministral-8b-2512",
    "ministral-8b-latest",
    "mistral-code-fim-latest",
    "mistral-code-latest",
    "mistral-large-latest",
    "mistral-large-2512",
    "voxtral-small-2507",
    "voxtral-small-latest",
];

fn check_mistral(model: &str, p: &InferenceRequestParams) -> Result<(), Error> {
    let options = super::mistral::Options::parse(p)?;
    if MISTRAL_NO_REASONING.contains(&model)
        && (effort(p).is_some() || options.reasoning_effort.is_some())
    {
        return Err(invalid(format!(
            "Mistral model {model} does not take reasoning settings"
        )));
    }
    Ok(())
}

/// A root schema normalized for Moonshot's MFJS validator: it must be an
/// object of type `object`, and its `$schema` is removed.
fn mfjs_root(schema: Value, what: &str) -> Result<Value, Error> {
    let Value::Object(mut object) = schema else {
        return Err(mfjs_root_error(what));
    };
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err(mfjs_root_error(what));
    }
    object.remove("$schema");
    Ok(mfjs(Value::Object(object)))
}
fn mfjs_root_error(what: &str) -> Error {
    invalid(format!(
        "Moonshot AI needs {what} that are a JSON schema object of type \"object\""
    ))
}

/// The subset of JSON schema Moonshot's MFJS validator accepts: a tuple
/// `items` array moves to the end of `prefixItems`, and a string `type` next
/// to `anyOf` moves into the branches without one, in every subschema
/// (`normalize-json-schema-for-mfjs.ts`).
fn mfjs(schema: Value) -> Value {
    let Value::Object(mut object) = schema else {
        return schema;
    };
    if let Some(Value::Array(tuple)) = object.get("items").cloned() {
        object.remove("items");
        let mut prefix = match object.remove("prefixItems") {
            Some(Value::Array(prefix)) => prefix,
            _ => Vec::new(),
        };
        prefix.extend(tuple);
        object.insert("prefixItems".into(), prefix.into());
    }
    if let Some(Value::String(kind)) = object.get("type").cloned()
        && let Some(Value::Array(branches)) = object.get_mut("anyOf")
    {
        for branch in branches.iter_mut() {
            if let Value::Object(branch) = branch
                && !branch.contains_key("type")
            {
                branch.insert("type".into(), json!(kind));
            }
        }
        object.remove("type");
    }
    for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(Value::Array(values)) = object.get_mut(key) {
            for value in values.iter_mut() {
                *value = mfjs(value.take());
            }
        }
    }
    // The TypeScript normalizer's keys, and the draft-07 `definitions`,
    // `dependencies` and `additionalItems`, so every subschema is reached; a
    // dependency that lists property names is not a schema and stays.
    for key in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
        "dependencies",
    ] {
        if let Some(Value::Object(map)) = object.get_mut(key) {
            for value in map.values_mut() {
                *value = mfjs(value.take());
            }
        }
    }
    for key in [
        "additionalProperties",
        "additionalItems",
        "propertyNames",
        "items",
        "contains",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(value) = object.get_mut(key)
            && value.is_object()
        {
            *value = mfjs(value.take());
        }
    }
    Value::Object(object)
}

/// The HTTP status of each error `code` or `type` a host can send in its
/// stream (`deepseek-chat-language-model.ts`).
const STREAM_ERROR_STATUS: [(&str, u16); 17] = [
    ("rate_limit_exceeded", 429),
    ("rate_limit_error", 429),
    ("server_error", 500),
    ("api_error", 500),
    ("internal_server_error", 500),
    ("overloaded_error", 503),
    ("service_unavailable", 503),
    ("timeout", 504),
    ("timeout_error", 504),
    ("authentication_error", 401),
    ("invalid_api_key", 401),
    ("permission_error", 403),
    ("not_found_error", 404),
    ("model_not_found", 404),
    ("bad_request", 400),
    ("context_length_exceeded", 400),
    ("invalid_request_error", 400),
];

/// The error an error event in a stream ends the call with. With `kinds`,
/// the kind and the retry flag come from the error's code and type;
/// otherwise the kind is `Response`.
pub(super) fn stream_error(kinds: bool, api: &ApiProfile, provider: &str, data: &Value) -> Error {
    let message = http::error_message(api, data)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{} stream returned an error", api.label));
    if !kinds {
        return http::response_error(provider, message);
    }
    let code = http::error_field(api, data, "code");
    let kind_name = http::error_field(api, data, "type");
    let quota = [&code, &kind_name]
        .iter()
        .any(|v| v.as_deref() == Some("insufficient_quota"));
    let lookup = |name: &Option<String>| {
        name.as_deref().and_then(|name| {
            STREAM_ERROR_STATUS
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, status)| *status)
        })
    };
    // `insufficient_quota` is a rate limit that waiting does not cure.
    let status = if quota {
        Some(429)
    } else {
        code.as_deref()
            .filter(|code| code.len() == 3)
            .and_then(|code| code.parse::<u16>().ok())
            .filter(|status| (400..=599).contains(status))
            .or_else(|| lookup(&code))
            .or_else(|| lookup(&kind_name))
    };
    let mut error = ProviderError::new(
        status.map_or(ProviderErrorKind::Response, ProviderErrorKind::from_status),
        message,
    )
    .with_provider(provider);
    error.is_retryable = !quota && status.is_some() && is_retryable_status(status);
    error.code = code;
    error.error_type = kind_name;
    error.param = http::error_field(api, data, "param");
    error.body = Some(data.clone());
    error.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xai_models_without_reasoning_follow_the_typescript_pattern() {
        for id in [
            "grok-4.20-reasoning",
            "grok-4.20-non-reasoning",
            "grok-4.20-0309-reasoning",
            "grok-4.20-0309-non-reasoning",
        ] {
            assert!(xai_without_reasoning(id), "{id}");
        }
        for id in [
            "grok-4.20",
            "grok-4.20-multi-agent-0309",
            "grok-4.20-03-reasoning",
            "grok-4.20-0309",
            "grok-4.6",
            "grok-4.20-0309-reasoning-x",
        ] {
            assert!(!xai_without_reasoning(id), "{id}");
        }
    }

    #[test]
    fn deepseek_legacy_models_follow_the_typescript_pattern() {
        for id in [
            "deepseek-chat",
            "deepseek-reasoner",
            "deepseek-v3.2",
            "deepseek-v3",
            "provider/deepseek-v2.5",
            "deepseek-v3.2x",
            "deepseek-chat:latest",
            "org.deepseek-v1-beta",
        ] {
            assert!(deepseek_legacy(id), "{id}");
        }
        for id in [
            "deepseek-v4-pro",
            "deepseek-v4-flash-0731",
            "deepseek-flash",
            "deepseek-pro",
            "deepseek-v5-pro",
            "deepseek-v10-flash",
            "deepseek-next",
            "provider/deepseek-v5-pro",
            "deepseek-v30",
            "xdeepseek-chat",
            "deepseek-chatter",
        ] {
            assert!(!deepseek_legacy(id), "{id}");
        }
    }
}
