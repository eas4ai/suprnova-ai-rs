use super::{
    chat, dialect, embedding, images, messages, params,
    profile::{self, ChatProfile, Dialect, ResponsesDialect, ResponsesProfile},
    realtime, responses, socket, speech, streaming, transcription,
};
use crate::{
    Error, Model, Provider, ProviderConfig, ProviderProtocol, StreamOptions,
    models::params::{ExtraHeaderValue, InferenceRequestParams},
    providers::{
        EventStream, http,
        operations::{
            AudioChunks, AudioFormat, AudioParams, AudioPrompt, CallLimits, EmbedParams,
            EvaluationInput, EvaluationParams, ImageParams, ImagePrompt, Item, Question,
            RealtimeCodec, RealtimeSessionConfig, RealtimeStartup, RealtimeToken,
            RealtimeTokenParams, StreamTranscribeParams, StreamTranslateParams, TranscribeInput,
            TranscribeParams, Transcription, TranscriptionStream, TranslationStream, WebRtcSession,
        },
    },
    transport::{HttpRequest, WebSocketRequest},
    types::{
        Message, ToolChoice,
        messages::{FilePart, Metadata},
    },
};
use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt, stream};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

/// The `ProviderProtocol` of one of OpenAI's language model protocols: its
/// class id, its stream on `wire` with the downloads `|this|` gives, and the
/// operations they share: embeddings (OAI-016), images (OAI-017), speech
/// (OAI-018), transcription (OAI-019), streaming audio (OAI-020),
/// realtime and Live sessions (OAI-024) and evaluation on the language model
/// (OAI-025).
macro_rules! openai_protocol {
    ($ty:ty, $class:expr, $wire:expr, |$this:ident| $downloads:expr) => {
        #[async_trait]
        impl ProviderProtocol for $ty {
            fn protocol_class_id(&self) -> &str {
                $class
            }
            fn stream(
                &self,
                model: &Model,
                messages: Vec<Message>,
                options: StreamOptions,
                provider: &dyn Provider,
            ) -> EventStream {
                let $this = self;
                start(model, messages, options, provider, $wire, $downloads)
            }
            async fn evaluate(
                &self,
                model: &Model,
                state: EvaluationInput,
                questions: BTreeMap<String, Question>,
                params: EvaluationParams,
                provider: &dyn Provider,
            ) -> Result<Item<Metadata>, Error> {
                let $this = self;
                let downloads = $downloads;
                let name = provider.name().to_owned();
                crate::ops::experimental::evaluate_with_language_model(
                    provider.name(),
                    state,
                    questions,
                    params,
                    |messages, options| async move {
                        let events: Vec<_> =
                            start(model, messages, options, provider, $wire, downloads)
                                .try_collect()
                                .await?;
                        crate::providers::google::language::hydrate(&name, events)
                    },
                )
                .await
            }
            async fn embed(
                &self,
                model: &Model,
                values: Vec<String>,
                params: EmbedParams,
                provider: &dyn Provider,
            ) -> Result<Item<Vec<Vec<f64>>>, Error> {
                embedding::embed(
                    embedding::OpenAIEmbeddingProtocol::new(),
                    model,
                    values,
                    params,
                    provider,
                )
                .await
            }
            fn embed_limits(&self, _model: &Model) -> CallLimits {
                embedding::openai_limits()
            }
            async fn generate_image(
                &self,
                model: &Model,
                prompt: ImagePrompt,
                params: ImageParams,
                provider: &dyn Provider,
            ) -> Result<Item<Vec<FilePart>>, Error> {
                images::generate(model, prompt, params, provider).await
            }
            fn image_limits(&self, model: &Model) -> CallLimits {
                images::limits(model)
            }
            async fn generate_audio(
                &self,
                model: &Model,
                prompt: AudioPrompt,
                params: AudioParams,
                provider: &dyn Provider,
            ) -> Result<Item<Vec<FilePart>>, Error> {
                speech::generate(model, prompt, params, provider).await
            }
            async fn transcribe(
                &self,
                model: &Model,
                audio: TranscribeInput,
                params: TranscribeParams,
                provider: &dyn Provider,
            ) -> Result<Item<Transcription>, Error> {
                transcription::transcribe(model, audio, params, provider).await
            }
            async fn stream_transcription(
                &self,
                model: &Model,
                audio: AudioChunks,
                format: AudioFormat,
                params: StreamTranscribeParams,
                provider: &dyn Provider,
            ) -> Result<TranscriptionStream, Error> {
                socket::stream_transcription(model, audio, format, params, provider).await
            }
            async fn stream_translation(
                &self,
                model: &Model,
                audio: AudioChunks,
                format: AudioFormat,
                params: StreamTranslateParams,
                provider: &dyn Provider,
            ) -> Result<TranslationStream, Error> {
                socket::stream_translation(model, audio, format, params, provider).await
            }
            async fn create_realtime_token(
                &self,
                model: &Model,
                params: RealtimeTokenParams,
                provider: &dyn Provider,
            ) -> Result<RealtimeToken, Error> {
                realtime::create_token(model, params, provider).await
            }
            fn realtime_session(
                &self,
                model: &Model,
                config: &RealtimeSessionConfig,
            ) -> Result<Value, Error> {
                if realtime::live(&model.id) {
                    realtime::live_session(config, &model.id, false)
                } else {
                    realtime::session(config, &model.id)
                }
            }
            fn realtime_codec(&self, model: &Model) -> Result<Box<dyn RealtimeCodec>, Error> {
                Ok(if realtime::live(&model.id) {
                    Box::new(realtime::LiveCodec {
                        model: model.id.clone(),
                    })
                } else {
                    Box::new(realtime::Codec {
                        model: model.id.clone(),
                    })
                })
            }
            fn realtime_connection(
                &self,
                model: &Model,
                token: &RealtimeToken,
            ) -> Result<(WebSocketRequest, bool), Error> {
                realtime::token_connection(model, token)
            }
            fn realtime_server_connection(
                &self,
                model: &Model,
                provider: &dyn Provider,
            ) -> Result<(WebSocketRequest, RealtimeStartup), Error> {
                realtime::server_connection(model, provider)
            }
            async fn create_webrtc_session(
                &self,
                model: &Model,
                offer_sdp: String,
                config: RealtimeSessionConfig,
                provider: &dyn Provider,
            ) -> Result<WebRtcSession, Error> {
                realtime::webrtc_session(model, offer_sdp, config, provider).await
            }
        }
    };
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OpenAIChatCompletionsProtocol;
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenAIResponsesProtocol;
/// Responses for the name `openai`, Chat Completions for every other name.
/// This is not the protocol a provider uses: `OpenAICompatibleProvider`
/// decides when it is built, from the API it targets.
#[deprecated(
    note = "choose by the API, not the name: use `Provider::default_protocol` of the provider"
)]
pub fn default_protocol(provider: &str) -> Arc<dyn ProviderProtocol> {
    if provider == "openai" {
        Arc::new(OpenAIResponsesProtocol)
    } else {
        Arc::new(OpenAIChatCompletionsProtocol)
    }
}

