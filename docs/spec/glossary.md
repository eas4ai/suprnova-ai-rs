# Glossary

**Package.** The Cargo package that `Cargo.toml` defines: `suprnova-ai`.
Before the rename it was `ai`.

**Library crate.** The crate other Rust code imports: `suprnova_ai`. Cargo
derives this name from the package name by changing `-` to `_`.

**Crate path.** A Rust path that starts with the library crate's name, such
as `suprnova_ai::Model`. Before the rename these paths started with `ai::`.

**Repository.** The Git repository on GitHub,
`https://github.com/eas4ai/suprnova-ai-rs`. Before the rename it was
`eas4ai/ai-rs`; GitHub redirects the old URL.

**Origin remote.** The Git remote named `origin` in a clone. Sudus pushes
its records to it, because it is this project's authority remote.

**Upstream.** The AI SDK for Python 0.8.0 at
`https://github.com/vercel-labs/ai-python`, which this crate ports. Upstream
names such as the Python package `ai` and the module `ai.types` keep their
spelling here.

**Model id.** A `provider:model` string that `get_model` parses. It splits
at the first colon; an id with no colon is an error.

**Provider.** An implementation of the `Provider` trait for one vendor API.
The `ProviderRegistry` maps provider names to providers and starts empty.

**Event.** One item of a model stream, a variant of the `Event` enum. It
serializes with the upstream `kind` discriminator.

**Agent.** The `Agent` type: it streams a model, runs the requested tools,
appends the results to the history, and repeats until the model gives a
final answer.

**Call.** One `stream` or `generate` call to a model, including its retries.

**Snapshot.** The JSON value `Model::to_value` returns, which
`Model::from_value` restores through a provider registry.

**Credential.** A value that authenticates a request: the API key value,
the values of a provider configuration's `env` map, and its header values.

**Retryable error.** An `Error::Provider` whose `is_retryable` is true, or an
`Error::Transport` connection failure or timeout.

**Step.** One model response in an agent run together with the tool calls it
asked for. A response replayed from an earlier run is not a step.

**Done (agent run).** An agent run is done when the model's last response is
a final answer: an assistant message with no tool calls. A run stops when it
is done, unless a step check or a deferred hook stops it first; a step check
can also send it on after a final answer.

**Step check.** A `StepCheck` in `RunOptions::step_checks`. The default loop
runs the checks after each step; each answers continue, steer with a user
message, or stop with a reason. The default list is empty.

**Host.** A vendor API that speaks a protocol the crate implements, such as
Cerebras over Chat Completions, xAI over Responses or MiniMax over Messages. `providers::hosts`
registers one provider per host.

**Host profile.** The data that turns the Chat Completions, Responses or
Messages adapter into one host's provider: its default base URL, its key variable,
and its request and response rules.

**Interrupted history.** What `AgentStream::interrupted_messages()` returns:
the run's history plus the step in progress, with a cancelled error result
for each tool call that had not finished.

**Scripted provider.** The provider in `suprnova_ai::testing`, behind the
`testing` feature. It answers each call with the next scripted response and
records what it received.

**Transport.** An implementation of the `HttpTransport` trait. It sends the
HTTP requests of a provider. The `reqwest` feature adds `ReqwestTransport`.

**UI adapter.** The `ui::ai_sdk` module. It converts Vercel AI SDK v6 UI
messages to core messages, and core events back to UI messages and
server-sent events.

**Check script.** `scripts/check.sh`. It runs the format check, the builds
and tests with no features and with all features, clippy and rustdoc, with
warnings denied.

**Thought signature.** An opaque `thoughtSignature` that a Gemini model returns
with a part of its response. The part's metadata keeps it under `google`, and
the history sends it back with that part, as Gemini 3 requires for tool calls.

**Express Mode.** Vertex AI access with an API key instead of Google Cloud
credentials. Requests go to the global host and send the key as
`x-goog-api-key`.

**Live API.** Gemini's WebSocket API for realtime sessions, live
transcription and speech translation.

**Interactions API.** Gemini's stateful generation API (`/interactions`). It
runs models and agents, keeps a server-side record of each interaction, and
can run in the background.

**Access token source.** An application-supplied object that returns a
Google OAuth access token for Vertex AI. It needs no crate feature.

**Vertex auth headers.** The headers that sign a Vertex AI request in: the
`authorization` header and, when the credentials name a quota project,
`x-goog-user-project`. The feature `google-auth` gets them from Google's
`google-cloud-auth` crate, which also finds application default credentials.
