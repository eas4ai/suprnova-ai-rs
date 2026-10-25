# suprnova-ai

A Rust library for language models, model operations and agents. It began as
a port of the AI SDK for Python 0.8.0 and follows the AI SDK for TypeScript
7.0.148, both by Vercel, and has changed from both in the ways `docs/PORT.md`
lists. It is licensed under the Apache License, Version 2.0; `LICENSE` holds
the license and `NOTICE` the credits.

The crate provides messages, events, tool schemas, stream aggregation, provider
contracts, streaming HTTP transport, OpenAI's APIs and the OpenAI-compatible and
Anthropic provider adapters (`providers::openai`, `providers::anthropic`), hosted providers on those
protocols (`providers::hosts`), Google's Gemini API and Vertex AI
(`providers::google`, `providers::google_vertex`), the model operations beyond
chat (`ops`), the agent loop with tools,
approval hooks and middleware (`agents`), and the AI SDK UI message stream adapter
(`ui::ai_sdk`). The provider registry starts empty; register the adapters you use.
The Vercel AI Gateway, OIDC, telemetry and the MCP client are not ported.

`ui::ai_sdk` converts Vercel AI SDK v6 UI requests into core messages and converts
core histories and event streams back to UI messages and SSE. It preserves the
Python 0.8.0 adapter protocol and message identity metadata.

```rust
use std::sync::Arc;
use suprnova_ai::{Model, ProviderConfig, ProviderProtocol, StreamOptions, stream};
use suprnova_ai::providers::{ConfiguredProvider, EventStream};
use suprnova_ai::types::{Event, Message};
use suprnova_ai::types::events::{StreamStart, StreamEnd, TextStart, TextDelta};
use futures::StreamExt;

struct ExampleProtocol;

#[async_trait::async_trait]
impl ProviderProtocol for ExampleProtocol {
    fn protocol_class_id(&self) -> &str { "example" }

    fn stream(
        &self,
        _model: &Model,
        _messages: Vec<Message>,
        _options: StreamOptions,
        _provider: &dyn suprnova_ai::Provider,
    ) -> EventStream {
        futures::stream::iter([
            Ok(Event::StreamStart(StreamStart::default())),
            Ok(Event::TextStart(TextStart::default())),
            Ok(Event::TextDelta(TextDelta::new("Hello"))),
            Ok(Event::StreamEnd(StreamEnd::default())),
        ]).boxed()
    }
}

# async fn example() -> Result<(), suprnova_ai::Error> {
let provider = Arc::new(ConfiguredProvider::new(
    "example", ProviderConfig::default(), Arc::new(ExampleProtocol), None,
));
let model = Model::new("example-model", provider);
let mut response = stream(&model, vec![suprnova_ai::user_message("Hello")], StreamOptions::default());
while let Some(event) = response.next().await {
    event?;
}
assert_eq!(response.text(), "Hello");
# Ok(())
# }
```

Use `StreamOptions::with_output::<T>()` with a `schemars::JsonSchema` type to send
an output schema. Read the result with `response.output::<T>()` for a
`serde::Deserialize` type. `generate` and `experimental_generate` return an
assistant message. They use native provider generation when available and fall
back to streaming when that operation is unsupported.

Provider configuration is explicit. The library does not read process environment
variables. Model IDs use `provider:model` and split at the first colon; an ID
without a colon is an error. Each registry handle names one provider.

Each `stream` or `generate` call retries a retryable error that arrives before
its first event. By default it retries twice, waiting 2 s and then 4 s, or the
provider's `retry-after` wait when that is under 60 s. Set `StreamOptions::retry`
to change this, or use `RetryPolicy::none()` to turn it off.
`StreamOptions::timeout` bounds the whole call and `StreamOptions::idle_timeout`
bounds the silence before each event; both are unset by default.
`ReqwestTransport::new()` limits only opening a connection, to 10 s, and uses TCP
keepalive to end a connection whose peer is gone. It sets no limit on a response,
because a model can think for minutes before or while it streams, so a provider
that keeps the connection open and stops answering is bounded only by these two
options. Retries and
timeouts use the Tokio timer, so run calls in a Tokio runtime with its time
driver enabled (`enable_time` or `enable_all`, which `#[tokio::main]` sets).
Without a runtime, a retryable error is returned without a retry and a call with a
timeout fails with a configuration error; in a runtime built without the time
driver, Tokio panics at the first wait. Dropping a call stops it, including a
pending retry wait.

`Model::to_value` leaves out credentials: the API key value, the `env` map and
the headers. `Model::from_value` takes them from the provider that the registry
holds under the snapshot's provider name, so register providers before restoring.
It also takes that provider's endpoint settings (the base URL and the names of its
key and URL variables), so a changed snapshot cannot send the key to another host
and an endpoint change applies to snapshots saved before it. A snapshot of another
provider class is refused.