/// The API a request uses: Responses or Chat Completions, as a server's
/// profile describes it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Wire {
    Responses(&'static ResponsesProfile),
    Chat(&'static ChatProfile),
}
impl Wire {
    pub(crate) fn api(self) -> &'static http::ApiProfile {
        match self {
            Self::Responses(profile) => &profile.api,
            Self::Chat(profile) => &profile.api,
        }
    }
    fn responses(self) -> bool {
        matches!(self, Self::Responses(_))
    }
    /// The Responses dialect, or `None` for Chat Completions.
    pub(crate) fn responses_dialect(self) -> Option<ResponsesDialect> {
        match self {
            Self::Responses(profile) => Some(profile.dialect),
            Self::Chat(_) => None,
        }
    }
}

/// Responses for a host, as its profile describes it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HostResponsesProtocol(pub(crate) &'static ResponsesProfile);
impl ProviderProtocol for HostResponsesProtocol {
    fn protocol_class_id(&self) -> &str {
        self.0.protocol_class_id
    }
    fn stream(
        &self,
        model: &Model,
        messages: Vec<Message>,
        options: StreamOptions,
        provider: &dyn Provider,
    ) -> EventStream {
        start(
            model,
            messages,
            options,
            provider,
            Wire::Responses(self.0),
            None,
        )
    }
}

