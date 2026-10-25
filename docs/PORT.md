# Inference core port

The source is AI SDK for Python 0.8.0 at
`https://github.com/vercel-labs/ai-python`. `LICENSE` holds the full text of
the Apache License, Version 2.0, and `NOTICE` the credits: the crate's author
and the Vercel copyrights of the Python and TypeScript SDKs. Source files carry
no license header, as in both upstream repositories. Behavior that the Python
source lacks follows the AI SDK for TypeScript 7.0.148 where `docs/spec/` says so;
"Departures from Python" below lists it.

## Module map

| Python module | Rust module | Responsibility |
| --- | --- | --- |
| `ai.__init__` | `suprnova_ai` | Core API re-exports |
| `ai.types.events` | `types::events` | Event vocabulary, hydration, replay, run state |
| `ai.types.messages` | `types::messages` | Messages, parts, special tool results, IDs |
| `ai.types.builders` | `types::builders` | Message and part constructors |
| `ai.types.media` | `types::media` | Detection, media types, data URLs, base64 |
| `ai.types.usage` | `types::usage` | Token usage and addition |
| `ai.types.tools` | `types::tools` | Function/provider declarations and tool configuration |
| `ai.models.core.model` | `models`, `models::core::model` | Model references and parsing |
| `ai.models.core.api` | `models`, `models::core::api` | Streams, generation, probing |
| `ai.models.core.params` | `models::params`, `models::core::params` | Request parameters and sentinels |
| `ai.models.core.helpers.files` | `models::files`, `models::core::helpers::files` | Bounded public-network downloads |
| `ai.providers.base` | `providers::base` | Provider/protocol traits, configuration, registry |
| Operation payloads used by `providers.base` | `providers::operations` | Typed prompts, params, and result containers |
| `ai.providers._optional` | Cargo features | Optional components resolve at compile time |
| `ai.errors` | `errors` | Framework errors, provider categories, HTTP context |
| `ai.util` | `util`, `futures`, Tokio | Stream composition and task/resource ownership |
| Upstream SDK HTTP clients | `transport` | Application-supplied HTTP transport and SSE framing |
| None; TypeScript `@ai-sdk/cerebras`, `deepinfra`, `deepseek`, `fireworks`, `mistral`, `moonshotai`, `togetherai`, `zai`, `xai`, `huggingface`, `minimax` | `providers::hosts` | Hosted providers on the Chat Completions, Responses and Messages protocols |
| `ai.ui.ai_sdk` | `ui::ai_sdk` | Inbound UI requests, persisted UI histories, approvals, identity metadata and outbound SSE |
| `ai.ops`, and the TypeScript core's video, upload, batch, streaming and realtime functions | `ops` | Operation front ends: splitting, polling, retries and streams |
| None; TypeScript `@ai-sdk/google` | `providers::google` | The Gemini API: generate content, every model kind, files, batches, the Live API and Interactions |
| None; TypeScript `@ai-sdk/google-vertex` | `providers::google_vertex` | Vertex AI: Gemini, Chirp, Cloud Speech-to-Text, auth, and the Anthropic, MaaS and xAI partner models |
| None | `transport::websocket`, `transport::TungsteniteTransport` | WebSocket contract for live and realtime sessions, and its tokio-tungstenite implementation |

## Wire format

Serialize the `Event` and `Part` enums for Python-compatible discriminators.
The Python 0.8.0 source uses `kind`, including `stream_start`, `tool_end`,
`builtin_tool_result`, and `file`. The brief called this field `type`; using
`type` would break the requested Python roundtrip. Nested special tool results
use their own Python `type` values, `content` and `messages`.

