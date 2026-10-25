# Roadmap

Current: mistral-whole

## rename-to-suprnova-ai

Requirements: PKG-001, PKG-002, PKG-003, PKG-004, PKG-005, PKG-006

Rename the crate from `ai` to `suprnova-ai` (library `suprnova_ai`) and the
repository from `eas4ai/ai-rs` to `eas4ai/suprnova-ai-rs`. Change the package
name in `Cargo.toml`, rewrite every `ai::` crate path in `src/`, `tests/`,
`README.md` and `docs/PORT.md`, retitle `README.md`, set the `Cargo.toml`
repository URL, reword `NOTICE`, move the `scripts/check.sh` build directory
to a `suprnova-ai-rs` path, and regenerate `Cargo.lock`. The public API does
not change apart from the crate name. suprchat is not changed: it pins an
older revision by Git URL, and GitHub redirects that URL.

Done when the identity, remote and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## core-hardening

Requirements: MOD-001, MOD-002, MOD-003, MOD-004, MOD-005, MOD-006, MOD-007, MOD-008, AGT-001, AGT-002, AGT-003, AGT-004, AGT-005, PKG-002, PKG-006

Harden the core before the provider port builds on it. Remove the gateway
remnants: the default `gateway` route in `get_model`, the alias folding in
`canonical_provider_id`, and the gateway-only `routing` and `tags` request
parameters with their types. Make model snapshots credential-free and
restore credentials from the registered provider of the same name. Add
call retries with backoff that honors `retry-after`, optional idle and
total timeouts, and drop-safe retry waits to `stream` and `generate`. Add
the `testing` feature with a scripted provider. In the agent loop, add
async step checks that continue, steer or stop, turn calls to unknown tools
into error results the model sees, add an optional tool concurrency cap,
total run usage and the run's stop reason. Update `README.md`,
`src/agents/README.md` and `docs/PORT.md` to state the departures from
Python. Each requirement's tests in `tests/spec_models.rs` and
`tests/spec_agents.rs` are named after it, and `scripts/check-core.sh`
reports one result per requirement.

Done when the core, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## chat-hosts

Requirements: HST-001, HST-002, HST-003, HST-004, HST-005, HST-006, HST-007, AGT-001, AGT-006, PKG-002, PKG-006

Add the first hosted providers on one shared provider layer. Move the parts
the OpenAI and Anthropic adapters each carry today into shared code under
`src/providers/`: the HTTP request builder (URL joins, query, auth and
header order, the bounded body read), the error mapping, and the SSE to JSON
reader. Their behavior does not change, so today's tests keep passing.
Give the Chat Completions protocol a host profile for the request and
response differences in `docs/spec/hosts.md`, and stop choosing the default
protocol by the provider name. Add `providers::hosts` with Cerebras,
DeepInfra, Fireworks, Together AI and Z.ai on Chat Completions and MiniMax on
the Anthropic Messages protocol, each restorable from a snapshot. Send
`stream_options.include_usage` only where HST-002 says. In the agent loop,
skip step checks after a step that ends on a deferred hook, count progress
across every run of the loop, and add `AgentStream::interrupted_messages()`.
Update `README.md`, `src/agents/README.md` and `docs/PORT.md`. Each
requirement's tests in `tests/spec_hosts.rs` and `tests/spec_agents.rs` are
named after it; `scripts/check-hosts.sh` and `scripts/check-core.sh` report
one result per requirement through the shared `scripts/spec-tests.sh`.

Done when the hosts, core, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## dialect-hosts

Requirements: HST-001, HST-002, HST-008, HST-009, HST-010, HST-011, HST-012, HST-013, HST-014, HST-015, HST-016, HST-017, HST-018, MOD-009, AGT-006, PKG-002, PKG-006