/// Chat Completions for a host, as its profile describes it, with the
/// transport that downloads the media URLs the host cannot fetch, if any.
#[derive(Clone)]
pub(crate) struct HostChatProtocol(
    pub(crate) &'static ChatProfile,
    pub(crate) Option<Arc<dyn crate::models::files::DownloadTransport>>,
);
impl ProviderProtocol for HostChatProtocol {
    fn protocol_class_id(&self) -> &str {
        self.0.protocol_class_id
    }
    fn stream(
        &self,
        model: &Model,
        messages: Vec<Message>,
        options: StreamOptions,
        provider: &dyn Provider,
    ) -> EventStream {
        start(
            model,
            messages,
            options,
            provider,
            Wire::Chat(self.0),
            self.1.clone(),
        )
    }
}
/// Chat Completions with explicit public-network media downloads.
/// Download transports must honor the core DNS-pinning contract. Runtime transports
/// are not serialized; configure downloads again after restoring a snapshot.
#[derive(Clone)]
pub struct OpenAIChatCompletionsWithDownloads {
    downloads: Arc<dyn crate::models::files::DownloadTransport>,
}
impl OpenAIChatCompletionsProtocol {
    pub fn with_download_transport(
        self,
        downloads: Arc<dyn crate::models::files::DownloadTransport>,
    ) -> OpenAIChatCompletionsWithDownloads {
        OpenAIChatCompletionsWithDownloads { downloads }
    }
}
openai_protocol!(
    OpenAIChatCompletionsProtocol,
    profile::OPENAI.protocol_class_id,
    Wire::Chat(&profile::OPENAI),
    |_this| None
);
openai_protocol!(
    OpenAIResponsesProtocol,
    "openai_responses",
    Wire::Responses(&profile::OPENAI_RESPONSES),
    |_this| None
);
openai_protocol!(
    OpenAIChatCompletionsWithDownloads,
    profile::OPENAI.protocol_class_id,
    Wire::Chat(&profile::OPENAI),
    |this| Some(this.downloads.clone())
);
fn start(
    model: &Model,
    messages: Vec<Message>,
    options: StreamOptions,
    provider: &dyn Provider,
    wire: Wire,
    downloads: Option<Arc<dyn crate::models::files::DownloadTransport>>,
) -> EventStream {
    let config = provider.config().clone();
    let provider_name = provider.name().to_owned();
    let model_id = model.id.clone();
    let transport = provider.shared_transport();
    let downloads = downloads.or_else(|| provider.download_transport());
    stream::once(async move {
        // Check the request before any download, with each file to download
        // standing in as bytes, so a call that fails validation sends nothing.
        if downloads.is_some()
            && let Some(inline) = messages::without_downloads(&messages, wire)
        {
            wire_request(wire, &model_id, &config, inline, &options)?;
        }
        let messages = messages::download_files(messages, wire, downloads.as_deref()).await?;
        let request = wire_request(wire, &model_id, &config, messages, &options)?;
        let transport = transport.ok_or_else(|| {
            Error::Configuration(format!(
                "{} protocols require Provider::shared_transport",
                wire.api().label
            ))
        })?;
        let response = http::send(
            wire.api(),
            transport.as_ref(),
            request,
            &provider_name,
            Some(&model_id),
        )
        .await?;
        let image_type = options
            .tools
            .iter()
            .find_map(|t| {
                t.tool_config
                    .as_ref()
                    .filter(|c| c.id.as_deref() == Some("openai.image_generation"))
                    .map(|c| {
                        c.args
                            .get("output_format")
                            .and_then(Value::as_str)
                            .unwrap_or("png")
                    })
            })
            .unwrap_or("png");
        let hold_reasoning = wire.responses_dialect() == Some(ResponsesDialect::OpenAI)
            && options
                .params
                .as_ref()
                .is_some_and(|p| params::unstored(p, wire));
        let logprobs = options
            .params
            .as_ref()
            .is_some_and(|p| params::asks_logprobs(p, wire));
        Ok::<_, Error>(streaming::parse(
            response.body,
            provider_name,
            wire,
            format!("image/{image_type}"),
            options.output_schema.is_some(),
            hold_reasoning,
            logprobs,
            super::tools::CallTools::of(&options.tools),
        ))
    })
    .try_flatten()
    .boxed()
}
/// Build a complete HTTP request without sending it.
pub fn build_request(
    model: &str,
    config: &ProviderConfig,
    messages: Vec<Message>,
    options: &StreamOptions,
    responses: bool,
) -> Result<HttpRequest, Error> {
    let wire = if responses {
        Wire::Responses(&profile::OPENAI_RESPONSES)
    } else {
        Wire::Chat(&profile::OPENAI)
    };
    wire_request(wire, model, config, messages, options)
}
pub(crate) fn wire_request(
    wire: Wire,
    model: &str,
    config: &ProviderConfig,
    messages: Vec<Message>,
    options: &StreamOptions,
) -> Result<HttpRequest, Error> {
    let responses = wire.responses();
    let p = options.params.clone().unwrap_or_default();
    let chat = match wire {
        Wire::Chat(profile) => Some(profile),
        Wire::Responses(_) => None,
    };
    match wire {
        Wire::Chat(profile) => dialect::check(profile, model, &p, options)?,
        Wire::Responses(profile) => dialect::check_responses(profile, model, &p, options)?,
    }
    // A host's own entries (HST-026), read before the request is built so an
    // invalid one sends nothing.
    let host_entry = match chat {
        Some(profile) => super::host_options::HostEntry::read(profile, &p)?,
        None => None,
    };
    // OpenAI's own Responses API reads the call's `openai` options.
    let call = match wire {
        Wire::Responses(profile) if profile.dialect == ResponsesDialect::OpenAI => {
            Some(responses::Call::new(model, &p)?)
        }
        _ => None,
    };
    // As does OpenAI's own Chat Completions API.
    let chat_call = match wire {
        Wire::Chat(profile) if profile.openai_own => Some(chat::ChatCall::new(model, &p)?),
        _ => None,
    };
    let mut body = params::build(&p, wire)?;
    match wire {
        Wire::Chat(profile) => dialect::request(profile, model, &p, &mut body)?,
        Wire::Responses(profile) => dialect::responses_request(profile, model, &mut body),
    }
    if let Some(call) = &call {
        call.apply(model, &p, &mut body);
    }
    if let Some(chat_call) = &chat_call {
        chat_call.apply(model, &mut body);
    }
    let use_references = body.get("store") != Some(&Value::Bool(false));
    body.insert("model".into(), json!(model));
    body.insert("stream".into(), json!(true));
    let host_tools = super::tools::CallTools::of(&options.tools);
    let quoted_results = match &call {
        Some(_) => super::tools::output_schema_tools(&options.tools)?,
        None => vec![],
    };
    let history = match &call {
        Some(call) => messages::History {
            model,
            references: use_references,
            reasoning: call.reasoning,
            system_role: call.system_role(),
            update_blocked: call.update_blocked(&body),
            pass_through_files: call.options.pass_through_unsupported_files == Some(true),
            conversation: call.options.conversation.is_some(),
            previous_response: call.options.previous_response_id.is_some(),
            quoted_results: &quoted_results,
            host_tools: &host_tools,
            deepseek_beta: false,
        },
        None => match &chat_call {
            Some(chat_call) => messages::History {
                reasoning: chat_call.reasoning,
                system_role: chat_call.system_role(),
                ..messages::History::plain(model, use_references)
            },
            None => messages::History::plain(model, use_references),
        },
    };
    let deepseek = chat.filter(|profile| profile.dialect == Dialect::DeepSeek);
    let mut history = history;
    history.deepseek_beta = deepseek.is_some() && super::deepseek::beta(config);
    let mut converted = messages::convert(messages, wire, history)?;
    if let Some(call) = &call {
        call.finish(model, &mut body, &mut converted, &options.tools)?;
    }
    if let (Some(profile), Some(schema)) = (chat, &options.output_schema)
        && let Some(message) = dialect::schema_message(profile, schema)?
    {
        converted.insert(0, message);
    }
    body.insert(
        if responses { "input" } else { "messages" }.into(),
        converted.into(),
    );
    if let Wire::Chat(profile) = wire
        && profile.include_usage
    {
        body.insert("stream_options".into(), json!({"include_usage":true}));
    }
    // A server that takes only automatic tool choice gets no tools for none.
    let mut send_tools = true;
    if let Wire::Chat(profile) = wire
        && profile.auto_tool_choice_only
    {
        let choice = options
            .tool_choice
            .as_ref()
            .or(p.tool_calling.as_ref().map(|t| &t.tool_choice));
        match choice {
            None | Some(ToolChoice::Auto) => {}
            Some(ToolChoice::None) => {
                send_tools = false;
                for key in ["tool_choice", "parallel_tool_calls"] {
                    body.remove(key);
                }
            }
            Some(other) => {
                return Err(params::invalid(format!(
                    "{} supports only automatic tool choice, not {}",
                    profile.api.label,
                    tool_choice_name(other)
                )));
            }
        }
    }
    if send_tools && !options.tools.is_empty() {
        body.insert(
            "tools".into(),
            super::tools::convert(&options.tools, wire, model)?.into(),
        );
    }
    if deepseek.is_some() {
        super::deepseek::strict_tools(&options.tools, &mut body, history.deepseek_beta)?;
    }
    if send_tools && let Some(choice) = &options.tool_choice {
        body.insert("tool_choice".into(), params::tool_choice(choice, wire)?);
    }
    if let Some(call) = &call {
        match &call.options.allowed_tools {
            Some(allowed) if !options.tools.is_empty() => {
                let choice = super::tools::allowed_tools(allowed, &options.tools, model)?;
                body.insert("tool_choice".into(), choice);
            }
            Some(_) => {}
            None => {
                if let Some(ToolChoice::Tool(name)) = dialect::tool_choice(&p, options) {
                    let choice = super::tools::named_choice(&name.0, &options.tools)?;
                    body.insert("tool_choice".into(), choice);
                }
            }
        }
    }
    if let Some(profile) = chat
        && profile.dialect == Dialect::Mistral
    {
        mistral_tools(&mut body, dialect::tool_choice(&p, options))?;
    }
    if wire.responses_dialect() == Some(ResponsesDialect::HuggingFace) {
        huggingface_tools(&mut body, dialect::tool_choice(&p, options));
    }
    if let Some(schema) = &options.output_schema {
        let name = options.output_type_name.as_deref().unwrap_or("output");
        let host_format = match chat {
            Some(profile) => dialect::response_format(profile, name, schema)?,
            None => None,
        };
        match wire {
            _ if host_format.is_some() => {
                body.insert("response_format".into(), host_format.into());
            }
            Wire::Responses(profile) if profile.dialect == ResponsesDialect::HuggingFace => {
                let text = body.entry("text").or_insert_with(|| json!({}));
                text["format"] =
                    json!({"type":"json_schema","name":name,"schema":schema,"strict":false});
            }
            Wire::Responses(_) => {
                let schema = strict_output_schema(schema, call.is_some().then_some(model))?;
                let strict = call.as_ref().is_none_or(responses::Call::strict);
                let text = body.entry("text").or_insert_with(|| json!({}));
                text["format"] =
                    json!({"type":"json_schema","name":name,"schema":schema,"strict":strict});
            }
            Wire::Chat(profile) if profile.sends_json_schema(model) => {
                let schema = strict_output_schema(schema, chat_call.is_some().then_some(model))?;
                let strict = match (&chat_call, &host_entry) {
                    (Some(chat_call), _) => chat_call.strict(),
                    (None, Some(entry)) => entry.strict(),
                    (None, None) => true,
                };
                body.insert("response_format".into(),json!({"type":"json_schema","json_schema":{"name":name,"schema":schema,"strict":strict}}));
            }
            Wire::Chat(_) => {
                body.insert("response_format".into(), json!({"type":"json_object"}));
            }
        }
    }
    // The `mistral` entry settles the schema's strictness or turns it into
    // JSON mode with the schema instruction (HST-042).
    if let Some(profile) = chat
        && profile.dialect == Dialect::Mistral
    {
        super::mistral::Options::parse(&p)?.output(options.output_schema.as_ref(), &mut body)?;
    }
    if let (Some(profile), Some(entry)) = (chat, &host_entry) {
        entry.apply(profile, &mut body);
    }
    // Match SDK extra_body merging: explicit escape-hatch keys win at the wire boundary.
    if let Some(extra) = &p.extra_body {
        body.extend(extra.clone());
    }
    let suffix: &[&str] = if responses {
        &["responses"]
    } else {
        &["chat", "completions"]
    };
    http_request(
        wire.api(),
        "POST",
        config,
        suffix,
        Some(Value::Object(body)),
        Some(&p),
    )
}
/// Mistral's tool choice: `any` for required or a named tool, whose name
/// filters the tools, and neither `tool_choice` nor `parallel_tool_calls`
/// without tools (`mistral-prepare-tools.ts`).
fn mistral_tools(
    body: &mut serde_json::Map<String, Value>,
    choice: Option<&ToolChoice>,
) -> Result<(), Error> {
    if let Some(ToolChoice::Tool(tool)) = choice {
        let tools = body.get_mut("tools").and_then(Value::as_array_mut);
        let found = tools.is_some_and(|tools| {
            tools.retain(|t| t["function"]["name"].as_str() == Some(tool.0.as_str()));
            !tools.is_empty()
        });
        if !found {
            return Err(params::invalid(format!(
                "Mistral tool choice names the tool {tool}, which the call does not declare"
            )));
        }
    }
    let has_tools = body
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty());
    if !has_tools {
        body.remove("tools");
        body.remove("tool_choice");
        body.remove("parallel_tool_calls");
        return Ok(());
    }
    if matches!(choice, Some(ToolChoice::Required | ToolChoice::Tool(_))) {
        body.insert("tool_choice".into(), json!("any"));
    }
    Ok(())
}
/// Hugging Face's tool choice: a named tool in the Chat Completions shape,
/// and neither tools nor `tool_choice` for `none`
/// (`huggingface-responses-prepare-tools.ts`).
fn huggingface_tools(body: &mut serde_json::Map<String, Value>, choice: Option<&ToolChoice>) {
    match choice {
        Some(ToolChoice::None) => {
            body.remove("tools");
            body.remove("tool_choice");
        }
        Some(ToolChoice::Tool(tool)) => {
            body.insert(
                "tool_choice".into(),
                json!({"type":"function","function":{"name":tool.0}}),
            );
        }
        _ => {}
    }
}
fn tool_choice_name(choice: &ToolChoice) -> &'static str {
    match choice {
        ToolChoice::Auto => "auto",
        ToolChoice::None => "none",
        ToolChoice::Required => "required",
        ToolChoice::Tool(_) => "a named tool",
        ToolChoice::Selection(_) => "a tool selection",
    }
}
pub(super) fn http_request(
    api: &http::ApiProfile,
    method: &str,
    config: &ProviderConfig,
    path: &[&str],
    body: Option<Value>,
    params: Option<&InferenceRequestParams>,
) -> Result<HttpRequest, Error> {
    let headers = params
        .and_then(|p| p.extra_headers.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|(name, value)| match value {
            ExtraHeaderValue::Value(value) => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect();
    let query = params
        .and_then(|p| p.extra_query.clone())
        .unwrap_or_default();
    http::request(api, config, method, path, body, headers, query)
}
/// The output schema in the crate's strict form; for OpenAI's own APIs,
/// normalized first for `model` (OAI-027).
fn strict_output_schema(schema: &schemars::Schema, openai: Option<&str>) -> Result<Value, Error> {
    let mut schema = serde_json::to_value(schema)?;
    if let Some(model) = openai {
        schema = super::schema::normalize(&schema, model)?;
    }
    strict_schema(&mut schema, 0)?;
    Ok(schema)
}
fn strict_schema(schema: &mut Value, _depth: usize) -> Result<(), Error> {
    let root = schema.clone();
    strict_node(schema, &root, 0)
}
fn strict_node(schema: &mut Value, root: &Value, depth: usize) -> Result<(), Error> {
    if depth > 128 {
        return Err(params::invalid("OpenAI output schema exceeds 128 levels"));
    }
    let Some(object) = schema.as_object_mut() else {
        return Ok(());
    };
    if object.len() > 1
        && let Some(reference) = object.get("$ref").and_then(Value::as_str)
    {
        let resolved = reference
            .strip_prefix('#')
            .and_then(|p| root.pointer(p))
            .and_then(Value::as_object)
            .ok_or_else(|| params::invalid("OpenAI schema references must resolve locally"))?;
        let mut expanded = resolved.clone();
        expanded.extend(object.clone());
        expanded.remove("$ref");
        *object = expanded;
    }
    if object.get("default") == Some(&Value::Null) {
        object.remove("default");
    }
    if object.get("type").and_then(Value::as_str) == Some("object")
        && !object.contains_key("additionalProperties")
    {
        object.insert("additionalProperties".into(), json!(false));
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        object.insert(
            "required".into(),
            json!(properties.keys().collect::<Vec<_>>()),
        );
    }
    for key in ["properties", "$defs", "definitions"] {
        if let Some(map) = object.get_mut(key).and_then(Value::as_object_mut) {
            for value in map.values_mut() {
                strict_node(value, root, depth + 1)?;
            }
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(value) = object.get_mut(key) {
            strict_node(value, root, depth + 1)?;
        }
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(values) = object.get_mut(key).and_then(Value::as_array_mut) {
            for value in values {
                strict_node(value, root, depth + 1)?;
            }
        }
    }
    if let Some(all) = object
        .get("allOf")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 1)
        && let Some(only) = all[0].as_object()
    {
        let mut flattened = object.clone();
        flattened.remove("allOf");
        flattened.extend(only.clone());
        *object = flattened;
        return strict_node(schema, root, depth + 1);
    }
    Ok(())
}
