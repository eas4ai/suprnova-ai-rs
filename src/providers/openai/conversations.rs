//! Mistral's Conversations API (`mistral-conversation-language-model.ts`):
//! a second language protocol of the Mistral provider, which a model takes
//! through `Model::with_protocol` (HST-043, HST-044). A call sends `POST
//! <base>/conversations` with `store: false`: the system messages as
//! `instructions`, the history as `inputs`, the function and web search
//! tools, and the chat body's fields as `completion_args`. The stream's text,
//! reasoning, function calls and web search executions become the crate's
//! events; the pages a search cites go as `sources` in the final message's
//! `mistral` metadata. A setting the TypeScript model drops with a warning
//! fails with `Error::Validation` before any request.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use futures::{StreamExt, TryStreamExt, stream, stream::BoxStream};
use serde_json::{Map, Value, json};

use super::{
    dialect::{self, Effort},
    messages, mistral,
    params::{self, invalid},
    profile::{ChatProfile, Setting},
    protocol::{Wire, http_request},
    tools,
};
use crate::{
    Error, Model, Provider, ProviderConfig, ProviderProtocol, StreamOptions,
    errors::{ProviderError, ProviderErrorKind},
    providers::http::{self, ApiProfile, SseJson},
    transport::{Body, HttpRequest},
    types::{
        Message, MessageRole, Part, Tool, ToolChoice,
        events::*,
        messages::{
            BuiltinToolCallPart, BuiltinToolReturnPart, Metadata, ToolCallPart, ToolResultPart,
            ToolResultValue, generate_id,
        },
        tools::{ToolConfig, ToolKind},
        usage::Usage,
    },
};

/// The protocol's class id in snapshots.
pub(crate) const CLASS_ID: &str = "mistral_conversations";
/// The ids of the API's provider-executed tools (`mistralProviderToolNames`).
pub(crate) const WEB_SEARCH: &str = "mistral.web_search";
pub(crate) const WEB_SEARCH_PREMIUM: &str = "mistral.web_search_premium";
const LABEL: &str = "Mistral's Conversations API";

/// The models the API reasons on (`reasoningEffortModelIds`); a reasoning
/// setting for another model fails.
const REASONING_MODELS: [&str; 19] = [
    "glm-5-2",
    "labs-leanstral-1-5",
    "labs-leanstral-1-5-1",
    "magistral-medium-latest",
    "magistral-small-latest",
    "mistral-large-4",
    "mistral-large-4-0",
    "mistral-medium",
    "mistral-medium-2604",
    "mistral-medium-3",
    "mistral-medium-3-5",
    "mistral-medium-3.5",
    "mistral-medium-latest",
    "mistral-small-2603",
    "mistral-small-latest",
    "mistral-vibe-cli-fast",
    "mistral-vibe-cli-latest",
    "mistral-vibe-cli-with-tools",
    "zai-glm-5-2",
];

/// The chat body fields the API takes in `completion_args` (`baseArgs`).
const COMPLETION_KEYS: [&str; 8] = [
    "max_tokens",
    "temperature",
    "top_p",
    "frequency_penalty",
    "presence_penalty",
    "random_seed",
    "reasoning_effort",
    "response_format",
];

/// The Conversations protocol of the Mistral host whose chat profile this
/// is: the profile gives the API's credentials, error rules and the chat
/// fields of `completion_args`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConversationsProtocol(pub(crate) &'static ChatProfile);

impl ProviderProtocol for ConversationsProtocol {
    fn protocol_class_id(&self) -> &str {
        CLASS_ID
    }

    fn stream(
        &self,
        model: &Model,
        messages: Vec<Message>,
        options: StreamOptions,
        provider: &dyn Provider,
    ) -> BoxStream<'static, Result<Event, Error>> {
        let profile = self.0;
        let config = provider.config().clone();
        let provider_name = provider.name().to_owned();
        let model_id = model.id.clone();
        let transport = provider.shared_transport();
        stream::once(async move {
            let request = request(profile, &model_id, &config, &messages, &options)?;
            let transport = transport.ok_or_else(|| {
                Error::Configuration(format!("{LABEL} requires Provider::shared_transport"))
            })?;
            let response = http::send(
                &profile.api,
                transport.as_ref(),
                request,
                &provider_name,
                Some(&model_id),
            )
            .await?;
            Ok::<_, Error>(read(profile, provider_name, response.body, &options.tools))
        })
        .try_flatten()
        .boxed()
    }
}