Add DeepSeek, Moonshot AI and Mistral to `providers::hosts` on the Chat
Completions protocol, each restorable from a snapshot. Their profiles also
change how messages, schemas and stream content are written and read: the
DeepSeek `thinking` field, V4 reasoning history and schema system message;
the Moonshot AI model families, MFJS schemas, video input and image
downloads through a provider built with a download transport; and the
Mistral seed, tool choice, thinking chunks, document parts and content
arrays. Each rejects the settings its TypeScript provider never sends. Key
Chat Completions tool call deltas without an index by their position, and
give the three hosts' stream errors a kind and a retry flag. Give
`ReqwestTransport::new()` only a 10 s connect limit and TCP keepalive. Make
`AgentStream::interrupted_messages()` keep the loop's copy of a response up
to its last received tool call, with the results of the tools that
finished, and add `AgentStream::stop()`. Update `README.md`,
`src/agents/README.md` and `docs/PORT.md`. Each requirement's tests in
`tests/spec_hosts.rs`, `tests/spec_models.rs` and `tests/spec_agents.rs`
are named after it; `scripts/check-hosts.sh` and `scripts/check-core.sh`
report one result per requirement through the shared
`scripts/spec-tests.sh`.

Done when the hosts, core, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## responses-hosts

Requirements: HST-001, HST-019, HST-020, HST-021, HST-022, HST-023, HST-024, HST-025, PKG-002, PKG-006

Give the Responses protocol a host profile, as Chat Completions has, with
OpenAI's rules as today's default, and add xAI and Hugging Face to
`providers::hosts` on it, each restorable from a snapshot and chosen by
models.dev metadata. xAI is ported for text: its settings, effort map, store
and include rules, history items without references, item metadata under
`xai`, cached-token usage and `code: error` messages; its built-in tools fail
with `Error::UnsupportedOperation`. Hugging Face gets its schema, tool choice
and history rules and takes only image files. Each host rejects the settings
its TypeScript package never sends. Fix the Responses reader for every
Responses provider: `response.failed` and `error` events end the call with a
classified `ProviderError`, `response.done` counts as `response.completed`,
`response.incomplete` always reports a finish reason, and reasoning keeps its
encrypted content when its summary ends first. Update `README.md`,
`docs/PORT.md` and the adapter docs. Each requirement's tests in
`tests/spec_hosts.rs` are named after it; `scripts/check-hosts.sh` reports
one result per requirement through the shared `scripts/spec-tests.sh`.

Done when the hosts, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## google

Requirements: GGL-001, GGL-002, GGL-003, GGL-004, GGL-005, GGL-006, GGL-007, GGL-008, GGL-009, GGL-010, GGL-011, GGL-012, GGL-013, GGL-014, GGL-015, GGL-016, GGL-017, GGL-018, GGL-019, GGL-020, GGL-021, GGL-022, GGL-023, GGL-024, GGL-025, GGL-026, GGL-027, GGL-028, GGL-029, GGL-030, GGL-031, GGL-032, VTX-001, VTX-002, VTX-003, VTX-004, VTX-005, VTX-006, VTX-007, VTX-008, VTX-009, VTX-010, VTX-011, VTX-012, VTX-013, VTX-014, VTX-015, OPS-001, OPS-002, OPS-003, OPS-004, OPS-005, OPS-006, OPS-007, OPS-008, OPS-009, OPS-010, OPS-011, PKG-002, PKG-006

Port Google whole: every model kind of the AI SDK for TypeScript 7.0.148
packages `@ai-sdk/google` and `@ai-sdk/google-vertex`. Add `suprnova_ai::ops`
with its front ends for embeddings, images, video jobs, speech,
transcription and its streaming form, speech translation, file uploads,
batches, realtime sessions and evaluation, and a `WebSocketTransport` trait
with a tokio-tungstenite implementation behind a feature. Add
`providers::google` for the Gemini API: language models streamed and
buffered, with thinking, thought signatures, schemas, files, tools and
grounding; embeddings, Gemini image generation, speech, transcription,
speech translation, Veo video, the files API, batches, the Live API with
ephemeral tokens and the realtime codec, the Interactions API with its
agents, background runs and stream reconnection, and evaluation. Add
`providers::google_vertex` for Vertex AI: Express Mode keys, credentials
from Google's `google-cloud-auth` crate behind the `google-auth` feature or
an access token source; Gemini, embeddings, images, video, Chirp and Gemini
speech, Cloud Speech-to-Text and Gemini transcription and Interactions on
Vertex; and the Anthropic, open-model and xAI partner providers. Media
responses read up to 256 MiB. The crate reads no environment. Update
`README.md`, `docs/PORT.md` and the provider docs. Each requirement's tests
in `tests/spec_google.rs`, `tests/spec_vertex.rs` and
`tests/spec_operations.rs` are named after it; `scripts/check-google.sh`,
`scripts/check-vertex.sh` and `scripts/check-operations.sh` report one
result per requirement through the shared `scripts/spec-tests.sh`. Live
tests use free-tier Gemini models only; the paid-only models (Veo, the
Gemini and Imagen image models, Pro speech) and Vertex AI, which has no
credentials here, are tested from recorded requests only.