Messages retain IDs, turn IDs, replay state, usage, metadata, cached tool results,
and hooks. A tool result distinguishes an absent `model_input` from explicit
JSON null. File bytes serialize to base64 text, as Python does. Deserialization
retains that text because the wire format does not distinguish bytes from text.
Event replay flags remain runtime control state and do not serialize.
Partial-result events retain cloneable aggregator factories. Tool-result events
retain cloneable original errors and their source chains. Both runtime handles
stay out of serialized events. An object-safe JSON reducer facade adapts the
core's generic `Aggregator` trait for partial-result factories.
`ValueAggregator::snapshot_result` defaults to ordinary JSON. Reducers that
produce typed message bundles override it to retain the distinction between
special results and ordinary JSON with similar fields.

A yielded event contains a snapshot of its message at that point in the stream.
Rust owns snapshots. Python shares mutable message references. The final handle
contains the final aggregate. Usage and provider metadata follow the source's
latest-value rules. A stream that reaches EOF without `StreamEnd` returns a
retryable incomplete-response error and retains its partial message. Dropping a
handle cancels consumption without manufacturing an incomplete-response error.

## Rust differences

All inference runs asynchronously. There is no synchronous runtime wrapper.
A handle implements `futures::Stream`; callers drain it or drop it. There is no
Python async context manager or implicit task-local agent context.

`serde` replaces Pydantic serialization. `schemars` derives JSON Schema for typed
Rust inputs and outputs. `ToolDefinition` adds typed schema construction and
argument/result validation. The Python-shaped `Tool`, `ToolSpec`, and `ToolConfig`
carry function and provider declarations through inference. Tool execution and
approval decisions belong to the later agent module.

The validator supports `type`, `required`, `enum`, `const`, `properties`, `items`,
`additionalProperties`, local JSON Pointer references, schema combinations, numeric bounds,
length bounds, tuples, and unique array items. It rejects unsupported validation
keywords and remote references. Format strings remain annotations. Reference and
schema nesting are bounded at 128 levels. Numeric comparisons use decimal
arithmetic over the stored JSON numbers. Decimal representations are limited to
8192 bytes and 4096 exponent places to bound arithmetic. Named `$anchor` references
are unsupported. It does not claim full JSON Schema
conformance. SuprChat's lockfile had no `jsonschema` dependency, so this port uses
a local validator for the requested subset.

`ProviderValue<T>` preserves the difference between provider defaults and explicit
values, including disabled reasoning. Request options own their values. Provider
parameters use explicit string keys and JSON values instead of Python type keys.
Callers must use stable keys understood by their adapters.

Providers receive immutable configuration snapshots by shared reference. They do
not inspect environment variables. Applications supply the `env` map if they want
the Python configuration naming and precedence rules. There is no bundled
models.dev catalog. Applications register provider instances or construct them
from explicit models.dev metadata. Concrete adapter packages can supply catalog
integration later. Unknown or unregistered handles return `UnsupportedProvider`.

Trait objects use explicit snapshot restoration factories in `ProviderRegistry`.
`Model::to_value` and `Model::from_value` keep the Python model/configuration
shape without a global deserialization registry, minus the credentials. Custom
providers and protocols must include their settings in `to_value` and register
corresponding factories. Debug formatting of provider configuration redacts values.

Deterministic IDs use `with_id_generator` scoped to the polled future. Tokio does
not inherit task-local values into spawned tasks. Scope each spawned task
explicitly. Default IDs retain Python's prefix and 12-hex-digit format.

Pure media helpers accept bytes and text explicitly. Extension inference uses
`mime_guess`; platform-specific Python mimetypes
registrations are not available. Media downloads enforce public-address checks,
revalidate every redirect, pin destinations, limit bodies to the requested size,
and bound DNS resolution. A custom `DownloadTransport` must honor the documented
pinning and redirect contract. The optional implementation creates a fresh client
per hop, keeps the original host for HTTP/TLS, disables environment proxies, and
disables automatic redirects. A per-download cookie jar preserves domain, path, and secure-cookie scope across
redirects. Body size limits apply to decoded bytes yielded by the transport.
Custom download transports must decode supported content encodings before yielding
chunks. The bundled client supports gzip, deflate, Brotli, and Zstandard.

