//! Tests for HST-001 to HST-041 in docs/spec/hosts.md. Each test name starts
//! with the requirement it checks; scripts/check-hosts.sh selects them by it.
//! A recording transport keeps every request the crate sends and answers it
//! with a reply written in the test, so no request leaves the machine. The
//! tests show the crate sends what the AI SDK for TypeScript 7.0.148 sends;
//! they cannot show that a host still accepts it.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use futures::{StreamExt, future::BoxFuture, stream};
use schemars::JsonSchema;
use serde_json::{Value, json};
use suprnova_ai::{
    Error, Model, Provider, ProviderConfig, ProviderRegistry, StreamOptions,
    errors::ProviderErrorKind,
    models::params::*,
    providers::{
        ModelsDevProvider, anthropic,
        hosts::{
            cerebras, deepinfra, deepseek, fireworks, huggingface, minimax, mistral, moonshotai,
            togetherai, xai, zai,
        },
        openai::{self, OpenAIChatCompletionsProtocol, OpenAICompatibleProvider},
    },
    system_message,
    transport::{HttpRequest, HttpResponse, HttpTransport, TransportError},
    types::{
        Event, Message, MessageRole, Part, Tool, ToolChoice, ToolChoiceMode, ToolSelection,
        messages::{
            BuiltinToolCallPart, BuiltinToolReturnPart, ContentOutput, ContentOutputType,
            ContentPart, FilePart, ReasoningPart, SpecialToolResult, TextPart, ToolCallPart,
            ToolResultPart, ToolResultValue,
        },
        tools::{ToolConfig, ToolSpec},
    },
    user_message,
};

/// Records each request and answers it with the next scripted reply, split
/// into 3-byte chunks so the readers meet every UTF-8 and SSE boundary.
#[derive(Default)]
struct Recorder {
    requests: Mutex<Vec<HttpRequest>>,
    replies: Mutex<VecDeque<(u16, Vec<u8>)>>,
}
impl Recorder {
    fn new(replies: impl IntoIterator<Item = (u16, Vec<u8>)>) -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::default(),
            replies: Mutex::new(replies.into_iter().collect()),
        })
    }
    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
    fn request(&self) -> HttpRequest {
        let requests = self.requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "expected exactly one request");
        requests[0].clone()
    }
    fn body(&self) -> Value {
        serde_json::from_slice(&self.request().body).unwrap()
    }
    fn header(&self, name: &str) -> Option<String> {
        self.request()
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }
}
impl HttpTransport for Recorder {
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        self.requests.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop_front();
        Box::pin(async move {
            let (status, bytes) =
                reply.ok_or_else(|| TransportError::Request("no scripted reply left".into()))?;
            let chunks: Vec<_> = bytes
                .chunks(3)
                .map(|b| Ok(Bytes::copy_from_slice(b)))
                .collect();
            Ok(HttpResponse {
                status,
                headers: vec![],
                body: stream::iter(chunks).boxed(),
            })
        })
    }
}

/// A Chat Completions event stream of these chunks.
fn chat_sse(chunks: &[Value]) -> (u16, Vec<u8>) {
    let mut text: String = chunks.iter().map(|c| format!("data: {c}\n\n")).collect();
    text.push_str("data: [DONE]\n\n");
    (200, text.into_bytes())
}
fn chat_text(text: &str, finish: &str) -> (u16, Vec<u8>) {
    chat_sse(&[
        json!({"id":"r1","model":"m","choices":[{"index":0,"delta":{"content":text}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":finish}]}),
    ])
}
/// A Messages event stream that answers `hi`.
fn messages_text() -> (u16, Vec<u8>) {
    let events = [
        json!({"type":"message_start","message":{"id":"msg_1","model":"m","content":[],"stop_reason":null,"usage":{"input_tokens":1,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}),
        json!({"type":"message_stop"}),
    ];
    let text: String = events
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
        .collect();
    (200, text.into_bytes())
}

type Register =
    fn(&mut ProviderRegistry, ProviderConfig, Arc<dyn HttpTransport>) -> Result<(), Error>;
struct Host {
    name: &'static str,
    register: Register,
    url: &'static str,
    key_env: &'static str,
    messages: bool,
    responses: bool,
}
const HOSTS: [Host; 11] = [
    Host {
        name: "cerebras",
        register: cerebras::register,
        url: "https://api.cerebras.ai/v1/chat/completions",
        key_env: "CEREBRAS_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "deepinfra",
        register: deepinfra::register,
        url: "https://api.deepinfra.com/v1/openai/chat/completions",
        key_env: "DEEPINFRA_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "fireworks",
        register: fireworks::register,
        url: "https://api.fireworks.ai/inference/v1/chat/completions",
        key_env: "FIREWORKS_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "togetherai",
        register: togetherai::register,
        url: "https://api.together.xyz/v1/chat/completions",
        key_env: "TOGETHER_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "zai",
        register: zai::register,
        url: "https://api.z.ai/api/paas/v4/chat/completions",
        key_env: "ZAI_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "minimax",
        register: minimax::register,
        url: "https://api.minimax.io/anthropic/v1/messages",
        key_env: "MINIMAX_API_KEY",
        messages: true,
        responses: false,
    },
    Host {
        name: "deepseek",
        register: deepseek::register,
        url: "https://api.deepseek.com/chat/completions",
        key_env: "DEEPSEEK_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "moonshotai",
        register: moonshotai::register,
        url: "https://api.moonshot.ai/v1/chat/completions",
        key_env: "MOONSHOT_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "mistral",
        register: mistral::register,
        url: "https://api.mistral.ai/v1/chat/completions",
        key_env: "MISTRAL_API_KEY",
        messages: false,
        responses: false,
    },
    Host {
        name: "xai",
        register: xai::register,
        url: "https://api.x.ai/v1/responses",
        key_env: "XAI_API_KEY",
        messages: false,
        responses: true,
    },
    Host {
        name: "huggingface",
        register: huggingface::register,
        url: "https://router.huggingface.co/v1/responses",
        key_env: "HUGGINGFACE_API_KEY",
        messages: false,
        responses: true,
    },
];
/// A reply in the host's protocol that answers `hi`.
fn reply(host: &Host) -> (u16, Vec<u8>) {
    if host.messages {
        messages_text()
    } else if host.responses {
        responses_text("hi")
    } else {
        chat_text("hi", "stop")
    }
}
/// A Responses event stream of these events.
fn responses_sse(events: &[Value]) -> (u16, Vec<u8>) {
    let text: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
    (200, text.into_bytes())
}
/// A Responses stream that answers `text`.
fn responses_text(text: &str) -> (u16, Vec<u8>) {
    responses_sse(&[
        json!({"type":"response.created","response":{"id":"resp_1","model":"m","status":"in_progress"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_1","role":"assistant","content":[]}}),
        json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"delta":text}),
        json!({"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"msg_1","role":"assistant","content":[{"type":"output_text","text":text}]}}),
        json!({"type":"response.completed","response":{"id":"resp_1","model":"m","status":"completed","output":[],"usage":{"input_tokens":3,"output_tokens":1}}}),
    ])
}
const CHAT_HOSTS: [&str; 5] = ["cerebras", "deepinfra", "fireworks", "togetherai", "zai"];
/// The hosts whose messages, schemas and stream content differ from OpenAI's.
const NEW_HOSTS: [&str; 3] = ["deepseek", "moonshotai", "mistral"];

fn host(name: &str) -> &'static Host {
    HOSTS.iter().find(|h| h.name == name).unwrap()
}
fn key_config() -> ProviderConfig {
    ProviderConfig {
        api_key_value: Some("sk-test".into()),
        ..Default::default()
    }
}
fn registry(host: &Host, config: ProviderConfig, transport: Arc<Recorder>) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    (host.register)(&mut registry, config, transport).unwrap();
    registry
}
fn model(name: &str, model_id: &str, transport: Arc<Recorder>) -> Model {
    let registry = registry(host(name), key_config(), transport);
    Model::new(model_id, registry.get_provider(name).unwrap())
}
/// Stream a call to its end or its first error and return its handle and
/// delivered events.
async fn call(
    model: &Model,
    messages: Vec<Message>,
    options: StreamOptions,
) -> (suprnova_ai::models::StreamHandle, Vec<Result<Event, Error>>) {
    let mut handle = suprnova_ai::stream(model, messages, options);
    let mut events = Vec::new();
    while let Some(event) = handle.next().await {
        let failed = event.is_err();
        events.push(event);
        if failed {
            break;
        }
    }
    (handle, events)
}
fn ok(events: &[Result<Event, Error>]) {
    for event in events {
        if let Err(error) = event {
            panic!("the call failed: {error:?}");
        }
    }
}
fn params(update: impl FnOnce(&mut InferenceRequestParams)) -> StreamOptions {
    let mut params = InferenceRequestParams::default();
    update(&mut params);
    StreamOptions {
        params: Some(params),
        ..Default::default()
    }
}
fn effort(value: Option<&str>) -> StreamOptions {
    params(|p| {
        p.reasoning = ProviderValue::Value(ReasoningParams {
            effort: ProviderValue::Value(value.map(str::to_owned)),
        })
    })
}
fn lookup_tool() -> Tool {
    Tool::function(
        "lookup",
        ToolSpec {
            description: Some("Look a value up".into()),
            params: json!({"type":"object","properties":{"q":{"type":"integer"}}})
                .try_into()
                .unwrap(),
        },
    )
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct Answer {
    value: u32,
}

// HST-001

#[tokio::test]
async fn hst_001_each_host_streams_its_protocol_from_its_default_url() {
    for host in &HOSTS {
        let reply = reply(host);
        let transport = Recorder::new([reply]);
        let registry = registry(host, key_config(), transport.clone());
        let handle = format!("@ai-sdk/{}", host.name);
        for id in [host.name, handle.as_str()] {
            let provider = registry.get_provider(id).unwrap();
            assert_eq!(provider.name(), host.name, "{id}");
        }
        let model = Model::new("test-model", registry.get_provider(host.name).unwrap());
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        assert_eq!(handle.text(), "hi", "{}", host.name);
        let request = transport.request();
        assert_eq!(request.method, "POST", "{}", host.name);
        assert_eq!(request.url, host.url);
        if host.messages {
            assert_eq!(transport.header("x-api-key").as_deref(), Some("sk-test"));
            assert_eq!(
                transport.header("anthropic-version").as_deref(),
                Some("2023-06-01")
            );
            assert_eq!(transport.header("authorization"), None);
        } else {
            assert_eq!(
                transport.header("authorization").as_deref(),
                Some("Bearer sk-test"),
                "{}",
                host.name
            );
            assert_eq!(transport.header("x-api-key"), None);
        }
        assert_eq!(transport.body()["model"], "test-model");
    }
}

#[tokio::test]
async fn hst_001_a_configured_base_url_and_an_env_key_are_used() {
    for host in &HOSTS {
        let reply = reply(host);
        let transport = Recorder::new([reply]);
        let config = ProviderConfig {
            default_base_url: "https://proxy.test/base".into(),
            env: [(host.key_env.to_owned(), "sk-env".to_owned())].into(),
            ..Default::default()
        };
        let registry = registry(host, config, transport.clone());
        let provider = registry.get_provider(host.name).unwrap();
        assert!(provider.is_configured(), "{}", host.name);
        let model = Model::new("test-model", provider);
        let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        let path = if host.messages {
            "/v1/messages"
        } else if host.responses {
            "/responses"
        } else {
            "/chat/completions"
        };
        assert_eq!(
            transport.request().url,
            format!("https://proxy.test/base{path}")
        );
        let key = if host.messages {
            transport.header("x-api-key")
        } else {
            transport.header("authorization")
        };
        let expected = if host.messages {
            "sk-env"
        } else {
            "Bearer sk-env"
        };
        assert_eq!(key.as_deref(), Some(expected), "{}", host.name);
    }
}

#[test]
fn hst_001_a_host_without_a_key_is_not_configured() {
    for host in &HOSTS {
        let registry = registry(host, ProviderConfig::default(), Recorder::new([]));
        assert!(
            !registry.get_provider(host.name).unwrap().is_configured(),
            "{}",
            host.name
        );
    }
}

#[test]
fn hst_001_every_host_registers_with_openai_and_anthropic() {
    let transport: Arc<dyn HttpTransport> = Recorder::new([]);
    let mut registry = ProviderRegistry::new();
    openai::register(
        &mut registry,
        Arc::new(OpenAICompatibleProvider::new(
            key_config(),
            transport.clone(),
        )),
    )
    .unwrap();
    openai::register_factories(&mut registry, transport.clone()).unwrap();
    anthropic::register(&mut registry, key_config(), transport.clone()).unwrap();
    for host in &HOSTS {
        (host.register)(&mut registry, key_config(), transport.clone()).unwrap();
    }
    for name in ["openai", "anthropic"]
        .into_iter()
        .chain(HOSTS.iter().map(|h| h.name))
    {
        assert_eq!(registry.get_provider(name).unwrap().name(), name);
    }
    // A second registration of a host fails and leaves the registry as it was.
    let error = cerebras::register(&mut registry, key_config(), transport).unwrap_err();
    assert!(matches!(error, Error::Configuration(_)), "{error:?}");
    assert_eq!(
        registry.get_provider("cerebras").unwrap().name(),
        "cerebras"
    );
}

#[tokio::test]
async fn hst_001_a_restored_snapshot_streams_from_the_registered_endpoint() {
    for host in &HOSTS {
        let reply = reply(host);
        let transport = Recorder::new([reply]);
        let registry = registry(host, key_config(), transport.clone());
        let model = Model::new("test-model", registry.get_provider(host.name).unwrap());
        let snapshot = model.to_value().unwrap();
        assert!(!snapshot.to_string().contains("sk-test"), "{snapshot}");
        let restored = Model::from_value(snapshot, &registry).unwrap();
        assert_eq!(restored.provider.name(), host.name);
        let (_, events) = call(&restored, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        assert_eq!(transport.request().url, host.url);
    }
}

// HST-002

#[tokio::test]
async fn hst_002_stream_options_follow_the_host() {
    for (name, sends) in [
        ("cerebras", false),
        ("deepinfra", false),
        ("zai", false),
        ("fireworks", true),
        ("togetherai", true),
        ("deepseek", true),
        ("moonshotai", true),
        ("mistral", false),
    ] {
        let transport = Recorder::new([chat_text("hi", "stop")]);
        let model = model(name, "m", transport.clone());
        let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        let body = transport.body();
        if sends {
            assert_eq!(
                body["stream_options"],
                json!({"include_usage": true}),
                "{name}"
            );
        } else {
            assert!(body.get("stream_options").is_none(), "{name}: {body}");
        }
    }
}

#[tokio::test]
async fn hst_002_openai_and_hostless_compatible_providers_ask_for_usage() {
    let openai = |transport: Arc<Recorder>| -> Arc<dyn Provider> {
        Arc::new(
            OpenAICompatibleProvider::new(
                ProviderConfig {
                    default_base_url: "https://openai.test/v1".into(),
                    ..key_config()
                },
                transport,
            )
            .with_protocol(Arc::new(OpenAIChatCompletionsProtocol)),
        )
    };
    let compatible = |transport: Arc<Recorder>| -> Arc<dyn Provider> {
        let metadata = ModelsDevProvider {
            id: "ollama".into(),
            npm: Some("@ai-sdk/openai-compatible".into()),
            api: Some("http://localhost:11434/v1".into()),
            env: vec![],
        };
        Arc::new(
            OpenAICompatibleProvider::from_modelsdev_provider(&metadata, key_config(), transport)
                .unwrap(),
        )
    };
    for build in [openai, compatible] {
        let transport = Recorder::new([chat_text("hi", "stop")]);
        let model = Model::new("m", build(transport.clone()));
        let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        assert!(transport.request().url.ends_with("/chat/completions"));
        assert_eq!(
            transport.body()["stream_options"],
            json!({"include_usage": true})
        );
    }
}

#[tokio::test]
async fn hst_002_models_dev_metadata_for_a_host_gets_its_rules() {
    for (id, npm, sends) in [
        ("deepinfra", "@ai-sdk/deepinfra", false),
        ("deepinfra", "@ai-sdk/openai-compatible", false),
        ("cerebras", "@ai-sdk/cerebras", false),
        ("fireworks", "@ai-sdk/fireworks", true),
    ] {
        let transport = Recorder::new([chat_text("hi", "stop")]);
        let metadata = ModelsDevProvider {
            id: id.into(),
            npm: Some(npm.into()),
            api: Some("https://host.test/v1".into()),
            env: vec![],
        };
        let provider = OpenAICompatibleProvider::from_modelsdev_provider(
            &metadata,
            key_config(),
            transport.clone(),
        )
        .unwrap();
        let model = Model::new("m", Arc::new(provider));
        let options = params(|p| {
            p.output = Some(OutputParams {
                max_tokens: Some(100),
                ..Default::default()
            })
        });
        let (_, events) = call(&model, vec![user_message("hello")], options).await;
        ok(&events);
        let body = transport.body();
        assert_eq!(
            body.get("stream_options").is_some(),
            sends,
            "{id} {npm}: {body}"
        );
        let limit = if id == "cerebras" {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };
        assert_eq!(body[limit], 100, "{id} {npm}: {body}");
    }
}

// HST-003

/// A history with an assistant turn that carries reasoning.
fn history_with_reasoning() -> Vec<Message> {
    vec![
        user_message("first"),
        Message::new(
            MessageRole::Assistant,
            vec![
                Part::Reasoning(ReasoningPart {
                    id: "r1".into(),
                    text: "thinking it over".into(),
                    provider_metadata: None,
                }),
                Part::Text(TextPart {
                    id: "t1".into(),
                    text: "an answer".into(),
                    provider_metadata: None,
                }),
            ],
        ),
        user_message("again"),
    ]
}

/// The body a host sends with an output token limit of 100, a reasoning
/// history, a reasoning effort and an output schema.
async fn field_rules_body(name: &str, model_id: &str, reasoning: &str) -> Value {
    let transport = Recorder::new([chat_text("{\"value\":1}", "stop")]);
    let model = model(name, model_id, transport.clone());
    let mut options = params(|p| {
        p.output = Some(OutputParams {
            max_tokens: Some(100),
            ..Default::default()
        });
        p.reasoning = ProviderValue::Value(ReasoningParams {
            effort: ProviderValue::Value(Some(reasoning.to_owned())),
        });
    })
    .with_output::<Answer>();
    options.output_type_name = Some("answer".into());
    let (_, events) = call(&model, history_with_reasoning(), options).await;
    ok(&events);
    transport.body()
}

fn assistant_entry(body: &Value) -> &Value {
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "assistant")
        .unwrap()
}

#[tokio::test]
async fn hst_003_limits_and_reasoning_use_each_hosts_fields() {
    for name in CHAT_HOSTS {
        let body = field_rules_body(name, "some-model", "high").await;
        let (limit, other) = if name == "cerebras" {
            ("max_completion_tokens", "max_tokens")
        } else {
            ("max_tokens", "max_completion_tokens")
        };
        assert_eq!(body[limit], 100, "{name}: {body}");
        assert!(body.get(other).is_none(), "{name}: {body}");
        let (field, other) = if name == "cerebras" {
            ("reasoning", "reasoning_content")
        } else {
            ("reasoning_content", "reasoning")
        };
        let entry = assistant_entry(&body);
        assert_eq!(entry[field], "thinking it over", "{name}: {entry}");
        assert!(entry.get(other).is_none(), "{name}: {entry}");
        assert_eq!(body["reasoning_effort"], "high", "{name}");
    }
}

#[tokio::test]
async fn hst_003_fireworks_maps_the_efforts_it_lacks() {
    for (effort, sent) in [
        ("minimal", "low"),
        ("low", "low"),
        ("medium", "medium"),
        ("high", "high"),
        ("xhigh", "high"),
        ("max", "high"),
    ] {
        let body = field_rules_body("fireworks", "some-model", effort).await;
        assert_eq!(body["reasoning_effort"], sent, "{effort}");
    }
    // Other hosts send the effort as it is.
    let body = field_rules_body("deepinfra", "some-model", "minimal").await;
    assert_eq!(body["reasoning_effort"], "minimal");
}

#[tokio::test]
async fn hst_003_an_output_schema_follows_each_hosts_support() {
    for (name, model_id, schema) in [
        ("cerebras", "some-model", true),
        ("deepinfra", "some-model", true),
        ("fireworks", "some-model", true),
        ("togetherai", "deepseek-ai/DeepSeek-V4-Flash-0731", true),
        (
            "togetherai",
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            false,
        ),
        ("zai", "glm-4.6", false),
    ] {
        let body = field_rules_body(name, model_id, "high").await;
        let format = &body["response_format"];
        if schema {
            assert_eq!(format["type"], "json_schema", "{name} {model_id}: {body}");
            assert_eq!(format["json_schema"]["name"], "answer");
            assert_eq!(format["json_schema"]["strict"], true);
            assert_eq!(
                format["json_schema"]["schema"]["properties"]["value"]["type"],
                "integer"
            );
        } else {
            assert_eq!(*format, json!({"type": "json_object"}), "{name} {model_id}");
        }
    }
}

// HST-004

#[tokio::test]
async fn hst_004_zai_tool_choice_none_sends_no_tools() {
    let by_option = StreamOptions {
        tools: vec![lookup_tool()],
        tool_choice: Some(ToolChoice::None),
        ..Default::default()
    };
    let mut by_param = params(|p| {
        p.tool_calling = Some(ToolCallingParams {
            tool_choice: ToolChoice::None,
            parallel_tool_calls: ProviderValue::Value(true),
            max_tool_calls: ProviderValue::Default,
        })
    });
    by_param.tools = vec![lookup_tool()];
    for options in [by_option, by_param] {
        let transport = Recorder::new([chat_text("hi", "stop")]);
        let model = model("zai", "glm-4.6", transport.clone());
        let (_, events) = call(&model, vec![user_message("hello")], options).await;
        ok(&events);
        let body = transport.body();
        for key in ["tools", "tool_choice", "parallel_tool_calls"] {
            assert!(body.get(key).is_none(), "{key}: {body}");
        }
    }
}

#[tokio::test]
async fn hst_004_zai_automatic_tool_choice_sends_the_tools() {
    let transport = Recorder::new([chat_text("hi", "stop")]);
    let model = model("zai", "glm-4.6", transport.clone());
    let options = StreamOptions {
        tools: vec![lookup_tool()],
        tool_choice: Some(ToolChoice::Auto),
        ..Default::default()
    };
    let (_, events) = call(&model, vec![user_message("hello")], options).await;
    ok(&events);
    let body = transport.body();
    assert_eq!(body["tools"][0]["function"]["name"], "lookup");
    assert_eq!(body["tool_choice"], "auto");
}

#[tokio::test]
async fn hst_004_zai_rejects_what_it_does_not_take_before_any_request() {
    let tool_choice = |choice: ToolChoice| StreamOptions {
        tools: vec![lookup_tool()],
        tool_choice: Some(choice),
        ..Default::default()
    };
    let sampling = |update: fn(&mut SamplerParams)| {
        params(|p| {
            let mut sampling = SamplerParams::default();
            update(&mut sampling);
            p.sampling = ProviderValue::Value(sampling);
        })
    };
    let cases = [
        ("required", tool_choice(ToolChoice::Required)),
        ("named", tool_choice(ToolChoice::Tool("lookup".into()))),
        (
            "selection",
            tool_choice(ToolChoice::Selection(ToolSelection::new(
                ["lookup"],
                ToolChoiceMode::Auto,
            ))),
        ),
        ("required by param", {
            let mut options = params(|p| {
                p.tool_calling = Some(ToolCallingParams {
                    tool_choice: ToolChoice::Required,
                    parallel_tool_calls: ProviderValue::Default,
                    max_tool_calls: ProviderValue::Default,
                })
            });
            options.tools = vec![lookup_tool()];
            options
        }),
        (
            "frequency penalty",
            sampling(|s| {
                s.repetition_penalty = Some(RepetitionPenaltyParams {
                    frequency_penalty: ProviderValue::Value(Some(0.5)),
                    ..Default::default()
                })
            }),
        ),
        (
            "presence penalty",
            sampling(|s| {
                s.repetition_penalty = Some(RepetitionPenaltyParams {
                    presence_penalty: ProviderValue::Value(Some(0.5)),
                    ..Default::default()
                })
            }),
        ),
        (
            "seed",
            sampling(|s| {
                s.seed = Some(SeedSamplerParams {
                    seed: ProviderValue::Value(Some(Seed::Value(7))),
                })
            }),
        ),
    ];
    for (case, options) in cases {
        let transport = Recorder::new([chat_text("hi", "stop")]);
        let model = model("zai", "glm-4.6", transport.clone());
        let (_, events) = call(&model, vec![user_message("hello")], options).await;
        let error = events
            .into_iter()
            .find_map(Result::err)
            .unwrap_or_else(|| panic!("{case}: the call did not fail"));
        assert!(matches!(error, Error::Validation(_)), "{case}: {error:?}");
        assert_eq!(transport.count(), 0, "{case}: a request was sent");
    }
}

#[tokio::test]
async fn hst_004_zai_finish_reasons_map_to_the_normalized_names() {
    for (raw, normalized) in [
        ("sensitive", "content_filter"),
        ("model_context_window_exceeded", "length"),
        ("network_error", "error"),
        ("stop", "stop"),
    ] {
        let transport = Recorder::new([chat_text("partial", raw)]);
        let model = model("zai", "glm-4.6", transport);
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        assert_eq!(handle.finish_reason(), Some(normalized), "{raw}");
    }
}

// HST-005

#[tokio::test]
async fn hst_005_error_messages_come_from_every_body_shape() {
    let compatible = |transport: Arc<Recorder>| -> Model {
        let metadata = ModelsDevProvider {
            id: "custom".into(),
            npm: Some("@ai-sdk/openai-compatible".into()),
            api: Some("https://custom.test/v1".into()),
            env: vec![],
        };
        Model::new(
            "m",
            Arc::new(
                OpenAICompatibleProvider::from_modelsdev_provider(
                    &metadata,
                    key_config(),
                    transport,
                )
                .unwrap(),
            ),
        )
    };
    let bodies = [
        json!({"error": {"message": "nested message", "type": "invalid_request_error"}}),
        json!({"error": "string message"}),
        json!({"message": "top-level message", "code": "bad"}),
    ];
    for body in bodies {
        let expected = crate_message(&body);
        for name in CHAT_HOSTS.into_iter().chain(NEW_HOSTS).chain(["custom"]) {
            let transport = Recorder::new([(400, body.to_string().into_bytes())]);
            let model = if name == "custom" {
                compatible(transport)
            } else {
                model(name, "m", transport)
            };
            let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
            let Some(Error::Provider(error)) = events.into_iter().find_map(Result::err) else {
                panic!("{name}: the call did not fail with a provider error");
            };
            assert_eq!(error.message, expected, "{name}: {body}");
            assert_eq!(error.kind, ProviderErrorKind::BadRequest, "{name}");
            assert_eq!(error.status_code(), Some(400));
        }
    }
}

fn crate_message(body: &Value) -> &str {
    body["error"]["message"]
        .as_str()
        .or_else(|| body["error"].as_str())
        .or_else(|| body["message"].as_str())
        .unwrap()
}

// HST-006

#[tokio::test]
async fn hst_006_deepinfra_counts_reasoning_reported_apart() {
    for (completion, reasoning, output) in [(10, 30, 40), (50, 30, 50)] {
        let transport = Recorder::new([chat_sse(&[
            json!({"choices":[{"index":0,"delta":{"content":"hi"}}]}),
            json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
            json!({"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":completion,"total_tokens":5 + completion,"completion_tokens_details":{"reasoning_tokens":reasoning}}}),
        ])]);
        let model = model("deepinfra", "google/gemma", transport);
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        let usage = handle.usage().unwrap();
        assert_eq!(usage.output_tokens, output, "{completion}/{reasoning}");
        assert_eq!(usage.reasoning_tokens, Some(reasoning));
        assert_eq!(usage.input_tokens, 5);
    }
}

/// Text, then a tool call, then a `tool_calls` finish.
fn text_then_tool_call() -> (u16, Vec<u8>) {
    chat_sse(&[
        json!({"choices":[{"index":0,"delta":{"content":"{\"value\":1}"}}]}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":1}"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ])
}

#[tokio::test]
async fn hst_006_cerebras_drops_tool_calls_after_text_under_an_output_schema() {
    let transport = Recorder::new([text_then_tool_call()]);
    let model = model("cerebras", "gpt-oss-120b", transport);
    let options = StreamOptions {
        tools: vec![lookup_tool()],
        ..Default::default()
    }
    .with_output::<Answer>();
    let (handle, events) = call(&model, vec![user_message("hello")], options).await;
    ok(&events);
    for event in &events {
        assert!(
            !matches!(
                event,
                Ok(Event::ToolStart(_) | Event::ToolDelta(_) | Event::ToolEnd(_))
            ),
            "{event:?}"
        );
    }
    assert_eq!(handle.text(), "{\"value\":1}");
    assert!(handle.tool_calls().is_empty());
    assert_eq!(handle.finish_reason(), Some("stop"));
}

#[tokio::test]
async fn hst_006_cerebras_drops_a_tool_call_that_started_before_the_text() {
    let transport = Recorder::new([chat_sse(&[
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{"content":"{\"value\":1}"}}]}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"1}"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ])]);
    let model = model("cerebras", "gpt-oss-120b", transport);
    let options = StreamOptions {
        tools: vec![lookup_tool()],
        ..Default::default()
    }
    .with_output::<Answer>();
    let (handle, events) = call(&model, vec![user_message("hello")], options).await;
    ok(&events);
    for event in &events {
        assert!(
            !matches!(
                event,
                Ok(Event::ToolStart(_) | Event::ToolDelta(_) | Event::ToolEnd(_))
            ),
            "{event:?}"
        );
    }
    assert!(handle.tool_calls().is_empty());
    assert_eq!(handle.finish_reason(), Some("stop"));
}

#[tokio::test]
async fn hst_006_cerebras_keeps_tool_calls_without_text_under_an_output_schema() {
    let transport = Recorder::new([chat_sse(&[
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":1}"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ])]);
    let model = model("cerebras", "gpt-oss-120b", transport);
    let options = StreamOptions {
        tools: vec![lookup_tool()],
        ..Default::default()
    }
    .with_output::<Answer>();
    let (handle, events) = call(&model, vec![user_message("hello")], options).await;
    ok(&events);
    let ends = events
        .iter()
        .filter(|e| matches!(e, Ok(Event::ToolEnd(_))))
        .count();
    assert_eq!(ends, 1);
    assert_eq!(handle.tool_calls().len(), 1);
    assert_eq!(handle.tool_calls()[0].tool_args, "{\"q\":1}");
    assert_eq!(handle.finish_reason(), Some("tool_call"));
}

#[tokio::test]
async fn hst_006_without_an_output_schema_cerebras_keeps_the_tool_call() {
    let transport = Recorder::new([text_then_tool_call()]);
    let model = model("cerebras", "gpt-oss-120b", transport);
    let options = StreamOptions {
        tools: vec![lookup_tool()],
        ..Default::default()
    };
    let (handle, events) = call(&model, vec![user_message("hello")], options).await;
    ok(&events);
    assert_eq!(handle.tool_calls().len(), 1);
    assert_eq!(handle.finish_reason(), Some("tool_call"));
}

// HST-007

async fn minimax_body(model_id: &str, options: StreamOptions) -> Value {
    let transport = Recorder::new([messages_text()]);
    let model = model("minimax", model_id, transport.clone());
    let (_, events) = call(&model, vec![user_message("hello")], options).await;
    ok(&events);
    transport.body()
}

#[tokio::test]
async fn hst_007_minimax_3_1_and_later_always_reason() {
    for model_id in ["MiniMax-M3.1", "minimax-m4"] {
        let body = minimax_body(model_id, effort(None)).await;
        assert!(body.get("thinking").is_none(), "{model_id}: {body}");
        assert_eq!(body["output_config"]["effort"], "low", "{model_id}");
        let body = minimax_body(model_id, effort(Some("minimal"))).await;
        assert_eq!(body["output_config"]["effort"], "low", "{model_id}");
        let body = minimax_body(model_id, effort(Some("high"))).await;
        assert_eq!(body["output_config"]["effort"], "high", "{model_id}");
        assert!(body.get("thinking").is_none(), "{model_id}: {body}");
    }
}

#[tokio::test]
async fn hst_007_a_summary_adds_no_thinking_when_reasoning_is_disabled() {
    let mut options = effort(None);
    options.params.as_mut().unwrap().output = Some(OutputParams {
        reasoning_summary: ProviderValue::Value(Some("summarized".into())),
        ..Default::default()
    });
    let body = minimax_body("MiniMax-M3.1", options.clone()).await;
    assert!(body.get("thinking").is_none(), "{body}");
    assert_eq!(body["output_config"]["effort"], "low");
    let body = minimax_body("MiniMax-M2", options).await;
    assert_eq!(body["thinking"], json!({"type": "disabled"}));
}

#[tokio::test]
async fn hst_007_older_minimax_models_keep_the_anthropic_mapping() {
    for options in [effort(None), effort(Some("minimal")), effort(Some("high"))] {
        let minimax = minimax_body("MiniMax-M2", options.clone()).await;
        let transport = Recorder::new([messages_text()]);
        let anthropic = Model::new(
            "MiniMax-M2",
            Arc::new(anthropic::AnthropicCompatibleProvider::new(
                key_config(),
                transport.clone(),
            )),
        );
        let (_, events) = call(&anthropic, vec![user_message("hello")], options).await;
        ok(&events);
        assert_eq!(minimax, transport.body());
    }
    let body = minimax_body("MiniMax-M2", effort(None)).await;
    assert_eq!(body["thinking"], json!({"type": "disabled"}));
}

// HST-008 to HST-018: DeepSeek, Moonshot AI and Mistral.

/// The body a host sends for these messages and options; the call must succeed.
async fn host_body(
    name: &str,
    model_id: &str,
    messages: Vec<Message>,
    options: StreamOptions,
) -> Value {
    let transport = Recorder::new([chat_text("{\"value\":1}", "stop")]);
    let model = model(name, model_id, transport.clone());
    let (_, events) = call(&model, messages, options).await;
    ok(&events);
    transport.body()
}
/// The error a call fails with before it sends any request.
async fn rejected(
    name: &str,
    model_id: &str,
    messages: Vec<Message>,
    options: StreamOptions,
) -> Error {
    let transport = Recorder::new([chat_text("hi", "stop")]);
    let model = model(name, model_id, transport.clone());
    let (_, events) = call(&model, messages, options).await;
    let error = events
        .into_iter()
        .find_map(Result::err)
        .unwrap_or_else(|| panic!("{name} {model_id}: the call did not fail"));
    assert_eq!(
        transport.count(),
        0,
        "{name} {model_id}: a request was sent"
    );
    error
}
async fn validation(name: &str, model_id: &str, options: StreamOptions) -> String {
    match rejected(name, model_id, vec![user_message("hello")], options).await {
        Error::Validation(message) => message,
        other => panic!("{name} {model_id}: {other:?}"),
    }
}
fn sampling(update: impl FnOnce(&mut SamplerParams)) -> StreamOptions {
    params(|p| {
        let mut sampling = SamplerParams::default();
        update(&mut sampling);
        p.sampling = ProviderValue::Value(sampling);
    })
}
fn temperature(value: f64) -> StreamOptions {
    sampling(|s| {
        s.temperature = Some(TemperatureSamplerParams {
            temperature: ProviderValue::Value(value),
        })
    })
}
fn with_effort(mut options: StreamOptions, value: Option<&str>) -> StreamOptions {
    options
        .params
        .get_or_insert_with(Default::default)
        .reasoning = ProviderValue::Value(ReasoningParams {
        effort: ProviderValue::Value(value.map(str::to_owned)),
    });
    options
}
fn tools(names: &[&str]) -> Vec<Tool> {
    names
        .iter()
        .map(|name| {
            Tool::function(
                *name,
                ToolSpec {
                    description: Some("A tool".into()),
                    params: json!({"type":"object","properties":{"q":{"type":"integer"}}})
                        .try_into()
                        .unwrap(),
                },
            )
        })
        .collect()
}
fn text_part(text: &str) -> Part {
    Part::Text(TextPart {
        id: format!("t-{text}"),
        text: text.into(),
        provider_metadata: None,
    })
}
fn reasoning_part(text: &str) -> Part {
    Part::Reasoning(ReasoningPart {
        id: format!("r-{text}"),
        text: text.into(),
        provider_metadata: None,
    })
}
fn assistant(parts: Vec<Part>) -> Message {
    Message::new(MessageRole::Assistant, parts)
}
fn tool_call(id: &str, name: &str) -> Part {
    Part::ToolCall(ToolCallPart::new(id, name, "{\"q\":1}"))
}
fn tool_message(result: ToolResultPart) -> Message {
    Message::new(MessageRole::Tool, vec![Part::ToolResult(result)])
}
fn user_file(file: FilePart) -> Message {
    Message::new(MessageRole::User, vec![text_part("look"), Part::File(file)])
}
fn assistants(body: &Value) -> Vec<&Value> {
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "assistant")
        .collect()
}

#[tokio::test]
async fn hst_008_each_new_host_rejects_the_settings_it_never_sends() {
    let with_tools = |mut options: StreamOptions| {
        options.tools = tools(&["lookup"]);
        options
    };
    let cases: Vec<(&str, StreamOptions, &[&str])> = vec![
        (
            "service tier",
            params(|p| {
                p.provider_service = Some(ProviderServiceParams {
                    service_tier: ProviderValue::Value("flex".into()),
                })
            }),
            &NEW_HOSTS,
        ),
        (
            "metadata",
            params(|p| p.metadata = Some([("k".to_owned(), "v".to_owned())].into())),
            &NEW_HOSTS,
        ),
        (
            "cache retention",
            params(|p| {
                p.cache = Some(CacheParams {
                    retention: ProviderValue::Value("24h".into()),
                    ..Default::default()
                })
            }),
            &NEW_HOSTS,
        ),
        (
            "tool selection",
            with_tools(StreamOptions {
                tool_choice: Some(ToolChoice::Selection(ToolSelection::new(
                    ["lookup"],
                    ToolChoiceMode::Auto,
                ))),
                ..Default::default()
            }),
            &NEW_HOSTS,
        ),
        (
            "safety identifier",
            params(|p| p.safety_identifier = Some("user-1".into())),
            &["deepseek", "mistral"],
        ),
        (
            "cache key",
            params(|p| {
                p.cache = Some(CacheParams {
                    key: Some("k1".into()),
                    ..Default::default()
                })
            }),
            &["deepseek"],
        ),
        (
            "parallel tool calls",
            with_tools(params(|p| {
                p.tool_calling = Some(ToolCallingParams {
                    tool_choice: ToolChoice::Auto,
                    parallel_tool_calls: ProviderValue::Value(false),
                    max_tool_calls: ProviderValue::Default,
                })
            })),
            &["deepseek", "moonshotai"],
        ),
    ];
    for (case, options, rejecting) in cases {
        for name in NEW_HOSTS {
            if rejecting.contains(&name) {
                let message = validation(name, "m", options.clone()).await;
                assert!(
                    message.contains("does not support"),
                    "{case} {name}: {message}"
                );
            } else {
                host_body(name, "m", vec![user_message("hello")], options.clone()).await;
            }
        }
    }
}

#[tokio::test]
async fn hst_008_moonshot_and_mistral_send_the_settings_they_take() {
    let options = params(|p| {
        p.cache = Some(CacheParams {
            key: Some("k1".into()),
            ..Default::default()
        });
        p.safety_identifier = Some("user-1".into());
    });
    let body = host_body(
        "moonshotai",
        "kimi-k3",
        vec![user_message("hello")],
        options,
    )
    .await;
    assert_eq!(body["prompt_cache_key"], "k1");
    assert_eq!(body["safety_identifier"], "user-1");
    let mut options = params(|p| {
        p.cache = Some(CacheParams {
            key: Some("k1".into()),
            ..Default::default()
        });
        p.tool_calling = Some(ToolCallingParams {
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: ProviderValue::Value(false),
            max_tool_calls: ProviderValue::Default,
        });
    });
    options.tools = tools(&["lookup"]);
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        options,
    )
    .await;
    assert_eq!(body["prompt_cache_key"], "k1");
    assert_eq!(body["parallel_tool_calls"], false);
}

#[tokio::test]
async fn hst_009_deepseek_sends_its_fields_and_thinking() {
    let history = vec![
        user_message("first"),
        assistant(vec![
            reasoning_part("thinking it over"),
            text_part("an answer"),
        ]),
        user_message("again"),
    ];
    let options = with_effort(
        params(|p| {
            p.output = Some(OutputParams {
                max_tokens: Some(100),
                ..Default::default()
            })
        }),
        Some("medium"),
    );
    let body = host_body("deepseek", "deepseek-v4-flash", history, options).await;
    assert_eq!(body["max_tokens"], 100, "{body}");
    assert!(body.get("max_completion_tokens").is_none());
    assert_eq!(
        assistants(&body)[0]["reasoning_content"],
        "thinking it over"
    );
    assert_eq!(body["thinking"], json!({"type": "enabled"}));
    assert_eq!(body["reasoning_effort"], "high");
    for (level, sent) in [
        ("minimal", "low"),
        ("low", "low"),
        ("high", "high"),
        ("xhigh", "max"),
        ("max", "max"),
    ] {
        let body = host_body(
            "deepseek",
            "deepseek-v4-flash",
            vec![user_message("hello")],
            effort(Some(level)),
        )
        .await;
        assert_eq!(body["reasoning_effort"], sent, "{level}");
        assert_eq!(body["thinking"], json!({"type": "enabled"}), "{level}");
    }
    let body = host_body(
        "deepseek",
        "deepseek-v4-flash",
        vec![user_message("hello")],
        effort(None),
    )
    .await;
    assert_eq!(body["thinking"], json!({"type": "disabled"}));
    assert!(body.get("reasoning_effort").is_none(), "{body}");
    let body = host_body(
        "deepseek",
        "deepseek-v4-flash",
        vec![user_message("hello")],
        Default::default(),
    )
    .await;
    assert!(
        body.get("thinking").is_none() && body.get("reasoning_effort").is_none(),
        "{body}"
    );
}

#[tokio::test]
async fn hst_009_deepseek_puts_the_schema_in_a_first_system_message() {
    let options = StreamOptions::default().with_output::<Answer>();
    let schema = serde_json::to_string(options.output_schema.as_ref().unwrap()).unwrap();
    let messages = vec![
        Message::new(MessageRole::System, vec![text_part("be brief")]),
        user_message("hello"),
    ];
    let body = host_body("deepseek", "deepseek-v4-flash", messages, options).await;
    assert_eq!(body["response_format"], json!({"type": "json_object"}));
    assert_eq!(
        body["messages"][0],
        json!({"role": "system", "content": format!("Return JSON that conforms to the following schema: {schema}")})
    );
    assert_eq!(body["messages"][1]["content"], "be brief");
}

#[tokio::test]
async fn hst_009_deepseek_rejects_penalties_seeds_and_sampling_while_it_reasons() {
    let penalty = sampling(|s| {
        s.repetition_penalty = Some(RepetitionPenaltyParams {
            presence_penalty: ProviderValue::Value(Some(0.5)),
            ..Default::default()
        })
    });
    let seed = sampling(|s| {
        s.seed = Some(SeedSamplerParams {
            seed: ProviderValue::Value(Some(Seed::Value(7))),
        })
    });
    let top_p = sampling(|s| {
        s.top_p = Some(TopPSamplerParams {
            top_p: ProviderValue::Value(Some(0.9)),
        })
    });
    validation("deepseek", "deepseek-chat", penalty).await;
    validation("deepseek", "deepseek-chat", seed).await;
    for (model_id, options) in [
        ("deepseek-v4-flash", temperature(0.2)),
        ("deepseek-v4-flash", top_p),
        ("deepseek-reasoner", temperature(0.2)),
        ("deepseek-chat", with_effort(temperature(0.2), Some("high"))),
    ] {
        let message = validation("deepseek", model_id, options).await;
        assert!(message.contains("reasoning"), "{model_id}: {message}");
    }
    for (model_id, options) in [
        ("deepseek-v4-flash", with_effort(temperature(0.2), None)),
        ("deepseek-chat", temperature(0.2)),
    ] {
        let body = host_body("deepseek", model_id, vec![user_message("hello")], options).await;
        assert_eq!(body["temperature"], 0.2, "{model_id}");
    }
}

#[tokio::test]
async fn hst_010_deepseek_reasoning_history_follows_the_model_version() {
    let history = vec![
        user_message("first"),
        assistant(vec![text_part("no reasoning here")]),
        user_message("second"),
        assistant(vec![
            reasoning_part("before the call"),
            tool_call("call_1", "lookup"),
        ]),
        tool_message(ToolResultPart::new("call_1", "lookup", json!({"value": 1}))),
    ];
    let options = StreamOptions {
        tools: tools(&["lookup"]),
        ..Default::default()
    };
    let body = host_body(
        "deepseek",
        "deepseek-v4-pro",
        history.clone(),
        options.clone(),
    )
    .await;
    let turns = assistants(&body);
    assert_eq!(turns[0]["reasoning_content"], "", "{body}");
    assert_eq!(turns[1]["reasoning_content"], "before the call");
    let mut legacy_history = history.clone();
    legacy_history[1] = assistant(vec![
        reasoning_part("old reasoning"),
        text_part("old answer"),
    ]);
    let body = host_body("deepseek", "deepseek-chat", legacy_history, options).await;
    let turns = assistants(&body);
    assert!(turns[0].get("reasoning_content").is_none(), "{body}");
    assert_eq!(turns[1]["reasoning_content"], "before the call");
}

#[tokio::test]
async fn hst_010_deepseek_takes_only_some_images() {
    for media_type in ["application/pdf", "image/bmp", "audio/mpeg"] {
        let file = FilePart::from_bytes(vec![1, 2, 3], Some(media_type), None).unwrap();
        let error = rejected(
            "deepseek",
            "deepseek-v4-pro",
            vec![user_file(file)],
            Default::default(),
        )
        .await;
        assert!(
            matches!(error, Error::Validation(_)),
            "{media_type}: {error:?}"
        );
    }
    let file = FilePart::from_bytes(vec![1, 2, 3], Some("image/png"), None).unwrap();
    let body = host_body(
        "deepseek",
        "deepseek-v4-pro",
        vec![user_file(file)],
        Default::default(),
    )
    .await;
    assert_eq!(
        body["messages"][0]["content"][1],
        json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,AQID"}})
    );
}

fn content_result(texts: &[&str]) -> ToolResultPart {
    ToolResultPart::new(
        "call_1",
        "lookup",
        ToolResultValue::Special(SpecialToolResult::Content(ContentOutput {
            output_type: ContentOutputType::Content,
            value: texts
                .iter()
                .map(|t| {
                    ContentPart::Text(TextPart {
                        id: format!("c-{t}"),
                        text: (*t).into(),
                        provider_metadata: None,
                    })
                })
                .collect(),
        })),
    )
}
fn tool_round(result: ToolResultPart) -> Vec<Message> {
    vec![
        user_message("hello"),
        assistant(vec![tool_call("call_1", "lookup")]),
        tool_message(result),
    ]
}
fn tool_entry(body: &Value) -> &Value {
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .unwrap()
}

#[tokio::test]
async fn hst_010_deepseek_sends_a_text_only_content_result_as_a_string() {
    let options = StreamOptions {
        tools: tools(&["lookup"]),
        ..Default::default()
    };
    let body = host_body(
        "deepseek",
        "deepseek-v4-pro",
        tool_round(content_result(&["a", "b"])),
        options,
    )
    .await;
    let content = &tool_entry(&body)["content"];
    let text = content.as_str().unwrap_or_else(|| panic!("{content}"));
    let parts: Value = serde_json::from_str(text).unwrap();
    assert_eq!(parts.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn hst_010_deepseek_usage_and_finish_reasons() {
    for (usage, cached) in [
        (
            json!({"prompt_tokens":100,"completion_tokens":5,"prompt_cache_hit_tokens":60,"prompt_tokens_details":{"cached_tokens":80}}),
            60,
        ),
        (
            json!({"prompt_tokens":100,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":80}}),
            80,
        ),
    ] {
        let transport = Recorder::new([chat_sse(&[
            json!({"choices":[{"index":0,"delta":{"content":"hi"}}]}),
            json!({"choices":[{"index":0,"delta":{},"finish_reason":"insufficient_system_resource"}],"usage":usage}),
        ])]);
        let model = model("deepseek", "deepseek-v4-pro", transport);
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        assert_eq!(handle.usage().unwrap().cache_read_tokens, Some(cached));
        assert_eq!(handle.finish_reason(), Some("error"));
    }
}

#[tokio::test]
async fn hst_011_moonshot_reasoning_follows_the_model_family() {
    let limit = params(|p| {
        p.output = Some(OutputParams {
            max_tokens: Some(100),
            ..Default::default()
        })
    });
    let body = host_body("moonshotai", "kimi-k3", vec![user_message("hello")], limit).await;
    assert_eq!(body["max_completion_tokens"], 100);
    assert!(body.get("max_tokens").is_none());
    for (model_id, level, reasoning_effort, thinking) in [
        ("kimi-k3", Some("xhigh"), Some("max"), None),
        ("kimi-k3", Some("minimal"), Some("low"), None),
        ("kimi-next", Some("medium"), Some("high"), None),
        ("kimi-k2.6", Some("high"), None, Some("enabled")),
        ("kimi-k2.5", Some("low"), None, Some("enabled")),
        ("kimi-k2.5", None, None, Some("disabled")),
        ("kimi-k2.6", None, None, Some("disabled")),
        ("kimi-k2.7-code", Some("high"), None, None),
        ("moonshot-v1-8k", None, None, None),
    ] {
        let body = host_body(
            "moonshotai",
            model_id,
            vec![user_message("hello")],
            effort(level),
        )
        .await;
        assert_eq!(
            body.get("reasoning_effort").and_then(Value::as_str),
            reasoning_effort,
            "{model_id} {level:?}: {body}"
        );
        assert_eq!(
            body["thinking"]["type"].as_str(),
            thinking,
            "{model_id} {level:?}: {body}"
        );
    }
}

#[tokio::test]
async fn hst_011_moonshot_rejects_what_a_model_does_not_take() {
    let required = StreamOptions {
        tools: tools(&["lookup"]),
        tool_choice: Some(ToolChoice::Required),
        ..Default::default()
    };
    let penalty = |frequency: bool| {
        sampling(|s| {
            let value = ProviderValue::Value(Some(0.5));
            s.repetition_penalty = Some(if frequency {
                RepetitionPenaltyParams {
                    frequency_penalty: value,
                    ..Default::default()
                }
            } else {
                RepetitionPenaltyParams {
                    presence_penalty: value,
                    ..Default::default()
                }
            })
        })
    };
    let top_p = sampling(|s| {
        s.top_p = Some(TopPSamplerParams {
            top_p: ProviderValue::Value(Some(0.9)),
        })
    });
    let seed = sampling(|s| {
        s.seed = Some(SeedSamplerParams {
            seed: ProviderValue::Value(Some(Seed::Value(7))),
        })
    });
    for (model_id, options) in [
        ("kimi-k3", effort(None)),
        ("kimi-k2.7-code", effort(None)),
        ("kimi-next", effort(None)),
        ("moonshot-v1-8k", effort(Some("high"))),
        ("moonshot-v1-8k", seed),
        ("kimi-k2.5", temperature(0.5)),
        ("kimi-k3", top_p),
        ("kimi-k2.6", penalty(true)),
        ("kimi-k2.7-code-highspeed", penalty(false)),
        ("kimi-k2.6", required.clone()),
        ("kimi-k2.7-code", required.clone()),
        ("kimi-k2.7-code-highspeed", required.clone()),
    ] {
        validation("moonshotai", model_id, options).await;
    }
    let body = host_body(
        "moonshotai",
        "moonshot-v1-8k",
        vec![user_message("hello")],
        temperature(0.5),
    )
    .await;
    assert_eq!(body["temperature"], 0.5);
    let body = host_body(
        "moonshotai",
        "kimi-k3",
        vec![user_message("hello")],
        required,
    )
    .await;
    assert_eq!(body["tool_choice"], "required");
}

/// A schema with a root `$schema`, an optional property, a tuple and a type
/// next to `anyOf`.
fn mfjs_input() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "pair": {"type": "array", "items": [{"type": "string"}, {"type": "integer"}]},
            "maybe": {"type": "string", "anyOf": [{"minLength": 1}, {"type": "null"}]}
        },
        "required": ["pair"]
    })
}
fn mfjs_output() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "pair": {"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}]},
            "maybe": {"anyOf": [{"type": "string", "minLength": 1}, {"type": "null"}]}
        },
        "required": ["pair"]
    })
}