OpenAI's Responses API stores each response unless the `openai` provider
parameter sets `{"store": false}`, and the history of a later call then refers to
stored reasoning and text by their item ids. OpenAI deletes stored items after a
while (30 days by default, by community reports), so a chat that resumes after
that needs `store` false: the crate then asks a reasoning model for its reasoning
as encrypted content and sends history in full. Tool calls always go back in
full, so each tool result pairs with its call.

`providers::openai` has every model kind of the TypeScript `@ai-sdk/openai`
4.0.99 package: the Responses, Chat Completions and Completions language
models (`OpenAIResponsesProtocol`, `OpenAIChatCompletionsProtocol` and
`OpenAICompletionsProtocol`), embeddings, images, speech, transcription and
its streaming form, speech translation, files, skills, batches, realtime and
Live sessions, and decisions (`OpenAIDecisionsProtocol`). Provider options are
the `openai` provider parameter, with the TypeScript option names; an unknown
option fails with `Error::Validation`. A setting that OpenAI takes for some
models but not for the call's model, such as a temperature on a reasoning
model, is left out and logged as a `tracing` debug event with target
`suprnova_ai::openai`. The `env` values `OPENAI_ORG_ID` and
`OPENAI_PROJECT_ID` set the organization and project headers. Build the
provider with `with_download_transport` to send the image, audio and PDF URLs
that OpenAI does not fetch, and with `with_websocket_transport` for streaming
transcription, translation and realtime sessions.
`AgentTool::with_provider_declaration` lets an agent run the tools that OpenAI
leaves to the application, such as `local_shell` and `apply_patch`.

`providers::anthropic` has every model kind of the TypeScript
`@ai-sdk/anthropic` 4.0.85 package: the Messages language model
(`AnthropicMessagesProtocol`), files, skills, batches and evaluation. Provider
options are the `anthropic` provider parameter, with the TypeScript option
names; an unknown option fails with `Error::Validation`. Where TypeScript
changes a setting with a warning, such as a temperature on a model that takes
none or disabled thinking on a model that always thinks, the crate makes the
same change and logs it as a `tracing` debug event with target
`suprnova_ai::anthropic`. Without an output limit a call asks for the model's
own limit, 128,000 tokens for current models; set one to cap a reply. An
`ANTHROPIC_AUTH_TOKEN` in the configuration's `env` goes as a bearer token in
place of the key, to Anthropic's own API only. `anthropic::tools` builds every
Anthropic tool version. Build the provider with `with_download_transport` to
send text file URLs, and pass `anthropic::container_id(&history)` as the next
call's `container` option to keep a code execution container.