`util::merge` uses `futures::stream::select_all`. It does not restart exhausted
streams. Python queues, dynamic task groups, and exception-group teardown belong
to the later agent runtime. Rust `Drop` handles stream and resource cancellation.

## Departures from Python

These follow `docs/spec/models.md`, `docs/spec/agents.md`, `docs/spec/hosts.md`,
`docs/spec/openai.md`, `docs/spec/anthropic.md`, `docs/spec/google.md`,
`docs/spec/vertex.md` and `docs/spec/operations.md`.

- Model ids must name their provider. Python routes an id without a colon to its
  gateway provider and folds `gateway`, `ai-gateway` and `vercel` into one
  registry entry; here such an id is a configuration error and every handle is
  its own name. The gateway-only `routing` and `tags` request parameters are gone.
- Snapshots leave out the API key value, the `env` map and the headers.
  `Model::from_value` takes them, and the endpoint fields, from the provider
  registered under the snapshot's name, so a changed snapshot cannot send the
  key elsewhere and an endpoint change applies to older snapshots. It fails
  when no provider has that name or its class differs. Python serializes the
  credentials.
- Calls retry a retryable error before the first event, with the TypeScript
  defaults: 2 retries, waits of 2 s and 4 s, and `retry-after-ms` or
  `retry-after` waits under 60 s. Optional idle and total timeouts end a call
  with a `Timeout` provider error. Python has neither.
- `ReqwestTransport::new()` limits only opening a connection, to 10 s, with TCP
  keepalive, and sets no limit on a response, so a thinking model's long
  stream is not cut off.
- The agent loop runs async step checks after each step, which continue, steer
  with a user message or stop with a reason. Python has no step limit or checks;
  the TypeScript tool loop stops at 20 steps by default, while this default has
  no checks, so a run stops when the model gives a final answer.
- A call to a tool without an executor becomes an error result the model sees,
  with the TypeScript message. Python ends the run.
- `RunOptions::tool_concurrency` caps the tools a step runs at once,
  `AgentStream::usage` sums the run's usage and `AgentStream::stop_reason` says
  why the run stopped. Python has none of them.
- `AgentStream::stop` stops a run and waits until its work has ended.
  `AgentStream::interrupted_messages` returns the history plus the step a
  stopped or failed run was in, with the results of the tools that finished
  and cancelled error results for the rest. Python loses that step.