Done when the google, vertex, operations, identity and checks mechanisms
pass for every requirement above, their receipts are current, and the
review and the adversary report have no open findings.

## openai-call-pairing

Requirements: OAI-001, PKG-002, PKG-006

Send each tool call of an assistant turn to OpenAI's Responses API as a full
`function_call` item, never as a reference to a stored item, so that every
`function_call_output` pairs with its call by `call_id`
(`src/providers/openai/messages.rs`). Replace the test in
`tests/providers_openai.rs` that asserts the reference. Add
`tests/fixtures/README.md`, which says that
`vertex-test-service-account-key.pem` is a key made only for the tests and
signs in to nothing, and a `.gitleaks.toml` that allowlists that file. The
tests of OAI-001 in `tests/spec_openai.rs` are named after it, and
`scripts/check-openai.sh` reports its result through the shared
`scripts/spec-tests.sh`.

Done when the openai, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## openai-history-replay

Requirements: OAI-002, PKG-002, PKG-006

Send the text and reasoning of an assistant turn to OpenAI's Responses API
in the shapes TypeScript sends (`src/providers/openai/messages.rs`): stored
text goes out as a reference only when a reasoning model's reasoning comes
before it in the same turn, and as its text otherwise; an unstored call
sends text without its stored id, sends only reasoning that has encrypted
content, and asks a reasoning model for that content
(`src/providers/openai/dialect.rs`). Name OpenAI's reasoning models by the
rule `openai-language-model-capabilities.ts` uses. Update the test in
`tests/providers_openai.rs` that expects the raw text item with `store`
false, say in `README.md` that a chat which resumes after OpenAI deletes its
stored items needs `store` false, and list the change under the departures
of `docs/PORT.md`. The tests of OAI-002 in `tests/spec_openai.rs` are named
after it, and `scripts/check-openai.sh` reports its result.

Done when the openai, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## license-attribution

Requirements: PKG-002, PKG-004, PKG-006, PKG-007

Credit the crate's own author and keep the Vercel credit. Replace `LICENSE`
with the full Apache License 2.0 text; rewrite `NOTICE` to name
`suprnova-ai-rs` and `Copyright 2026 Shawn McAllister <@eas4ai>`, say that the
crate began as a port of the AI SDK for Python 0.8.0 and has changed from it
as `docs/PORT.md` lists, and keep both Vercel copyrights and upstream URLs.
Remove the license header from every file under `src/`, `tests/` and
`scripts/`. List the author in `Cargo.toml` `authors`, and say in the opening
of `README.md` what the crate derives from and that it is licensed under the
Apache License 2.0. `docs/PORT.md` states where the license and notice live.
`scripts/check-identity.sh` reports PKG-007 with the other identity checks.

Done when the identity and checks mechanisms pass for every requirement
above, their receipts are current, and the review and the adversary report
have no open findings.

## openai-whole

Requirements: OAI-001, OAI-002, OAI-003, OAI-004, OAI-005, OAI-006, OAI-007, OAI-008, OAI-009, OAI-010, OAI-011, OAI-012, OAI-013, OAI-014, OAI-015, OAI-016, OAI-017, OAI-018, OAI-019, OAI-020, OAI-021, OAI-022, OAI-023, OAI-024, OAI-025, OAI-026, OAI-027, OPS-012, OPS-013, OPS-014, PKG-002, PKG-006

