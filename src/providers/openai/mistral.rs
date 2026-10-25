//! Mistral's own request rules beyond the dialect's field names, as
//! `@ai-sdk/mistral` 4.0.69 sends them: the `mistral` options entry of a
//! chat call (HST-042). A value outside an option's schema, or an option
//! name the entry does not take, fails with `Error::Validation` before any
//! request, as the OpenAI and Anthropic option entries do; TypeScript's
//! schema strips an unknown name.

use serde_json::{Map, Number, Value, json};

use super::params::invalid;
use crate::{Error, models::params::InferenceRequestParams};

/// The `mistral` entry of a chat call (`mistralLanguageModelChatOptions`).
#[derive(Debug, Default, PartialEq)]
pub(super) struct Options {
    safe_prompt: Option<bool>,
    document_image_limit: Option<Number>,
    document_page_limit: Option<Number>,
    structured_outputs: Option<bool>,
    strict_json_schema: Option<bool>,
    parallel_tool_calls: Option<bool>,
    prompt_cache_key: Option<String>,
    /// `high` or `none`, in place of the core reasoning setting.
    pub(super) reasoning_effort: Option<&'static str>,
}

fn bad(option: &str, expected: &str, value: &Value) -> Error {
    invalid(format!(
        "mistral option {option} must be {expected}, not {value}"
    ))
}

fn flag(option: &str, value: &Value) -> Result<bool, Error> {
    value
        .as_bool()
        .ok_or_else(|| bad(option, "a boolean", value))
}

fn number(option: &str, value: &Value) -> Result<Number, Error> {
    match value {
        Value::Number(number) => Ok(number.clone()),
        other => Err(bad(option, "a number", other)),
    }
}

impl Options {
    /// The `mistral` entry of the call's provider parameters; none gives the
    /// defaults.
    pub(super) fn parse(p: &InferenceRequestParams) -> Result<Self, Error> {
        let Some(value) = p
            .provider_params
            .as_ref()
            .and_then(|params| params.get("mistral"))
        else {
            return Ok(Self::default());
        };
        let Value::Object(entry) = value else {
            return Err(invalid("mistral provider options must be an object"));
        };
        let mut options = Self::default();
        for (option, value) in entry {
            match option.as_str() {
                "safePrompt" => options.safe_prompt = Some(flag(option, value)?),
                "documentImageLimit" => {
                    options.document_image_limit = Some(number(option, value)?);
                }
                "documentPageLimit" => {
                    options.document_page_limit = Some(number(option, value)?);
                }
                "structuredOutputs" => options.structured_outputs = Some(flag(option, value)?),
                "strictJsonSchema" => options.strict_json_schema = Some(flag(option, value)?),
                "parallelToolCalls" => options.parallel_tool_calls = Some(flag(option, value)?),
                "promptCacheKey" => {
                    let key = value
                        .as_str()
                        .ok_or_else(|| bad(option, "a string", value))?;
                    options.prompt_cache_key = Some(key.to_owned());
                }
                "reasoningEffort" => {
                    options.reasoning_effort = Some(match value.as_str() {
                        Some("high") => "high",
                        Some("none") => "none",
                        _ => return Err(bad(option, "high or none", value)),
                    });
                }
                other => {
                    return Err(invalid(format!("mistral option {other} is not known")));
                }
            }
        }
        Ok(options)
    }

    /// Whether an output schema goes out as a JSON schema, the default, or
    /// as JSON mode with the schema in the instruction.
    fn structured_outputs(&self) -> bool {
        self.structured_outputs.unwrap_or(true)
    }

    /// The `strict` of a JSON schema response format, false when unset.
    fn strict(&self) -> bool {
        self.strict_json_schema.unwrap_or(false)
    }

    /// Put the options into the body over the core settings they replace.
    /// Parallel tool calls go only with tools, which `mistral_tools` settles
    /// once the tools are in.
    pub(super) fn apply(&self, body: &mut Map<String, Value>) {
        if let Some(safe) = self.safe_prompt {
            body.insert("safe_prompt".into(), json!(safe));
        }
        if let Some(limit) = &self.document_image_limit {
            body.insert("document_image_limit".into(), Value::Number(limit.clone()));
        }
        if let Some(limit) = &self.document_page_limit {
            body.insert("document_page_limit".into(), Value::Number(limit.clone()));
        }
        if let Some(key) = &self.prompt_cache_key {
            body.insert("prompt_cache_key".into(), json!(key));
        }
        if let Some(parallel) = self.parallel_tool_calls {
            body.insert("parallel_tool_calls".into(), json!(parallel));
        }
        if let Some(effort) = self.reasoning_effort {
            body.insert("reasoning_effort".into(), json!(effort));
        }
    }

    /// Settle an output schema once the response format and the messages
    /// are in the body. With structured outputs, the schema's `strict` is the
    /// option's. Without them, the schema goes as JSON mode with the
    /// instruction at the end of a first system message, after a blank line,
    /// or in a new first system message (`injectJsonInstructionIntoMessages`).
    pub(super) fn output(
        &self,
        schema: Option<&schemars::Schema>,
        body: &mut Map<String, Value>,
    ) -> Result<(), Error> {
        let Some(schema) = schema else {
            return Ok(());
        };
        if self.structured_outputs() {
            if let Some(format) = body.get_mut("response_format") {
                format["json_schema"]["strict"] = json!(self.strict());
            }
            return Ok(());
        }
        body.insert("response_format".into(), json!({"type": "json_object"}));
        let instruction = format!(
            "JSON schema:\n{}\nYou MUST answer with a JSON object that matches the JSON schema above.",
            serde_json::to_string(schema)?
        );
        let messages = body.entry("messages").or_insert_with(|| json!([]));
        let Some(messages) = messages.as_array_mut() else {
            return Ok(());
        };
        match messages.first_mut() {
            Some(first) if first["role"] == "system" => {
                let text = match first["content"].as_str() {
                    Some("") | None => instruction,
                    Some(content) => format!("{content}\n\n{instruction}"),
                };
                first["content"] = json!(text);
            }
            _ => messages.insert(0, json!({"role": "system", "content": instruction})),
        }
        Ok(())
    }
}