/// A web search tool of the Conversations API, named after its wire name.
pub(crate) fn web_search_tool(id: &'static str) -> Tool {
    let name = id.strip_prefix("mistral.").unwrap_or(id);
    Tool::provider(
        name,
        ToolConfig {
            id: Some(id.to_owned()),
            args: Map::new(),
        },
    )
    .expect("a provider tool with an id is well formed")
}

/// The wire name of a web search tool, or `None` for another provider tool.
fn wire_name(tool: &Tool) -> Option<&'static str> {
    match tool.tool_config.as_ref()?.id.as_deref()? {
        WEB_SEARCH => Some("web_search"),
        WEB_SEARCH_PREMIUM => Some("web_search_premium"),
        _ => None,
    }
}

/// The name of a call's tool on the wire: a web search tool's wire name,
/// else the name itself (`toProviderToolName`).
fn provider_name<'a>(tools: &'a [Tool], name: &'a str) -> &'a str {
    tools
        .iter()
        .find(|tool| tool.name == name)
        .and_then(wire_name)
        .unwrap_or(name)
}

/// The request of a call (`getArgs`).
fn request(
    profile: &'static ChatProfile,
    model: &str,
    config: &ProviderConfig,
    messages: &[Message],
    options: &StreamOptions,
) -> Result<HttpRequest, Error> {
    let p = options.params.clone().unwrap_or_default();
    // The settings the chat refuses (HST-008), and the cache key and
    // parallel tool calls only the chat sends.
    dialect::check_settings(profile.api.label, profile.unsupported, &p, options)?;
    dialect::check_settings(
        LABEL,
        &[Setting::CacheKey, Setting::ParallelToolCalls],
        &p,
        options,
    )?;
    let mistral = mistral::Options::parse(&p)?;
    if let Some(option) = mistral.conversations_unsupported() {
        return Err(invalid(format!(
            "{LABEL} does not take the mistral option {option}"
        )));
    }
    // The entry's effort, else the core effort at Mistral's one level.
    let effort = mistral.reasoning_effort.or_else(|| {
        dialect::effort(&p).map(|effort| match effort {
            Effort::Disabled => "none",
            Effort::Level(_) => "high",
        })
    });
    if effort.is_some() && !REASONING_MODELS.contains(&model) {
        return Err(invalid(format!(
            "{LABEL} does not reason on model {model}, which takes no reasoning settings"
        )));
    }
    let (sent_tools, tool_choice) = convert_tools(
        profile,
        model,
        &options.tools,
        dialect::tool_choice(&p, options),
    )?;
    let mut chat = params::build(&p, Wire::Chat(profile))?;
    dialect::request(profile, model, &p, &mut chat)?;
    let mut args = Map::new();
    for key in COMPLETION_KEYS {
        if let Some(value) = chat.remove(key) {
            args.insert(key.to_owned(), value);
        }
    }
    if let Some(effort) = effort {
        args.insert("reasoning_effort".into(), json!(effort));
    }
    let mut instruction = None;
    if let Some(schema) = &options.output_schema {
        let name = options.output_type_name.as_deref().unwrap_or("output");
        if mistral.structured_outputs() {
            args.insert(
                "response_format".into(),
                json!({"type":"json_schema","json_schema":{"name":name,"strict":mistral.strict(),"schema":schema}}),
            );
        } else {
            args.insert("response_format".into(), json!({"type": "json_object"}));
            instruction = Some(mistral::instruction(schema)?);
        }
    }
    if let Some(choice) = tool_choice {
        args.insert("tool_choice".into(), json!(choice));
    }
    let (instructions, inputs) = convert_messages(profile, &options.tools, messages, instruction)?;
    let mut body = Map::new();
    body.insert("model".into(), json!(model));
    body.insert("store".into(), json!(false));
    body.insert("stream".into(), json!(true));
    if let Some(instructions) = instructions {
        body.insert("instructions".into(), json!(instructions));
    }
    body.insert("inputs".into(), Value::Array(inputs));
    if let Some(tools) = sent_tools {
        body.insert("tools".into(), Value::Array(tools));
    }
    body.insert("completion_args".into(), Value::Object(args));
    // The escape hatch wins at the wire boundary, as it does for the chat.
    if let Some(extra) = &p.extra_body {
        body.extend(extra.clone());
    }
    http_request(
        &profile.api,
        "POST",
        config,
        &["conversations"],
        Some(Value::Object(body)),
        Some(&p),
    )
}