Port OpenAI whole: every model kind of the AI SDK for TypeScript 7.0.148
package `@ai-sdk/openai`. Give the Responses and Chat Completions protocols
the TypeScript provider options, model capabilities, user files, history
items, tools, stream items, usage and prompt caching; add the host-run
provider tools, with `AgentTool::with_provider_declaration`, and MCP
approvals through the `ToolApproval` hook. Add `OpenAICompletionsProtocol`
and an embedding protocol that OpenAI-compatible servers can also use, and
OpenAI's images, speech, transcription and its streaming form, speech
translation, files, skills, batches, realtime and Live sessions and
decisions. Add the `ops` front ends for file info, download and delete,
skill uploads, realtime server connections and WebRTC sessions, and
`OpenAICompatibleProvider::with_download_transport`, and log the settings a
model does not take at debug through the `tracing` crate. OAI-001 and
OAI-002 stay as they are. Update `README.md`, `docs/PORT.md` and the provider docs.
Each requirement's tests in `tests/spec_openai.rs` and
`tests/spec_operations.rs` are named after it; `scripts/check-openai.sh` and
`scripts/check-operations.sh` report one result per requirement. Live tests
use only the calls the developer approves: OpenAI has no free tier.

Done when the openai, operations, identity and checks mechanisms pass for
every requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## anthropic-whole

Requirements: ANT-001, ANT-002, ANT-003, ANT-004, ANT-005, ANT-006, ANT-007, ANT-008, ANT-009, ANT-010, ANT-011, ANT-012, ANT-013, ANT-014, ANT-015, ANT-016, ANT-017, ANT-018, ANT-019, ANT-020, OAI-004, OAI-007, OAI-014, PKG-002, PKG-006, PKG-007

Port Anthropic whole: every model kind of the AI SDK for TypeScript 7.0.148
package `@ai-sdk/anthropic` 4.0.85. Give the Messages protocol the
TypeScript provider options, the core parameter and reasoning mapping, model
capabilities, structured output, function and provider tools with the
application's tool names, user files, system messages, history, stream items,
sources, usage, final metadata and prompt caching; deliver the provider tools
the application runs as tool calls and the ones Anthropic runs as built-in
calls; add `AnthropicCompatibleProvider::with_download_transport`, the bearer
token, `anthropic::tools` for every tool version and
`anthropic::container_id`; and add Anthropic's files, skills, batches and
evaluation. MiniMax and Vertex AI's Anthropic models take the shared rules
with their TypeScript settings and keep HST-001, HST-007 and VTX-013. Log the
settings TypeScript changes with a warning at debug through the `tracing`
crate. Fix the wording of OAI-004, OAI-007, OAI-014 and PKG-007 that the
openai-whole and license-attribution reviews left for this spec change.
Update `README.md`, `docs/PORT.md` and the provider docs. Each
requirement's tests in `tests/spec_anthropic.rs` are named after it, and
`scripts/check-anthropic.sh` reports one result per requirement. Live tests
use only `claude-haiku-5-5`, with few and small calls (the developer,
2026-10-24: "Just use haiku 5.5").

Done when the anthropic, openai, identity and checks mechanisms pass for
every requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## anthropic-system-placement

Requirements: ANT-011, PKG-002, PKG-006

Send a later system message with content where Anthropic takes it
(`src/providers/anthropic/messages.rs`): in place when it follows a user
message, or an assistant message that ends in a server tool result, and
comes before an assistant message or ends the history; otherwise just after
the last user message before the model's next reply, with the user messages
it separated sent as one and the move logged at debug; and fail with
`Error::Validation` before any request when no user message follows it. A
system message that holds only an effort stays in place. Rewrite the ANT-011
tests in `tests/spec_anthropic.rs` and the test in
`tests/anthropic_adapter.rs` that puts a system message between two user
messages, and update the system message departure in `docs/PORT.md`. Check
the fix with at most two calls on `claude-haiku-5-5`.

Done when the anthropic, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## compatible-hosts-whole

Requirements: HST-026, HST-027, HST-028, HST-029, HST-030, HST-031, HST-032, PKG-002, PKG-006