- The `testing` feature adds a scripted provider for tests without a network.
- `providers::hosts` adds Cerebras, DeepInfra, DeepSeek, Fireworks, Mistral,
  Moonshot AI, Together AI, Z.ai, xAI, Hugging Face and MiniMax, which Python
  lacks, with the request and response differences of their TypeScript
  7.0.148 packages; xAI and Hugging Face use the Responses API, and
  models.dev metadata for them selects it. Chat
  Completions sends `stream_options.include_usage` for OpenAI, OpenAI-compatible
  servers without a host profile, DeepSeek, Fireworks, Moonshot AI and Together
  AI only. Together AI, DeepInfra and Fireworks bring every model kind of
  their TypeScript packages: their own options entry on chat calls,
  completions, embeddings, images and Together AI's reranking. Their image
  calls return TypeScript's warnings; Together AI warns for an aspect ratio,
  which it leaves out, where TypeScript warns for a size by mistake. A
  DeepInfra image goes to the base URL without its last segment `openai`, so
  a base URL that does not end in `/openai` fails with
  `Error::Configuration`. Other departures from TypeScript: an image size
  that is not `<width>x<height>` fails with `Error::Validation`, where
  TypeScript sends `NaN` or the split text; Fireworks' chat entry values are
  checked, and its image options refuse `null`, where TypeScript sends them
  unchecked; Fireworks images leave out the size or aspect ratio they warn
  about, which TypeScript sends; a Fireworks Kontext call needs a download
  transport and fails with `Error::Validation` before its job starts without
  one, and fetches a result on another origin than the base URL through it
  without the key, where TypeScript fetches the result itself; Fireworks
  image errors take the body's message, where TypeScript uses the HTTP
  status text; and the deprecated `openai-compatible` entry is not read.
  DeepSeek and MiniMax bring every model kind of their TypeScript packages
  too: DeepSeek's `deepseek` chat options, message names, the assistant
  prefix and strict tools of a `/beta` base URL, image parts with
  `imageDetail` and `fileData`, the `deepseek` metadata of the final message
  and file uploads through `ops::upload_file`; MiniMax's `minimax` entry read
  over `anthropic`, the image and video blocks of MiniMax 3.1 and later, and
  its video models as jobs through `ops::generate_video`. Departures: the
  `deepseek` chat entry, DeepSeek's file options and MiniMax's video options
  refuse an option name their schemas do not take, where TypeScript strips
  it; DeepSeek's final metadata comes beside the `openai` entry every Chat
  Completions host gets; a MiniMax video call needs a download transport and
  fails with `Error::Validation` before its job starts without one, fetches
  the finished video through it without the key as TypeScript's core does,
  fails with `Error::Validation` on the `pollIntervalMs` and `pollTimeoutMs`
  options, since `ops::generate_video` polls by its `VideoOptions`, has no
  `webhookUrl`, and makes `n` videos as `n` jobs where TypeScript warns and
  makes one; a MiniMax video goes to the base URL without its last
  segment `anthropic`, so a base URL that does not end in `/anthropic` fails
  with `Error::Configuration`; a MiniMax video call also fails before its
  job without a Tokio timer to poll with; the finished video is read up to
  256 MiB, the limit Google's videos get, where TypeScript reads it without
  one; and a MiniMax job is its `task_id` alone, so a status checked from a
  stored `VideoJob` through `ops::video_status` completes without the
  `resolvedInputs` that `ops::generate_video` merges from the start.
- Chat Completions tool call deltas without an index, as Mistral and Moonshot
  AI send them, are told apart by their place in the chunk and their id; Python
  merges them. Text that follows reasoning, or reasoning that follows text,
  starts a new block.
- A Chat Completions or Responses stream delivers `StreamStart` with its first
  chunk that is not an error, so an error the server sends first is retried
  when it is retryable. Python starts the stream before it reads.
- A Responses `response.failed` or `error` event ends the call with a
  `ProviderError` whose kind and retry flag come from the error's code and
  type; Python reports a failed response as the finish reason `error`.
  `response.done` counts as `response.completed`. An xAI reasoning item
  streams as one reasoning block, ended by the finished item, so the block
  keeps the encrypted content that xAI sends only with that item.
- The OpenAI and Anthropic adapters share one HTTP layer. Both report a
  transport failure with a fixed message that keeps the original error as its
  source, and a broken response body as a connection error; an error response
  whose body cannot be read keeps the kind of its HTTP status. Both read an
  error body's message from `error.message`, a string `error` or `message`.
  OpenAI reads the code, parameter and type from the `error` object first and
  Anthropic from the top level first, as the Python SDKs do. An array in
  `extra_query` sends one pair per item, as httpx does.
- On OpenAI's Responses API, a tool call that the application ran goes back in
  the history as a full `function_call` item (its `call_id`, name, arguments
  and namespace), never as an `item_reference` or a replayed raw item, so its
  `function_call_output` pairs with it by `call_id`. Python sends a reference
  when `store` is not false and the raw item when it is; TypeScript sends the
  full item.
- On OpenAI's Responses API, stored text goes out as a reference only when the
  model is not a reasoning model or a reasoning item precedes the text in its
  turn, and each stored item is referenced once. Text that goes out in full
  is one assistant message per part, with the `phase` of its item. With
  `store` false, a reasoning model is asked for encrypted reasoning, the
  stream holds each reasoning block open until its item ends so that the
  block keeps the item's final encrypted content, text goes out without its
  item id, and reasoning only with its encrypted content. Python references
  every stored part and replays raw items; TypeScript sends these shapes.