/// The tools and tool choice of a call (`prepareConversationTools`): each
/// function tool as the chat sends it, each web search tool by its type, a
/// named choice keeping only the named tool under `any`.
fn convert_tools(
    profile: &'static ChatProfile,
    model: &str,
    tools: &[Tool],
    choice: Option<&ToolChoice>,
) -> Result<(Option<Vec<Value>>, Option<&'static str>), Error> {
    if tools.is_empty() {
        return Ok((None, None));
    }
    let mut sent = Vec::with_capacity(tools.len());
    for tool in tools {
        match tool.kind {
            ToolKind::Function => sent.extend(tools::convert(
                std::slice::from_ref(tool),
                Wire::Chat(profile),
                model,
            )?),
            ToolKind::Provider => match wire_name(tool) {
                Some(name) => sent.push(json!({"type": name})),
                None => {
                    let id = tool
                        .tool_config
                        .as_ref()
                        .and_then(|config| config.id.as_deref())
                        .unwrap_or(&tool.name);
                    return Err(invalid(format!(
                        "{LABEL} does not support the provider tool {id}"
                    )));
                }
            },
        }
    }
    let choice = match choice {
        None => None,
        Some(ToolChoice::Auto) => Some("auto"),
        Some(ToolChoice::None) => Some("none"),
        Some(ToolChoice::Required) => Some("any"),
        Some(ToolChoice::Tool(tool)) => {
            let name = provider_name(tools, &tool.0);
            sent.retain(|sent| {
                if sent["type"] == "function" {
                    sent["function"]["name"] == name
                } else {
                    sent["type"] == name
                }
            });
            if sent.is_empty() {
                return Err(invalid(format!(
                    "Mistral tool choice names the tool {tool}, which the call does not declare"
                )));
            }
            Some("any")
        }
        Some(ToolChoice::Selection(_)) => {
            return Err(invalid(format!(
                "{LABEL} does not support a tool selection"
            )));
        }
    };
    Ok((Some(sent), choice))
}