#[tokio::test]
async fn hst_012_moonshot_schemas_go_out_as_mfjs() {
    let tool = Tool::function(
        "lookup",
        ToolSpec {
            description: Some("Look a value up".into()),
            params: mfjs_input().try_into().unwrap(),
        },
    );
    let options = StreamOptions {
        tools: vec![tool],
        output_schema: Some(mfjs_input().try_into().unwrap()),
        output_type_name: Some("answer".into()),
        ..Default::default()
    };
    let body = host_body(
        "moonshotai",
        "kimi-k3",
        vec![user_message("hello")],
        options,
    )
    .await;
    assert_eq!(
        body["tools"][0]["function"]["parameters"],
        mfjs_output(),
        "{body}"
    );
    assert_eq!(
        body["response_format"],
        json!({"type": "json_schema", "json_schema": {"name": "answer", "strict": true, "schema": mfjs_output()}})
    );
    let tool = Tool::function(
        "lookup",
        ToolSpec {
            description: None,
            params: json!({"type": "string"}).try_into().unwrap(),
        },
    );
    let options = StreamOptions {
        tools: vec![tool],
        ..Default::default()
    };
    validation("moonshotai", "kimi-k3", options).await;
}

/// Answers downloads with the bytes 1, 2, 3 as a JPEG image, or with the
/// text `downloaded words`, or fails them, and records the URLs.
#[derive(Default)]
struct Downloads {
    urls: Mutex<Vec<String>>,
    /// The headers of each request, in the order of `urls`.
    headers: Mutex<Vec<Vec<(String, String)>>>,
    text: bool,
    fail: bool,
}
impl suprnova_ai::models::files::DownloadTransport for Downloads {
    fn resolve<'a>(
        &'a self,
        _host: &'a str,
        _port: u16,
    ) -> BoxFuture<'a, Result<Vec<std::net::IpAddr>, suprnova_ai::models::files::DownloadError>>
    {
        Box::pin(async { Ok(vec![std::net::IpAddr::from([93, 184, 216, 34])]) })
    }
    fn send_pinned(
        &self,
        request: HttpRequest,
        _addresses: Vec<std::net::IpAddr>,
    ) -> BoxFuture<'_, Result<HttpResponse, suprnova_ai::models::files::DownloadError>> {
        self.urls.lock().unwrap().push(request.url);
        self.headers.lock().unwrap().push(request.headers);
        let (status, content_type, body): (u16, &str, &'static [u8]) = if self.fail {
            (404, "text/plain", b"missing")
        } else if self.text {
            (200, "text/plain", b"downloaded words")
        } else {
            (200, "image/jpeg", &[1, 2, 3])
        };
        Box::pin(async move {
            Ok(HttpResponse {
                status,
                headers: vec![("content-type".into(), content_type.into())],
                body: stream::iter([Ok(Bytes::from_static(body))]).boxed(),
            })
        })
    }
}

#[tokio::test]
async fn hst_013_moonshot_media_is_sent_inline() {
    let video = FilePart::from_bytes(vec![1, 2, 3], Some("video/mp4"), None).unwrap();
    let body = host_body(
        "moonshotai",
        "kimi-k3",
        vec![user_file(video)],
        Default::default(),
    )
    .await;
    assert_eq!(
        body["messages"][0]["content"][1],
        json!({"type": "video_url", "video_url": {"url": "data:video/mp4;base64,AQID"}})
    );
    let text = FilePart::from_bytes(b"plain words".to_vec(), Some("text/plain"), None).unwrap();
    let body = host_body(
        "moonshotai",
        "kimi-k3",
        vec![user_file(text)],
        Default::default(),
    )
    .await;
    assert_eq!(
        body["messages"][0]["content"][1],
        json!({"type": "text", "text": "plain words"})
    );
    for media_type in ["application/pdf", "audio/mpeg"] {
        let file = FilePart::from_bytes(vec![1, 2, 3], Some(media_type), None).unwrap();
        let error = rejected(
            "moonshotai",
            "kimi-k3",
            vec![user_file(file)],
            Default::default(),
        )
        .await;
        assert!(
            matches!(error, Error::Validation(_)),
            "{media_type}: {error:?}"
        );
    }
    let image = || FilePart::from_url("https://images.test/cat.jpg", Some("image/jpeg")).unwrap();
    let error = rejected(
        "moonshotai",
        "kimi-k3",
        vec![user_file(image())],
        Default::default(),
    )
    .await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");

    // With a download transport the URL is fetched and sent inline, also by
    // a model restored from a snapshot.
    let transport = Recorder::new([chat_text("hi", "stop"), chat_text("hi", "stop")]);
    let downloads = Arc::new(Downloads::default());
    let mut registry = ProviderRegistry::new();
    moonshotai::register_with_downloads(
        &mut registry,
        key_config(),
        transport.clone(),
        downloads.clone(),
    )
    .unwrap();
    let model = Model::new("kimi-k3", registry.get_provider("moonshotai").unwrap());
    let restored = Model::from_value(model.to_value().unwrap(), &registry).unwrap();
    for model in [model, restored] {
        let (_, events) = call(&model, vec![user_file(image())], Default::default()).await;
        ok(&events);
    }
    assert_eq!(
        *downloads.urls.lock().unwrap(),
        ["https://images.test/cat.jpg", "https://images.test/cat.jpg"]
    );
    for request in transport.requests.lock().unwrap().iter() {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(
            body["messages"][0]["content"][1],
            json!({"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AQID"}})
        );
    }
}

#[tokio::test]
async fn hst_013_moonshot_assistant_and_tool_messages() {
    let options = StreamOptions {
        tools: tools(&["lookup"]),
        ..Default::default()
    };
    let result = ToolResultPart::new("call_1", "lookup", json!({"value": 1}));
    let body = host_body("moonshotai", "kimi-k3", tool_round(result), options.clone()).await;
    let turn = assistants(&body)[0];
    assert!(turn.get("content").is_some_and(Value::is_null), "{turn}");
    assert_eq!(tool_entry(&body)["content"], "{\"value\":1}");
    let body = host_body(
        "moonshotai",
        "kimi-k3",
        tool_round(content_result(&["a"])),
        options,
    )
    .await;
    assert!(tool_entry(&body)["content"].is_string(), "{body}");
}

#[tokio::test]
async fn hst_013_moonshot_usage_can_come_in_the_choice() {
    let choice_usage = json!({"prompt_tokens":12,"completion_tokens":5,"total_tokens":17});
    let top_usage =
        json!({"prompt_tokens":99,"completion_tokens":33,"total_tokens":132,"cached_tokens":10});
    let in_choice =
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop","usage":choice_usage}]});
    let top = json!({"choices":[],"usage":top_usage});
    let text = json!({"choices":[{"index":0,"delta":{"content":"hi"}}]});
    for (chunks, input, output, cached) in [
        (vec![text.clone(), in_choice.clone()], 12, 5, None),
        (
            vec![text.clone(), in_choice.clone(), top.clone()],
            99,
            33,
            Some(10),
        ),
        (
            vec![text.clone(), top.clone(), in_choice.clone()],
            99,
            33,
            Some(10),
        ),
    ] {
        let transport = Recorder::new([chat_sse(&chunks)]);
        let model = model("moonshotai", "kimi-k2.5", transport);
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        let usage = handle.usage().unwrap();
        assert_eq!((usage.input_tokens, usage.output_tokens), (input, output));
        assert_eq!(usage.cache_read_tokens, cached);
    }
}