- `OpenAICompatibleProvider::from_modelsdev_provider` selects Responses when the
  metadata names `@ai-sdk/openai`, not when the id is `openai`, and gives
  metadata for a Chat Completions host of `providers::hosts`, by package or id,
  that host's rules.
- `ops` adds Python's `ai.ops` front ends and the TypeScript core's video jobs,
  uploads, file info, downloads and deletes, skill uploads, batches, streaming
  transcription and translation, realtime sessions, server connections and
  WebRTC sessions, and evaluation. Every operation that returns one result retries a retryable
  error twice, as the TypeScript core does; a streaming operation retries only
  before its first part and before it reads any audio.
- `providers::openai` ports the TypeScript `@ai-sdk/openai` 4.0.99 package,
  with every model kind; Python has Responses and Chat Completions only.
  Provider options use the TypeScript names and fail with `Error::Validation`
  when unknown. Settings OpenAI never takes, such as a seed on Responses, fail
  with `Error::Validation`, where TypeScript drops them with a warning.
  Settings OpenAI takes for other models only are left out and logged as a
  `tracing` debug event with target `suprnova_ai::openai`, which names the
  setting and the model. On Responses a `text/*` file goes as `input_text`
  unless the option `passThroughUnsupportedFiles` sends it as a file; OpenAI's
  Chat Completions API refuses it, as TypeScript does, where Python sends it
  as text. An MCP approval
  request is a pending `ToolApproval` hook. Tools the application runs, such
  as `local_shell`, `shell` with a local environment, `apply_patch`,
  `computer` and custom tools, are tool calls an agent runs through
  `AgentTool::with_provider_declaration`. Web search, file search and citation
  sources are in the final message metadata under `openai.sources`. A core
  cache mode fails: use the option `promptCacheOptions`.
- `providers::anthropic` ports the TypeScript `@ai-sdk/anthropic` 4.0.85
  package, with every model kind: the Messages language model, files,
  skills, batches and evaluation. Provider options use the TypeScript names
  and fail with `Error::Validation` when unknown; settings Anthropic never
  takes, such as a seed or core metadata, fail too. Where TypeScript changes
  or leaves out a setting with a warning, the crate does the same and logs a
  `tracing` debug event with target `suprnova_ai::anthropic`. Departures from
  the Python port: the default output limit is the model's limit, not 8,192;
  a system message after the first goes as a mid-conversation system
  message, not joined to the first, in place where Anthropic takes it and
  otherwise just after the last user message before the model's reply,
  with the user messages it separated sent as one (TypeScript sends it in
  place, which Anthropic refuses), and a history with no user message after
  it fails with `Error::Validation` before any request; input tokens
  count cache writes; tool call arguments that are not a JSON object go as
  `rawInvalidInput`; `pause_turn` finishes as `stop`; and the output schema
  is TypeScript's sanitized schema. Built-in tool calls and results use TypeScript's shapes,
  sources are in the final message metadata under `anthropic.sources`, and a
  fallback or container upload block is an empty text part with `anthropic`
  metadata. MiniMax and Vertex AI's Anthropic models share these rules with
  their own settings. A batch's `results_url` on another origin than the
  base URL is refused, so the key stays with it. Other changes a caller
  sees: a file in a tool result that is not an image or a PDF, such as a
  text file, is left out and logged; a tool choice without tools is not
  sent; and a message without a stop reason finishes as `other`. Two
  changes to the crate's own API: `WebSearchOptions` gains
  `response_inclusion` and `WebFetchOptions` gains `use_cache` and
  `response_inclusion`, for the 2026-03-18 web tools, so a struct literal
  of either must end with `..Default::default()`; and the public constant
  `tools::BETA_HEADERS` is gone, because each tool version now adds its own
  beta.