/// The instructions and inputs of a history
/// (`convertToMistralConversationInputs`), with JSON mode's instruction at
/// the end of a first system message or in a new one
/// (`injectJsonInstructionIntoMessages`).
fn convert_messages(
    profile: &'static ChatProfile,
    tools: &[Tool],
    messages: &[Message],
    instruction: Option<String>,
) -> Result<(Option<String>, Vec<Value>), Error> {
    let mut system: Vec<String> = Vec::new();
    let mut inputs = Vec::new();
    let mut executions = BTreeSet::new();
    for message in messages {
        match message.role {
            MessageRole::System => system.push(message.text()),
            MessageRole::User => inputs.push(json!({
                "type": "message.input",
                "role": "user",
                "content": user_content(message)?,
            })),
            MessageRole::Assistant => {
                let mut content = Vec::new();
                for part in &message.parts {
                    match part {
                        Part::Text(text) if !text.text.is_empty() => {
                            content.push(json!({"type": "text", "text": text.text}));
                        }
                        Part::Reasoning(reasoning) if !reasoning.text.is_empty() => {
                            content.push(json!({
                                "type": "thinking",
                                "thinking": [{"type": "text", "text": reasoning.text}],
                                "closed": true,
                            }));
                        }
                        Part::ToolCall(call) => {
                            flush(&mut content, &mut inputs);
                            inputs.push(json!({
                                "type": "function.call",
                                "tool_call_id": call.tool_call_id,
                                "name": call.tool_name,
                                "arguments": call.tool_args,
                            }));
                        }
                        Part::BuiltinToolCall(call) => {
                            flush(&mut content, &mut inputs);
                            inputs.push(builtin_call(tools, call));
                            executions.insert(call.tool_call_id.clone());
                        }
                        Part::BuiltinToolReturn(ret) => {
                            flush(&mut content, &mut inputs);
                            inputs.push(json!({
                                "type": "function.result",
                                "tool_call_id": ret.tool_call_id,
                                "result": builtin_result(&ret.result),
                            }));
                        }
                        Part::ToolResult(result) => {
                            flush(&mut content, &mut inputs);
                            inputs.push(tool_result(profile, result, &executions)?);
                        }
                        Part::File(_) => {
                            return Err(invalid(format!(
                                "{LABEL} does not take files in an assistant message"
                            )));
                        }
                        Part::Text(_) | Part::Reasoning(_) | Part::Hook(_) => {}
                    }
                }
                flush(&mut content, &mut inputs);
            }
            MessageRole::Tool => {
                for part in &message.parts {
                    if let Part::ToolResult(result) = part {
                        inputs.push(tool_result(profile, result, &executions)?);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(instruction) = instruction {
        if messages
            .first()
            .is_some_and(|message| message.role == MessageRole::System)
        {
            let first = &mut system[0];
            if first.is_empty() {
                *first = instruction;
            } else {
                first.push_str("\n\n");
                first.push_str(&instruction);
            }
        } else {
            system.insert(0, instruction);
        }
    }
    let instructions = (!system.is_empty()).then(|| system.join("\n\n"));
    Ok((instructions, inputs))
}

/// Send the assistant content gathered so far as one input, cut before a
/// tool call or result.
fn flush(content: &mut Vec<Value>, inputs: &mut Vec<Value>) {
    if !content.is_empty() {
        inputs.push(json!({
            "type": "message.input",
            "role": "assistant",
            "content": std::mem::take(content),
        }));
    }
}

/// A user message's parts as the chat sends them (HST-015).
fn user_content(message: &Message) -> Result<Vec<Value>, Error> {
    let mut content = Vec::new();
    for part in &message.parts {
        match part {
            Part::Text(text) => content.push(json!({"type": "text", "text": text.text})),
            Part::File(file) => {
                let media_type = if file.media_type == "image/*" {
                    "image/jpeg"
                } else {
                    &file.media_type
                };
                content.push(messages::mistral_file(file, media_type)?);
            }
            _ => {}
        }
    }
    Ok(content)
}

/// A built-in call replayed as a function call, since entry ids cannot be
/// sent as inputs: named by its `mistral` metadata's `function`, else `name`,
/// else its tool's wire name; its arguments the `arguments` text of its
/// input, else the input as given.
fn builtin_call(tools: &[Tool], call: &BuiltinToolCallPart) -> Value {
    let metadata = call
        .provider_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("mistral"));
    let name = ["function", "name"]
        .iter()
        .find_map(|key| metadata?.get(*key)?.as_str())
        .map_or_else(
            || provider_name(tools, &call.tool_name).to_owned(),
            str::to_owned,
        );
    let arguments = serde_json::from_str::<Value>(&call.tool_args)
        .ok()
        .and_then(|input| input.get("arguments")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| call.tool_args.clone());
    json!({
        "type": "function.call",
        "tool_call_id": call.tool_call_id,
        "name": name,
        "arguments": arguments,
    })
}

/// The replayed result of a built-in call: `info.result` when a string, else
/// the JSON of `info.result`, of `info`, or `null`.
fn builtin_result(value: &Value) -> String {
    match value.get("info") {
        Some(Value::Object(info)) => match info.get("result") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Null) | None => Value::Object(info.clone()).to_string(),
            Some(result) => result.to_string(),
        },
        _ => "null".to_owned(),
    }
}

/// A tool result as a `function.result`: a built-in execution's by the rule
/// above, a function's as the chat's string content (HST-015).
fn tool_result(
    profile: &'static ChatProfile,
    result: &ToolResultPart,
    executions: &BTreeSet<String>,
) -> Result<Value, Error> {
    let text = if executions.contains(&result.tool_call_id) {
        match result.get_model_input() {
            ToolResultValue::Json(value) => builtin_result(value),
            ToolResultValue::Special(_) => "null".to_owned(),
        }
    } else {
        match messages::tool_result(result.get_model_input(), Wire::Chat(profile))? {
            Value::String(text) => text,
            other => other.to_string(),
        }
    };
    Ok(json!({
        "type": "function.result",
        "tool_call_id": result.tool_call_id,
        "result": text,
    }))
}

/// Read a response body as the crate's events.
fn read(
    profile: &'static ChatProfile,
    provider: String,
    body: Body,
    tools: &[Tool],
) -> BoxStream<'static, Result<Event, Error>> {
    struct State {
        source: BoxStream<'static, Result<SseJson, Error>>,
        reader: Reader,
        done: bool,
    }
    let state = State {
        source: http::sse_json(profile.api, provider.clone(), body),
        reader: Reader::new(&profile.api, provider, tools),
        done: false,
    };
    stream::try_unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.reader.events.pop_front() {
                return Ok(Some((event, state)));
            }
            if state.done {
                return Ok(None);
            }
            match state.source.next().await {
                Some(Ok(SseJson::Object(value))) => state.reader.event(&value)?,
                Some(Ok(SseJson::Done)) => {}
                Some(Err(error)) => return Err(error),
                None => {
                    state.reader.end();
                    state.done = true;
                }
            }
        }
    })
    .boxed()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Text,
    Reasoning,
}