#[tokio::test]
async fn hst_014_mistral_request_fields_reasoning_and_schema() {
    let options = sampling(|s| {
        s.seed = Some(SeedSamplerParams {
            seed: ProviderValue::Value(Some(Seed::Value(7))),
        })
    });
    let mut options = options;
    options.params.as_mut().unwrap().output = Some(OutputParams {
        max_tokens: Some(100),
        ..Default::default()
    });
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        options,
    )
    .await;
    assert_eq!(body["max_tokens"], 100);
    assert_eq!(body["random_seed"], 7);
    assert!(body.get("seed").is_none(), "{body}");
    for (level, sent) in [
        (Some("low"), "high"),
        (Some("max"), "high"),
        (Some("high"), "high"),
        (Some("auto"), "high"),
        (None, "none"),
    ] {
        let body = host_body(
            "mistral",
            "mistral-medium-latest",
            vec![user_message("hello")],
            effort(level),
        )
        .await;
        assert_eq!(body["reasoning_effort"], sent, "{level:?}");
    }
    for level in [Some("high"), None] {
        validation("mistral", "mistral-large-latest", effort(level)).await;
    }
    let schema = json!({"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}},"required":["a"]});
    let options = StreamOptions {
        output_schema: Some(schema.clone().try_into().unwrap()),
        output_type_name: Some("answer".into()),
        ..Default::default()
    };
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        options,
    )
    .await;
    assert_eq!(
        body["response_format"],
        json!({"type": "json_schema", "json_schema": {"name": "answer", "strict": false, "schema": schema}})
    );
}

#[tokio::test]
async fn hst_014_mistral_tool_choice() {
    let choose = |choice: ToolChoice| StreamOptions {
        tools: tools(&["a", "b"]),
        tool_choice: Some(choice),
        ..Default::default()
    };
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        choose(ToolChoice::Required),
    )
    .await;
    assert_eq!(body["tool_choice"], "any");
    assert_eq!(body["tools"].as_array().unwrap().len(), 2);
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        choose(ToolChoice::Tool("b".into())),
    )
    .await;
    assert_eq!(body["tool_choice"], "any");
    let names: Vec<_> = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].clone())
        .collect();
    assert_eq!(names, [json!("b")]);
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        choose(ToolChoice::None),
    )
    .await;
    assert_eq!(body["tool_choice"], "none");
    let no_tools = params(|p| {
        p.tool_calling = Some(ToolCallingParams {
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: ProviderValue::Value(true),
            max_tool_calls: ProviderValue::Default,
        })
    });
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        no_tools,
    )
    .await;
    for key in ["tools", "tool_choice", "parallel_tool_calls"] {
        assert!(body.get(key).is_none(), "{key}: {body}");
    }
}

#[tokio::test]
async fn hst_015_mistral_messages() {
    let history = vec![
        user_message("first"),
        assistant(vec![reasoning_part("R"), text_part("T")]),
        user_message("second"),
        assistant(vec![tool_call("call_1", "t")]),
        tool_message(ToolResultPart::new("call_1", "t", json!({"value": 1}))),
        assistant(vec![text_part("The answer is")]),
    ];
    let options = StreamOptions {
        tools: tools(&["t"]),
        ..Default::default()
    };
    let body = host_body("mistral", "mistral-medium-latest", history, options).await;
    let turns = assistants(&body);
    assert_eq!(
        turns[0]["content"],
        json!([
            {"type": "thinking", "thinking": [{"type": "text", "text": "R"}], "closed": true},
            {"type": "text", "text": "T"}
        ])
    );
    for turn in &turns {
        assert!(turn.get("reasoning_content").is_none(), "{turn}");
    }
    assert_eq!(turns[1]["content"], "");
    assert_eq!(turns[1]["tool_calls"][0]["id"], "call_1");
    assert!(turns[0].get("prefix").is_none() && turns[1].get("prefix").is_none());
    assert_eq!(turns[2]["prefix"], true);
    let tool = tool_entry(&body);
    assert_eq!(tool["name"], "t");
    assert_eq!(tool["content"], "{\"value\":1}");
}

#[tokio::test]
async fn hst_015_mistral_files() {
    for (file, part) in [
        (
            FilePart::from_bytes(vec![1, 2, 3], Some("image/png"), None).unwrap(),
            json!({"type": "image_url", "image_url": "data:image/png;base64,AQID"}),
        ),
        (
            FilePart::from_bytes(vec![1, 2, 3], Some("application/pdf"), None).unwrap(),
            json!({"type": "document_url", "document_url": "data:application/pdf;base64,AQID"}),
        ),
        (
            FilePart::from_url("https://docs.test/a.pdf", Some("application/pdf")).unwrap(),
            json!({"type": "document_url", "document_url": "https://docs.test/a.pdf"}),
        ),
    ] {
        let body = host_body(
            "mistral",
            "mistral-small-latest",
            vec![user_file(file)],
            Default::default(),
        )
        .await;
        assert_eq!(body["messages"][0]["content"][1], part);
    }
    for media_type in ["audio/mpeg", "text/plain"] {
        let file = FilePart::from_bytes(vec![1, 2, 3], Some(media_type), None).unwrap();
        let error = rejected(
            "mistral",
            "mistral-small-latest",
            vec![user_file(file)],
            Default::default(),
        )
        .await;
        assert!(
            matches!(error, Error::Validation(_)),
            "{media_type}: {error:?}"
        );
    }
}