- `providers::google` and `providers::google_vertex` port the TypeScript
  Google packages, which Python lacks, with every model kind. Settings those
  packages drop with a warning fail with `Error::Validation`; operations return
  the TypeScript warnings in `Item.warnings`. Grounding sources are in the final
  message metadata under `google.sources`, and `Tool::with_strict` marks a
  strict function tool.
- No Google key goes in a URL: Live API sockets send it as `x-goog-api-key`, a
  realtime server connection sends its token as `Authorization: Token`, and a
  Veo download sends the key only to the base URL's origin. A Veo URL or
  redirect to another origin goes through the download transport, which
  checks each address, or fails without one. The Vertex auth
  headers go only to the base URL's host and `googleapis.com` hosts.
- Every Interactions request sends `Api-Revision: 2026-05-20` unless the
  configured headers set one, as Google's Go SDK does. An agent or background
  call that is dropped or times out cancels the interaction; TypeScript cancels
  only on abort. A poll timeout is not retried, so a call never starts a second
  interaction after the first ran for the whole timeout. A reasoning effort or
  summary on Interactions fails: use the options `thinkingLevel` and
  `thinkingSummaries`.
- Generate content, image, speech, video and Interactions responses are read up
  to 256 MiB per body and per stream event, as Google's Go SDK reads stream
  lines; other responses keep the 8 MiB limit.
- Vertex AI signs in with an access token source, or with the `google-auth`
  feature, google-cloud-auth credentials or a service account from the `env`
  map; the crate reads no credential file. Vertex Anthropic sends an output
  schema as the forced `json` tool, takes only the Anthropic tool versions
  Vertex AI lists, and parses a buffered `rawPredict` response with the stream
  parser. MaaS downloads every file URL first.

## UI adapter

`ui::ai_sdk::to_messages` returns core history and extracted approval responses.
`apply_approvals` accepts an explicit `ApprovalResolver` supplied by the agent
runtime or application. `ToolResultValidator` supplies optional tool output
validation and coercion; the adapter does not depend on an ambient agent context
or Python runtime annotations.

`UIMessage` deserialization validates known part fields, accepts Python field
names and browser aliases, retains extra application fields, and skips unknown
part types. Parts use JSON values because tool and data type names are dynamic.
Core messages and events remain the crate's existing typed structures.

`to_ui_messages` collapses assistant, tool, and internal records into bubbles and
records the source IDs in `metadata.aiPython.sourceMessages`. Special content and
message-bundle results carry explicit `toolResultKinds` hints. The inbound path
uses those hints rather than inferring special results from arbitrary JSON.

`to_stream` emits typed UI events; `to_sse` encodes the same events as SSE and
appends `data: [DONE]` on successful exhaustion. Both consume fallible core event
streams lazily. A source error terminates the stream without a successful finish
or done marker. Applications set `UI_MESSAGE_STREAM_HEADERS` on the HTTP response.
The encoder preserves camelCase aliases, omitted top-level nulls, nested nulls,
Python JSON spacing, finite-float notation, ASCII escaping, and dataclass field
order. JSON values retain insertion order through Serde's `preserve_order` feature and use `float_roundtrip`
when parsing decimal numbers. Core `Metadata` retains its existing sorted map
representation.

An empty assistant `turn_id` uses the message ID and consumes the first record.
The Python implementation otherwise stalls on this input. Other turn grouping
rules follow the source.

## Test map