`providers::hosts` has Cerebras, DeepInfra, DeepSeek, Fireworks, Mistral,
Moonshot AI, Together AI and Z.ai on Chat Completions, xAI and Hugging Face on
the Responses API, and MiniMax on the Anthropic Messages protocol. Register one with its module's `register`, for
example `hosts::cerebras::register(&mut registry, config, transport)`, under its
name (`cerebras`) and package handle (`@ai-sdk/cerebras`). Each host follows its
package in the AI SDK for TypeScript 7.0.148. Options that package drops with a
warning, such as Z.ai's penalties, seed and forced tool choices, fail here with
`Error::Validation` before any request; DeepSeek, Moonshot AI, Mistral, xAI and
Hugging Face also reject the request settings their packages never send, such as
a service tier.
An output schema goes to a host without JSON schema support as JSON mode, which
does not enforce the schema. Moonshot AI does not fetch media URLs: register it
with `hosts::moonshotai::register_with_downloads` to send image and video URLs.
DeepSeek reads its `deepseek` options entry, message names, the assistant
prefix and strict tools of a base URL ending in `/beta`, and image parts as
its package does, carries DeepSeek's metadata on the final message, and
uploads images through `ops::upload_file`. MiniMax reads its `minimax` entry
over the `anthropic` one; MiniMax 3.1 and later take image and video blocks
with `detail` and `fps`; and its video models (`MiniMax-H3`, `MiniMax-H3-Max`)
run as jobs through `ops::generate_video` on a provider registered with
`hosts::minimax::register_with_downloads`, which fetches the finished video.
xAI is ported for text: its built-in tools fail with
`Error::UnsupportedOperation`, and its `store` setting is the provider
parameter `xai` (`{"store": false}`).
Together AI, DeepInfra and Fireworks are ported whole. A chat call reads the
host's entry (`togetherai`, `deepinfra` or `fireworks`) as the TypeScript
compatible model does: `user`, `reasoningEffort`, `textVerbosity` and
`strictJsonSchema`, with any other key, such as `repetition_penalty`, sent in
the body; Fireworks also takes `promptCacheKey`, `serviceTier`, `thinking` and
`reasoningHistory`. Set `togetherai::completion_protocol()` (or DeepInfra's or
Fireworks') with `Model::with_protocol` for a completion model. `ops::embed`
and `ops::generate_image` work with any of their models, and `ops::rerank`
with Together AI's. Image calls make one image each and return the TypeScript
warnings in `Item.warnings`; DeepInfra fetches input image URLs, and Fireworks
a Kontext result on another server, through the provider's download
transport: register them with `register_with_downloads`, or build one with
`provider(...).with_download_transport(...)`. A Fireworks Kontext call
without a download transport fails before its job starts.
Hosts do not support `list_models` or `probe`.

`providers::google` is the Gemini API and `providers::google_vertex` is Vertex
AI, with every model kind of the TypeScript `@ai-sdk/google` and
`@ai-sdk/google-vertex` 7.0.148 packages. Register them with
`google::register` (handles `google` and `@ai-sdk/google`) and
`google_vertex::register_provider` (`google-vertex` and
`@ai-sdk/google-vertex`). The Gemini API key goes out as `x-goog-api-key`,
also on Live API sockets, and never in a URL. Build the Gemini provider with
`with_download_transport` to send the file URLs Gemini does not fetch and to
fetch a Veo video from another origin than the base URL's. Vertex AI takes its project and
location from the `env` values `GOOGLE_VERTEX_PROJECT` and
`GOOGLE_VERTEX_LOCATION`, and signs in with an Express Mode key
(`GOOGLE_VERTEX_API_KEY`), an `AccessTokenSource`, or, with the `google-auth`
feature, google-cloud-auth `Credentials` or a service account in
`GOOGLE_CLIENT_EMAIL`, `GOOGLE_PRIVATE_KEY` and `GOOGLE_PRIVATE_KEY_ID`. A
service account needs a process rustls crypto provider, such as
`rustls::crypto::ring::default_provider().install_default()`. Without one, a
call with the `env` service account fails with `Error::Configuration`, and
google-cloud-auth panics when it signs with service-account `Credentials` the
application built, unless another crate in the build turns on its
`default-rustls-provider` feature. The Vertex auth headers go only to the
base URL's host and `googleapis.com` hosts. `google_vertex::anthropic`,
`google_vertex::maas` and `google_vertex::xai` add Claude, the open models and
xAI on Vertex AI. `google::interactions::protocol()` and `agent_protocol()` run
a model or an agent on the Interactions API; set one with
`Model::with_protocol`.

`ops` holds the operations beyond chat, with the names of the Python
`ai.ops` module: `embed`, `generate_image`, `generate_video`, `generate_audio`,
`transcribe`, `rerank` and `experimental::evaluate`, and the TypeScript core's
`start_video` and `video_status`, `upload_file`, `file_info`,
`download_file`, `delete_file`, `upload_skill`, `batch`,
`stream_transcription`, `stream_translation` and `realtime`. Each retries a retryable error twice by
default, as model calls do. Live and realtime sessions need a WebSocket
transport: implement `transport::WebSocketTransport`, or enable the
`websocket` feature for `transport::TungsteniteTransport`.

Enable the `testing` feature for `testing::ScriptedProvider`. It answers each call
with the next scripted response and records each request, with no network.

The default feature set contains no HTTP client. Implement `HttpTransport` to
use an existing client, or enable the optional `reqwest` feature. Media downloads
use a separate `DownloadTransport` contract that pins validated DNS addresses.

Run `./scripts/check.sh` for the pinned toolchain build, test, lint, and documentation checks.

For UI requests, parse `UIMessage` with Serde and pass the core event stream to
`to_sse`. Set the response headers from `UI_MESSAGE_STREAM_HEADERS`. The SSE
adapter consumes events lazily and propagates source errors.

```rust
use suprnova_ai::ui::ai_sdk::{UIMessage, to_messages, to_sse, UI_MESSAGE_STREAM_HEADERS};

# fn example(model: &suprnova_ai::Model, request: Vec<UIMessage>) -> Result<(), suprnova_ai::Error> {
let (messages, approvals) = to_messages(&request, None)?;
// Apply approvals through a run-scoped ApprovalResolver before polling the run.
let body = to_sse(suprnova_ai::stream(model, messages, suprnova_ai::StreamOptions::default()));
let headers = UI_MESSAGE_STREAM_HEADERS;
# let _ = (approvals, body, headers);
# Ok(())
# }
```

See [docs/PORT.md](docs/PORT.md) for compatibility, test coverage, and deferred work.