#[tokio::test]
async fn hst_016_mistral_content_parts_become_separate_blocks() {
    let transport = Recorder::new([chat_sse(&[
        json!({"choices":[{"index":0,"delta":{"role":"assistant","content":[{"type":"thinking","thinking":[{"type":"text","text":"A"}]}]}}]}),
        json!({"choices":[{"index":0,"delta":{"content":[{"type":"text","text":"B"},{"type":"reference","reference_ids":[1]}]}}]}),
        json!({"choices":[{"index":0,"delta":{"content":[{"type":"thinking","thinking":[{"type":"text","text":"C"}]}]}}]}),
        json!({"choices":[{"index":0,"delta":{"content":"D"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":4,"num_cached_tokens":5}}),
    ])]);
    let mistral = model("mistral", "mistral-medium-latest", transport);
    let (handle, events) = call(&mistral, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut open: Option<String> = None;
    for event in events.iter().flatten() {
        match event {
            Event::ReasoningStart(start) => {
                assert!(open.is_none(), "a block was still open: {open:?}");
                open = Some(start.block_id.clone());
                blocks.push(("reasoning".into(), String::new()));
            }
            Event::TextStart(start) => {
                assert!(open.is_none(), "a block was still open: {open:?}");
                open = Some(start.block_id.clone());
                blocks.push(("text".into(), String::new()));
            }
            Event::ReasoningDelta(delta) => blocks.last_mut().unwrap().1.push_str(&delta.chunk),
            Event::TextDelta(delta) => blocks.last_mut().unwrap().1.push_str(&delta.chunk),
            Event::ReasoningEnd(end) => {
                assert_eq!(open.take().as_deref(), Some(end.block_id.as_str()));
            }
            Event::TextEnd(end) => {
                assert_eq!(open.take().as_deref(), Some(end.block_id.as_str()));
            }
            _ => {}
        }
    }
    let expected: Vec<(String, String)> = [
        ("reasoning", "A"),
        ("text", "B"),
        ("reasoning", "C"),
        ("text", "D"),
    ]
    .iter()
    .map(|(k, t)| ((*k).into(), (*t).into()))
    .collect();
    assert_eq!(blocks, expected);
    assert_eq!(handle.usage().unwrap().cache_read_tokens, Some(5));
    for (raw, normalized) in [("model_length", "length"), ("error", "error")] {
        let transport = Recorder::new([chat_text("partial", raw)]);
        let model = model("mistral", "mistral-medium-latest", transport);
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        assert_eq!(handle.finish_reason(), Some(normalized), "{raw}");
        assert_eq!(handle.text(), "partial");
    }
}

#[tokio::test]
async fn hst_017_tool_calls_without_an_index_stay_apart() {
    let fixture = chat_sse(&[
        json!({"choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"id":"weather_0","type":"function","function":{"name":"weather","arguments":""}},{"id":"time_1","type":"function","function":{"name":"time","arguments":""}}]}}]}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":"{\"location\":\"San Francisco\"}"}},{"function":{"arguments":"{\"zone\":\"UTC\"}"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ]);
    let separate_chunks = chat_sse(&[
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"id":"a","function":{"name":"lookup","arguments":"{\"q\":1}"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"id":"b","function":{"name":"lookup","arguments":"{\"q\":2}"}}]},"finish_reason":"tool_calls"}]}),
    ]);
    for (name, reply, expected) in [
        (
            "moonshotai",
            fixture,
            vec![
                ("weather_0", "weather", "{\"location\":\"San Francisco\"}"),
                ("time_1", "time", "{\"zone\":\"UTC\"}"),
            ],
        ),
        (
            "mistral",
            separate_chunks,
            vec![("a", "lookup", "{\"q\":1}"), ("b", "lookup", "{\"q\":2}")],
        ),
    ] {
        let transport = Recorder::new([reply]);
        let model = model(name, "m", transport);
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        let calls: Vec<_> = handle
            .tool_calls()
            .iter()
            .map(|c| {
                (
                    c.tool_call_id.clone(),
                    c.tool_name.clone(),
                    c.tool_args.clone(),
                )
            })
            .collect();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(id, n, a)| (id.to_owned(), n.to_owned(), a.to_owned()))
            .collect();
        assert_eq!(calls, expected, "{name}");
    }
}

#[tokio::test]
async fn hst_017_a_malformed_tool_call_index_is_a_response_error() {
    let transport = Recorder::new([chat_sse(&[
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":"not-a-number","id":"a","function":{"name":"lookup","arguments":"{}"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ])]);
    let model = model("cerebras", "m", transport);
    let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    let Some(Error::Provider(error)) = events.into_iter().find_map(Result::err) else {
        panic!("the call did not fail with a provider error");
    };
    assert_eq!(error.kind, ProviderErrorKind::Response, "{error:?}");
}

/// The error a stream that sends this error event first ends the call with,
/// with retries off.
async fn stream_error(name: &str, event: Value) -> suprnova_ai::errors::ProviderError {
    let transport = Recorder::new([chat_sse(&[event])]);
    let model = model(name, "m", transport);
    let mut options = StreamOptions::default();
    options.retry.max_retries = 0;
    let (_, events) = call(&model, vec![user_message("hello")], options).await;
    match events.into_iter().find_map(Result::err) {
        Some(Error::Provider(error)) => *error,
        other => panic!("{name}: {other:?}"),
    }
}

#[tokio::test]
async fn hst_018_stream_errors_get_a_kind_from_their_code_and_type() {
    let error = stream_error(
        "deepseek",
        json!({"error":{"message":"m","type":"rate_limit_error","code":"rate_limit_exceeded"}}),
    )
    .await;
    assert_eq!(error.message, "m");
    assert_eq!(error.code.as_deref(), Some("rate_limit_exceeded"));
    assert_eq!(error.error_type.as_deref(), Some("rate_limit_error"));
    assert_eq!(error.kind, ProviderErrorKind::RateLimit);
    assert!(error.is_retryable);
    let error = stream_error(
        "deepseek",
        json!({"error":{"message":"m","code":"insufficient_quota"}}),
    )
    .await;
    assert_eq!(error.kind, ProviderErrorKind::RateLimit);
    assert!(!error.is_retryable);
    for code in [json!(503), json!("503")] {
        let error = stream_error("deepseek", json!({"error":{"message":"m","code":code}})).await;
        assert_eq!(error.kind, ProviderErrorKind::ServiceUnavailable, "{code}");
        assert!(error.is_retryable);
    }
    let error = stream_error(
        "moonshotai",
        json!({"error":{"message":"m","type":"server_error"}}),
    )
    .await;
    assert_eq!(error.kind, ProviderErrorKind::InternalServer);
    assert!(error.is_retryable);
    let error = stream_error(
        "mistral",
        json!({"object":"error","message":"m","type":"invalid_request_error","param":null,"code":"3230"}),
    )
    .await;
    assert_eq!(error.message, "m");
    assert_eq!(error.kind, ProviderErrorKind::BadRequest);
    assert!(!error.is_retryable);
    let error = stream_error(
        "deepseek",
        json!({"error":{"message":"m","type":"unknown_error"}}),
    )
    .await;
    assert_eq!(error.kind, ProviderErrorKind::Response);
    assert!(!error.is_retryable);
}

#[tokio::test(start_paused = true)]
async fn hst_018_a_retryable_error_first_in_the_stream_is_retried() {
    let transport = Recorder::new([
        chat_sse(&[json!({"error":{"message":"busy","type":"overloaded_error"}})]),
        chat_text("hi", "stop"),
    ]);
    let model = model("deepseek", "deepseek-v4-pro", transport.clone());
    let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    assert_eq!(handle.text(), "hi");
    assert_eq!(transport.count(), 2);
}

#[tokio::test]
async fn hst_014_mistral_rejects_a_named_tool_it_was_not_given() {
    let options = StreamOptions {
        tools: tools(&["a"]),
        tool_choice: Some(ToolChoice::Tool("b".into())),
        ..Default::default()
    };
    validation("mistral", "mistral-small-latest", options).await;
}

/// A Moonshot AI model whose provider has this download transport.
fn moonshot_with(downloads: Arc<Downloads>, transport: Arc<Recorder>) -> Model {
    let mut registry = ProviderRegistry::new();
    moonshotai::register_with_downloads(&mut registry, key_config(), transport, downloads).unwrap();
    Model::new("kimi-k3", registry.get_provider("moonshotai").unwrap())
}

#[tokio::test]
async fn hst_013_a_rejected_call_downloads_nothing() {
    let image = || FilePart::from_url("https://images.test/cat.jpg", Some("image/jpeg")).unwrap();
    let pdf = FilePart::from_bytes(vec![1, 2, 3], Some("application/pdf"), None).unwrap();
    let tiff = FilePart::from_url("https://images.test/cat.tiff", Some("image/tiff")).unwrap();
    let service_tier = params(|p| {
        p.provider_service = Some(ProviderServiceParams {
            service_tier: ProviderValue::Value("flex".into()),
        })
    });
    let string_root = StreamOptions {
        tools: vec![Tool::function(
            "lookup",
            ToolSpec {
                description: None,
                params: json!({"type": "string"}).try_into().unwrap(),
            },
        )],
        ..Default::default()
    };
    let cases = [
        ("service tier", vec![user_file(image())], service_tier),
        (
            "seed",
            vec![user_file(image())],
            sampling(|s| {
                s.seed = Some(SeedSamplerParams {
                    seed: ProviderValue::Value(Some(Seed::Value(7))),
                })
            }),
        ),
        ("string root", vec![user_file(image())], string_root),
        (
            "pdf",
            vec![Message::new(
                MessageRole::User,
                vec![Part::File(image()), Part::File(pdf)],
            )],
            Default::default(),
        ),
        ("tiff", vec![user_file(tiff)], Default::default()),
    ];
    for (case, messages, options) in cases {
        let downloads = Arc::new(Downloads {
            fail: true,
            ..Default::default()
        });
        let transport = Recorder::new([chat_text("hi", "stop")]);
        let model = moonshot_with(downloads.clone(), transport.clone());
        let (_, events) = call(&model, messages, options).await;
        let error = events.into_iter().find_map(Result::err);
        assert!(
            matches!(error, Some(Error::Validation(_))),
            "{case}: {error:?}"
        );
        assert!(
            downloads.urls.lock().unwrap().is_empty(),
            "{case}: a download was sent"
        );
        assert_eq!(transport.count(), 0, "{case}: a request was sent");
    }
}

#[tokio::test]
async fn hst_013_a_moonshot_text_url_is_downloaded_or_rejected() {
    let text = || FilePart::from_url("https://docs.test/notes.txt", Some("text/plain")).unwrap();
    let error = rejected(
        "moonshotai",
        "kimi-k3",
        vec![user_file(text())],
        Default::default(),
    )
    .await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    let downloads = Arc::new(Downloads {
        text: true,
        ..Default::default()
    });
    let transport = Recorder::new([chat_text("hi", "stop")]);
    let model = moonshot_with(downloads.clone(), transport.clone());
    let (_, events) = call(&model, vec![user_file(text())], Default::default()).await;
    ok(&events);
    assert_eq!(
        *downloads.urls.lock().unwrap(),
        ["https://docs.test/notes.txt"]
    );
    assert_eq!(
        transport.body()["messages"][0]["content"][1],
        json!({"type": "text", "text": "downloaded words"})
    );
}

#[tokio::test]
async fn hst_012_moonshot_normalizes_tuples_in_every_subschema() {
    let tuple = || json!({"type": "array", "items": [{"type": "string"}, {"type": "integer"}]});
    let prefix =
        || json!({"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}]});
    let schema = json!({
        "type": "object",
        "properties": {"list": {"type": "array", "additionalItems": tuple()}},
        "definitions": {"Pair": tuple()},
        "dependencies": {"list": {"properties": {"pair": tuple()}}, "other": ["list"]}
    });
    let tool = Tool::function(
        "lookup",
        ToolSpec {
            description: None,
            params: schema.try_into().unwrap(),
        },
    );
    let options = StreamOptions {
        tools: vec![tool],
        ..Default::default()
    };
    let body = host_body(
        "moonshotai",
        "kimi-k3",
        vec![user_message("hello")],
        options,
    )
    .await;
    let sent = &body["tools"][0]["function"]["parameters"];
    assert_eq!(
        sent["properties"]["list"]["additionalItems"],
        prefix(),
        "{sent}"
    );
    assert_eq!(sent["definitions"]["Pair"], prefix(), "{sent}");
    assert_eq!(
        sent["dependencies"]["list"]["properties"]["pair"],
        prefix(),
        "{sent}"
    );
    assert_eq!(sent["dependencies"]["other"], json!(["list"]));
}

// HST-019 to HST-025: xAI, Hugging Face and every Responses provider.

/// The body a Responses host sends; the call must succeed.
async fn responses_body(
    name: &str,
    model_id: &str,
    messages: Vec<Message>,
    options: StreamOptions,
) -> Value {
    let transport = Recorder::new([responses_text("{\"value\":1}")]);
    let model = model(name, model_id, transport.clone());
    let (_, events) = call(&model, messages, options).await;
    ok(&events);
    transport.body()
}
fn metadata_under(key: &str, value: Value) -> Option<suprnova_ai::types::messages::Metadata> {
    Some([(key.to_owned(), value)].into())
}
fn items(body: &Value) -> &Vec<Value> {
    body["input"].as_array().unwrap()
}
fn holds_reference(body: &Value) -> bool {
    items(body)
        .iter()
        .any(|item| item["type"] == "item_reference")
}

#[tokio::test]
async fn hst_019_models_dev_metadata_for_xai_and_hugging_face_gets_their_rules() {
    for (id, npm, host) in [
        ("xai", "@ai-sdk/openai-compatible", "xai"),
        ("hf", "@ai-sdk/huggingface", "huggingface"),
    ] {
        let options = || {
            let mut options = effort(Some("minimal"));
            options.tools = tools(&["lookup"]);
            options.tool_choice = Some(ToolChoice::Tool("lookup".into()));
            options
        };
        let transport = Recorder::new([responses_text("hi")]);
        let metadata = ModelsDevProvider {
            id: id.into(),
            npm: Some(npm.into()),
            api: Some("https://host.test/v1".into()),
            env: vec![],
        };
        let provider = OpenAICompatibleProvider::from_modelsdev_provider(
            &metadata,
            key_config(),
            transport.clone(),
        )
        .unwrap();
        let (_, events) = call(
            &Model::new("m", Arc::new(provider)),
            vec![user_message("hello")],
            options(),
        )
        .await;
        ok(&events);
        assert_eq!(
            transport.request().url,
            "https://host.test/v1/responses",
            "{id}"
        );
        let expected = responses_body(host, "m", vec![user_message("hello")], options()).await;
        assert_eq!(transport.body(), expected, "{id}");
    }
}

#[tokio::test]
async fn hst_020_xai_sends_its_sampling_settings() {
    let options = sampling(|s| {
        s.top_k = Some(TopKSamplerParams {
            top_k: ProviderValue::Value(Some(40)),
        });
        s.min_p = Some(MinPSamplerParams {
            min_p: ProviderValue::Value(Some(0.1)),
        });
        s.seed = Some(SeedSamplerParams {
            seed: ProviderValue::Value(Some(Seed::Value(7))),
        });
    });
    let body = responses_body("xai", "grok-4.7", vec![user_message("hello")], options).await;
    assert_eq!(
        (
            body["top_k"].clone(),
            body["min_p"].clone(),
            body["seed"].clone()
        ),
        (json!(40), json!(0.1), json!(7)),
        "{body}"
    );
}

#[tokio::test]
async fn hst_020_each_responses_host_rejects_the_settings_it_never_sends() {
    let output = |update: fn(&mut OutputParams)| {
        params(|p| {
            let mut output = OutputParams::default();
            update(&mut output);
            p.output = Some(output);
        })
    };
    let tool_calling = |update: fn(&mut ToolCallingParams)| {
        let mut options = params(|p| {
            let mut calling = ToolCallingParams {
                tool_choice: ToolChoice::Auto,
                parallel_tool_calls: ProviderValue::Default,
                max_tool_calls: ProviderValue::Default,
            };
            update(&mut calling);
            p.tool_calling = Some(calling);
        });
        options.tools = tools(&["lookup"]);
        options
    };
    let cases: Vec<(&str, StreamOptions, &[&str])> = vec![
        (
            "frequency penalty",
            sampling(|s| {
                s.repetition_penalty = Some(RepetitionPenaltyParams {
                    frequency_penalty: ProviderValue::Value(Some(0.5)),
                    ..Default::default()
                })
            }),
            &["xai", "huggingface"],
        ),
        (
            "presence penalty",
            sampling(|s| {
                s.repetition_penalty = Some(RepetitionPenaltyParams {
                    presence_penalty: ProviderValue::Value(Some(0.5)),
                    ..Default::default()
                })
            }),
            &["xai", "huggingface"],
        ),
        (
            "reasoning summary",
            output(|o| o.reasoning_summary = ProviderValue::Value(Some("auto".into()))),
            &["xai", "huggingface"],
        ),
        (
            "text verbosity",
            output(|o| o.text_verbosity = ProviderValue::Value(Some("low".into()))),
            &["xai", "huggingface"],
        ),
        (
            "max tool calls",
            tool_calling(|t| t.max_tool_calls = ProviderValue::Value(Some(3))),
            &["xai", "huggingface"],
        ),
        (
            "context management",
            params(|p| {
                p.context_management = Some(ContextManagementParams {
                    compaction: Some(TokenThreshold(1000)),
                })
            }),
            &["xai", "huggingface"],
        ),
        (
            "cache retention",
            params(|p| {
                p.cache = Some(CacheParams {
                    retention: ProviderValue::Value("24h".into()),
                    ..Default::default()
                })
            }),
            &["xai", "huggingface"],
        ),
        (
            "metadata",
            params(|p| p.metadata = Some([("k".to_owned(), "v".to_owned())].into())),
            &["xai"],
        ),
        (
            "tool selection",
            StreamOptions {
                tools: tools(&["lookup"]),
                tool_choice: Some(ToolChoice::Selection(ToolSelection::new(
                    ["lookup"],
                    ToolChoiceMode::Auto,
                ))),
                ..Default::default()
            },
            &["xai", "huggingface"],
        ),
        (
            "top-k",
            sampling(|s| {
                s.top_k = Some(TopKSamplerParams {
                    top_k: ProviderValue::Value(Some(40)),
                })
            }),
            &["huggingface"],
        ),
        (
            "min-p",
            sampling(|s| {
                s.min_p = Some(MinPSamplerParams {
                    min_p: ProviderValue::Value(Some(0.1)),
                })
            }),
            &["huggingface"],
        ),
        (
            "seed",
            sampling(|s| {
                s.seed = Some(SeedSamplerParams {
                    seed: ProviderValue::Value(Some(Seed::Value(7))),
                })
            }),
            &["huggingface"],
        ),
        (
            "include",
            output(|o| o.include = Some(["reasoning.encrypted_content".to_owned()].into())),
            &["huggingface"],
        ),
        (
            "parallel tool calls",
            tool_calling(|t| t.parallel_tool_calls = ProviderValue::Value(false)),
            &["huggingface"],
        ),
        (
            "service tier",
            params(|p| {
                p.provider_service = Some(ProviderServiceParams {
                    service_tier: ProviderValue::Value("priority".into()),
                })
            }),
            &["huggingface"],
        ),
        (
            "safety identifier",
            params(|p| p.safety_identifier = Some("user-1".into())),
            &["huggingface"],
        ),
        (
            "cache key",
            params(|p| {
                p.cache = Some(CacheParams {
                    key: Some("k1".into()),
                    ..Default::default()
                })
            }),
            &["huggingface"],
        ),
    ];
    for (case, options, rejecting) in cases {
        for name in ["xai", "huggingface"] {
            if rejecting.contains(&name) {
                let message = validation(name, "m", options.clone()).await;
                assert!(
                    message.contains("does not support"),
                    "{case} {name}: {message}"
                );
            } else {
                responses_body(name, "m", vec![user_message("hello")], options.clone()).await;
            }
        }
    }
}

#[tokio::test]
async fn hst_021_xai_reasoning_store_and_tools() {
    for (model_id, level, sent) in [
        ("grok-4.7", Some("minimal"), "low"),
        ("grok-4.7", Some("medium"), "medium"),
        ("grok-4.7", Some("xhigh"), "high"),
        ("grok-4.7", Some("max"), "high"),
        ("grok-4.6", Some("xhigh"), "xhigh"),
        ("grok-4.6", Some("max"), "xhigh"),
        ("grok-4.7", None, "none"),
    ] {
        let body =
            responses_body("xai", model_id, vec![user_message("hello")], effort(level)).await;
        assert_eq!(body["reasoning"]["effort"], sent, "{model_id} {level:?}");
    }
    for model_id in ["grok-4.20-0309-reasoning", "grok-4.20-non-reasoning"] {
        for level in [Some("high"), None] {
            validation("xai", model_id, effort(level)).await;
        }
    }
    let store = |value: Option<bool>| {
        params(|p| {
            if let Some(value) = value {
                p.provider_params = Some([("xai".to_owned(), json!({"store": value}))].into());
            }
        })
    };
    let body = responses_body(
        "xai",
        "grok-4.7",
        vec![user_message("hello")],
        store(Some(false)),
    )
    .await;
    assert_eq!(body["store"], false, "{body}");
    assert!(
        body["include"]
            .as_array()
            .unwrap()
            .contains(&json!("reasoning.encrypted_content")),
        "{body}"
    );
    for value in [Some(true), None] {
        let body =
            responses_body("xai", "grok-4.7", vec![user_message("hello")], store(value)).await;
        assert!(body.get("store").is_none(), "{value:?}: {body}");
    }
    let named = StreamOptions {
        tools: tools(&["lookup"]),
        tool_choice: Some(ToolChoice::Tool("lookup".into())),
        ..Default::default()
    };
    let body = responses_body("xai", "grok-4.7", vec![user_message("hello")], named).await;
    assert_eq!(
        body["tool_choice"],
        json!({"type": "function", "name": "lookup"})
    );
    let provider_tool = StreamOptions {
        tools: vec![openai::tools::web_search(json!({})).unwrap()],
        ..Default::default()
    };
    let error = rejected(
        "xai",
        "grok-4.7",
        vec![user_message("hello")],
        provider_tool,
    )
    .await;
    assert!(matches!(error, Error::UnsupportedOperation(_)), "{error:?}");
}

#[tokio::test]
async fn hst_022_xai_history_has_its_own_items() {
    let reasoning = Part::Reasoning(ReasoningPart {
        id: "r1".into(),
        text: "R".into(),
        provider_metadata: metadata_under(
            "xai",
            json!({"item_id":"rs_1","reasoning_encrypted_content":"enc"}),
        ),
    });
    let bare_reasoning = reasoning_part("no id");
    let text = Part::Text(TextPart {
        id: "t1".into(),
        text: "T".into(),
        provider_metadata: metadata_under("xai", json!({"item_id":"msg_1"})),
    });
    let mut call = ToolCallPart::new("call_1", "lookup", "{\"q\":1}");
    call.provider_metadata = metadata_under("xai", json!({"item_id":"fc_1"}));
    let history = vec![
        user_message("hello"),
        assistant(vec![bare_reasoning, reasoning, text, Part::ToolCall(call)]),
        tool_message(ToolResultPart::new("call_1", "lookup", json!({"value": 1}))),
    ];
    let options = StreamOptions {
        tools: tools(&["lookup"]),
        ..Default::default()
    };
    let body = responses_body("xai", "grok-4.7", history, options).await;
    assert!(!holds_reference(&body), "{body}");
    let input = items(&body);
    assert_eq!(
        input[1],
        json!({"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"R"}],"status":"completed","encrypted_content":"enc"})
    );
    assert_eq!(
        input[2],
        json!({"role":"assistant","content":"T","id":"msg_1"})
    );
    assert_eq!(
        input[3],
        json!({"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"{\"q\":1}","status":"completed"})
    );
    assert_eq!(input[4]["type"], "function_call_output");
    assert_eq!(input.len(), 5, "{body}");

    let csv = FilePart::from_url("https://data.test/table.csv", Some("text/csv")).unwrap();
    let body = responses_body("xai", "grok-4.7", vec![user_file(csv)], Default::default()).await;
    assert_eq!(
        items(&body)[0]["content"][1],
        json!({"type":"input_file","file_url":"https://data.test/table.csv"})
    );
    let pdf = FilePart::from_bytes(vec![1, 2, 3], Some("application/pdf"), None).unwrap();
    let error = rejected(
        "xai",
        "grok-4.7",
        vec![user_file(pdf.clone())],
        Default::default(),
    )
    .await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    let result = ToolResultPart::new(
        "call_1",
        "lookup",
        ToolResultValue::Special(SpecialToolResult::Content(ContentOutput {
            output_type: ContentOutputType::Content,
            value: vec![ContentPart::File(pdf)],
        })),
    );
    let options = StreamOptions {
        tools: tools(&["lookup"]),
        ..Default::default()
    };
    let body = responses_body("xai", "grok-4.7", tool_round(result), options).await;
    let output = items(&body)
        .iter()
        .find(|i| i["type"] == "function_call_output")
        .unwrap();
    assert_eq!(
        output["output"],
        json!([{"type":"input_text","text":"[file: application/pdf]"}])
    );
}

/// An xAI stream with reasoning whose summary part ends before its item,
/// which carries the encrypted content, then a message.
fn xai_reasoning_stream(terminal: &str) -> (u16, Vec<u8>) {
    responses_sse(&[
        json!({"type":"response.created","response":{"id":"resp_1","model":"grok-4.7","status":"in_progress"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_1","summary":[]}}),
        json!({"type":"response.reasoning_summary_part.added","item_id":"rs_1","output_index":0,"summary_index":0}),
        json!({"type":"response.reasoning_summary_text.delta","item_id":"rs_1","output_index":0,"summary_index":0,"delta":"thinking"}),
        json!({"type":"response.reasoning_summary_part.done","item_id":"rs_1","output_index":0,"summary_index":0}),
        json!({"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"thinking"}],"encrypted_content":"enc-1"}}),
        json!({"type":"response.output_item.added","output_index":1,"item":{"type":"message","id":"msg_1","role":"assistant","content":[]}}),
        json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":1,"delta":"answer"}),
        json!({"type":"response.output_item.done","output_index":1,"item":{"type":"message","id":"msg_1","role":"assistant","content":[{"type":"output_text","text":"answer"}]}}),
        json!({"type":terminal,"response":{"id":"resp_1","model":"grok-4.7","status":"completed","output":[],"usage":{"input_tokens":4142,"input_tokens_details":{"cached_tokens":4328},"output_tokens":10}}}),
    ])
}

#[tokio::test]
async fn hst_023_xai_keeps_its_item_ids_and_counts_cached_input() {
    let transport = Recorder::new([
        xai_reasoning_stream("response.completed"),
        responses_text("ok"),
    ]);
    let model = model("xai", "grok-4.7", transport.clone());
    let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    let usage = handle.usage().unwrap();
    assert_eq!(
        (usage.input_tokens, usage.cache_read_tokens),
        (8470, Some(4328))
    );
    for part in &handle.message().parts {
        let md = match part {
            Part::Reasoning(r) => r.provider_metadata.as_ref(),
            Part::Text(t) => t.provider_metadata.as_ref(),
            _ => None,
        };
        let md = md.unwrap_or_else(|| panic!("a part has no metadata: {part:?}"));
        assert!(
            md.contains_key("xai") && !md.contains_key("openai"),
            "{md:?}"
        );
    }
    let history = vec![
        user_message("hello"),
        handle.message().clone(),
        user_message("again"),
    ];
    let (_, events) = call(&model, history, Default::default()).await;
    ok(&events);
    let requests = transport.requests.lock().unwrap();
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let input = items(&body);
    assert!(
        input.iter().any(|i| i["type"] == "reasoning"
            && i["encrypted_content"] == "enc-1"
            && i["id"] == "rs_1"),
        "{body}"
    );
    assert!(
        input
            .iter()
            .any(|i| i["role"] == "assistant" && i["id"] == "msg_1" && i["content"] == "answer"),
        "{body}"
    );
}

#[tokio::test]
async fn hst_023_xai_error_bodies_put_the_code_first() {
    let body =
        json!({"code":"Client specified an invalid argument","error":"Invalid request content"});
    let transport = Recorder::new([(400, body.to_string().into_bytes())]);
    let model = model("xai", "grok-4.7", transport);
    let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    let Some(Error::Provider(error)) = events.into_iter().find_map(Result::err) else {
        panic!("the call did not fail with a provider error");
    };
    assert_eq!(
        error.message,
        "Client specified an invalid argument: Invalid request content"
    );
    assert_eq!(
        error.code.as_deref(),
        Some("Client specified an invalid argument")
    );
    assert_eq!(error.kind, ProviderErrorKind::BadRequest);
}

#[tokio::test]
async fn hst_024_hugging_face_requests() {
    let schema = json!({"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}},"required":["a"]});
    let options = StreamOptions {
        output_schema: Some(schema.clone().try_into().unwrap()),
        output_type_name: Some("answer".into()),
        ..Default::default()
    };
    let body = responses_body("huggingface", "m", vec![user_message("hello")], options).await;
    assert_eq!(
        body["text"]["format"],
        json!({"type":"json_schema","name":"answer","strict":false,"schema":schema})
    );
    let choose = |choice: ToolChoice| StreamOptions {
        tools: tools(&["lookup"]),
        tool_choice: Some(choice),
        ..Default::default()
    };
    let body = responses_body(
        "huggingface",
        "m",
        vec![user_message("hello")],
        choose(ToolChoice::Required),
    )
    .await;
    assert_eq!(body["tool_choice"], "required");
    let body = responses_body(
        "huggingface",
        "m",
        vec![user_message("hello")],
        choose(ToolChoice::Tool("lookup".into())),
    )
    .await;
    assert_eq!(
        body["tool_choice"],
        json!({"type":"function","function":{"name":"lookup"}})
    );
    let body = responses_body(
        "huggingface",
        "m",
        vec![user_message("hello")],
        choose(ToolChoice::None),
    )
    .await;
    assert!(
        body.get("tools").is_none() && body.get("tool_choice").is_none(),
        "{body}"
    );
    let provider_tool = StreamOptions {
        tools: vec![openai::tools::web_search(json!({})).unwrap()],
        ..Default::default()
    };
    let error = rejected(
        "huggingface",
        "m",
        vec![user_message("hello")],
        provider_tool,
    )
    .await;
    assert!(matches!(error, Error::UnsupportedOperation(_)), "{error:?}");
    let pdf = FilePart::from_bytes(vec![1, 2, 3], Some("application/pdf"), None).unwrap();
    let error = rejected("huggingface", "m", vec![user_file(pdf)], Default::default()).await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
}

#[tokio::test]
async fn hst_024_hugging_face_history() {
    let mut call = ToolCallPart::new("call_1", "lookup", "{\"q\":1}");
    call.provider_metadata = metadata_under("huggingface", json!({"item_id":"fc_1"}));
    let history = vec![
        user_message("hello"),
        assistant(vec![
            reasoning_part("R"),
            text_part("T"),
            Part::ToolCall(call),
        ]),
        tool_message(ToolResultPart::new("call_1", "lookup", json!({"value": 1}))),
    ];
    let options = StreamOptions {
        tools: tools(&["lookup"]),
        ..Default::default()
    };
    let body = responses_body("huggingface", "m", history, options).await;
    assert!(!holds_reference(&body), "{body}");
    let input = items(&body);
    assert_eq!(
        input[1],
        json!({"role":"assistant","content":[{"type":"output_text","text":"R"}]})
    );
    assert_eq!(
        input[2],
        json!({"role":"assistant","content":[{"type":"output_text","text":"T"}]})
    );
    assert_eq!(
        input[3],
        json!({"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{\"q\":1}"})
    );
    assert_eq!(
        input[4],
        json!({"type":"function_call_output","call_id":"call_1","output":"{\"value\":1}"})
    );
}

/// An OpenAI model on the Responses API with this transport.
fn openai_responses(transport: Arc<Recorder>) -> Model {
    Model::new(
        "gpt-5.4",
        Arc::new(OpenAICompatibleProvider::new(
            ProviderConfig {
                default_base_url: "https://openai.test/v1".into(),
                ..key_config()
            },
            transport,
        )),
    )
}
/// The provider error a Responses stream of these events ends with.
async fn responses_error(model: Model) -> suprnova_ai::errors::ProviderError {
    let mut options = StreamOptions::default();
    options.retry.max_retries = 0;
    let (_, events) = call(&model, vec![user_message("hello")], options).await;
    match events.into_iter().find_map(Result::err) {
        Some(Error::Provider(error)) => *error,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn hst_025_failed_and_error_events_end_the_call_with_a_classified_error() {
    let created = json!({"type":"response.created","response":{"id":"resp_1","model":"m","status":"in_progress"}});
    let failed = json!({"type":"response.failed","response":{"id":"resp_1","status":"failed","error":{"code":"server_error","message":"m"}}});
    let error_event =
        json!({"type":"error","code":"rate_limit_exceeded","message":"slow down","param":null});
    for build in [
        (|t: Arc<Recorder>| openai_responses(t)) as fn(Arc<Recorder>) -> Model,
        |t| model("xai", "grok-4.7", t),
    ] {
        let error = responses_error(build(Recorder::new([responses_sse(&[
            created.clone(),
            failed.clone(),
        ])])))
        .await;
        assert_eq!(error.message, "m");
        assert_eq!(error.code.as_deref(), Some("server_error"));
        assert_eq!(error.kind, ProviderErrorKind::InternalServer);
        let error = responses_error(build(Recorder::new([responses_sse(&[
            created.clone(),
            error_event.clone(),
        ])])))
        .await;
        assert_eq!(error.message, "slow down");
        assert_eq!(error.kind, ProviderErrorKind::RateLimit);
    }
}

#[tokio::test]
async fn hst_025_done_incomplete_and_parted_reasoning() {
    let outcome = |reply: (u16, Vec<u8>)| async move {
        let model = model("xai", "grok-4.7", Recorder::new([reply]));
        let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        (
            handle.finish_reason().map(str::to_owned),
            handle.usage().cloned(),
            handle.message().clone(),
        )
    };
    let (finish_done, usage_done, message) = outcome(xai_reasoning_stream("response.done")).await;
    let (finish_completed, usage_completed, _) =
        outcome(xai_reasoning_stream("response.completed")).await;
    assert_eq!(finish_done, finish_completed);
    assert_eq!(finish_done.as_deref(), Some("stop"));
    assert_eq!(usage_done, usage_completed);
    let encrypted = message.parts.iter().find_map(|part| match part {
        Part::Reasoning(r) => r
            .provider_metadata
            .as_ref()
            .map(|m| m["xai"]["reasoning_encrypted_content"].clone()),
        _ => None,
    });
    assert_eq!(encrypted, Some(json!("enc-1")), "{message:?}");
    let incomplete = responses_sse(&[
        json!({"type":"response.created","response":{"id":"resp_1","model":"m"}}),
        json!({"type":"response.output_text.delta","item_id":"msg_1","delta":"cut"}),
        json!({"type":"response.incomplete","response":{"id":"resp_1","incomplete_details":{"reason":"max_output_tokens"}}}),
    ]);
    let model = openai_responses(Recorder::new([incomplete]));
    let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    assert_eq!(handle.finish_reason(), Some("length"));
}

#[tokio::test]
async fn hst_025_xai_reasoning_in_two_summary_parts_keeps_its_encrypted_content() {
    let stream = responses_sse(&[
        json!({"type":"response.created","response":{"id":"resp_1","model":"grok-4.7","status":"in_progress"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_1","summary":[]}}),
        json!({"type":"response.reasoning_summary_part.added","item_id":"rs_1","output_index":0,"summary_index":0}),
        json!({"type":"response.reasoning_summary_text.delta","item_id":"rs_1","output_index":0,"summary_index":0,"delta":"first "}),
        json!({"type":"response.reasoning_summary_part.done","item_id":"rs_1","output_index":0,"summary_index":0}),
        json!({"type":"response.reasoning_summary_part.added","item_id":"rs_1","output_index":0,"summary_index":1}),
        json!({"type":"response.reasoning_summary_text.delta","item_id":"rs_1","output_index":0,"summary_index":1,"delta":"second"}),
        json!({"type":"response.reasoning_summary_part.done","item_id":"rs_1","output_index":0,"summary_index":1}),
        json!({"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"first "},{"type":"summary_text","text":"second"}],"encrypted_content":"enc-2"}}),
        json!({"type":"response.output_item.added","output_index":1,"item":{"type":"message","id":"msg_1","role":"assistant","content":[]}}),
        json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":1,"delta":"answer"}),
        json!({"type":"response.output_item.done","output_index":1,"item":{"type":"message","id":"msg_1","role":"assistant","content":[{"type":"output_text","text":"answer"}]}}),
        json!({"type":"response.completed","response":{"id":"resp_1","model":"grok-4.7","status":"completed","output":[]}}),
    ]);
    let transport = Recorder::new([stream, responses_text("ok")]);
    let model = model("xai", "grok-4.7", transport.clone());
    let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    let reasoning: Vec<_> = handle
        .message()
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Reasoning(r) => Some((
                r.text.clone(),
                r.provider_metadata
                    .as_ref()
                    .map(|m| m["xai"]["reasoning_encrypted_content"].clone()),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        reasoning,
        [("first second".to_owned(), Some(json!("enc-2")))]
    );
    let history = vec![
        user_message("hello"),
        handle.message().clone(),
        user_message("again"),
    ];
    let (_, events) = call(&model, history, Default::default()).await;
    ok(&events);
    let requests = transport.requests.lock().unwrap();
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let sent: Vec<_> = items(&body)
        .iter()
        .filter(|i| i["type"] == "reasoning")
        .collect();
    assert_eq!(sent.len(), 1, "{body}");
    assert_eq!(sent[0]["encrypted_content"], "enc-2");
}

#[tokio::test]
async fn hst_025_openai_reasoning_in_two_summary_parts_is_referenced_once() {
    let stream = responses_sse(&[
        json!({"type":"response.created","response":{"id":"resp_1","model":"gpt-5.4","status":"in_progress"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"enc"}}),
        json!({"type":"response.reasoning_summary_part.added","item_id":"rs_1","output_index":0,"summary_index":0}),
        json!({"type":"response.reasoning_summary_text.delta","item_id":"rs_1","output_index":0,"summary_index":0,"delta":"first"}),
        json!({"type":"response.reasoning_summary_part.done","item_id":"rs_1","output_index":0,"summary_index":0}),
        json!({"type":"response.reasoning_summary_part.added","item_id":"rs_1","output_index":0,"summary_index":1}),
        json!({"type":"response.reasoning_summary_text.delta","item_id":"rs_1","output_index":0,"summary_index":1,"delta":"second"}),
        json!({"type":"response.reasoning_summary_part.done","item_id":"rs_1","output_index":0,"summary_index":1}),
        json!({"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"first"},{"type":"summary_text","text":"second"}],"encrypted_content":"enc"}}),
        json!({"type":"response.completed","response":{"id":"resp_1","model":"gpt-5.4","status":"completed","output":[]}}),
    ]);
    let transport = Recorder::new([stream, responses_text("ok")]);
    let model = openai_responses(transport.clone());
    let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    let history = vec![
        user_message("hello"),
        handle.message().clone(),
        user_message("again"),
    ];
    let (_, events) = call(&model, history, Default::default()).await;
    ok(&events);
    let requests = transport.requests.lock().unwrap();
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let references: Vec<_> = items(&body)
        .iter()
        .filter(|i| i["type"] == "item_reference" && i["id"] == "rs_1")
        .collect();
    assert_eq!(references.len(), 1, "{body}");
}

#[tokio::test]
async fn hst_025_a_failed_event_without_a_response_still_fails() {
    let stream = responses_sse(&[
        json!({"type":"response.created","response":{"id":"resp_1","model":"m","status":"in_progress"}}),
        json!({"type":"response.failed"}),
    ]);
    let error = responses_error(openai_responses(Recorder::new([stream]))).await;
    assert_eq!(error.kind, ProviderErrorKind::Response, "{error:?}");
}

#[test]
fn hst_019_the_openai_package_does_not_hide_a_responses_host_id() {
    for (id, class) in [
        ("xai", "xai_responses"),
        ("huggingface", "huggingface_responses"),
    ] {
        let metadata = ModelsDevProvider {
            id: id.into(),
            npm: Some("@ai-sdk/openai".into()),
            api: Some("https://host.test/v1".into()),
            env: vec![],
        };
        let provider = OpenAICompatibleProvider::from_modelsdev_provider(
            &metadata,
            key_config(),
            Recorder::new([]),
        )
        .unwrap();
        assert_eq!(
            provider.protocol().unwrap().protocol_class_id(),
            class,
            "{id}"
        );
    }
}

#[tokio::test]
async fn hst_022_and_hst_024_send_every_text_part() {
    let empty = Part::Text(TextPart {
        id: "t0".into(),
        text: String::new(),
        provider_metadata: metadata_under("xai", json!({"item_id":"msg_0"})),
    });
    let history = || {
        vec![
            user_message("hello"),
            assistant(vec![empty.clone(), text_part("T")]),
            user_message("again"),
        ]
    };
    let body = responses_body("xai", "grok-4.7", history(), Default::default()).await;
    assert_eq!(
        items(&body)[1],
        json!({"role":"assistant","content":"","id":"msg_0"}),
        "{body}"
    );
    let body = responses_body("huggingface", "m", history(), Default::default()).await;
    assert_eq!(
        items(&body)[1],
        json!({"role":"assistant","content":[{"type":"output_text","text":""}]}),
        "{body}"
    );
}

// HST-026 to HST-032: Together AI, DeepInfra and Fireworks, whole.

const COMPATIBLE_HOSTS: [&str; 3] = ["togetherai", "deepinfra", "fireworks"];

/// A call with these provider entries and a core temperature of 0.7.
fn entries(entries: &[(&str, Value)]) -> StreamOptions {
    let mut options = temperature(0.7);
    let params = options.params.take().unwrap_or_default();
    options.params = Some(entries.iter().fold(params, |p, (key, value)| {
        p.with_provider_param(*key, value.clone())
    }));
    options
}

// HST-026

#[tokio::test]
async fn hst_026_each_host_reads_its_entry() {
    for name in COMPATIBLE_HOSTS {
        let entry = json!({"user": "u", "reasoningEffort": "low", "textVerbosity": "low",
            "repetition_penalty": 1.1, "temperature": 0.3, "messages": []});
        let body = host_body(
            name,
            "some-model",
            vec![user_message("hello")],
            entries(&[(name, entry)]),
        )
        .await;
        assert_eq!(body["user"], "u", "{name}: {body}");
        assert_eq!(body["reasoning_effort"], "low", "{name}: {body}");
        assert_eq!(body["verbosity"], "low", "{name}: {body}");
        assert_eq!(body["repetition_penalty"], 1.1, "{name}: {body}");
        assert_eq!(body["temperature"], 0.3, "{name}: {body}");
        assert_eq!(body["messages"][0]["content"], "hello", "{name}: {body}");
        for key in ["reasoningEffort", "textVerbosity"] {
            assert!(body.get(key).is_none(), "{name}: {key} in {body}");
        }
    }
}

#[tokio::test]
async fn hst_026_the_hosts_entry_wins_over_openai_compatible_and_sets_strict() {
    let body = host_body(
        "togetherai",
        "some-model",
        vec![user_message("hello")],
        entries(&[(
            "openaiCompatible",
            json!({"user": "v", "textVerbosity": "high"}),
        )]),
    )
    .await;
    assert_eq!(body["user"], "v", "{body}");
    assert_eq!(body["verbosity"], "high", "{body}");
    let body = host_body(
        "togetherai",
        "some-model",
        vec![user_message("hello")],
        entries(&[
            ("openaiCompatible", json!({"user": "v"})),
            ("togetherai", json!({"user": "h"})),
        ]),
    )
    .await;
    assert_eq!(body["user"], "h", "{body}");
    let mut options =
        entries(&[("deepinfra", json!({"strictJsonSchema": false}))]).with_output::<Answer>();
    options.output_type_name = Some("answer".into());
    let body = host_body(
        "deepinfra",
        "some-model",
        vec![user_message("hello")],
        options,
    )
    .await;
    assert_eq!(
        body["response_format"]["json_schema"]["strict"], false,
        "{body}"
    );
    assert!(body.get("strictJsonSchema").is_none(), "{body}");
}

#[tokio::test]
async fn hst_026_fireworks_renames_its_options() {
    let entry = json!({"promptCacheKey": "k", "serviceTier": "priority",
        "thinking": {"type": "enabled", "budgetTokens": 2048},
        "reasoningHistory": "preserved", "reasoningEffort": "xhigh"});
    let body = host_body(
        "fireworks",
        "some-model",
        vec![user_message("hello")],
        entries(&[("fireworks", entry)]),
    )
    .await;
    assert_eq!(body["prompt_cache_key"], "k", "{body}");
    assert_eq!(body["service_tier"], "priority", "{body}");
    assert_eq!(
        body["thinking"],
        json!({"type": "enabled", "budget_tokens": 2048}),
        "{body}"
    );
    assert_eq!(body["reasoning_history"], "preserved", "{body}");
    assert_eq!(body["reasoning_effort"], "high", "{body}");
    for key in [
        "promptCacheKey",
        "serviceTier",
        "reasoningHistory",
        "reasoningEffort",
    ] {
        assert!(body.get(key).is_none(), "{key} in {body}");
    }
}

#[tokio::test]
async fn hst_026_invalid_entries_fail_before_a_request() {
    for (name, entry) in [
        ("togetherai", json!("fast")),
        ("deepinfra", json!({"user": 7})),
        ("fireworks", json!({"serviceTier": "flex"})),
        ("fireworks", json!({"thinking": {"type": "auto"}})),
        ("fireworks", json!({"thinking": {"budgetTokens": 512}})),
        ("fireworks", json!({"reasoningHistory": "full"})),
    ] {
        let error = rejected(
            name,
            "some-model",
            vec![user_message("hello")],
            entries(&[(name, entry.clone())]),
        )
        .await;
        assert!(
            matches!(error, Error::Validation(_)),
            "{name} {entry}: {error:?}"
        );
    }
}

// HST-027

type CompletionProtocol = fn() -> openai::OpenAICompletionsProtocol;

/// The three hosts' completion protocols, completions URLs, and whether
/// they ask for streamed usage.
const COMPLETIONS: [(&str, CompletionProtocol, &str, bool); 3] = [
    (
        "togetherai",
        togetherai::completion_protocol,
        "https://api.together.xyz/v1/completions",
        true,
    ),
    (
        "deepinfra",
        deepinfra::completion_protocol,
        "https://api.deepinfra.com/v1/openai/completions",
        false,
    ),
    (
        "fireworks",
        fireworks::completion_protocol,
        "https://api.fireworks.ai/inference/v1/completions",
        true,
    ),
];

fn completion_stream(text: &str) -> (u16, Vec<u8>) {
    chat_sse(&[
        json!({"id": "cmpl_1", "object": "text_completion", "model": "m", "choices": [{"index": 0, "text": text, "finish_reason": null}]}),
        json!({"id": "cmpl_1", "object": "text_completion", "model": "m", "choices": [{"index": 0, "text": "", "finish_reason": "stop"}]}),
    ])
}

fn completions_model(name: &str, protocol: CompletionProtocol, transport: Arc<Recorder>) -> Model {
    model(name, "some-model", transport).with_protocol(Arc::new(protocol()))
}

#[tokio::test]
async fn hst_027_each_host_gives_its_completion_protocol() {
    for (name, protocol, url, usage) in COMPLETIONS {
        let transport = Recorder::new([completion_stream("hi")]);
        let model = completions_model(name, protocol, transport.clone());
        let (handle, events) = call(
            &model,
            vec![user_message("hello")],
            entries(&[(name, json!({"echo": true, "suffix": "end"}))]),
        )
        .await;
        ok(&events);
        assert_eq!(handle.text(), "hi", "{name}");
        let request = transport.request();
        assert_eq!(request.url, url, "{name}");
        assert_eq!(
            transport.header("authorization").as_deref(),
            Some("Bearer sk-test"),
            "{name}"
        );
        let body = transport.body();
        assert_eq!(body["stream_options"].is_object(), usage, "{name}: {body}");
        assert_eq!(body["echo"], true, "{name}: {body}");
        assert_eq!(body["suffix"], "end", "{name}: {body}");
    }
}

#[tokio::test]
async fn hst_027_completion_errors_read_as_the_hosts_chat_errors() {
    for (status, body, message, kind) in [
        (
            400,
            json!({"error": "bad prompt"}),
            "bad prompt",
            ProviderErrorKind::BadRequest,
        ),
        (
            503,
            json!({"error": {"message": "busy"}}),
            "busy",
            ProviderErrorKind::ServiceUnavailable,
        ),
    ] {
        let transport = Recorder::new([(status, body.to_string().into_bytes())]);
        let model = completions_model("fireworks", fireworks::completion_protocol, transport);
        let options = StreamOptions {
            retry: suprnova_ai::RetryPolicy::none(),
            ..Default::default()
        };
        let (_, events) = call(&model, vec![user_message("hello")], options).await;
        let error = events
            .into_iter()
            .find_map(Result::err)
            .expect("the call failed");
        let Error::Provider(error) = error else {
            panic!("{error:?}");
        };
        assert_eq!(error.message, message);
        assert_eq!(error.kind, kind, "HTTP {status}");
    }
}

// HST-028

/// A server that answers each embedding request with one embedding per
/// input value, `[i, len]` for the value at index `i`.
#[derive(Default)]
struct EmbedServer {
    requests: Mutex<Vec<HttpRequest>>,
}
impl HttpTransport for EmbedServer {
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        self.requests.lock().unwrap().push(request);
        let count = body["input"].as_array().map_or(0, Vec::len);
        let data: Vec<Value> = (0..count)
            .map(|i| json!({"object": "embedding", "index": i, "embedding": [i as f64, count as f64]}))
            .collect();
        let reply = json!({"object": "list", "data": data, "usage": {"prompt_tokens": count}});
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: stream::iter([Ok(Bytes::from(reply.to_string()))]).boxed(),
            })
        })
    }
}

fn embed_params(entry: Option<(&str, Value)>) -> suprnova_ai::providers::operations::EmbedParams {
    suprnova_ai::providers::operations::EmbedParams {
        provider_options: entry
            .map(|(key, value)| [(key.to_owned(), value)].into())
            .unwrap_or_default(),
    }
}

#[tokio::test]
async fn hst_028_each_host_embeds_on_the_compatible_protocol() {
    for (name, url) in [
        ("togetherai", "https://api.together.xyz/v1/embeddings"),
        (
            "deepinfra",
            "https://api.deepinfra.com/v1/openai/embeddings",
        ),
        (
            "fireworks",
            "https://api.fireworks.ai/inference/v1/embeddings",
        ),
    ] {
        let server = Arc::new(EmbedServer::default());
        let mut registry = ProviderRegistry::new();
        (host(name).register)(&mut registry, key_config(), server.clone()).unwrap();
        let model = Model::new("some-embedder", registry.get_provider(name).unwrap());
        let item = suprnova_ai::ops::embed(
            &model,
            vec!["a".into(), "b".into()],
            embed_params(Some((name, json!({"dimensions": 256})))),
            Default::default(),
        )
        .await
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(item.value, vec![vec![0.0, 2.0], vec![1.0, 2.0]], "{name}");
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "{name}");
        assert_eq!(requests[0].url, url, "{name}");
        let auth = requests[0]
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.as_str());
        assert_eq!(auth, Some("Bearer sk-test"), "{name}");
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body,
            json!({"model": "some-embedder", "input": ["a", "b"], "encoding_format": "float", "dimensions": 256}),
            "{name}"
        );
    }
}

#[tokio::test]
async fn hst_028_a_host_takes_2048_values_per_call() {
    let server = Arc::new(EmbedServer::default());
    let mut registry = ProviderRegistry::new();
    togetherai::register(&mut registry, key_config(), server.clone()).unwrap();
    let model = Model::new(
        "some-embedder",
        registry.get_provider("togetherai").unwrap(),
    );
    let values: Vec<String> = (0..2049).map(|i| format!("v{i}")).collect();
    let item = suprnova_ai::ops::embed(&model, values, embed_params(None), Default::default())
        .await
        .unwrap();
    assert_eq!(item.value.len(), 2049);
    let mut sizes: Vec<usize> = server
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| {
            serde_json::from_slice::<Value>(&r.body).unwrap()["input"]
                .as_array()
                .unwrap()
                .len()
        })
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![1, 2048]);
}

// HST-029 to HST-031: images.

use suprnova_ai::providers::operations::{ImageParams, ImagePrompt, MediaInput, Warning};

/// The start of a PNG file, which media type detection reads.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0";

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
fn json_reply(value: Value) -> (u16, Vec<u8>) {
    (200, value.to_string().into_bytes())
}
fn image_params(
    size: Option<&str>,
    aspect_ratio: Option<&str>,
    seed: Option<i64>,
    entry: Option<(&str, Value)>,
) -> ImageParams {
    ImageParams {
        n: 1,
        size: size.map(str::to_owned),
        aspect_ratio: aspect_ratio.map(str::to_owned),
        seed,
        provider_options: entry
            .map(|(key, value)| [(key.to_owned(), value)].into())
            .unwrap_or_default(),
    }
}
fn prompt(text: &str, images: Vec<MediaInput>, mask: Option<MediaInput>) -> ImagePrompt {
    ImagePrompt {
        text: Some(text.into()),
        images,
        mask,
    }
}
fn warned(warnings: &[Warning], kind: &str, feature: Option<&str>) -> bool {
    warnings
        .iter()
        .any(|w| w.kind == kind && w.feature.as_deref() == feature)
}
async fn image(
    name: &str,
    model_id: &str,
    transport: Arc<Recorder>,
    prompt: ImagePrompt,
    params: ImageParams,
) -> Result<suprnova_ai::providers::operations::Item<Vec<FilePart>>, Error> {
    suprnova_ai::ops::generate_image(
        &model(name, model_id, transport),
        prompt,
        params,
        Default::default(),
    )
    .await
}
fn png_bytes(file: &FilePart) -> &[u8] {
    match &file.data {
        suprnova_ai::types::messages::FileData::Bytes(bytes) => bytes,
        other => panic!("{other:?}"),
    }
}

// HST-029

#[tokio::test]
async fn hst_029_together_sends_the_generation_typescript_sends() {
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(PNG)}]}))]);
    let item = image(
        "togetherai",
        "black-forest-labs/FLUX.1-kontext-pro",
        transport.clone(),
        prompt("a cat", vec![MediaInput::Bytes(PNG.to_vec())], None),
        image_params(
            Some("1024x768"),
            None,
            Some(7),
            Some(("togetherai", json!({"steps": 4}))),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        transport.request().url,
        "https://api.together.xyz/v1/images/generations"
    );
    assert_eq!(
        transport.body(),
        json!({"model": "black-forest-labs/FLUX.1-kontext-pro", "prompt": "a cat", "seed": 7,
            "width": 1024, "height": 768, "image_url": format!("data:image/png;base64,{}", b64(PNG)),
            "response_format": "base64", "steps": 4})
    );
    assert_eq!(item.value.len(), 1);
    assert_eq!(png_bytes(&item.value[0]), PNG);
    assert_eq!(item.value[0].media_type, "image/png");
    assert!(item.warnings.is_empty(), "{:?}", item.warnings);
}

#[tokio::test]
async fn hst_029_gemini_3_pro_image_takes_no_seed_or_diffusion_options() {
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(PNG)}]}))]);
    let entry = json!({"steps": 4, "guidance": 3, "negative_prompt": "x",
        "disable_safety_checker": true, "other": 1});
    let item = image(
        "togetherai",
        "google/gemini-3-pro-image",
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(None, None, Some(7), Some(("togetherai", entry))),
    )
    .await
    .unwrap();
    let body = transport.body();
    for key in [
        "seed",
        "steps",
        "guidance",
        "negative_prompt",
        "disable_safety_checker",
    ] {
        assert!(body.get(key).is_none(), "{key} in {body}");
    }
    assert_eq!(body["other"], 1, "{body}");
    assert!(
        warned(&item.warnings, "unsupported", Some("seed")),
        "{:?}",
        item.warnings
    );
}

#[tokio::test]
async fn hst_029_an_aspect_ratio_and_extra_images_are_warned_and_left_out() {
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(PNG)}]}))]);
    let item = image(
        "togetherai",
        "black-forest-labs/FLUX.1-kontext-pro",
        transport.clone(),
        prompt(
            "a cat",
            vec![
                MediaInput::Url("https://img.test/a.png".into()),
                MediaInput::Url("https://img.test/b.png".into()),
            ],
            None,
        ),
        image_params(None, Some("16:9"), None, None),
    )
    .await
    .unwrap();
    let body = transport.body();
    assert!(body.get("aspect_ratio").is_none(), "{body}");
    assert_eq!(body["image_url"], "https://img.test/a.png", "{body}");
    assert!(
        warned(&item.warnings, "unsupported", Some("aspectRatio")),
        "{:?}",
        item.warnings
    );
    assert!(warned(&item.warnings, "other", None), "{:?}", item.warnings);
}