| Python tests | Rust integration tests | Coverage |
| --- | --- | --- |
| `tests/types/test_events.py` | `tests/types_events.rs` | Discriminators, omitted messages, hydration, multiple messages, retry, replay, run state |
| `tests/types/test_messages.py` | `tests/types_messages.rs` | Text/reasoning, media parts, output, special results, model-input presence, deterministic IDs |
| `tests/types/test_builders.py` | `tests/types_builders.rs` | Message parts, tool-message merges, constructors, errors |
| `tests/types/test_media.py` | `tests/types_media.rs` | Signature detection, URLs, inference, base64 and invalid data |
| `tests/types/test_tools.py` | `tests/types_tools.rs` | Function/provider shapes, opaque payloads, required fields; additional typed-schema validation |
| `tests/types/test_exclude_if_gc.py` | `tests/types_messages.rs`, `tests/types_events.rs` | Serialization omission rules; Python GC internals do not apply to Rust ownership |
| `tests/models/core/test_api.py` | `tests/models_core.rs` | Aggregation, option forwarding, generation fallback, protocol selection, replay, incomplete streams, probe |
| `tests/models/core/helpers/test_files.py` | `tests/models_files.rs`, `tests/transport_reqwest.rs` | Address policy, DNS/redirect validation, pinning, size/error handling, cleanup, original HTTP host |
| Base/configuration cases in `tests/models/test_resolution.py` and `tests/providers/*/test_provider.py` | `tests/models_core.rs`, `tests/providers_base.rs` | Explicit configuration, templates, aliases, duplicate registry entries, model resolution/restoration, unsupported operations |
| `ai.errors` contract | `tests/errors.rs` | Status mapping, retryability, context and error sources |
| Canonical Python wire fixtures | `tests/python_compat.rs`, `tests/fixtures/python-0.8.0.json` | Actual upstream serialization of all 21 events, 5 message roles, 3 tools, and usage |
| New transport boundary | `tests/transport.rs`, `tests/transport_reqwest.rs` | SSE chunk boundaries, limits, EOF, errors, HTTP request/response and redirects |
| `tests/ui/ai_sdk/test_inbound_messages.py` | `tests/ui_inbound_messages.rs` | Tool boundaries, approvals, special results, optional tool validation, UI-only parts and built-in tools |
| `tests/ui/ai_sdk/test_approvals.py` | `tests/ui_approvals.rs` | Approval extraction, hook identity, metadata and explicit registry resolution |
| `tests/ui/ai_sdk/test_outbound_messages.py` | `tests/ui_outbound_messages.rs` | Bubbles, tool deduplication, files, provider metadata and persisted identity roundtrips |
| `tests/ui/ai_sdk/test_outbound_stream.py` | `tests/ui_outbound_stream.rs` | Stream boundaries, retries, tools, typed partial results, hooks, files and SSE encoding |
| Captured Python UI outputs | `tests/ui_python_compat.rs`, `tests/fixtures/ui-python-0.8.0.json` | Requests, histories, event streams and exact JSON encoding from the unmodified Python tests |
| UI transport integration | `tests/ui_transport.rs`, `tests/ui_aggregator.rs` | Scripted `HttpTransport`, byte-split SSE, hydration, lazy conversion, error termination and typed reducer compatibility |
| `docs/spec/models.md` (no Python source) | `tests/spec_models.rs` | MOD-001 to MOD-009: model ids, gateway removal, credential-free snapshots, retries, timeouts, drop cancellation, the scripted provider, the reqwest transport's time limits |
| `docs/spec/agents.md` (no Python source) | `tests/spec_agents.rs` | AGT-001 to AGT-006: step checks, unknown tools, tool concurrency, run usage, stop reasons, interrupted history |
| `docs/spec/google.md` (no Python source) | `tests/spec_google.rs` | GGL-001 to GGL-032: the Gemini API's language model, options, tools, history, responses, errors, embeddings, images, speech, transcription, live transcription and translation, Veo, files, batches, realtime, Interactions, evaluation and size limits |
| `docs/spec/vertex.md` (no Python source) | `tests/spec_vertex.rs` | VTX-001 to VTX-015: Vertex AI endpoints, Express Mode, auth, the Gemini differences, streamed arguments, tool result downloads, embeddings, images, video, Chirp, Cloud Speech-to-Text, Gemini transcription, Interactions and the Anthropic, MaaS and xAI partner models |
| `docs/spec/operations.md` (no Python source) | `tests/spec_operations.rs` | OPS-001 to OPS-014: front-end pass-through and retries, embedding runs, image and video batches, video jobs, uploads, batches, streaming transcription and translation, realtime connections, the WebSocket transport, the evaluation adapter, file info, downloads and deletes, skill uploads, and server and WebRTC realtime sessions |
| `docs/spec/openai.md` (no Python source) | `tests/spec_openai.rs` | OAI-001 to OAI-027: Responses history under every `store` setting, reasoning models, endpoints, headers and errors, Responses and Chat Completions options, model-dependent settings, includes, files, tools, application-run tools, stream items, usage, Completions, embeddings, images, speech, transcription and its streaming form, translation, files, skills, batches, realtime and Live sessions, decisions, prompt caching and JSON schema normalization |
| `docs/spec/anthropic.md` (no Python source) | `tests/spec_anthropic.rs` | ANT-001 to ANT-020: endpoints, auth token and betas, provider options, core parameters and reasoning, model capabilities and their logged changes, structured output, tools and tool names, application-run and Anthropic-run tools and their history, files, system messages, assistant history, the stream reader, usage and final metadata, prompt caching, the shared hosts, file and skill uploads, batches and evaluation |
| `docs/spec/hosts.md` (no Python source) | `tests/spec_hosts.rs` | HST-001 to HST-041: host endpoints and keys, usage requests, field rules, Z.ai limits, error bodies, usage and JSON-mode fixes, MiniMax effort, the DeepSeek, Moonshot AI and Mistral dialects, tool calls without an index, stream error kinds, the xAI and Hugging Face Responses rules, Responses errors and terminal events, the options, completions, embeddings, images and reranking of Together AI, DeepInfra and Fireworks, DeepSeek's options, message names, prefix, strict tools, images, final metadata and file uploads, and MiniMax's own entry, 3.1 media blocks and video jobs |