/// A built-in execution that has started and not ended.
struct Execution {
    name: String,
    arguments: String,
    function: Option<String>,
}

/// Reads the stream's events (`doStream`).
struct Reader {
    api: &'static ApiProfile,
    provider: String,
    /// The call's web search tools: their wire names and the call's names.
    names: Vec<(&'static str, String)>,
    events: VecDeque<Event>,
    /// The open text or reasoning block: its id and kind.
    open: Option<(String, Kind)>,
    /// The function calls read, in order: id, name and arguments.
    calls: Vec<(String, String, String)>,
    executions: BTreeMap<String, Execution>,
    sources: Vec<Value>,
    seen: BTreeSet<String>,
    response_id: Option<String>,
    model: Option<String>,
    usage: Option<Value>,
    done: bool,
    has_function_call: bool,
    /// Whether an event or a content part had a shape the reader does not
    /// know; TypeScript's closed schemas make such a chunk an error.
    malformed: bool,
}

impl Reader {
    fn new(api: &'static ApiProfile, provider: String, tools: &[Tool]) -> Self {
        Self {
            api,
            provider,
            names: tools
                .iter()
                .filter_map(|tool| wire_name(tool).map(|wire| (wire, tool.name.clone())))
                .collect(),
            events: VecDeque::from([Event::StreamStart(StreamStart::default())]),
            open: None,
            calls: Vec::new(),
            executions: BTreeMap::new(),
            sources: Vec::new(),
            seen: BTreeSet::new(),
            response_id: None,
            model: None,
            usage: None,
            done: false,
            has_function_call: false,
            malformed: false,
        }
    }

    /// The call's name of a tool the API names (`toCustomToolName`).
    fn custom_name(&self, wire: &str) -> String {
        self.names
            .iter()
            .find(|(known, _)| *known == wire)
            .map_or_else(|| wire.to_owned(), |(_, name)| name.clone())
    }

    /// End the open block, if any.
    fn close(&mut self) {
        if let Some((block_id, kind)) = self.open.take() {
            self.events.push_back(match kind {
                Kind::Text => Event::TextEnd(TextEnd {
                    event: ModelEventMeta::default(),
                    block_id,
                }),
                Kind::Reasoning => Event::ReasoningEnd(ReasoningEnd {
                    event: ModelEventMeta::default(),
                    block_id,
                }),
            });
        }
    }

    /// Deliver text or reasoning in the block of this key, opening it after
    /// ending another.
    fn content(&mut self, key: &str, kind: Kind, text: &str) {
        let block_id = format!(
            "{key}-{}",
            match kind {
                Kind::Text => "text",
                Kind::Reasoning => "reasoning",
            }
        );
        if self
            .open
            .as_ref()
            .is_none_or(|(open, open_kind)| *open != block_id || *open_kind != kind)
        {
            self.close();
            self.events.push_back(match kind {
                Kind::Text => Event::TextStart(TextStart {
                    event: ModelEventMeta::default(),
                    block_id: block_id.clone(),
                }),
                Kind::Reasoning => Event::ReasoningStart(ReasoningStart {
                    event: ModelEventMeta::default(),
                    block_id: block_id.clone(),
                }),
            });
            self.open = Some((block_id.clone(), kind));
        }
        self.events.push_back(match kind {
            Kind::Text => Event::TextDelta(TextDelta {
                event: ModelEventMeta::default(),
                chunk: text.to_owned(),
                block_id,
            }),
            Kind::Reasoning => Event::ReasoningDelta(ReasoningDelta {
                event: ModelEventMeta::default(),
                chunk: text.to_owned(),
                block_id,
            }),
        });
    }