#[tokio::test]
async fn hst_029_a_mask_fails_before_a_request() {
    let transport = Recorder::new([]);
    let error = image(
        "togetherai",
        "black-forest-labs/FLUX.1-kontext-pro",
        transport.clone(),
        prompt(
            "a cat",
            vec![MediaInput::Bytes(PNG.to_vec())],
            Some(MediaInput::Bytes(PNG.to_vec())),
        ),
        image_params(None, None, None, None),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
}

// HST-030

#[tokio::test]
async fn hst_030_deepinfra_generates_at_its_inference_url() {
    let transport = Recorder::new([json_reply(
        json!({"images": [format!("data:image/png;base64,{}", b64(PNG))]}),
    )]);
    let item = image(
        "deepinfra",
        "black-forest-labs/FLUX-1-schnell",
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(
            Some("1024x768"),
            Some("16:9"),
            Some(7),
            Some(("deepinfra", json!({"num_inference_steps": 4}))),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        transport.request().url,
        "https://api.deepinfra.com/v1/inference/black-forest-labs/FLUX-1-schnell"
    );
    assert_eq!(
        transport.body(),
        json!({"prompt": "a cat", "num_images": 1, "aspect_ratio": "16:9", "width": "1024",
            "height": "768", "seed": 7, "num_inference_steps": 4})
    );
    assert_eq!(png_bytes(&item.value[0]), PNG);
}

#[tokio::test]
async fn hst_030_deepinfra_edits_images_as_a_form() {
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(PNG)}]}))]);
    let item = image(
        "deepinfra",
        "black-forest-labs/FLUX.1-Kontext-dev",
        transport.clone(),
        prompt(
            "a cat",
            vec![
                MediaInput::Bytes(PNG.to_vec()),
                MediaInput::Bytes(PNG.to_vec()),
            ],
            Some(MediaInput::Bytes(PNG.to_vec())),
        ),
        image_params(
            Some("512x512"),
            None,
            None,
            Some(("deepinfra", json!({"guidance": 3}))),
        ),
    )
    .await
    .unwrap();
    let request = transport.request();
    assert_eq!(
        request.url,
        "https://api.deepinfra.com/v1/openai/images/edits"
    );
    let form = String::from_utf8_lossy(&request.body).into_owned();
    for (field, count) in [
        ("name=\"model\"", 1),
        ("name=\"prompt\"", 1),
        ("name=\"image\"", 2),
        ("name=\"mask\"", 1),
        ("name=\"n\"", 1),
        ("name=\"size\"", 1),
        ("name=\"guidance\"", 1),
    ] {
        assert_eq!(form.matches(field).count(), count, "{field} in {form}");
    }
    assert!(
        form.contains("black-forest-labs/FLUX.1-Kontext-dev"),
        "{form}"
    );
    assert_eq!(png_bytes(&item.value[0]), PNG);
}

#[tokio::test]
async fn hst_030_an_image_url_without_a_download_transport_fails_before_a_request() {
    let transport = Recorder::new([]);
    let error = image(
        "deepinfra",
        "black-forest-labs/FLUX.1-Kontext-dev",
        transport.clone(),
        prompt(
            "a cat",
            vec![MediaInput::Url("https://img.test/a.png".into())],
            None,
        ),
        image_params(None, None, None, None),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
}

#[tokio::test]
async fn hst_030_a_base_url_not_ending_in_openai_fails_before_a_request() {
    let transport = Recorder::new([]);
    let config = ProviderConfig {
        default_base_url: "https://api.deepinfra.com/v1".into(),
        ..key_config()
    };
    let model = Model::new(
        "black-forest-labs/FLUX-1-schnell",
        Arc::new(deepinfra::provider(config, transport.clone())),
    );
    let error = suprnova_ai::ops::generate_image(
        &model,
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Configuration(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
}

#[tokio::test]
async fn hst_030_deepinfra_image_errors_give_their_message() {
    let transport = Recorder::new([(400, br#"{"detail": {"error": "bad size"}}"#.to_vec())]);
    let error = image(
        "deepinfra",
        "black-forest-labs/FLUX-1-schnell",
        transport,
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap_err();
    let Error::Provider(error) = error else {
        panic!("{error:?}");
    };
    assert_eq!(error.message, "bad size");
    assert_eq!(error.kind, ProviderErrorKind::BadRequest);
}

// HST-031

const FIREWORKS: &str = "https://api.fireworks.ai/inference/v1";

fn fireworks_requests(transport: &Recorder) -> Vec<HttpRequest> {
    transport.requests.lock().unwrap().clone()
}

#[tokio::test]
async fn hst_031_a_size_model_generates_at_image_generation() {
    let model_id = "accounts/fireworks/models/stable-diffusion-xl-1024-v1-0";
    let transport = Recorder::new([(200, PNG.to_vec())]);
    let item = image(
        "fireworks",
        model_id,
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(
            Some("1024x768"),
            Some("16:9"),
            Some(7),
            Some(("fireworks", json!({"cfg_scale": 7}))),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        transport.request().url,
        format!("{FIREWORKS}/image_generation/{model_id}")
    );
    assert_eq!(
        transport.body(),
        json!({"prompt": "a cat", "seed": 7, "samples": 1, "width": "1024", "height": "768",
            "cfg_scale": 7})
    );
    assert_eq!(png_bytes(&item.value[0]), PNG);
    assert!(
        warned(&item.warnings, "unsupported", Some("aspectRatio")),
        "{:?}",
        item.warnings
    );
}

#[tokio::test]
async fn hst_031_another_model_generates_at_its_text_to_image_workflow() {
    let model_id = "accounts/fireworks/models/flux-1-dev-fp8";
    let transport = Recorder::new([(200, PNG.to_vec())]);
    let item = image(
        "fireworks",
        model_id,
        transport.clone(),
        prompt(
            "a cat",
            vec![
                MediaInput::Url("https://img.test/a.png".into()),
                MediaInput::Url("https://img.test/b.png".into()),
            ],
            Some(MediaInput::Url("https://img.test/m.png".into())),
        ),
        image_params(Some("512x512"), Some("16:9"), None, None),
    )
    .await
    .unwrap();
    assert_eq!(
        transport.request().url,
        format!("{FIREWORKS}/workflows/{model_id}/text_to_image")
    );
    assert_eq!(
        transport.body(),
        json!({"prompt": "a cat", "aspect_ratio": "16:9", "samples": 1,
            "input_image": "https://img.test/a.png"})
    );
    for (kind, feature) in [
        ("unsupported", Some("size")),
        ("other", None),
        ("unsupported", Some("mask")),
    ] {
        assert!(
            warned(&item.warnings, kind, feature),
            "{feature:?}: {:?}",
            item.warnings
        );
    }
}

fn kontext_replies(sample: &str, final_status: &str) -> Vec<(u16, Vec<u8>)> {
    vec![
        json_reply(json!({"request_id": "req-1"})),
        json_reply(json!({"id": "req-1", "status": "Pending", "result": null})),
        json_reply(json!({"id": "req-1", "status": final_status, "result": {"sample": sample}})),
        (200, PNG.to_vec()),
    ]
}

/// Generate with a Kontext model on a Fireworks provider built with a
/// download transport, which a Kontext call needs.
async fn kontext_image(
    model_id: &str,
    transport: Arc<Recorder>,
    prompt: ImagePrompt,
    params: ImageParams,
) -> Result<suprnova_ai::providers::operations::Item<Vec<FilePart>>, Error> {
    let model = Model::new(
        model_id,
        Arc::new(
            fireworks::provider(key_config(), transport)
                .with_download_transport(Arc::new(Downloads::default())),
        ),
    );
    suprnova_ai::ops::generate_image(&model, prompt, params, Default::default()).await
}

#[tokio::test(start_paused = true)]
async fn hst_031_a_kontext_call_without_a_download_transport_fails_before_a_request() {
    // Its sample is usually on another origin, so no job is started and
    // billed whose sample the call could not fetch; not even one on the base
    // origin.
    for (model_id, sample) in [
        (
            "accounts/fireworks/models/flux-kontext-pro",
            "https://cdn.fireworks.test/x.png",
        ),
        (
            "accounts/fireworks/models/flux-kontext-max",
            "https://api.fireworks.ai/inference/v1/samples/x.png",
        ),
    ] {
        let transport = Recorder::new(kontext_replies(sample, "Ready"));
        let error = image(
            "fireworks",
            model_id,
            transport.clone(),
            prompt("a cat", vec![], None),
            image_params(None, None, None, None),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, Error::Validation(_)),
            "{model_id}: {error:?}"
        );
        assert_eq!(transport.count(), 0, "{model_id}");
    }
}

#[tokio::test(start_paused = true)]
async fn hst_031_a_kontext_model_polls_and_downloads_from_the_base_origin_with_the_key() {
    let model_id = "accounts/fireworks/models/flux-kontext-pro";
    let sample = "https://api.fireworks.ai/inference/v1/samples/x.png";
    let transport = Recorder::new(kontext_replies(sample, "Ready"));
    let item = kontext_image(
        model_id,
        transport.clone(),
        prompt("a cat", vec![MediaInput::Bytes(PNG.to_vec())], None),
        image_params(
            Some("1024x768"),
            Some("1:1"),
            Some(7),
            Some(("fireworks", json!({"safety_tolerance": 2}))),
        ),
    )
    .await
    .unwrap();
    let requests = fireworks_requests(&transport);
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].url, format!("{FIREWORKS}/workflows/{model_id}"));
    // The whole body: the size is warned and left out, as for every model
    // that is not a size model.
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).unwrap(),
        json!({"prompt": "a cat", "aspect_ratio": "1:1", "seed": 7, "samples": 1,
            "input_image": format!("data:image/png;base64,{}", b64(PNG)),
            "safety_tolerance": 2})
    );
    assert!(
        warned(&item.warnings, "unsupported", Some("size")),
        "{:?}",
        item.warnings
    );
    for poll in &requests[1..3] {
        assert_eq!(
            poll.url,
            format!("{FIREWORKS}/workflows/{model_id}/get_result")
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&poll.body).unwrap(),
            json!({"id": "req-1"})
        );
    }
    assert_eq!(requests[3].method, "GET");
    assert_eq!(requests[3].url, sample);
    assert!(
        requests[3]
            .headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("authorization") && v == "Bearer sk-test"),
        "{:?}",
        requests[3].headers
    );
    assert_eq!(png_bytes(&item.value[0]), PNG);
}

#[tokio::test(start_paused = true)]
async fn hst_031_a_sample_on_another_origin_downloads_without_the_key() {
    let model_id = "accounts/fireworks/models/flux-kontext-max";
    let sample = "https://cdn.fireworks.test/x.png";
    // The sample comes through the download transport.
    let transport = Recorder::new(kontext_replies(sample, "Ready"));
    let downloads = Arc::new(Downloads::default());
    let model = Model::new(
        model_id,
        Arc::new(
            fireworks::provider(key_config(), transport.clone())
                .with_download_transport(downloads.clone()),
        ),
    );
    let item = suprnova_ai::ops::generate_image(
        &model,
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(fireworks_requests(&transport).len(), 3);
    assert_eq!(*downloads.urls.lock().unwrap(), vec![sample.to_owned()]);
    assert_eq!(png_bytes(&item.value[0]), [1, 2, 3]);
}

#[tokio::test(start_paused = true)]
async fn hst_031_a_failed_or_slow_kontext_job_fails_the_call() {
    let model_id = "accounts/fireworks/models/flux-kontext-pro";
    let transport = Recorder::new(kontext_replies("unused", "Failed"));
    let error = kontext_image(
        model_id,
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Provider(_)), "{error:?}");
    assert_eq!(fireworks_requests(&transport).len(), 3);
    // A job still pending after 120 s fails.
    let mut replies = vec![json_reply(json!({"request_id": "req-1"}))];
    replies.extend(
        (0..400).map(|_| json_reply(json!({"id": "req-1", "status": "Pending", "result": null}))),
    );
    let transport = Recorder::new(replies);
    let started = tokio::time::Instant::now();
    let error = kontext_image(
        model_id,
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap_err();
    let Error::Provider(timeout) = &error else {
        panic!("{error:?}");
    };
    assert_eq!(timeout.kind, ProviderErrorKind::Timeout);
    assert!(!timeout.is_retryable);
    let waited = started.elapsed().as_secs();
    assert!((119..=121).contains(&waited), "waited {waited} s");
    assert!(fireworks_requests(&transport).len() < 400);
}

#[tokio::test]
async fn hst_031_invalid_fireworks_entries_fail_before_a_request() {
    // Fireworks' schema takes no null for an option, unlike Together AI's
    // and DeepInfra's.
    for entry in [
        json!({"output_format": "gif"}),
        json!({"safety_tolerance": 7}),
        json!({"output_format": null}),
        json!({"safety_tolerance": null}),
        json!({"steps": null}),
    ] {
        let transport = Recorder::new([]);
        let error = image(
            "fireworks",
            "accounts/fireworks/models/flux-1-dev-fp8",
            transport.clone(),
            prompt("a cat", vec![], None),
            image_params(None, None, None, Some(("fireworks", entry.clone()))),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Validation(_)), "{entry}: {error:?}");
        assert_eq!(transport.count(), 0);
    }
}

// HST-032

use suprnova_ai::providers::operations::{Documents, RankedDocument, RerankParams};

fn rerank_reply() -> (u16, Vec<u8>) {
    json_reply(json!({"id": "rr-1", "model": "Salesforce/Llama-Rank-v1",
        "results": [{"index": 1, "relevance_score": 0.9}, {"index": 0, "relevance_score": 0.2}],
        "usage": {"prompt_tokens": 9, "completion_tokens": 0, "total_tokens": 9}}))
}
async fn rerank(
    transport: Arc<Recorder>,
    documents: Documents,
    params: RerankParams,
) -> Result<suprnova_ai::providers::operations::Item<Vec<RankedDocument>>, Error> {
    suprnova_ai::ops::rerank(
        &model("togetherai", "Salesforce/Llama-Rank-v1", transport),
        documents,
        "which one".into(),
        params,
        Default::default(),
    )
    .await
}

#[tokio::test]
async fn hst_032_together_reranks_json_documents() {
    let transport = Recorder::new([rerank_reply()]);
    let documents = Documents::Json(vec![
        [("title".to_owned(), json!("a"))].into(),
        [("title".to_owned(), json!("b"))].into(),
    ]);
    let item = rerank(
        transport.clone(),
        documents,
        RerankParams {
            top_n: Some(1),
            provider_options: [("togetherai".to_owned(), json!({"rankFields": ["title"]}))].into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        transport.request().url,
        "https://api.together.xyz/v1/rerank"
    );
    assert_eq!(
        transport.body(),
        json!({"model": "Salesforce/Llama-Rank-v1", "query": "which one",
            "documents": [{"title": "a"}, {"title": "b"}], "top_n": 1,
            "rank_fields": ["title"], "return_documents": false})
    );
    assert_eq!(
        item.value,
        vec![
            RankedDocument {
                index: 1,
                score: 0.9
            },
            RankedDocument {
                index: 0,
                score: 0.2
            }
        ]
    );
    let metadata = item.metadata.expect("metadata");
    assert_eq!(metadata["id"], "rr-1");
    assert_eq!(metadata["model"], "Salesforce/Llama-Rank-v1");
}

#[tokio::test]
async fn hst_032_text_documents_go_as_texts_without_unset_fields() {
    let transport = Recorder::new([rerank_reply()]);
    rerank(
        transport.clone(),
        Documents::Text(vec!["a".into(), "b".into()]),
        RerankParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        transport.body(),
        json!({"model": "Salesforce/Llama-Rank-v1", "query": "which one",
            "documents": ["a", "b"], "return_documents": false})
    );
}

#[tokio::test]
async fn hst_032_an_unknown_option_fails_and_errors_give_their_message() {
    let transport = Recorder::new([]);
    let error = rerank(
        transport.clone(),
        Documents::Text(vec!["a".into()]),
        RerankParams {
            top_n: None,
            provider_options: [("togetherai".to_owned(), json!({"rank_fields": ["x"]}))].into(),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
    let transport = Recorder::new([(400, br#"{"error": {"message": "bad query"}}"#.to_vec())]);
    let error = rerank(
        transport,
        Documents::Text(vec!["a".into()]),
        RerankParams::default(),
    )
    .await
    .unwrap_err();
    let Error::Provider(error) = error else {
        panic!("{error:?}");
    };
    assert_eq!(error.message, "bad query");
}

#[tokio::test(start_paused = true)]
async fn hst_030_and_hst_031_a_registry_carries_the_download_transport() {
    // A Fireworks Kontext sample on another origin.
    let transport = Recorder::new(kontext_replies("https://cdn.fireworks.test/x.png", "Ready"));
    let downloads = Arc::new(Downloads::default());
    let mut registry = ProviderRegistry::new();
    fireworks::register_with_downloads(
        &mut registry,
        key_config(),
        transport.clone(),
        downloads.clone(),
    )
    .unwrap();
    let model = Model::new(
        "accounts/fireworks/models/flux-kontext-pro",
        registry.get_provider("fireworks").unwrap(),
    );
    let item = suprnova_ai::ops::generate_image(
        &model,
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(png_bytes(&item.value[0]), [1, 2, 3]);
    assert_eq!(
        *downloads.urls.lock().unwrap(),
        vec!["https://cdn.fireworks.test/x.png".to_owned()]
    );
    // A DeepInfra edit of an image URL.
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(PNG)}]}))]);
    let downloads = Arc::new(Downloads::default());
    let mut registry = ProviderRegistry::new();
    deepinfra::register_with_downloads(
        &mut registry,
        key_config(),
        transport.clone(),
        downloads.clone(),
    )
    .unwrap();
    let model = Model::new(
        "black-forest-labs/FLUX.1-Kontext-dev",
        registry.get_provider("deepinfra").unwrap(),
    );
    suprnova_ai::ops::generate_image(
        &model,
        prompt(
            "a cat",
            vec![MediaInput::Url("https://img.test/a.png".into())],
            None,
        ),
        image_params(None, None, None, None),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        *downloads.urls.lock().unwrap(),
        vec!["https://img.test/a.png".to_owned()]
    );
    let form = String::from_utf8_lossy(&transport.request().body).into_owned();
    assert!(form.contains("image/jpeg"), "{form}");
}

// HST-027 to HST-032 through models.dev metadata.

fn modelsdev(
    id: &str,
    npm: &str,
    api: &str,
    transport: Arc<dyn HttpTransport>,
) -> Arc<dyn Provider> {
    let metadata: ModelsDevProvider =
        serde_json::from_value(json!({"id": id, "npm": npm, "api": api, "env": []})).unwrap();
    Arc::new(
        OpenAICompatibleProvider::from_modelsdev_provider(&metadata, key_config(), transport)
            .unwrap(),
    )
}

#[tokio::test(start_paused = true)]
async fn hst_031_a_restored_kontext_model_keeps_the_download_transport() {
    let model_id = "accounts/fireworks/models/flux-kontext-pro";
    let sample = "https://cdn.fireworks.test/x.png";
    let generate = |model: Model| async move {
        suprnova_ai::ops::generate_image(
            &model,
            prompt("a cat", vec![], None),
            image_params(None, None, None, None),
            Default::default(),
        )
        .await
    };
    // A models.dev provider for Fireworks, registered with a download
    // transport: the restored provider takes it from the registered one.
    let transport = Recorder::new(kontext_replies(sample, "Ready"));
    let downloads = Arc::new(Downloads::default());
    let metadata: ModelsDevProvider = serde_json::from_value(
        json!({"id": "fireworks-ai", "npm": "@ai-sdk/fireworks", "api": FIREWORKS, "env": []}),
    )
    .unwrap();
    let provider = OpenAICompatibleProvider::from_modelsdev_provider(
        &metadata,
        key_config(),
        transport.clone(),
    )
    .unwrap()
    .with_download_transport(downloads.clone());
    let mut registry = ProviderRegistry::new();
    openai::register(&mut registry, Arc::new(provider)).unwrap();
    openai::register_factories(&mut registry, transport.clone()).unwrap();
    let model = Model::new(model_id, registry.get_provider("fireworks-ai").unwrap());
    let restored = Model::from_value(model.to_value().unwrap(), &registry).unwrap();
    let item = generate(restored).await.unwrap();
    assert_eq!(png_bytes(&item.value[0]), [1, 2, 3]);
    assert_eq!(*downloads.urls.lock().unwrap(), [sample]);

    // A model with the Fireworks protocol set on it, from a registry made
    // with register_with_downloads: the restored protocol gets the
    // registry's download transport.
    let transport = Recorder::new(kontext_replies(sample, "Ready"));
    let downloads = Arc::new(Downloads::default());
    let mut registry = ProviderRegistry::new();
    fireworks::register_with_downloads(
        &mut registry,
        key_config(),
        transport.clone(),
        downloads.clone(),
    )
    .unwrap();
    let provider = registry.get_provider("fireworks").unwrap();
    let model =
        Model::new(model_id, provider.clone()).with_protocol(provider.default_protocol().unwrap());
    let restored = Model::from_value(model.to_value().unwrap(), &registry).unwrap();
    assert!(restored.protocol.is_some());
    let item = generate(restored).await.unwrap();
    assert_eq!(png_bytes(&item.value[0]), [1, 2, 3]);
    assert_eq!(*downloads.urls.lock().unwrap(), [sample]);
}

#[tokio::test]
async fn hst_028_to_hst_032_a_models_dev_provider_for_a_host_serves_its_model_kinds() {
    // Embeddings, under the host's entry, for the models.dev id fireworks-ai.
    let server = Arc::new(EmbedServer::default());
    let provider = modelsdev(
        "fireworks-ai",
        "@ai-sdk/fireworks",
        FIREWORKS,
        server.clone(),
    );
    let item = suprnova_ai::ops::embed(
        &Model::new("some-embedder", provider),
        vec!["a".into()],
        embed_params(Some(("fireworks", json!({"dimensions": 8})))),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(item.value.len(), 1);
    let body: Value = serde_json::from_slice(&server.requests.lock().unwrap()[0].body).unwrap();
    assert_eq!(body["dimensions"], 8, "{body}");
    // Images and reranking for Together AI.
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(PNG)}]}))]);
    let provider = modelsdev(
        "togetherai",
        "@ai-sdk/togetherai",
        "https://api.together.xyz/v1",
        transport.clone(),
    );
    let item = suprnova_ai::ops::generate_image(
        &Model::new("black-forest-labs/FLUX.1-dev", provider),
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(png_bytes(&item.value[0]), PNG);
    let transport = Recorder::new([rerank_reply()]);
    let provider = modelsdev(
        "togetherai",
        "@ai-sdk/togetherai",
        "https://api.together.xyz/v1",
        transport.clone(),
    );
    suprnova_ai::ops::rerank(
        &Model::new("Salesforce/Llama-Rank-v1", provider),
        Documents::Text(vec!["a".into()]),
        "q".into(),
        RerankParams::default(),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        transport.request().url,
        "https://api.together.xyz/v1/rerank"
    );
}

#[tokio::test]
async fn hst_027_a_models_dev_provider_for_a_host_keeps_its_completion_rules() {
    // The host's entry under its own name, and its error kinds.
    let transport = Recorder::new([completion_stream("hi")]);
    let provider = modelsdev(
        "fireworks-ai",
        "@ai-sdk/fireworks",
        FIREWORKS,
        transport.clone(),
    );
    let model = Model::new("some-model", provider)
        .with_protocol(Arc::new(fireworks::completion_protocol()));
    let (_, events) = call(
        &model,
        vec![user_message("hello")],
        entries(&[("fireworks", json!({"suffix": "end"}))]),
    )
    .await;
    ok(&events);
    assert_eq!(transport.body()["suffix"], "end");
    let transport = Recorder::new([(503, br#"{"error": {"message": "busy"}}"#.to_vec())]);
    let provider = modelsdev("fireworks-ai", "@ai-sdk/fireworks", FIREWORKS, transport);
    let model = Model::new("some-model", provider)
        .with_protocol(Arc::new(fireworks::completion_protocol()));
    let options = StreamOptions {
        retry: suprnova_ai::RetryPolicy::none(),
        ..Default::default()
    };
    let (_, events) = call(&model, vec![user_message("hello")], options).await;
    let Some(Err(Error::Provider(error))) = events.into_iter().find(Result::is_err) else {
        panic!("the call did not fail with a provider error");
    };
    assert_eq!(error.kind, ProviderErrorKind::ServiceUnavailable);
}

#[tokio::test]
async fn hst_030_a_models_dev_provider_lends_its_download_transport_to_images() {
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(PNG)}]}))]);
    let downloads = Arc::new(Downloads::default());
    let metadata: ModelsDevProvider = serde_json::from_value(json!({"id": "deepinfra",
        "npm": "@ai-sdk/deepinfra", "api": "https://api.deepinfra.com/v1/openai", "env": []}))
    .unwrap();
    let provider = OpenAICompatibleProvider::from_modelsdev_provider(
        &metadata,
        key_config(),
        transport.clone(),
    )
    .unwrap()
    .with_download_transport(downloads.clone());
    suprnova_ai::ops::generate_image(
        &Model::new("black-forest-labs/FLUX.1-Kontext-dev", Arc::new(provider)),
        prompt(
            "a cat",
            vec![MediaInput::Url("https://img.test/a.png".into())],
            None,
        ),
        image_params(None, None, None, None),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        *downloads.urls.lock().unwrap(),
        vec!["https://img.test/a.png".to_owned()]
    );
    assert_eq!(
        transport.request().url,
        "https://api.deepinfra.com/v1/openai/images/edits"
    );
}