Port Together AI (`@ai-sdk/togetherai` 3.0.76), DeepInfra
(`@ai-sdk/deepinfra` 3.0.75) and Fireworks (`@ai-sdk/fireworks` 3.0.78)
whole: read each host's own options on chat calls, give each host its
completion protocol, embed through the host's chat model, and add Together
AI's images and reranking, DeepInfra's images and edits, and Fireworks'
images with the Kontext polling. Image calls return TypeScript's warnings
in `Item.warnings`. Update `README.md`, `docs/PORT.md` and the host module
docs. The tests of each requirement in `tests/spec_hosts.rs` are named after
it, and `scripts/check-hosts.sh` reports one result per requirement. Live
calls run only on Together AI's free models
(`meta-llama/Llama-3.3-70B-Instruct-Turbo-Free` and
`black-forest-labs/FLUX.1-schnell-Free`), at most four; every other model
kind is checked with recorded requests only. This is the first of four
commitments that port the remaining hosts whole; DeepSeek and MiniMax,
Mistral, and xAI follow.

Done when the hosts, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## fireworks-kontext-downloads

Requirements: HST-031, PKG-002, PKG-006

Fail a Fireworks Kontext image call (`flux-kontext-pro` and
`flux-kontext-max`) with `Error::Validation` before any request when the
provider has no download transport, so no job is started and billed whose
sample the crate cannot fetch (`src/providers/hosts/images.rs`). Add the
test to the HST-031 tests in `tests/spec_hosts.rs`, change any existing
Kontext test that runs without a download transport, and say in the
Fireworks module doc and `docs/PORT.md` that Kontext needs one. No live
calls: Kontext is paid, so the check uses recorded requests only.

Done when the hosts, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## deepseek-minimax-whole

Requirements: HST-033, HST-034, HST-035, HST-036, HST-037, HST-038, HST-039, HST-040, HST-041, PKG-002, PKG-006

Port DeepSeek (`@ai-sdk/deepseek` 3.0.70) and MiniMax (`@ai-sdk/minimax`
3.0.62) whole. DeepSeek: the `deepseek` chat options, message names and
prefix completion, strict tools on a `/beta` base URL, image parts by file
id, URL, detail and `file_data`, the final metadata, and the files API
through `ops::upload_file` on the `providers::hosts::deepseek` provider.
MiniMax: the `minimax` options and metadata, the 3.1+ image and video parts
and cache control key, and the H3 video models through
`ops::generate_video`, `ops::start_video` and `ops::video_status` on the
`providers::hosts::minimax` provider, which needs a download transport for
them (`register_with_downloads`). Video calls return TypeScript's warnings
in `Item.warnings` and make one video per call (OPS-003). Update
`README.md`, `docs/PORT.md` and the two host module docs. The tests of each
requirement in `tests/spec_hosts.rs` are named after it, and
`scripts/check-hosts.sh` reports one result per requirement. Live calls: at
most four on DeepSeek (`deepseek-v4-flash` chat and one small image
upload), none on MiniMax, which has no key here, so MiniMax is checked with
recorded requests only. This is the second of the four commitments that
port the remaining hosts whole; Mistral and xAI follow.

Done when the hosts, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.

## mistral-whole

Requirements: HST-042, HST-043, HST-044, HST-045, HST-046, HST-047, PKG-002, PKG-006

Port Mistral (`@ai-sdk/mistral` 4.0.69) whole. The `mistral` chat options
(safe prompt, document limits, prompt cache key, parallel tool calls,
reasoning effort, strict schemas, JSON-object mode and strict function
tools) on the `providers::hosts::mistral` chat model; the Conversations API
as the `mistral::conversations()` protocol attached with
`Model::with_protocol`, with the `mistral::web_search()` and
`mistral::web_search_premium()` built-in tools, sources in the final
`mistral.sources` metadata and snapshot restore through `mistral::register`;
embeddings through `ops::embed`, speech through `ops::generate_audio` and
transcription through `ops::transcribe` on the same provider. Speech calls
return TypeScript's warnings in `Item.warnings` (HST-046); the other model
kinds refuse an unsupported setting with `Error::Validation` (HST-008).
Update `README.md`, `docs/PORT.md` and the host module docs. The tests of
each requirement in `tests/spec_hosts.rs` are named after it, and
`scripts/check-hosts.sh` reports one result per requirement. Live calls:
none; there is no Mistral key here, so every requirement is checked with
recorded requests only. This is the third of the four commitments that port
the remaining hosts whole; xAI follows.

Done when the hosts, identity and checks mechanisms pass for every
requirement above, their receipts are current, and the review and the
adversary report have no open findings.