Tests combine related Python parameter cases into Rust table-driven tests. They do
not reproduce Python test counts. Python garbage-collector closure behavior,
Pydantic class construction, agent `Context`, and telemetry span tests are deferred
with their corresponding runtime components. Provider adapter-specific HTTP paths,
OIDC, SDK imports, models.dev catalog lookups, and vendor errors are deferred with
the adapters. No live provider credentials are needed by the Rust suite.

Regenerate wire fixtures with `scripts/generate_python_fixtures.py`, an explicit
path to the upstream checkout, and an isolated Python environment with Pydantic
2.13 or newer. The Rust tests read the committed fixture and do not need Python.

Regenerate UI fixtures with `scripts/generate_ui_fixtures.py UPSTREAM DESTINATION`
in an isolated environment containing the upstream runtime dependencies, pytest,
pytest-asyncio and async-solipsism. This runs all upstream `tests/ui` tests and
records their adapter calls. Runtime-only aggregator factories are checked by the
Rust streaming tests because they cannot be stored in JSON fixtures.

## Later modules

Implemented after the core cut: `providers::openai` (every model kind of the
TypeScript OpenAI package), `providers::anthropic` (every model kind of the
TypeScript Anthropic package), `providers::hosts` (hosted providers on those
two protocols), `providers::google` and `providers::google_vertex` (every model kind of the
TypeScript Google packages), `ops` (operation front ends), `agents` (Agent,
tools, hooks, aggregators, runtime, middleware, buffering) and `ui::ai_sdk`. Each keeps the core contracts and
the Python tests' semantics; see its module documentation and tests.

Still absent, by decision: the Vercel AI Gateway protocol, routing and catalog, OIDC,
telemetry spans, the MCP client, durable workflow integrations and
environment-loading convenience factories.

The absent modules have no placeholder implementations.