#[tokio::test]
async fn hst_029_and_hst_030_an_image_response_above_8_mib_is_read() {
    // About 9 MiB of image bytes, base64 in a JSON response.
    let mut big = PNG.to_vec();
    big.resize(9 * 1024 * 1024, 0);
    let transport = Recorder::new([json_reply(json!({"data": [{"b64_json": b64(&big)}]}))]);
    let item = image(
        "togetherai",
        "black-forest-labs/FLUX.1-dev",
        transport,
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap();
    assert_eq!(png_bytes(&item.value[0]).len(), big.len());
    let transport = Recorder::new([json_reply(
        json!({"images": [format!("data:image/png;base64,{}", b64(&big))]}),
    )]);
    let item = image(
        "deepinfra",
        "black-forest-labs/FLUX-1-schnell",
        transport,
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap();
    assert_eq!(png_bytes(&item.value[0]).len(), big.len());
}

#[tokio::test]
async fn hst_027_a_completion_entry_does_not_replace_the_prompt_or_the_stream() {
    let transport = Recorder::new([completion_stream("hi")]);
    let model = completions_model(
        "deepinfra",
        deepinfra::completion_protocol,
        transport.clone(),
    );
    let entry = json!({"prompt": "other", "stop": ["x"], "stream": false,
        "stream_options": {"include_usage": true}, "repetition_penalty": 1.1});
    let (_, events) = call(
        &model,
        vec![user_message("hello")],
        entries(&[("deepinfra", entry)]),
    )
    .await;
    ok(&events);
    let body = transport.body();
    assert_ne!(body["prompt"], "other", "{body}");
    assert_ne!(body["stop"], json!(["x"]), "{body}");
    assert_eq!(body["stream"], true, "{body}");
    assert!(body.get("stream_options").is_none(), "{body}");
    assert_eq!(body["repetition_penalty"], 1.1, "{body}");
}

#[tokio::test(start_paused = true)]
async fn hst_031_image_bytes_are_asked_for_as_any_type_and_must_be_an_image() {
    let accept = |request: &HttpRequest| {
        request
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("accept"))
            .map(|(_, v)| v.clone())
    };
    let transport = Recorder::new([(200, PNG.to_vec())]);
    image(
        "fireworks",
        "accounts/fireworks/models/flux-1-dev-fp8",
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap();
    assert_eq!(accept(&transport.request()).as_deref(), Some("*/*"));
    let transport = Recorder::new(kontext_replies(
        "https://api.fireworks.ai/inference/v1/samples/x.png",
        "Ready",
    ));
    kontext_image(
        "accounts/fireworks/models/flux-kontext-pro",
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap();
    let requests = fireworks_requests(&transport);
    assert_eq!(accept(&requests[0]).as_deref(), Some("application/json"));
    assert_eq!(accept(&requests[3]).as_deref(), Some("*/*"));
    // A body that is not an image is an error, not a PNG.
    let transport = Recorder::new([json_reply(json!({"error": "not an image"}))]);
    let error = image(
        "fireworks",
        "accounts/fireworks/models/flux-1-dev-fp8",
        transport,
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Provider(_)), "{error:?}");
}

#[test]
fn hst_031_a_kontext_call_with_no_download_transport_nor_tokio_runtime_fails_as_validation() {
    // The missing download transport is what HST-031 names, so it is what
    // the call reports.
    let transport = Recorder::new(kontext_replies("unused", "Ready"));
    let error = futures::executor::block_on(image(
        "fireworks",
        "accounts/fireworks/models/flux-kontext-pro",
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    ))
    .unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
}

#[test]
fn hst_031_a_kontext_call_without_a_tokio_runtime_fails_before_a_request() {
    let transport = Recorder::new(kontext_replies("unused", "Ready"));
    let error = futures::executor::block_on(kontext_image(
        "accounts/fireworks/models/flux-kontext-pro",
        transport.clone(),
        prompt("a cat", vec![], None),
        image_params(None, None, None, None),
    ))
    .unwrap_err();
    assert!(matches!(error, Error::Configuration(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
}

// HST-033 to HST-041: DeepSeek and MiniMax, whole.

/// The options with the provider entry `key` added to their parameters.
fn with_entry(mut options: StreamOptions, key: &str, value: Value) -> StreamOptions {
    let params = options.params.take().unwrap_or_default();
    options.params = Some(params.with_provider_param(key, value));
    options
}
/// Options holding only a `deepseek` entry.
fn deepseek_entry(value: Value) -> StreamOptions {
    with_entry(StreamOptions::default(), "deepseek", value)
}

#[tokio::test]
async fn hst_033_deepseek_sends_its_options_as_typescript_sends_them() {
    let body = host_body(
        "deepseek",
        "deepseek-v4-flash",
        vec![user_message("hello")],
        with_effort(
            deepseek_entry(
                json!({"topLogprobs": 3, "userId": "user_1", "reasoningEffort": "xhigh"}),
            ),
            Some("low"),
        ),
    )
    .await;
    assert_eq!(body["logprobs"], true, "{body}");
    assert_eq!(body["top_logprobs"], 3, "{body}");
    assert_eq!(body["user_id"], "user_1", "{body}");
    assert_eq!(body["reasoning_effort"], "max", "{body}");
    assert_eq!(body["thinking"], json!({"type": "enabled"}), "{body}");
    for key in ["topLogprobs", "userId", "reasoningEffort", "deepseek"] {
        assert!(body.get(key).is_none(), "{key} in {body}");
    }
    let body = host_body(
        "deepseek",
        "deepseek-v4-flash",
        vec![user_message("hello")],
        deepseek_entry(json!({"logprobs": true})),
    )
    .await;
    assert_eq!(body["logprobs"], true, "{body}");
    assert!(body.get("top_logprobs").is_none(), "{body}");
    let body = host_body(
        "deepseek",
        "deepseek-v4-flash",
        vec![user_message("hello")],
        deepseek_entry(json!({"thinking": {"type": "adaptive"}})),
    )
    .await;
    assert_eq!(body["thinking"], json!({"type": "enabled"}), "{body}");
    // The option's effort wins over the core effort, which still turns
    // thinking on.
    let body = host_body(
        "deepseek",
        "deepseek-v4-flash",
        vec![user_message("hello")],
        with_effort(
            deepseek_entry(json!({"reasoningEffort": "medium"})),
            Some("low"),
        ),
    )
    .await;
    assert_eq!(body["reasoning_effort"], "high", "{body}");
    assert_eq!(body["thinking"], json!({"type": "enabled"}), "{body}");
}

#[tokio::test]
async fn hst_033_the_thinking_option_decides_thinking_and_the_sampling_rule() {
    // Disabled thinking takes a temperature and sends no effort.
    let body = host_body(
        "deepseek",
        "deepseek-v4-flash",
        vec![user_message("hello")],
        with_entry(
            temperature(0.2),
            "deepseek",
            json!({"thinking": {"type": "disabled"}, "reasoningEffort": "high"}),
        ),
    )
    .await;
    assert_eq!(body["thinking"], json!({"type": "disabled"}), "{body}");
    assert_eq!(body["temperature"], 0.2, "{body}");
    assert!(body.get("reasoning_effort").is_none(), "{body}");
    // Enabled thinking on a model that does not think by itself makes a
    // temperature fail (HST-009).
    let message = validation(
        "deepseek",
        "deepseek-chat",
        with_entry(
            temperature(0.2),
            "deepseek",
            json!({"thinking": {"type": "enabled"}}),
        ),
    )
    .await;
    assert!(message.contains("temperature"), "{message}");
}

#[tokio::test]
async fn hst_033_invalid_deepseek_entries_fail_before_a_request() {
    for value in [
        json!("fast"),
        json!({"topLogprobs": 21}),
        json!({"topLogprobs": 2.5}),
        json!({"topLogprobs": -1}),
        json!({"userId": "user 1"}),
        json!({"userId": "a".repeat(513)}),
        json!({"userId": ""}),
        json!({"thinking": {"type": "auto"}}),
        json!({"thinking": "enabled"}),
        json!({"reasoningEffort": "minimal"}),
        json!({"logprobs": "yes"}),
        json!({"unknown": 1}),
    ] {
        let message = validation(
            "deepseek",
            "deepseek-v4-flash",
            deepseek_entry(value.clone()),
        )
        .await;
        assert!(message.contains("deepseek"), "{value}: {message}");
    }
}

const DEEPSEEK_URL: &str = "https://api.deepseek.com";
const DEEPSEEK_BETA_URL: &str = "https://api.deepseek.com/beta";

/// A DeepSeek model on a base URL, since only one ending in `/beta` turns
/// the beta features on.
fn deepseek_model(base_url: &str, model_id: &str, transport: Arc<Recorder>) -> Model {
    let config = ProviderConfig {
        default_base_url: base_url.into(),
        ..key_config()
    };
    Model::new(model_id, Arc::new(deepseek::provider(config, transport)))
}
async fn deepseek_body(base_url: &str, messages: Vec<Message>, options: StreamOptions) -> Value {
    let transport = Recorder::new([chat_text("hi", "stop")]);
    let model = deepseek_model(base_url, "deepseek-v4-flash", transport.clone());
    let (_, events) = call(&model, messages, options).await;
    ok(&events);
    transport.body()
}
async fn deepseek_rejected(
    base_url: &str,
    messages: Vec<Message>,
    options: StreamOptions,
) -> Error {
    let transport = Recorder::new([chat_text("hi", "stop")]);
    let model = deepseek_model(base_url, "deepseek-v4-flash", transport.clone());
    let (_, events) = call(&model, messages, options).await;
    let error = events
        .into_iter()
        .find_map(Result::err)
        .expect("the call did not fail");
    assert_eq!(transport.count(), 0, "a request was sent");
    error
}
/// The message with this `deepseek` metadata.
fn with_deepseek(mut message: Message, metadata: Value) -> Message {
    message.provider_metadata = Some([("deepseek".to_owned(), metadata)].into());
    message
}
fn with_tools(tools: Vec<Tool>) -> StreamOptions {
    StreamOptions {
        tools,
        ..Default::default()
    }
}

#[tokio::test]
async fn hst_034_message_names_go_on_system_user_and_assistant_messages() {
    let png = FilePart::from_bytes(PNG.to_vec(), Some("image/png"), None).unwrap();
    let messages = vec![
        with_deepseek(
            Message::new(MessageRole::System, vec![text_part("be brief")]),
            json!({"name": "sys"}),
        ),
        with_deepseek(user_message("hello"), json!({"name": "usr"})),
        with_deepseek(
            assistant(vec![tool_call("call_1", "lookup")]),
            json!({"name": "asst"}),
        ),
        with_deepseek(
            tool_message(ToolResultPart::new(
                "call_1",
                "lookup",
                ToolResultValue::Json(json!("42")),
            )),
            json!({"name": "tool"}),
        ),
        // Other metadata keys are passed over, and a user message with parts
        // is named too.
        with_deepseek(user_file(png), json!({"name": "usr2", "other": 1})),
    ];
    let body = deepseek_body(DEEPSEEK_URL, messages, Default::default()).await;
    let sent = body["messages"].as_array().unwrap();
    assert_eq!(sent[0]["name"], "sys", "{body}");
    assert_eq!(sent[1]["name"], "usr", "{body}");
    assert_eq!(sent[2]["name"], "asst", "{body}");
    assert_eq!(sent[3]["role"], "tool", "{body}");
    assert!(sent[3].get("name").is_none(), "{body}");
    assert_eq!(sent[4]["name"], "usr2", "{body}");
    assert!(sent[4]["content"].is_array(), "{body}");
    for entry in sent {
        assert!(entry.get("prefix").is_none(), "{body}");
    }
}

#[tokio::test]
async fn hst_034_a_prefix_and_strict_tools_go_out_on_a_beta_base_url() {
    let messages = vec![
        user_message("hello"),
        with_deepseek(
            assistant(vec![text_part("The answer is")]),
            json!({"prefix": true}),
        ),
    ];
    let body = deepseek_body(DEEPSEEK_BETA_URL, messages, Default::default()).await;
    let last = body["messages"].as_array().unwrap().last().unwrap();
    assert_eq!(last["role"], "assistant", "{body}");
    assert_eq!(last["prefix"], true, "{body}");
    let body = deepseek_body(
        DEEPSEEK_BETA_URL,
        vec![user_message("hello")],
        with_tools(vec![lookup_tool().with_strict(true)]),
    )
    .await;
    assert_eq!(body["tools"][0]["function"]["strict"], true, "{body}");
    // A tool that is not strict sends no strict, on either base URL.
    for base_url in [DEEPSEEK_URL, DEEPSEEK_BETA_URL] {
        let body = deepseek_body(
            base_url,
            vec![user_message("hello")],
            with_tools(vec![lookup_tool()]),
        )
        .await;
        assert!(
            body["tools"][0]["function"].get("strict").is_none(),
            "{body}"
        );
    }
}

#[tokio::test]
async fn hst_034_a_misplaced_prefix_or_strict_tool_fails_before_a_request() {
    let prefixed = |message: Message| with_deepseek(message, json!({"prefix": true}));
    let answer = || assistant(vec![text_part("a")]);
    let result = || ToolResultPart::new("call_1", "lookup", ToolResultValue::Json(json!("42")));
    let cases: Vec<(&str, Vec<Message>, StreamOptions)> = vec![
        // A prefix on a user message, on a tool message, on an assistant
        // message that is not last, on the default base URL, and one that is
        // not true.
        (
            DEEPSEEK_BETA_URL,
            vec![prefixed(user_message("hello"))],
            Default::default(),
        ),
        (
            DEEPSEEK_BETA_URL,
            vec![
                user_message("hello"),
                assistant(vec![tool_call("call_1", "lookup")]),
                prefixed(tool_message(result())),
            ],
            Default::default(),
        ),
        (
            DEEPSEEK_BETA_URL,
            vec![
                user_message("hello"),
                prefixed(answer()),
                user_message("more"),
            ],
            Default::default(),
        ),
        (
            DEEPSEEK_URL,
            vec![user_message("hello"), prefixed(answer())],
            Default::default(),
        ),
        (
            DEEPSEEK_BETA_URL,
            vec![
                user_message("hello"),
                with_deepseek(answer(), json!({"prefix": false})),
            ],
            Default::default(),
        ),
        // A name that is not a string.
        (
            DEEPSEEK_URL,
            vec![with_deepseek(user_message("hello"), json!({"name": 7}))],
            Default::default(),
        ),
        // A strict tool on the default base URL, and one beside a tool that
        // is not strict.
        (
            DEEPSEEK_URL,
            vec![user_message("hello")],
            with_tools(vec![lookup_tool().with_strict(true)]),
        ),
        (
            DEEPSEEK_BETA_URL,
            vec![user_message("hello")],
            with_tools(vec![
                lookup_tool().with_strict(true),
                tools(&["other"]).remove(0),
            ]),
        ),
    ];
    for (index, (base_url, messages, options)) in cases.into_iter().enumerate() {
        let error = deepseek_rejected(base_url, messages, options).await;
        assert!(
            matches!(error, Error::Validation(_)),
            "case {index}: {error:?}"
        );
    }
}

/// A file part naming an uploaded file by its id.
fn file_id_part(id: &str, media_type: &str) -> FilePart {
    FilePart {
        id: format!("f-{id}"),
        data: suprnova_ai::types::messages::FileData::Text(id.into()),
        media_type: media_type.into(),
        filename: None,
        provider_metadata: None,
    }
}
/// The part with this `deepseek` metadata.
fn deepseek_file(mut file: FilePart, metadata: Value) -> FilePart {
    file.provider_metadata = Some([("deepseek".to_owned(), metadata)].into());
    file
}
fn file_result(files: Vec<FilePart>) -> ToolResultPart {
    ToolResultPart::new(
        "call_1",
        "lookup",
        ToolResultValue::Special(SpecialToolResult::Content(ContentOutput {
            output_type: ContentOutputType::Content,
            value: files.into_iter().map(ContentPart::File).collect(),
        })),
    )
}
fn image_url(path: &str) -> String {
    format!("https://img.test/{path}.png")
}

#[tokio::test]
async fn hst_035_deepseek_sends_image_parts_as_typescript_sends_them() {
    let url = image_url(&"a".repeat(79));
    assert_eq!(url.len(), 100);
    let png_data_url = format!("data:image/png;base64,{}", b64(PNG));
    let message = Message::new(
        MessageRole::User,
        vec![
            text_part("look"),
            Part::File(deepseek_file(
                FilePart::from_url(&url, Some("image/png")).unwrap(),
                json!({"imageDetail": "high"}),
            )),
            Part::File(FilePart::from_bytes(PNG.to_vec(), Some("image/png"), None).unwrap()),
            Part::File(deepseek_file(
                FilePart::from_bytes(PNG.to_vec(), Some("image/png"), Some("cat.png".into()))
                    .unwrap(),
                json!({"fileData": true}),
            )),
            Part::File(file_id_part("file-api-abc123", "image/png")),
            Part::File(FilePart::from_bytes(vec![1, 2, 3], Some("image/jpg"), None).unwrap()),
        ],
    );
    let body = deepseek_body(DEEPSEEK_URL, vec![message], Default::default()).await;
    let content = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(
        content[1],
        json!({"type": "image_url", "image_url": {"url": url, "detail": "high"}}),
        "{body}"
    );
    assert_eq!(
        content[2],
        json!({"type": "image_url", "image_url": {"url": png_data_url}}),
        "{body}"
    );
    assert_eq!(
        content[3],
        json!({"type": "file", "file_data": png_data_url, "filename": "cat.png"}),
        "{body}"
    );
    assert_eq!(
        content[4],
        json!({"type": "file", "file_id": "file-api-abc123"}),
        "{body}"
    );
    assert_eq!(
        content[5],
        json!({"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AQID"}}),
        "{body}"
    );
}

#[tokio::test]
async fn hst_035_deepseek_sends_tool_result_images_the_same_way_without_file_data() {
    let url = image_url("tool");
    let result = file_result(vec![
        file_id_part("file-api-abc123", "image/png"),
        deepseek_file(
            FilePart::from_url(&url, Some("image/png")).unwrap(),
            json!({"imageDetail": "low"}),
        ),
        // `fileData` is not read in a tool result.
        deepseek_file(
            FilePart::from_bytes(PNG.to_vec(), Some("image/png"), None).unwrap(),
            json!({"fileData": true}),
        ),
    ]);
    let body = deepseek_body(DEEPSEEK_URL, tool_round(result), Default::default()).await;
    let content = tool_entry(&body)["content"].as_array().unwrap();
    assert_eq!(
        content[0],
        json!({"type": "file", "file_id": "file-api-abc123"}),
        "{body}"
    );
    assert_eq!(
        content[1],
        json!({"type": "image_url", "image_url": {"url": url, "detail": "low"}}),
        "{body}"
    );
    assert_eq!(
        content[2],
        json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{}", b64(PNG))}}),
        "{body}"
    );
}

#[tokio::test]
async fn hst_035_invalid_deepseek_image_parts_fail_before_a_request() {
    let long_url = image_url(&"a".repeat(8172));
    assert_eq!(long_url.len(), 8193);
    let png = || FilePart::from_bytes(PNG.to_vec(), Some("image/png"), None).unwrap();
    let url = || FilePart::from_url(image_url("x"), Some("image/png")).unwrap();
    let parts = [
        FilePart::from_url(&long_url, Some("image/png")).unwrap(),
        deepseek_file(url(), json!({"fileData": true})),
        deepseek_file(png(), json!({"fileData": true, "imageDetail": "low"})),
        deepseek_file(png(), json!({"fileData": false})),
        deepseek_file(png(), json!({"imageDetail": "medium"})),
        deepseek_file(url(), json!({"imageDetail": 2})),
    ];
    for (index, part) in parts.into_iter().enumerate() {
        let error =
            deepseek_rejected(DEEPSEEK_URL, vec![user_file(part)], Default::default()).await;
        assert!(
            matches!(error, Error::Validation(_)),
            "part {index}: {error:?}"
        );
    }
    let result = file_result(vec![
        FilePart::from_url(&long_url, Some("image/png")).unwrap(),
    ]);
    let error = deepseek_rejected(DEEPSEEK_URL, tool_round(result), Default::default()).await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
}

#[tokio::test]
async fn hst_036_the_final_message_carries_deepseek_metadata() {
    let logprob =
        |token: &str| json!({"token": token, "logprob": -0.1, "bytes": null, "top_logprobs": []});
    let transport = Recorder::new([chat_sse(&[
        json!({"id":"r1","object":"chat.completion.chunk","model":"m","system_fingerprint":"fp_1",
            "choices":[{"index":0,"delta":{"role":"assistant","content":"hi"},"logprobs":{"content":[logprob("h")]}}]}),
        json!({"object":"chat.completion.chunk","system_fingerprint":"fp_1",
            "choices":[{"index":0,"delta":{"content":"!"},"logprobs":{"content":[logprob("!")],"reasoning_content":[logprob("r")]}}]}),
        json!({"object":"chat.completion.chunk",
            "choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{}"}}]}}]}),
        json!({"object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],
            "usage":{"prompt_tokens":100,"completion_tokens":5,"prompt_cache_hit_tokens":60,"prompt_cache_miss_tokens":40}}),
    ])]);
    let first = model("deepseek", "deepseek-v4-flash", transport);
    let (handle, events) = call(
        &first,
        vec![user_message("hello")],
        with_tools(tools(&["lookup"])),
    )
    .await;
    ok(&events);
    let md = handle.message().provider_metadata.as_ref().unwrap()["deepseek"].clone();
    assert_eq!(
        md,
        json!({
            "promptCacheHitTokens": 60,
            "promptCacheMissTokens": 40,
            "responseObject": "chat.completion.chunk",
            "choiceIndex": 0,
            "messageRole": "assistant",
            "toolCallTypes": ["function"],
            "logprobs": {"content": [logprob("h"), logprob("!")], "reasoning_content": [logprob("r")]},
            "systemFingerprint": "fp_1",
        })
    );
    // Nothing the stream did not carry is reported.
    let transport = Recorder::new([chat_text("hi", "stop")]);
    let second = model("deepseek", "deepseek-v4-flash", transport);
    let (handle, events) = call(&second, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    let md = handle.message().provider_metadata.as_ref().unwrap()["deepseek"].clone();
    for key in [
        "logprobs",
        "systemFingerprint",
        "responseObject",
        "promptCacheHitTokens",
        "promptCacheMissTokens",
        "toolCallTypes",
        "messageRole",
    ] {
        assert!(md.get(key).is_none(), "{key} in {md}");
    }
    assert_eq!(md["choiceIndex"], 0, "{md}");
}

// HST-037

use suprnova_ai::providers::operations::{UploadFile, UploadParams};

fn deepseek_provider(transport: Arc<Recorder>) -> Arc<dyn Provider> {
    registry(host("deepseek"), key_config(), transport)
        .get_provider("deepseek")
        .unwrap()
}
fn upload(data: &[u8], media_type: &str, filename: Option<&str>) -> UploadFile {
    UploadFile {
        data: Bytes::copy_from_slice(data),
        media_type: media_type.into(),
        filename: filename.map(str::to_owned),
    }
}
fn upload_params(entry: Option<Value>) -> UploadParams {
    UploadParams {
        provider_options: entry
            .map(|value| [("deepseek".to_owned(), value)].into())
            .unwrap_or_default(),
    }
}
fn form_text(transport: &Recorder) -> String {
    String::from_utf8_lossy(&transport.request().body).into_owned()
}
fn form_field(name: &str, value: &str) -> String {
    format!("name=\"{name}\"\r\n\r\n{value}\r\n")
}
async fn deepseek_upload(
    transport: Arc<Recorder>,
    file: UploadFile,
    entry: Option<Value>,
) -> Result<
    suprnova_ai::providers::operations::Item<suprnova_ai::providers::operations::UploadedFile>,
    Error,
> {
    let provider = deepseek_provider(transport);
    suprnova_ai::ops::upload_file(
        provider.as_ref(),
        file,
        upload_params(entry),
        Default::default(),
    )
    .await
}

#[tokio::test]
async fn hst_037_deepseek_uploads_a_file_as_typescript_uploads_it() {
    let transport = Recorder::new([json_reply(json!({
        "id": "file-api-xyz789", "object": "file", "bytes": 24, "created_at": 1800000000,
        "expires_at": 1800003600, "filename": "a.png", "purpose": "user_data"
    }))]);
    let uploaded = deepseek_upload(
        transport.clone(),
        upload(PNG, "image/png", Some("a.png")),
        Some(json!({"expiresAfter": 3600})),
    )
    .await
    .unwrap();
    let request = transport.request();
    assert_eq!(request.url, "https://api.deepseek.com/files");
    assert_eq!(request.method, "POST");
    assert_eq!(
        transport.header("authorization").as_deref(),
        Some("Bearer sk-test")
    );
    let sent = form_text(&transport);
    assert!(sent.contains(&form_field("purpose", "user_data")), "{sent}");
    assert!(
        sent.contains(&form_field("expires_after[anchor]", "created_at")),
        "{sent}"
    );
    assert!(
        sent.contains(&form_field("expires_after[seconds]", "3600")),
        "{sent}"
    );
    assert!(
        sent.contains("name=\"file\"; filename=\"a.png\"\r\nContent-Type: image/png\r\n\r\n"),
        "{sent}"
    );
    assert!(request.body.windows(PNG.len()).any(|window| window == PNG));
    let value = &uploaded.value;
    assert_eq!(
        value.provider_reference,
        [("deepseek".to_owned(), "file-api-xyz789".to_owned())].into()
    );
    assert_eq!(value.filename.as_deref(), Some("a.png"));
    assert_eq!(value.media_type.as_deref(), Some("image/png"));
    assert_eq!(value.size_bytes, Some(24));
    assert_eq!(
        value.created_at.as_deref(),
        Some("2027-01-15T08:00:00.000Z")
    );
    assert_eq!(
        value.expires_at.as_deref(),
        Some("2027-01-15T09:00:00.000Z")
    );
    assert_eq!(
        uploaded.provider_metadata.as_ref().unwrap()["deepseek"],
        json!({"object": "file", "filename": "a.png", "purpose": "user_data", "bytes": 24,
            "createdAt": 1800000000, "expiresAt": 1800003600})
    );
    // Without an expiry option no expiry goes, and what the response lacks
    // is absent; the given filename stands in for the response's.
    let transport = Recorder::new([json_reply(json!({"id": "file-2"}))]);
    let uploaded = deepseek_upload(
        transport.clone(),
        upload(PNG, "image/png", Some("b.png")),
        None,
    )
    .await
    .unwrap();
    assert!(!form_text(&transport).contains("expires_after"));
    let value = &uploaded.value;
    assert_eq!(value.filename.as_deref(), Some("b.png"));
    assert_eq!(value.size_bytes, None);
    assert_eq!(value.created_at, None);
    assert_eq!(value.expires_at, None);
    assert_eq!(
        uploaded.provider_metadata.as_ref().unwrap()["deepseek"],
        json!({})
    );
}

#[tokio::test]
async fn hst_037_invalid_deepseek_uploads_fail_before_a_request() {
    let big = vec![0u8; 65 * 1024 * 1024];
    let long_name = "a".repeat(513);
    let cases: Vec<(UploadFile, Option<Value>)> = vec![
        (upload(&big, "image/png", Some("a.png")), None),
        (upload(PNG, "image/png", Some(&long_name)), None),
        (upload(PNG, "application/pdf", Some("a.png")), None),
        (upload(b"%PDF-1.4", "image/jpeg", Some("a.jpg")), None),
        (
            upload(b"\x00\x01\x02", "application/octet-stream", Some("a.bin")),
            None,
        ),
        (
            upload(PNG, "image/png", Some("a.png")),
            Some(json!({"expiresAfter": 60})),
        ),
        (
            upload(PNG, "image/png", Some("a.png")),
            Some(json!({"expiresAfter": 3600.5})),
        ),
        (upload(PNG, "image/png", Some("a.png")), Some(json!("soon"))),
    ];
    for (index, (file, entry)) in cases.into_iter().enumerate() {
        let transport = Recorder::new([json_reply(json!({"id": "file-1"}))]);
        let error = deepseek_upload(transport.clone(), file, entry)
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Validation(_)),
            "case {index}: {error:?}"
        );
        assert_eq!(transport.count(), 0, "case {index}");
    }
    // A generic type passes with a detected image or a known extension, and
    // a declared type is read before its parameters, in any case.
    for (index, file) in [
        upload(PNG, "application/octet-stream", None),
        upload(b"\x00\x01\x02", "", Some("a.webp")),
        upload(PNG, "image/*", Some("x")),
        upload(PNG, "IMAGE/PNG; charset=binary", None),
    ]
    .into_iter()
    .enumerate()
    {
        let transport = Recorder::new([json_reply(json!({"id": "file-1"}))]);
        deepseek_upload(transport.clone(), file, None)
            .await
            .unwrap_or_else(|error| panic!("case {index}: {error:?}"));
        assert_eq!(transport.count(), 1, "case {index}");
    }
}

#[tokio::test]
async fn hst_037_deepseek_file_info_download_and_delete_are_unsupported() {
    let transport = Recorder::new([]);
    let provider = deepseek_provider(transport.clone());
    let reference = [("deepseek".to_owned(), "file-1".to_owned())].into();
    let info = suprnova_ai::ops::file_info(provider.as_ref(), &reference, Default::default())
        .await
        .unwrap_err();
    assert!(matches!(info, Error::UnsupportedOperation(_)), "{info:?}");
    let download =
        suprnova_ai::ops::download_file(provider.as_ref(), &reference, Default::default())
            .await
            .unwrap_err();
    assert!(
        matches!(download, Error::UnsupportedOperation(_)),
        "{download:?}"
    );
    let delete = suprnova_ai::ops::delete_file(provider.as_ref(), &reference, Default::default())
        .await
        .unwrap_err();
    assert!(
        matches!(delete, Error::UnsupportedOperation(_)),
        "{delete:?}"
    );
    assert_eq!(transport.count(), 0);
}

// HST-038

/// Options with `anthropic` and `minimax` entries, over disabled core
/// reasoning when `reasoning` is false.
fn minimax_entries(anthropic: Option<Value>, minimax: Option<Value>) -> StreamOptions {
    let mut options = StreamOptions::default();
    if let Some(value) = anthropic {
        options = with_entry(options, "anthropic", value);
    }
    if let Some(value) = minimax {
        options = with_entry(options, "minimax", value);
    }
    options
}

#[tokio::test]
async fn hst_038_the_minimax_entry_is_read_over_the_anthropic_entry() {
    let body = minimax_body(
        "MiniMax-M3.1",
        minimax_entries(
            Some(json!({"effort": "high"})),
            Some(json!({"effort": "low"})),
        ),
    )
    .await;
    assert_eq!(body["output_config"]["effort"], "low", "{body}");
    // An effort in the entry wins over the core reasoning setting.
    let body = minimax_body(
        "MiniMax-M3.1",
        with_effort(
            minimax_entries(None, Some(json!({"effort": "medium"}))),
            None,
        ),
    )
    .await;
    assert_eq!(body["output_config"]["effort"], "medium", "{body}");
    assert!(body.get("thinking").is_none(), "{body}");
    let body = minimax_body(
        "MiniMax-M2",
        with_effort(
            minimax_entries(None, Some(json!({"effort": "high"}))),
            Some("low"),
        ),
    )
    .await;
    assert_eq!(body["output_config"]["effort"], "high", "{body}");
    // The entry takes every anthropic option.
    let body = minimax_body(
        "MiniMax-M2",
        minimax_entries(None, Some(json!({"thinking": {"type": "disabled"}}))),
    )
    .await;
    assert_eq!(body["thinking"], json!({"type": "disabled"}), "{body}");
    let body = minimax_body(
        "MiniMax-M2",
        minimax_entries(
            Some(json!({"thinking": {"type": "disabled"}, "metadata": {"userId": "u1"}})),
            Some(json!({"thinking": {"type": "enabled", "budgetTokens": 2048}})),
        ),
    )
    .await;
    assert_eq!(
        body["thinking"],
        json!({"type": "enabled", "budget_tokens": 2048}),
        "{body}"
    );
    assert_eq!(body["metadata"], json!({"user_id": "u1"}), "{body}");
}