    /// Read one event.
    fn event(&mut self, value: &Value) -> Result<(), Error> {
        match value["type"].as_str().unwrap_or("") {
            "conversation.response.started" => {
                if let Some(id) = value["conversation_id"].as_str() {
                    self.response_id = Some(id.to_owned());
                }
            }
            "conversation.response.done" => {
                self.usage = value
                    .get("usage")
                    .filter(|usage| usage.is_object())
                    .cloned();
                self.done = true;
            }
            "conversation.response.error" => {
                return Err(dialect::stream_error(true, self.api, &self.provider, value));
            }
            "message.output.delta" => self.output_delta(value),
            "function.call.delta" => self.function_call(value),
            "tool.execution.started" | "tool.execution.delta" => self.execution(value),
            "tool.execution.done" => self.execution_done(value)?,
            _ => self.malformed = true,
        }
        Ok(())
    }

    fn output_delta(&mut self, value: &Value) {
        if self.model.is_none()
            && let Some(model) = value["model"].as_str()
        {
            self.model = Some(model.to_owned());
        }
        let key = format!(
            "{}-{}",
            value["id"].as_str().unwrap_or(""),
            value["content_index"].as_u64().unwrap_or(0)
        );
        match &value["content"] {
            Value::String(text) => {
                if !text.is_empty() {
                    self.content(&key, Kind::Text, text);
                }
            }
            Value::Object(part) => match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = part["text"].as_str().filter(|text| !text.is_empty()) {
                        self.content(&key, Kind::Text, text);
                    }
                }
                Some("thinking") => {
                    let text: String = part["thinking"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|chunk| chunk["text"].as_str())
                        .collect();
                    if !text.is_empty() {
                        self.content(&key, Kind::Reasoning, &text);
                    }
                }
                Some("tool_reference") => {
                    if let Some(url) = part["url"].as_str()
                        && self.seen.insert(url.to_owned())
                    {
                        self.sources.push(json!({
                            "sourceType": "url",
                            "id": generate_id("source"),
                            "url": url,
                            "title": part["title"].as_str().unwrap_or(url),
                        }));
                    }
                }
                _ => self.malformed = true,
            },
            _ => self.malformed = true,
        }
    }

    fn function_call(&mut self, value: &Value) {
        self.close();
        self.has_function_call = true;
        let id = value["tool_call_id"].as_str().unwrap_or("").to_owned();
        let name = value["name"].as_str().unwrap_or("").to_owned();
        let arguments = value["arguments"].as_str().unwrap_or("").to_owned();
        match self.calls.iter_mut().find(|(call_id, ..)| *call_id == id) {
            Some((_, call_name, call_arguments)) => {
                if call_name.is_empty() {
                    *call_name = name;
                }
                call_arguments.push_str(&arguments);
            }
            None => {
                self.events.push_back(Event::ToolStart(ToolStart {
                    event: ModelEventMeta::default(),
                    tool_call_id: id.clone(),
                    tool_name: name.clone(),
                }));
                self.calls.push((id.clone(), name, arguments.clone()));
            }
        }
        self.events.push_back(Event::ToolDelta(ToolDelta {
            event: ModelEventMeta::default(),
            chunk: arguments,
            tool_call_id: id,
        }));
    }

    fn execution(&mut self, value: &Value) {
        self.close();
        let id = value["id"].as_str().unwrap_or("").to_owned();
        let arguments = value["arguments"].as_str().unwrap_or("").to_owned();
        match self.executions.get_mut(&id) {
            Some(execution) => execution.arguments.push_str(&arguments),
            None => {
                let name = value["name"].as_str().unwrap_or("").to_owned();
                let function = value["function"].as_str().map(str::to_owned);
                self.events
                    .push_back(Event::BuiltinToolStart(BuiltinToolStart {
                        event: ModelEventMeta::default(),
                        tool_call_id: id.clone(),
                        tool_name: self.custom_name(&name),
                    }));
                self.executions.insert(
                    id,
                    Execution {
                        name,
                        arguments,
                        function,
                    },
                );
            }
        }
    }

    /// End an execution: the call with its arguments and `mistral` metadata,
    /// then its return with the event's `info`.
    fn execution_done(&mut self, value: &Value) -> Result<(), Error> {
        let id = value["id"].as_str().unwrap_or("").to_owned();
        let Some(execution) = self.executions.remove(&id) else {
            return Err(ProviderError::new(
                ProviderErrorKind::Response,
                format!("Mistral tool execution {id} ended without a start"),
            )
            .with_provider(&self.provider)
            .into());
        };
        let function = value["function"]
            .as_str()
            .map(str::to_owned)
            .or(execution.function);
        let name = self.custom_name(&execution.name);
        let arguments = json!({"arguments": execution.arguments}).to_string();
        let mut mistral = json!({"type": "tool.execution", "name": execution.name});
        if let Some(function) = &function {
            mistral["function"] = json!(function);
        }
        let metadata: Metadata = [("mistral".to_owned(), mistral)].into();
        self.events
            .push_back(Event::BuiltinToolDelta(BuiltinToolDelta {
                event: ModelEventMeta::default(),
                chunk: arguments.clone(),
                tool_call_id: id.clone(),
            }));
        self.events.push_back(Event::BuiltinToolEnd(BuiltinToolEnd {
            event: ModelEventMeta {
                provider_metadata: Some(metadata.clone()),
                ..Default::default()
            },
            tool_call: BuiltinToolCallPart {
                id: generate_id("part"),
                tool_call_id: id.clone(),
                tool_name: name.clone(),
                tool_args: arguments,
                provider_metadata: Some(metadata),
            },
            tool_call_id: id.clone(),
        }));
        let result = match value.get("info").filter(|info| !info.is_null()) {
            Some(info) => json!({"info": info}),
            None => json!({}),
        };
        self.events
            .push_back(Event::BuiltinToolResult(BuiltinToolResult {
                event: ModelEventMeta::default(),
                result: BuiltinToolReturnPart {
                    id: generate_id("part"),
                    tool_call_id: id.clone(),
                    tool_name: name,
                    result,
                    is_error: false,
                    provider_metadata: None,
                },
                tool_call_id: id,
            }));
        Ok(())
    }

    /// End the stream: the open block, the function calls, then the finish
    /// with the usage and the sources.
    fn end(&mut self) {
        self.close();
        for (id, name, arguments) in std::mem::take(&mut self.calls) {
            self.events.push_back(Event::ToolEnd(ToolEnd {
                event: ModelEventMeta::default(),
                tool_call: ToolCallPart::new(id.clone(), name, arguments),
                tool_call_id: id,
            }));
        }
        let finish = if self.malformed || !self.done || !self.executions.is_empty() {
            "error"
        } else if self.has_function_call {
            "tool_call"
        } else {
            "stop"
        };
        let usage = self.usage.as_ref().map(|value| Usage {
            input_tokens: value["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: value["completion_tokens"].as_u64().unwrap_or(0),
            reasoning_tokens: None,
            cache_read_tokens: value["num_cached_tokens"]
                .as_u64()
                .or_else(|| value["prompt_tokens_details"]["cached_tokens"].as_u64())
                .or_else(|| value["prompt_token_details"]["cached_tokens"].as_u64()),
            cache_write_tokens: None,
            raw: Some(value.clone()),
        });
        let provider_metadata = (!self.sources.is_empty())
            .then(|| [("mistral".to_owned(), json!({"sources": self.sources}))].into());
        self.events.push_back(Event::StreamEnd(StreamEnd {
            event: ModelEventMeta {
                usage,
                provider_metadata,
                ..Default::default()
            },
            finish_reason: Some(finish.into()),
            response_id: self.response_id.clone(),
            response_model: self.model.clone(),
        }));
    }
}