#[tokio::test]
async fn hst_038_the_final_metadata_is_also_given_under_minimax_when_the_entry_was_used() {
    let transport = Recorder::new([messages_text()]);
    let first = model("minimax", "MiniMax-M3.1", transport);
    let (handle, events) = call(
        &first,
        vec![user_message("hello")],
        minimax_entries(None, Some(json!({"effort": "low"}))),
    )
    .await;
    ok(&events);
    let metadata = handle.message().provider_metadata.clone().unwrap();
    assert_eq!(metadata["minimax"], metadata["anthropic"], "{metadata:?}");
    assert_eq!(
        metadata["minimax"]["stopReason"], "end_turn",
        "{metadata:?}"
    );
    let transport = Recorder::new([messages_text()]);
    let second = model("minimax", "MiniMax-M3.1", transport);
    let (handle, events) = call(
        &second,
        vec![user_message("hello")],
        minimax_entries(Some(json!({"effort": "low"})), None),
    )
    .await;
    ok(&events);
    let metadata = handle.message().provider_metadata.clone().unwrap();
    assert!(!metadata.contains_key("minimax"), "{metadata:?}");
    assert_eq!(
        metadata["anthropic"]["stopReason"], "end_turn",
        "{metadata:?}"
    );
}

#[tokio::test]
async fn hst_038_an_invalid_minimax_entry_fails_before_a_request() {
    for value in [
        json!("fast"),
        json!({"effort": "huge"}),
        json!({"unknown": 1}),
    ] {
        let message = validation(
            "minimax",
            "MiniMax-M3.1",
            minimax_entries(None, Some(value.clone())),
        )
        .await;
        assert!(message.contains("minimax"), "{value}: {message}");
    }
}

#[tokio::test]
async fn hst_037_a_file_time_the_clock_cannot_hold_is_left_out() {
    let transport = Recorder::new([json_reply(json!({
        "id": "file-3", "created_at": 18446744073709551615u64, "expires_at": 1800003600
    }))]);
    let uploaded = deepseek_upload(transport, upload(PNG, "image/png", None), None)
        .await
        .unwrap();
    assert_eq!(uploaded.value.created_at, None);
    assert_eq!(
        uploaded.value.expires_at.as_deref(),
        Some("2027-01-15T09:00:00.000Z")
    );
}

// HST-039

/// The body a MiniMax model sends for these messages; the call must succeed.
async fn minimax_messages_body(model_id: &str, messages: Vec<Message>) -> Value {
    let transport = Recorder::new([messages_text()]);
    let model = model("minimax", model_id, transport.clone());
    let (_, events) = call(&model, messages, Default::default()).await;
    ok(&events);
    transport.body()
}
/// The part with this `minimax` metadata.
fn minimax_file(mut file: FilePart, metadata: Value) -> FilePart {
    file.provider_metadata = Some([("minimax".to_owned(), metadata)].into());
    file
}
fn mp4_url() -> FilePart {
    FilePart::from_url("https://v.test/a.mp4", Some("video/mp4")).unwrap()
}

#[tokio::test]
async fn hst_039_minimax_3_1_sends_images_and_videos_as_its_own_blocks() {
    let mut text = TextPart::new("look");
    text.provider_metadata = Some(
        [(
            "minimax".to_owned(),
            json!({"cacheControl": {"type": "ephemeral"}}),
        )]
        .into(),
    );
    let message = Message::new(
        MessageRole::User,
        vec![
            Part::Text(text),
            Part::File(minimax_file(
                FilePart::from_bytes(PNG.to_vec(), Some("image/png"), None).unwrap(),
                json!({"detail": "high"}),
            )),
            Part::File(minimax_file(mp4_url(), json!({"fps": 2}))),
            Part::File(file_id_part("mm_file://abc", "video/mp4")),
            Part::File(FilePart::from_url(image_url("u"), Some("image/png")).unwrap()),
        ],
    );
    let body = minimax_messages_body("MiniMax-M3.1", vec![message]).await;
    let content = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(
        content[0],
        json!({"type": "text", "text": "look", "cache_control": {"type": "ephemeral"}}),
        "{body}"
    );
    assert_eq!(
        content[1],
        json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64(PNG), "detail": "high"}}),
        "{body}"
    );
    assert_eq!(
        content[2],
        json!({"type": "video", "source": {"type": "url", "url": "https://v.test/a.mp4", "fps": 2}}),
        "{body}"
    );
    assert_eq!(
        content[3],
        json!({"type": "video", "source": {"type": "url", "url": "mm_file://abc"}}),
        "{body}"
    );
    assert_eq!(
        content[4],
        json!({"type": "image", "source": {"type": "url", "url": image_url("u")}}),
        "{body}"
    );
    // The `anthropic` cache control still applies when `minimax` sets none,
    // and a message's `minimax` cache control marks its last part.
    let mut anthropic_text = TextPart::new("old");
    anthropic_text.provider_metadata = Some(
        [(
            "anthropic".to_owned(),
            json!({"cacheControl": {"type": "ephemeral", "ttl": "1h"}}),
        )]
        .into(),
    );
    let mut message = Message::new(
        MessageRole::User,
        vec![Part::Text(anthropic_text), text_part("last")],
    );
    message.provider_metadata = Some(
        [(
            "minimax".to_owned(),
            json!({"cacheControl": {"type": "ephemeral"}}),
        )]
        .into(),
    );
    let body = minimax_messages_body("MiniMax-M3.1", vec![message]).await;
    let content = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(
        content[0]["cache_control"],
        json!({"type": "ephemeral", "ttl": "1h"}),
        "{body}"
    );
    assert_eq!(
        content[1]["cache_control"],
        json!({"type": "ephemeral"}),
        "{body}"
    );
}

#[tokio::test]
async fn hst_039_earlier_minimax_models_keep_the_anthropic_conversion() {
    // A video fails as the Anthropic adapter fails it.
    let error = rejected(
        "minimax",
        "MiniMax-M2",
        vec![user_file(mp4_url())],
        Default::default(),
    )
    .await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    // An image goes as an Anthropic image block, without MiniMax's detail,
    // and the `minimax` cache control is not read.
    let mut text = TextPart::new("look");
    text.provider_metadata = Some(
        [(
            "minimax".to_owned(),
            json!({"cacheControl": {"type": "ephemeral"}}),
        )]
        .into(),
    );
    let message = Message::new(
        MessageRole::User,
        vec![
            Part::Text(text),
            Part::File(minimax_file(
                FilePart::from_bytes(PNG.to_vec(), Some("image/png"), None).unwrap(),
                json!({"detail": "high"}),
            )),
        ],
    );
    let body = minimax_messages_body("MiniMax-M2", vec![message]).await;
    let content = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(
        content[0],
        json!({"type": "text", "text": "look"}),
        "{body}"
    );
    assert_eq!(
        content[1],
        json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64(PNG)}}),
        "{body}"
    );
}

#[tokio::test]
async fn hst_039_invalid_minimax_media_parts_fail_before_a_request() {
    let png = || FilePart::from_bytes(PNG.to_vec(), Some("image/png"), None).unwrap();
    let parts = [
        file_id_part("mm_file://abc", "image/png"),
        minimax_file(png(), json!({"detail": "auto"})),
        minimax_file(mp4_url(), json!({"fps": 10})),
        minimax_file(mp4_url(), json!({"fps": "2"})),
    ];
    for (index, part) in parts.into_iter().enumerate() {
        let error = rejected(
            "minimax",
            "MiniMax-M3.1",
            vec![user_file(part)],
            Default::default(),
        )
        .await;
        assert!(
            matches!(error, Error::Validation(_)),
            "part {index}: {error:?}"
        );
    }
}

// HST-040

use suprnova_ai::providers::operations::{FrameImage, FrameType, VideoParams, VideoPrompt};

const MINIMAX_URL: &str = "https://api.minimax.io/anthropic";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF";

/// A MiniMax video model on a provider with a download transport, which a
/// video call needs, on this base URL.
fn minimax_video_model(
    base_url: &str,
    model_id: &str,
    transport: Arc<Recorder>,
    downloads: Option<Arc<Downloads>>,
) -> Model {
    let config = ProviderConfig {
        default_base_url: base_url.into(),
        ..key_config()
    };
    let provider = minimax::provider(config, transport);
    let provider = match downloads {
        Some(downloads) => provider.with_download_transport(downloads),
        None => provider,
    };
    Model::new(model_id, Arc::new(provider))
}
fn video_prompt(text: &str) -> VideoPrompt {
    VideoPrompt {
        text: Some(text.into()),
        ..Default::default()
    }
}
fn video_params(entry: Option<Value>) -> VideoParams {
    VideoParams {
        provider_options: entry
            .map(|value| [("minimax".to_owned(), value)].into())
            .unwrap_or_default(),
        ..Default::default()
    }
}
fn frame(image: MediaInput, frame_type: FrameType) -> FrameImage {
    FrameImage { image, frame_type }
}
fn url_input(url: &str, media_type: &str) -> MediaInput {
    MediaInput::File(FilePart::from_url(url, Some(media_type)).unwrap())
}
/// Start a job on a MiniMax model; the request it sent and the started item.
async fn minimax_start(
    model_id: &str,
    prompt: VideoPrompt,
    params: VideoParams,
) -> (
    Arc<Recorder>,
    suprnova_ai::providers::operations::Item<suprnova_ai::providers::operations::VideoJob>,
) {
    let transport = Recorder::new([json_reply(json!({"task_id": "t1"}))]);
    let model = minimax_video_model(
        MINIMAX_URL,
        model_id,
        transport.clone(),
        Some(Arc::new(Downloads::default())),
    );
    let item = suprnova_ai::ops::start_video(&model, prompt, params, Default::default())
        .await
        .unwrap();
    (transport, item)
}
fn content_roles(body: &Value) -> Vec<(String, String)> {
    body["content"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|part| {
            (
                part["type"].as_str().unwrap().to_owned(),
                part["role"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn hst_040_a_text_to_video_call_sends_the_body_typescript_sends() {
    let (transport, item) =
        minimax_start("MiniMax-H3", video_prompt("a cat"), video_params(None)).await;
    let request = transport.request();
    assert_eq!(request.url, "https://api.minimax.io/v2/video_generation");
    assert_eq!(request.method, "POST");
    assert_eq!(
        transport.header("authorization").as_deref(),
        Some("Bearer sk-test")
    );
    assert!(transport.header("x-api-key").is_none());
    assert_eq!(
        transport.body(),
        json!({"model": "MiniMax-H3", "content": [{"type": "text", "text": "a cat"}],
            "resolution": "2K", "duration": 5, "ratio": "16:9"})
    );
    assert!(item.warnings.is_empty(), "{:?}", item.warnings);
    assert_eq!(item.value.operation, json!("t1"));
    assert_eq!(
        item.provider_metadata.as_ref().unwrap()["minimax"],
        json!({"taskId": "t1", "resolvedInputs": {"imageCount": 0, "referenceVideoUrls": []}})
    );
    // Another model takes every resolution, and a named tier in any case.
    let (transport, _) = minimax_start(
        "MiniMax-Hailuo-02",
        video_prompt("a cat"),
        VideoParams {
            resolution: Some("480p".into()),
            aspect_ratio: Some("9:16".into()),
            ..video_params(None)
        },
    )
    .await;
    let body = transport.body();
    assert_eq!(body["resolution"], "480P", "{body}");
    assert_eq!(body["ratio"], "9:16", "{body}");
}

#[tokio::test]
async fn hst_040_frames_set_the_resolution_tier_and_drop_the_ratio() {
    let prompt = VideoPrompt {
        text: Some("a cat".into()),
        image: None,
        frame_images: vec![
            frame(MediaInput::Bytes(PNG.to_vec()), FrameType::FirstFrame),
            frame(
                MediaInput::File(
                    FilePart::from_bytes(JPEG.to_vec(), Some("image/jpeg"), None).unwrap(),
                ),
                FrameType::LastFrame,
            ),
        ],
        references: vec![],
    };
    let params = VideoParams {
        duration: Some(20),
        resolution: Some("1366x768".into()),
        aspect_ratio: Some("1:1".into()),
        ..video_params(None)
    };
    let (transport, item) = minimax_start("MiniMax-H3-Max", prompt, params).await;
    let body = transport.body();
    assert_eq!(
        body["content"][1],
        json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{}", b64(PNG))}, "role": "first_frame"}),
        "{body}"
    );
    assert_eq!(
        body["content"][2],
        json!({"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", b64(JPEG))}, "role": "last_frame"}),
        "{body}"
    );
    assert_eq!(body["content"].as_array().unwrap().len(), 3, "{body}");
    assert_eq!(body["resolution"], "768P", "{body}");
    assert_eq!(body["duration"], 15, "{body}");
    assert!(body.get("ratio").is_none(), "{body}");
    assert!(
        warned(&item.warnings, "unsupported", Some("duration")),
        "{:?}",
        item.warnings
    );
    assert!(
        warned(&item.warnings, "unsupported", Some("aspectRatio")),
        "{:?}",
        item.warnings
    );
    assert_eq!(
        item.provider_metadata.as_ref().unwrap()["minimax"]["resolvedInputs"],
        json!({"imageCount": 2, "referenceVideoUrls": []})
    );
    // A plain image is the first frame, and a last frame without a first is
    // warned about and left out.
    let prompt = VideoPrompt {
        text: None,
        image: Some(MediaInput::Url("https://img.test/first.png".into())),
        frame_images: vec![],
        references: vec![],
    };
    let (transport, _) = minimax_start("MiniMax-H3", prompt, video_params(None)).await;
    let body = transport.body();
    assert_eq!(
        body["content"][0],
        json!({"type": "text", "text": ""}),
        "{body}"
    );
    assert_eq!(
        body["content"][1],
        json!({"type": "image_url", "image_url": {"url": "https://img.test/first.png"}, "role": "first_frame"}),
        "{body}"
    );
    let prompt = VideoPrompt {
        text: Some("a cat".into()),
        image: None,
        frame_images: vec![frame(MediaInput::Bytes(PNG.to_vec()), FrameType::LastFrame)],
        references: vec![],
    };
    let (transport, item) = minimax_start("MiniMax-H3", prompt, video_params(None)).await;
    let body = transport.body();
    assert_eq!(body["content"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["ratio"], "16:9", "{body}");
    assert!(
        warned(&item.warnings, "unsupported", Some("frameImages")),
        "{:?}",
        item.warnings
    );
}

#[tokio::test]
async fn hst_040_references_go_by_media_type_within_their_limits() {
    let prompt = VideoPrompt {
        text: Some("a cat".into()),
        image: None,
        frame_images: vec![],
        references: vec![
            url_input("https://img.test/1.png", "image/png"),
            url_input("https://img.test/2.png", "image/png"),
            url_input("https://v.test/a.mp4", "video/mp4"),
        ],
    };
    let params = VideoParams {
        duration: Some(3),
        ..video_params(Some(json!({
            "ratio": "adaptive", "aigcWatermark": true,
            "referenceAudioUrls": ["https://a.test/1.mp3", "https://a.test/2.mp3", "https://a.test/3.mp3", "https://a.test/4.mp3"]
        })))
    };
    let (transport, item) = minimax_start("MiniMax-H3", prompt, params).await;
    let body = transport.body();
    assert_eq!(
        content_roles(&body),
        vec![
            ("image_url".to_owned(), "reference_image".to_owned()),
            ("image_url".to_owned(), "reference_image".to_owned()),
            ("video_url".to_owned(), "reference_video".to_owned()),
            ("audio_url".to_owned(), "reference_audio".to_owned()),
            ("audio_url".to_owned(), "reference_audio".to_owned()),
            ("audio_url".to_owned(), "reference_audio".to_owned()),
        ],
        "{body}"
    );
    assert_eq!(
        body["content"][3],
        json!({"type": "video_url", "video_url": {"url": "https://v.test/a.mp4"}, "role": "reference_video"}),
        "{body}"
    );
    assert_eq!(
        body["content"][4],
        json!({"type": "audio_url", "audio_url": {"url": "https://a.test/1.mp3"}, "role": "reference_audio"}),
        "{body}"
    );
    assert_eq!(body["ratio"], "adaptive", "{body}");
    assert_eq!(body["aigc_watermark"], true, "{body}");
    assert_eq!(body["duration"], 4, "{body}");
    assert!(
        warned(&item.warnings, "unsupported", Some("referenceAudioUrls")),
        "{:?}",
        item.warnings
    );
    assert!(
        warned(&item.warnings, "unsupported", Some("duration")),
        "{:?}",
        item.warnings
    );
    assert_eq!(
        item.provider_metadata.as_ref().unwrap()["minimax"]["resolvedInputs"],
        json!({"imageCount": 2, "referenceVideoUrls": ["https://v.test/a.mp4"]})
    );
    // A URL reference without a media type goes as an image with a warning;
    // MiniMax-H3-Max takes no references.
    let prompt = VideoPrompt {
        text: Some("a cat".into()),
        image: None,
        frame_images: vec![],
        references: vec![MediaInput::Url("https://img.test/x".into())],
    };
    let (transport, item) = minimax_start("MiniMax-H3", prompt.clone(), video_params(None)).await;
    let body = transport.body();
    assert_eq!(
        content_roles(&body),
        vec![("image_url".to_owned(), "reference_image".to_owned())],
        "{body}"
    );
    assert!(body.get("ratio").is_none(), "{body}");
    assert!(
        warned(&item.warnings, "unsupported", Some("inputReferences")),
        "{:?}",
        item.warnings
    );
    let (transport, item) = minimax_start("MiniMax-H3-Max", prompt, video_params(None)).await;
    let body = transport.body();
    assert_eq!(body["content"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["resolution"], "768P", "{body}");
    assert!(
        warned(&item.warnings, "unsupported", Some("inputReferences")),
        "{:?}",
        item.warnings
    );
}

#[tokio::test]
async fn hst_040_fps_seed_and_audio_are_warned_about_and_left_out() {
    let params = VideoParams {
        fps: Some(24),
        seed: Some(7),
        generate_audio: Some(true),
        ..video_params(None)
    };
    let (transport, item) = minimax_start("MiniMax-H3", video_prompt("a cat"), params).await;
    let body = transport.body();
    for key in ["fps", "seed", "generate_audio", "generateAudio"] {
        assert!(body.get(key).is_none(), "{key} in {body}");
    }
    for feature in ["fps", "seed", "generateAudio"] {
        assert!(
            warned(&item.warnings, "unsupported", Some(feature)),
            "{feature}: {:?}",
            item.warnings
        );
    }
}

#[tokio::test]
async fn hst_040_a_video_call_that_cannot_be_served_fails_before_a_request() {
    let start = |base_url: &str, downloads: bool, prompt: VideoPrompt, params: VideoParams| {
        let transport = Recorder::new([json_reply(json!({"task_id": "t1"}))]);
        let model = minimax_video_model(
            base_url,
            "MiniMax-H3",
            transport.clone(),
            downloads.then(|| Arc::new(Downloads::default())),
        );
        async move {
            let error = suprnova_ai::ops::start_video(&model, prompt, params, Default::default())
                .await
                .unwrap_err();
            assert_eq!(transport.count(), 0, "{error:?}");
            error
        }
    };
    let error = start(
        MINIMAX_URL,
        false,
        video_prompt("a cat"),
        video_params(None),
    )
    .await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    let error = start(
        "https://proxy.test/v1",
        true,
        video_prompt("a cat"),
        video_params(None),
    )
    .await;
    assert!(matches!(error, Error::Configuration(_)), "{error:?}");
    let prompt = VideoPrompt {
        image: Some(MediaInput::Bytes(vec![1, 2, 3])),
        ..video_prompt("a cat")
    };
    let error = start(MINIMAX_URL, true, prompt, video_params(None)).await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    for entry in [
        json!({"resolution": "1080P"}),
        json!({"ratio": "2:1"}),
        json!({"aigcWatermark": "yes"}),
        json!({"referenceAudioUrls": "https://a.test/1.mp3"}),
        json!({"pollTimeoutMs": 1000}),
        json!({"pollIntervalMs": 1000}),
        json!({"unknown": 1}),
        json!("fast"),
    ] {
        let error = start(
            MINIMAX_URL,
            true,
            video_prompt("a cat"),
            video_params(Some(entry.clone())),
        )
        .await;
        assert!(matches!(error, Error::Validation(_)), "{entry}: {error:?}");
    }
}

// HST-041

use suprnova_ai::providers::operations::{VideoJob, VideoStatus};

fn task(status: &str, extra: Value) -> (u16, Vec<u8>) {
    let mut task = json!({"id": "t1", "status": status});
    if let (Some(task), Some(extra)) = (task.as_object_mut(), extra.as_object()) {
        task.extend(extra.clone());
    }
    json_reply(json!({"task": task}))
}
fn minimax_job(model_id: &str) -> VideoJob {
    VideoJob {
        provider: "minimax".into(),
        model_id: model_id.into(),
        operation: json!("t1"),
    }
}
async fn minimax_status(replies: Vec<(u16, Vec<u8>)>) -> Result<VideoStatus, Error> {
    let transport = Recorder::new(replies);
    let model = minimax_video_model(
        MINIMAX_URL,
        "MiniMax-H3",
        transport,
        Some(Arc::new(Downloads::default())),
    );
    suprnova_ai::ops::video_status(&model, &minimax_job("MiniMax-H3"), Default::default()).await
}

#[tokio::test(start_paused = true)]
async fn hst_041_a_job_is_checked_with_the_key_and_its_video_fetched_without_it() {
    let video_url = "https://cdn.minimax.test/v.mp4";
    let transport = Recorder::new([
        json_reply(json!({"task_id": "t1"})),
        task("running", json!({})),
        task(
            "succeeded",
            json!({"content": {"url": video_url}, "duration": 5, "ratio": "16:9",
                "resolution": "2K", "usage": {"total_seconds": 5}}),
        ),
    ]);
    let downloads = Arc::new(Downloads::default());
    let model = minimax_video_model(
        MINIMAX_URL,
        "MiniMax-H3",
        transport.clone(),
        Some(downloads.clone()),
    );
    let prompt = VideoPrompt {
        text: Some("a cat".into()),
        image: Some(MediaInput::Bytes(PNG.to_vec())),
        frame_images: vec![],
        references: vec![],
    };
    let options = suprnova_ai::ops::VideoOptions {
        poll_interval: std::time::Duration::from_secs(1),
        ..Default::default()
    };
    let item = suprnova_ai::ops::generate_video(&model, prompt, video_params(None), options)
        .await
        .unwrap();
    let requests = transport.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3, "{requests:?}");
    for request in &requests[1..] {
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.url,
            "https://api.minimax.io/v2/query/video_generation/t1"
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("authorization")
                    && value == "Bearer sk-test"),
            "{request:?}"
        );
    }
    assert_eq!(*downloads.urls.lock().unwrap(), vec![video_url.to_owned()]);
    let download_headers = downloads.headers.lock().unwrap()[0].clone();
    assert!(
        !download_headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("authorization")
                || value.contains("sk-test")),
        "{download_headers:?}"
    );
    assert_eq!(item.value.len(), 1);
    assert_eq!(item.value[0].media_type, "video/mp4");
    assert_eq!(item.value[0].data.as_bytes().unwrap(), &[1, 2, 3]);
    assert_eq!(
        item.provider_metadata.as_ref().unwrap()["minimax"],
        json!({
            "taskId": "t1", "videoUrl": video_url,
            "resolvedInputs": {"imageCount": 1, "referenceVideoUrls": []},
            "duration": 5, "ratio": "16:9", "resolution": "2K", "usage": {"totalSeconds": 5}
        })
    );
    assert!(item.warnings.is_empty(), "{:?}", item.warnings);
}

#[tokio::test]
async fn hst_041_a_failed_cancelled_or_expired_task_fails_and_others_are_pending() {
    let status = minimax_status(vec![task(
        "failed",
        json!({"error": {"message": "m", "code": 7}}),
    )])
    .await
    .unwrap();
    let VideoStatus::Failed { message } = status else {
        panic!("{status:?}");
    };
    for needle in ["m", "7", "t1"] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
    for status in ["cancelled", "expired"] {
        let result = minimax_status(vec![task(status, json!({}))]).await.unwrap();
        let VideoStatus::Failed { message } = result else {
            panic!("{status}: {result:?}");
        };
        assert!(message.contains("t1"), "{status}: {message}");
    }
    for status in ["queued", "running", "processing"] {
        let result = minimax_status(vec![task(status, json!({}))]).await.unwrap();
        assert_eq!(result, VideoStatus::Pending, "{status}");
    }
    let completed = minimax_status(vec![task(
        "succeeded",
        json!({"content": {"url": "https://cdn.minimax.test/v.mp4"}}),
    )])
    .await
    .unwrap();
    let VideoStatus::Completed(item) = completed else {
        panic!("{completed:?}");
    };
    assert_eq!(
        item.value[0].data.as_text(),
        Some("https://cdn.minimax.test/v.mp4")
    );
    assert_eq!(
        item.provider_metadata.as_ref().unwrap()["minimax"],
        json!({"taskId": "t1", "videoUrl": "https://cdn.minimax.test/v.mp4"})
    );
}

#[tokio::test]
async fn hst_041_a_missing_task_id_or_url_and_an_error_body_are_reported() {
    let error = minimax_status(vec![task("succeeded", json!({"content": {}}))])
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Provider(_)), "{error:?}");
    assert!(error.to_string().contains("t1"), "{error}");
    let transport = Recorder::new([json_reply(json!({"object": "task"}))]);
    let model = minimax_video_model(
        MINIMAX_URL,
        "MiniMax-H3",
        transport,
        Some(Arc::new(Downloads::default())),
    );
    let error = suprnova_ai::ops::start_video(
        &model,
        video_prompt("a cat"),
        video_params(None),
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Provider(_)), "{error:?}");
    let error = minimax_status(vec![(400, br#"{"error": {"message": "bad"}}"#.to_vec())])
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Provider(_)), "{error:?}");
    assert!(error.to_string().contains("bad"), "{error}");
}

#[tokio::test(start_paused = true)]
async fn hst_041_a_minimax_video_model_makes_one_video_per_job() {
    let done = || {
        task(
            "succeeded",
            json!({"content": {"url": "https://cdn.minimax.test/v.mp4"}}),
        )
    };
    let transport = Recorder::new([
        json_reply(json!({"task_id": "t1"})),
        json_reply(json!({"task_id": "t2"})),
        done(),
        done(),
    ]);
    let model = minimax_video_model(
        MINIMAX_URL,
        "MiniMax-H3",
        transport.clone(),
        Some(Arc::new(Downloads::default())),
    );
    let params = VideoParams {
        n: 2,
        ..video_params(None)
    };
    let options = suprnova_ai::ops::VideoOptions {
        poll_interval: std::time::Duration::from_secs(1),
        ..Default::default()
    };
    let item = suprnova_ai::ops::generate_video(&model, video_prompt("a cat"), params, options)
        .await
        .unwrap();
    let starts = transport
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request.url.ends_with("/v2/video_generation"))
        .count();
    assert_eq!(starts, 2);
    assert_eq!(item.value.len(), 2);
}

#[test]
fn hst_040_a_video_call_without_a_tokio_runtime_fails_before_a_request() {
    // Polling needs a Tokio timer, so a job that could not be polled is not
    // started and billed; the missing download transport is reported first.
    let transport = Recorder::new([json_reply(json!({"task_id": "t1"}))]);
    let model = minimax_video_model(
        MINIMAX_URL,
        "MiniMax-H3",
        transport.clone(),
        Some(Arc::new(Downloads::default())),
    );
    let error = futures::executor::block_on(suprnova_ai::ops::start_video(
        &model,
        video_prompt("a cat"),
        video_params(None),
        Default::default(),
    ))
    .unwrap_err();
    assert!(matches!(error, Error::Configuration(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
    let transport = Recorder::new([json_reply(json!({"task_id": "t1"}))]);
    let model = minimax_video_model(MINIMAX_URL, "MiniMax-H3", transport.clone(), None);
    let error = futures::executor::block_on(suprnova_ai::ops::start_video(
        &model,
        video_prompt("a cat"),
        video_params(None),
        Default::default(),
    ))
    .unwrap_err();
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    assert_eq!(transport.count(), 0);
}

#[tokio::test]
async fn hst_040_a_start_asked_for_several_videos_warns_and_starts_one_job() {
    let params = VideoParams {
        n: 2,
        ..video_params(None)
    };
    let (transport, item) = minimax_start("MiniMax-H3", video_prompt("a cat"), params).await;
    assert_eq!(transport.count(), 1);
    assert!(
        warned(&item.warnings, "unsupported", Some("n")),
        "{:?}",
        item.warnings
    );
    // generate_video gives each job one video, so it never warns.
    let (_, item) = minimax_start("MiniMax-H3", video_prompt("a cat"), video_params(None)).await;
    assert!(item.warnings.is_empty(), "{:?}", item.warnings);
}

#[tokio::test]
async fn hst_039_an_uploaded_file_id_is_refused_for_a_minimax_image_and_sent_for_a_video() {
    // An Anthropic `file_` id names an uploaded file too (ANT-010): no image
    // may name one, and a video's id goes as an mm_file:// URL.
    let error = rejected(
        "minimax",
        "MiniMax-M3.1",
        vec![user_file(file_id_part("file_abc", "image/png"))],
        Default::default(),
    )
    .await;
    assert!(matches!(error, Error::Validation(_)), "{error:?}");
    let body = minimax_messages_body(
        "MiniMax-M3.1",
        vec![user_file(file_id_part("file_abc", "video/mp4"))],
    )
    .await;
    assert_eq!(
        body["messages"][0]["content"][1],
        json!({"type": "video", "source": {"type": "url", "url": "mm_file://file_abc"}}),
        "{body}"
    );
}

#[tokio::test]
async fn hst_038_a_null_minimax_entry_reads_nothing_and_copies_no_metadata() {
    let transport = Recorder::new([messages_text()]);
    let model = model("minimax", "MiniMax-M3.1", transport.clone());
    let (handle, events) = call(
        &model,
        vec![user_message("hello")],
        minimax_entries(None, Some(Value::Null)),
    )
    .await;
    ok(&events);
    let metadata = handle.message().provider_metadata.clone().unwrap();
    assert!(!metadata.contains_key("minimax"), "{metadata:?}");
    assert!(transport.body().get("output_config").is_none());
}

// HST-042 to HST-047: Mistral, whole.

/// Options holding only a `mistral` entry.
fn mistral_entry(value: Value) -> StreamOptions {
    with_entry(StreamOptions::default(), "mistral", value)
}
/// The output schema of the Mistral tests, as the body carries it.
fn answer_schema() -> Value {
    json!({"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}},"required":["a"]})
}
fn with_output_schema(mut options: StreamOptions, schema: &Value) -> StreamOptions {
    options.output_schema = Some(schema.clone().try_into().unwrap());
    options.output_type_name = Some("answer".into());
    options
}

#[tokio::test]
async fn hst_042_mistral_sends_its_options_as_typescript_sends_them() {
    let mut options = params(|p| {
        p.cache = Some(CacheParams {
            key: Some("core".into()),
            ..Default::default()
        });
        p.tool_calling = Some(ToolCallingParams {
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: ProviderValue::Value(true),
            max_tool_calls: ProviderValue::Default,
        });
    });
    options.tools = tools(&["lookup"]);
    let options = with_effort(
        with_entry(
            options,
            "mistral",
            json!({"safePrompt": true, "documentImageLimit": 4, "documentPageLimit": 16, "promptCacheKey": "k", "parallelToolCalls": false, "reasoningEffort": "none"}),
        ),
        Some("high"),
    );
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        options,
    )
    .await;
    assert_eq!(body["safe_prompt"], true, "{body}");
    assert_eq!(body["document_image_limit"], 4, "{body}");
    assert_eq!(body["document_page_limit"], 16, "{body}");
    // The entry's cache key and parallel tool calls replace the core ones.
    assert_eq!(body["prompt_cache_key"], "k", "{body}");
    assert_eq!(body["parallel_tool_calls"], false, "{body}");
    // The entry's effort replaces the mapping of the core effort.
    assert_eq!(body["reasoning_effort"], "none", "{body}");
    for key in [
        "safePrompt",
        "documentImageLimit",
        "documentPageLimit",
        "promptCacheKey",
        "parallelToolCalls",
        "reasoningEffort",
        "mistral",
    ] {
        assert!(body.get(key).is_none(), "{key} in {body}");
    }
    // Without tools, parallel tool calls stay out (HST-014).
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        mistral_entry(json!({"parallelToolCalls": false, "reasoningEffort": "high"})),
    )
    .await;
    assert!(body.get("parallel_tool_calls").is_none(), "{body}");
    assert_eq!(body["reasoning_effort"], "high", "{body}");
    assert!(body.get("safe_prompt").is_none(), "{body}");
}

#[tokio::test]
async fn hst_042_strict_schemas_and_json_object_mode_follow_the_entry() {
    let schema = answer_schema();
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        with_output_schema(mistral_entry(json!({"strictJsonSchema": true})), &schema),
    )
    .await;
    assert_eq!(
        body["response_format"],
        json!({"type": "json_schema", "json_schema": {"name": "answer", "strict": true, "schema": schema}}),
        "{body}"
    );
    // JSON-object mode puts the instruction at the end of a first system
    // message, after a blank line.
    let instruction = format!(
        "JSON schema:\n{}\nYou MUST answer with a JSON object that matches the JSON schema above.",
        serde_json::to_string(&schema).unwrap()
    );
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![system_message("S"), user_message("hello")],
        with_output_schema(mistral_entry(json!({"structuredOutputs": false})), &schema),
    )
    .await;
    assert_eq!(
        body["response_format"],
        json!({"type": "json_object"}),
        "{body}"
    );
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2, "{body}");
    assert_eq!(
        messages[0],
        json!({"role": "system", "content": format!("S\n\n{instruction}")}),
        "{body}"
    );
    assert_eq!(messages[1]["role"], "user", "{body}");
    // Or in a new first system message when the first message is not one.
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello"), system_message("later")],
        with_output_schema(mistral_entry(json!({"structuredOutputs": false})), &schema),
    )
    .await;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3, "{body}");
    assert_eq!(
        messages[0],
        json!({"role": "system", "content": instruction}),
        "{body}"
    );
    assert_eq!(messages[1]["role"], "user", "{body}");
    assert_eq!(
        messages[2],
        json!({"role": "system", "content": "later"}),
        "{body}"
    );
    // Without the entry, the schema goes out as a JSON schema that is not
    // strict, and the system message stays as it is (HST-014).
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![system_message("S"), user_message("hello")],
        with_output_schema(StreamOptions::default(), &schema),
    )
    .await;
    assert_eq!(
        body["response_format"]["json_schema"]["strict"], false,
        "{body}"
    );
    assert_eq!(body["messages"][0]["content"], "S", "{body}");
}

#[tokio::test]
async fn hst_042_a_strict_tool_goes_out_with_strict_true() {
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        with_tools(vec![lookup_tool().with_strict(true)]),
    )
    .await;
    assert_eq!(body["tools"][0]["function"]["strict"], true, "{body}");
    let body = host_body(
        "mistral",
        "mistral-small-latest",
        vec![user_message("hello")],
        with_tools(vec![lookup_tool()]),
    )
    .await;
    assert!(
        body["tools"][0]["function"].get("strict").is_none(),
        "{body}"
    );
}

#[tokio::test]
async fn hst_042_invalid_mistral_entries_fail_before_a_request() {
    for value in [
        json!([]),
        json!({"reasoningEffort": "medium"}),
        json!({"safePrompt": "yes"}),
        json!({"topK": 3}),
        json!({"documentImageLimit": "4"}),
        json!({"documentPageLimit": true}),
        json!({"promptCacheKey": 1}),
        json!({"parallelToolCalls": "no"}),
        json!({"structuredOutputs": 1}),
        json!({"strictJsonSchema": "no"}),
    ] {
        let message = validation(
            "mistral",
            "mistral-small-latest",
            mistral_entry(value.clone()),
        )
        .await;
        assert!(message.contains("mistral"), "{value}: {message}");
    }
    // A model without reasoning settings takes no effort from the entry
    // either (HST-014).
    let message = validation(
        "mistral",
        "mistral-large-latest",
        mistral_entry(json!({"reasoningEffort": "high"})),
    )
    .await;
    assert!(message.contains("mistral-large-latest"), "{message}");
}

#[tokio::test]
async fn hst_042_cached_tokens_come_from_prompt_tokens_details() {
    for (usage, cached) in [
        (
            json!({"prompt_tokens":3,"completion_tokens":4,"prompt_tokens_details":{"cached_tokens":4}}),
            4,
        ),
        (
            json!({"prompt_tokens":3,"completion_tokens":4,"num_cached_tokens":5,"prompt_tokens_details":{"cached_tokens":4}}),
            5,
        ),
        (
            json!({"prompt_tokens":3,"completion_tokens":4,"prompt_token_details":{"cached_tokens":6}}),
            6,
        ),
    ] {
        let transport = Recorder::new([chat_sse(&[
            json!({"choices":[{"index":0,"delta":{"role":"assistant","content":"D"},"finish_reason":"stop"}],"usage":usage}),
        ])]);
        let mistral = model("mistral", "mistral-medium-latest", transport);
        let (handle, events) =
            call(&mistral, vec![user_message("hello")], Default::default()).await;
        ok(&events);
        assert_eq!(
            handle.usage().unwrap().cache_read_tokens,
            Some(cached),
            "{usage}"
        );
    }
}

// HST-043 and HST-044: the Conversations API.

/// A Conversations stream of these events.
fn conversation_sse(events: &[Value]) -> (u16, Vec<u8>) {
    chat_sse(events)
}
fn conversation_started() -> Value {
    json!({"type":"conversation.response.started","conversation_id":"conv_1","created_at":"2026-10-24T00:00:00Z"})
}
/// A finished Conversations stream that answers `text`.
fn conversation_text(text: &str) -> (u16, Vec<u8>) {
    conversation_sse(&[
        conversation_started(),
        json!({"type":"message.output.delta","id":"m1","content_index":0,"model":"mistral-medium-2604","content":text}),
        json!({"type":"conversation.response.done","usage":{"prompt_tokens":1,"completion_tokens":1}}),
    ])
}
fn conversations_model(model_id: &str, transport: Arc<Recorder>) -> Model {
    model("mistral", model_id, transport).with_protocol(mistral::conversations())
}
/// The one request a Conversations call sends, and its body.
async fn conversations_request(
    model_id: &str,
    messages: Vec<Message>,
    options: StreamOptions,
) -> (HttpRequest, Value) {
    let transport = Recorder::new([conversation_text("{\"value\":1}")]);
    let model = conversations_model(model_id, transport.clone());
    let (_, events) = call(&model, messages, options).await;
    ok(&events);
    (transport.request(), transport.body())
}
async fn conversations_body(
    model_id: &str,
    messages: Vec<Message>,
    options: StreamOptions,
) -> Value {
    conversations_request(model_id, messages, options).await.1
}
/// The validation message a Conversations call fails with before any request.
async fn conversations_validation(
    model_id: &str,
    messages: Vec<Message>,
    options: StreamOptions,
) -> String {
    let transport = Recorder::new([conversation_text("hi")]);
    let model = conversations_model(model_id, transport.clone());
    let (_, events) = call(&model, messages, options).await;
    let error = events
        .into_iter()
        .find_map(Result::err)
        .expect("the call did not fail");
    assert_eq!(transport.count(), 0, "a request was sent");
    match error {
        Error::Validation(message) => message,
        other => panic!("{other:?}"),
    }
}
fn builtin_call(id: &str, name: &str, args: &str, metadata: Value) -> Part {
    Part::BuiltinToolCall(BuiltinToolCallPart {
        id: format!("b-{id}"),
        tool_call_id: id.into(),
        tool_name: name.into(),
        tool_args: args.into(),
        provider_metadata: Some([("mistral".to_owned(), metadata)].into()),
    })
}
fn builtin_return(id: &str, name: &str, result: Value) -> Part {
    Part::BuiltinToolReturn(BuiltinToolReturnPart {
        id: format!("br-{id}"),
        tool_call_id: id.into(),
        tool_name: name.into(),
        result,
        is_error: false,
        provider_metadata: None,
    })
}
fn conversation_tools() -> Vec<Tool> {
    vec![tools(&["t"]).remove(0), mistral::web_search()]
}

#[tokio::test]
async fn hst_043_a_conversations_call_sends_the_body_typescript_sends() {
    let schema = answer_schema();
    let history = vec![
        system_message("S"),
        user_message("U"),
        assistant(vec![
            reasoning_part("R"),
            text_part("T"),
            Part::ToolCall(ToolCallPart::new("c1", "t", "{\"a\":1}")),
        ]),
        tool_message(ToolResultPart::new("c1", "t", json!({"b": 2}))),
        assistant(vec![
            builtin_call(
                "e1",
                "web_search",
                "{\"arguments\":\"q\"}",
                json!({"type": "tool.execution", "name": "web_search", "function": "web_search"}),
            ),
            builtin_return("e1", "web_search", json!({"info": {"result": "r"}})),
        ]),
        user_message("V"),
    ];
    let mut options = sampling(|s| {
        s.seed = Some(SeedSamplerParams {
            seed: ProviderValue::Value(Some(Seed::Value(7))),
        })
    });
    options.params.as_mut().unwrap().output = Some(OutputParams {
        max_tokens: Some(100),
        ..Default::default()
    });
    let mut options = with_effort(options, Some("low"));
    options.tools = vec![
        tools(&["t"]).remove(0).with_strict(true),
        mistral::web_search(),
    ];
    options.output_schema = Some(schema.clone().try_into().unwrap());
    let (request, body) = conversations_request("mistral-medium-latest", history, options).await;
    assert_eq!(request.url, "https://api.mistral.ai/v1/conversations");
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("authorization")
                && value == "Bearer sk-test"),
        "{:?}",
        request.headers
    );
    let expected = json!({
        "model": "mistral-medium-latest",
        "store": false,
        "stream": true,
        "instructions": "S",
        "inputs": [
            {"type": "message.input", "role": "user", "content": [{"type": "text", "text": "U"}]},
            {"type": "message.input", "role": "assistant", "content": [
                {"type": "thinking", "thinking": [{"type": "text", "text": "R"}], "closed": true},
                {"type": "text", "text": "T"}
            ]},
            {"type": "function.call", "tool_call_id": "c1", "name": "t", "arguments": "{\"a\":1}"},
            {"type": "function.result", "tool_call_id": "c1", "result": "{\"b\":2}"},
            {"type": "function.call", "tool_call_id": "e1", "name": "web_search", "arguments": "q"},
            {"type": "function.result", "tool_call_id": "e1", "result": "r"},
            {"type": "message.input", "role": "user", "content": [{"type": "text", "text": "V"}]}
        ],
        "tools": [
            {"type": "function", "function": {"name": "t", "description": "A tool", "parameters": {"type": "object", "properties": {"q": {"type": "integer"}}}, "strict": true}},
            {"type": "web_search"}
        ],
        "completion_args": {
            "max_tokens": 100,
            "random_seed": 7,
            "reasoning_effort": "high",
            "response_format": {"type": "json_schema", "json_schema": {"name": "output", "strict": false, "schema": schema}}
        }
    });
    assert_eq!(body, expected, "{body}");
}

#[tokio::test]
async fn hst_043_built_in_calls_are_replayed_from_their_metadata_and_results() {
    // A call without a function in its metadata goes by the metadata name,
    // then by the tool's wire name; arguments that are not a JSON object with
    // an `arguments` string go as given. A return without a string result
    // goes as the JSON of `info.result`, of `info`, or `null`.
    let history = vec![
        user_message("U"),
        assistant(vec![
            builtin_call(
                "e1",
                "web_search",
                "{\"arguments\":\"q\"}",
                json!({"name": "web_search_premium"}),
            ),
            builtin_return("e1", "web_search", json!({"info": {"result": {"k": 1}}})),
            builtin_call("e2", "web_search", "{\"x\":1}", json!({})),
            builtin_return("e2", "web_search", json!({"info": {"pages": 2}})),
            builtin_call("e3", "web_search", "", Value::Null),
            builtin_return("e3", "web_search", json!({})),
        ]),
    ];
    let body = conversations_body(
        "mistral-medium-latest",
        history,
        StreamOptions {
            tools: vec![mistral::web_search()],
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        body["inputs"],
        json!([
            {"type": "message.input", "role": "user", "content": [{"type": "text", "text": "U"}]},
            {"type": "function.call", "tool_call_id": "e1", "name": "web_search_premium", "arguments": "q"},
            {"type": "function.result", "tool_call_id": "e1", "result": "{\"k\":1}"},
            {"type": "function.call", "tool_call_id": "e2", "name": "web_search", "arguments": "{\"x\":1}"},
            {"type": "function.result", "tool_call_id": "e2", "result": "{\"pages\":2}"},
            {"type": "function.call", "tool_call_id": "e3", "name": "web_search", "arguments": ""},
            {"type": "function.result", "tool_call_id": "e3", "result": "null"}
        ]),
        "{body}"
    );
}

#[tokio::test]
async fn hst_043_a_named_tool_choice_keeps_the_named_function_or_web_search() {
    let choose = |name: &str| StreamOptions {
        tools: conversation_tools(),
        tool_choice: Some(ToolChoice::Tool(name.into())),
        ..Default::default()
    };
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        choose("web_search"),
    )
    .await;
    assert_eq!(body["tools"], json!([{"type": "web_search"}]), "{body}");
    assert_eq!(body["completion_args"]["tool_choice"], "any", "{body}");
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        choose("t"),
    )
    .await;
    assert_eq!(body["tools"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["tools"][0]["function"]["name"], "t", "{body}");
    assert_eq!(body["completion_args"]["tool_choice"], "any", "{body}");
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        StreamOptions {
            tools: vec![mistral::web_search_premium()],
            tool_choice: Some(ToolChoice::Required),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        body["tools"],
        json!([{"type": "web_search_premium"}]),
        "{body}"
    );
    assert_eq!(body["completion_args"]["tool_choice"], "any", "{body}");
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        StreamOptions {
            tools: conversation_tools(),
            tool_choice: Some(ToolChoice::None),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(body["tools"].as_array().unwrap().len(), 2, "{body}");
    assert_eq!(body["completion_args"]["tool_choice"], "none", "{body}");
    // Without tools, neither tools nor a tool choice goes out.
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        Default::default(),
    )
    .await;
    assert!(body.get("tools").is_none(), "{body}");
    assert!(
        body["completion_args"].get("tool_choice").is_none(),
        "{body}"
    );
    let message = conversations_validation(
        "mistral-medium-latest",
        vec![user_message("hi")],
        choose("x"),
    )
    .await;
    assert!(message.contains('x'), "{message}");
}

#[tokio::test]
async fn hst_043_json_mode_puts_the_instruction_in_the_instructions() {
    let schema = answer_schema();
    let instruction = format!(
        "JSON schema:\n{}\nYou MUST answer with a JSON object that matches the JSON schema above.",
        serde_json::to_string(&schema).unwrap()
    );
    let json_mode =
        || with_output_schema(mistral_entry(json!({"structuredOutputs": false})), &schema);
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        json_mode(),
    )
    .await;
    assert_eq!(
        body["completion_args"]["response_format"],
        json!({"type": "json_object"}),
        "{body}"
    );
    assert_eq!(body["instructions"], instruction, "{body}");
    let body = conversations_body(
        "mistral-medium-latest",
        vec![
            system_message("S"),
            user_message("hi"),
            system_message("later"),
        ],
        json_mode(),
    )
    .await;
    assert_eq!(
        body["instructions"],
        format!("S\n\n{instruction}\n\nlater"),
        "{body}"
    );
    // System messages join with a blank line; without one there are no
    // instructions.
    let body = conversations_body(
        "mistral-medium-latest",
        vec![system_message("A"), user_message("hi"), system_message("B")],
        Default::default(),
    )
    .await;
    assert_eq!(body["instructions"], "A\n\nB", "{body}");
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        Default::default(),
    )
    .await;
    assert!(body.get("instructions").is_none(), "{body}");
    assert!(
        body["completion_args"].get("response_format").is_none(),
        "{body}"
    );
    let body = conversations_body(
        "mistral-medium-latest",
        vec![user_message("hi")],
        with_output_schema(mistral_entry(json!({"strictJsonSchema": true})), &schema),
    )
    .await;
    assert_eq!(
        body["completion_args"]["response_format"]["json_schema"]["strict"], true,
        "{body}"
    );
}

#[tokio::test]
async fn hst_043_the_conversations_api_refuses_what_it_does_not_take() {
    // Reasoning settings on a model the API does not reason on.
    for options in [
        effort(Some("low")),
        effort(None),
        mistral_entry(json!({"reasoningEffort": "high"})),
    ] {
        let message =
            conversations_validation("mistral-large-latest", vec![user_message("hi")], options)
                .await;
        assert!(message.contains("mistral-large-latest"), "{message}");
    }
    // The chat-only options of the `mistral` entry.
    for (key, value) in [
        ("safePrompt", json!(true)),
        ("documentImageLimit", json!(4)),
        ("documentPageLimit", json!(16)),
        ("promptCacheKey", json!("k")),
        ("parallelToolCalls", json!(false)),
    ] {
        let message = conversations_validation(
            "mistral-medium-latest",
            vec![user_message("hi")],
            mistral_entry(json!({key: value})),
        )
        .await;
        assert!(message.contains(key), "{key}: {message}");
    }
    // A core cache key, parallel tool calls and a setting HST-008 names.
    let cache = params(|p| {
        p.cache = Some(CacheParams {
            key: Some("k1".into()),
            ..Default::default()
        })
    });
    let message =
        conversations_validation("mistral-medium-latest", vec![user_message("hi")], cache).await;
    assert!(message.contains("cache key"), "{message}");
    let mut parallel = params(|p| {
        p.tool_calling = Some(ToolCallingParams {
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: ProviderValue::Value(false),
            max_tool_calls: ProviderValue::Default,
        })
    });
    parallel.tools = tools(&["t"]);
    let message =
        conversations_validation("mistral-medium-latest", vec![user_message("hi")], parallel).await;
    assert!(message.contains("parallel tool calls"), "{message}");
    let identifier = params(|p| p.safety_identifier = Some("user-1".into()));
    let message = conversations_validation(
        "mistral-medium-latest",
        vec![user_message("hi")],
        identifier,
    )
    .await;
    assert!(message.contains("safety identifier"), "{message}");
    // Another provider's tool.
    let search = Tool::provider(
        "search",
        ToolConfig {
            id: Some("anthropic.web_search_20250305".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let message = conversations_validation(
        "mistral-medium-latest",
        vec![user_message("hi")],
        StreamOptions {
            tools: vec![search],
            ..Default::default()
        },
    )
    .await;
    assert!(
        message.contains("anthropic.web_search_20250305"),
        "{message}"
    );
}

#[tokio::test]
async fn hst_043_a_snapshot_of_a_conversations_model_restores_its_protocol() {
    let transport = Recorder::new([conversation_text("hi")]);
    let registry = registry(host("mistral"), key_config(), transport.clone());
    let model = Model::new(
        "mistral-medium-latest",
        registry.get_provider("mistral").unwrap(),
    )
    .with_protocol(mistral::conversations());
    let snapshot = model.to_value().unwrap();
    assert!(!snapshot.to_string().contains("sk-test"), "{snapshot}");
    let restored = Model::from_value(snapshot, &registry).unwrap();
    let (_, events) = call(&restored, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    assert_eq!(
        transport.request().url,
        "https://api.mistral.ai/v1/conversations"
    );
}

#[tokio::test]
async fn hst_044_a_conversations_stream_is_read_as_typescript_reads_it() {
    let transport = Recorder::new([conversation_sse(&[
        conversation_started(),
        json!({"type":"message.output.delta","id":"m1","content_index":0,"model":"mistral-medium-2604","content":{"type":"thinking","thinking":[{"type":"text","text":"A"}]}}),
        json!({"type":"message.output.delta","id":"m1","content_index":1,"content":{"type":"text","text":"B"}}),
        json!({"type":"tool.execution.started","id":"e1","name":"web_search","arguments":"{\"q"}),
        json!({"type":"tool.execution.delta","id":"e1","name":"web_search","arguments":"\":1}"}),
        json!({"type":"tool.execution.done","id":"e1","name":"web_search","function":"web_search","info":{"result":"r"}}),
        json!({"type":"message.output.delta","id":"m2","content_index":0,"content":{"type":"tool_reference","tool":"web_search","title":"A","url":"https://a.test"}}),
        json!({"type":"message.output.delta","id":"m2","content_index":0,"content":{"type":"tool_reference","tool":"web_search","url":"https://a.test"}}),
        json!({"type":"message.output.delta","id":"m2","content_index":0,"content":"C"}),
        json!({"type":"function.call.delta","tool_call_id":"c1","name":"t","arguments":"{}","output_index":0}),
        json!({"type":"conversation.response.done","usage":{"prompt_tokens":12,"completion_tokens":5,"num_cached_tokens":3}}),
    ])]);
    let model = conversations_model("mistral-medium-latest", transport);
    let options = StreamOptions {
        tools: conversation_tools(),
        ..Default::default()
    };
    let (handle, events) = call(&model, vec![user_message("hello")], options).await;
    ok(&events);
    let mut log: Vec<String> = Vec::new();
    let mut open: Option<String> = None;
    for event in events.iter().flatten() {
        match event {
            Event::ReasoningStart(start) => {
                assert!(open.is_none(), "a block was still open: {open:?}");
                open = Some(start.block_id.clone());
                log.push("reasoning:".into());
            }
            Event::TextStart(start) => {
                assert!(open.is_none(), "a block was still open: {open:?}");
                open = Some(start.block_id.clone());
                log.push("text:".into());
            }
            Event::ReasoningDelta(delta) => log.last_mut().unwrap().push_str(&delta.chunk),
            Event::TextDelta(delta) => log.last_mut().unwrap().push_str(&delta.chunk),
            Event::ReasoningEnd(end) => {
                assert_eq!(open.take().as_deref(), Some(end.block_id.as_str()));
            }
            Event::TextEnd(end) => {
                assert_eq!(open.take().as_deref(), Some(end.block_id.as_str()));
            }
            Event::BuiltinToolEnd(end) => {
                assert!(open.is_none(), "a block was still open: {open:?}");
                let call = &end.tool_call;
                log.push(format!(
                    "builtin:{}:{}:{}",
                    call.tool_call_id, call.tool_name, call.tool_args
                ));
                assert_eq!(
                    call.provider_metadata.as_ref().unwrap()["mistral"],
                    json!({"type": "tool.execution", "name": "web_search", "function": "web_search"})
                );
            }
            Event::BuiltinToolResult(result) => {
                let part = &result.result;
                log.push(format!(
                    "result:{}:{}:{}",
                    part.tool_call_id, part.tool_name, part.result
                ));
            }
            Event::ToolEnd(end) => {
                let call = &end.tool_call;
                log.push(format!(
                    "call:{}:{}:{}",
                    call.tool_call_id, call.tool_name, call.tool_args
                ));
            }
            _ => {}
        }
    }
    let args = json!({"arguments": "{\"q\":1}"}).to_string();
    assert_eq!(
        log,
        [
            "reasoning:A".to_owned(),
            "text:B".to_owned(),
            format!("builtin:e1:web_search:{args}"),
            "result:e1:web_search:{\"info\":{\"result\":\"r\"}}".to_owned(),
            "text:C".to_owned(),
            "call:c1:t:{}".to_owned(),
        ]
    );
    assert_eq!(handle.finish_reason(), Some("tool_call"));
    assert_eq!(handle.response_id(), Some("conv_1"));
    assert_eq!(handle.response_model(), Some("mistral-medium-2604"));
    let usage = handle.usage().unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens
        ),
        (12, 5, Some(3))
    );
    let sources = handle.message().provider_metadata.as_ref().unwrap()["mistral"]["sources"]
        .as_array()
        .cloned()
        .unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0]["sourceType"], "url");
    assert_eq!(sources[0]["url"], "https://a.test");
    assert_eq!(sources[0]["title"], "A");
    assert!(
        sources[0]["id"].as_str().is_some_and(|id| !id.is_empty()),
        "{sources:?}"
    );
}

#[tokio::test]
async fn hst_044_an_unfinished_stream_and_stream_errors_are_reported() {
    // A stream that ends without `conversation.response.done`, or with an
    // execution still open, finishes with `error`.
    for events in [
        vec![
            conversation_started(),
            json!({"type":"message.output.delta","id":"m1","content":"A"}),
        ],
        vec![
            conversation_started(),
            json!({"type":"tool.execution.started","id":"e1","name":"web_search","arguments":"{}"}),
            json!({"type":"conversation.response.done","usage":{"prompt_tokens":1,"completion_tokens":1}}),
        ],
    ] {
        let transport = Recorder::new([conversation_sse(&events)]);
        let model = conversations_model("mistral-medium-latest", transport);
        let options = StreamOptions {
            tools: vec![mistral::web_search()],
            ..Default::default()
        };
        let (handle, events) = call(&model, vec![user_message("hello")], options).await;
        ok(&events);
        assert_eq!(handle.finish_reason(), Some("error"));
    }
    // A finished stream of text alone stops, with zero usage when none came.
    let transport = Recorder::new([conversation_sse(&[
        conversation_started(),
        json!({"type":"message.output.delta","id":"m1","content":"A"}),
        json!({"type":"conversation.response.done","usage":{}}),
    ])]);
    let model = conversations_model("mistral-medium-latest", transport);
    let (handle, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    ok(&events);
    assert_eq!(handle.finish_reason(), Some("stop"));
    assert_eq!(handle.message().text(), "A");
    let usage = handle.usage().unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (0, 0));
    // An error event ends the call with a classified provider error.
    let transport = Recorder::new([conversation_sse(&[
        conversation_started(),
        json!({"type":"conversation.response.error","message":"m","code":429}),
    ])]);
    let model = conversations_model("mistral-medium-latest", transport);
    let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    let Some(Error::Provider(error)) = events.into_iter().find_map(Result::err) else {
        panic!("the error event did not end the call with a provider error");
    };
    assert_eq!(error.kind, ProviderErrorKind::RateLimit, "{error:?}");
    assert_eq!(error.message, "m");
    assert_eq!(error.code.as_deref(), Some("429"));
    // An execution that ends without a start is a response error.
    let transport = Recorder::new([conversation_sse(&[
        conversation_started(),
        json!({"type":"tool.execution.done","id":"e9","name":"web_search"}),
    ])]);
    let model = conversations_model("mistral-medium-latest", transport);
    let (_, events) = call(&model, vec![user_message("hello")], Default::default()).await;
    let Some(Error::Provider(error)) = events.into_iter().find_map(Result::err) else {
        panic!("the stray execution did not end the call with a provider error");
    };
    assert_eq!(error.kind, ProviderErrorKind::Response, "{error:?}");
    assert!(error.message.contains("e9"), "{}", error.message);
}
