Prefix: MOD

# Model references and calls

How a model id resolves, what a model snapshot holds, and how one model call
retries, times out and stops. A call is one `suprnova_ai::stream` or
`suprnova_ai::generate` call, including its retries; every agent step makes
one call.

[MOD-001] `get_model` MUST resolve only an id of the form `provider:model`.
An id without a colon MUST fail with `Error::Configuration`, whose message
names the id and the `provider:model` form, whatever providers the registry
holds.
Falsifier: `get_model("gpt-5", registry)` returns a model, for example the provider registered as `gateway`; or it returns an error other than `Error::Configuration`; or the error message lacks `gpt-5` or `provider:model`.
Mechanism: core
Status: Agreed 2026-10-22

[MOD-002] The crate MUST NOT keep any Vercel AI Gateway routing. The
provider handles `gateway`, `ai-gateway` and `vercel` MUST be ordinary,
separate names, and the request parameters MUST have no field that only the
gateway reads.
Falsifier: A provider registered under one of `gateway`, `ai-gateway` or `vercel` resolves under another of them, or registering two different providers under two of them fails as a duplicate; or a file under `src/` or `tests/`, or `README.md`, contains `canonical_provider_id`, `RoutingParams`, `RoutingTarget`, `ProviderRankingStrategy`, `GeoRegion`, `CloudRegion` or `routing_target`; or `InferenceRequestParams` has a `routing` or `tags` field; or `README.md` contains the word gateway in backticks.
Mechanism: core
Rationale: Python reads `routing` and `tags` only in its gateway protocol (`providers/ai_gateway/protocol/_shared.py:212-243,422`), and no Rust adapter reads them; OpenRouter routing gets its own options when OpenRouter is ported.
Status: Agreed 2026-10-22

[MOD-003] A model snapshot MUST NOT contain a credential. `Model::to_value`
and `Provider::to_value` MUST leave out the API key value, the `env` map and
the headers, for the default `Provider::to_value`, the OpenAI-compatible
provider and the Anthropic-compatible provider.
Falsifier: For each of a `ConfiguredProvider`, an OpenAI-compatible and an Anthropic-compatible provider configured with `api_key_value` `sk-direct-secret`, `api_key_env` `KEY_VAR`, an `env` entry `KEY_VAR` = `sk-env-secret` and a header `authorization` = `Bearer header-secret`, the JSON text of `Model::to_value` contains `sk-direct-secret`, `sk-env-secret` or `header-secret`, or has an `api_key_value`, `env` or `headers` key.
Mechanism: core
Rationale: Python serializes the whole configuration (`providers/base.py:306-316`) and this port copied it (`src/providers/base.rs:226-234`); durable workflow steps store model references, so a snapshot must be safe to store.
Status: Agreed 2026-10-22

[MOD-004] `Model::from_value` MUST take the API key value, the `env` map and
the headers from the provider that the given registry holds under the
snapshot's provider name. When the registry holds no provider under that
name, it MUST fail with `Error::Configuration` naming the provider.
Falsifier: A snapshot restored into a registry whose provider of the same name holds API key `sk-live` sends a request whose credential header (`authorization` for OpenAI, `x-api-key` for Anthropic) lacks `sk-live`; or restoring a snapshot whose provider name the registry does not hold succeeds, fails with another error kind, or fails with a message that lacks the provider name.
Mechanism: core
Status: Agreed 2026-10-22

[MOD-005] A call MUST retry a retryable error that arrives before the
provider stream's first event, and only such an error. A retryable error is
an `Error::Provider` whose `is_retryable` is true, or an `Error::Transport`
connection failure or timeout. `StreamOptions::retry` sets the policy. The
default makes at most 2 retries, waiting 2 s before the first and 4 s before
the second (initial delay 2 s, factor 2); `max_retries` 0 disables retries.
A `retry-after-ms` or `retry-after` response header (seconds or an HTTP date)
replaces the computed wait when its value is at least 0 and is under 60 s or
under the computed wait. When the retries run out, the call returns the last
error.
Falsifier: Under paused Tokio time with the default policy, a call whose first two attempts fail with HTTP 429 and whose third succeeds does not return the third response, or does not wait 2 s and then 4 s; a `retry-after: 1` header does not make the first wait 1 s, or a `retry-after: 120` header changes it from 2 s; a non-retryable error (HTTP 400), or a retryable error after the first event, leads to another attempt; three HTTP 503 failures in a row lead to more than three attempts or return an error other than the third one; or `max_retries` 0 makes a second attempt.
Mechanism: core
Rationale: These are the TypeScript SDK defaults (`packages/ai/src/util/retry-with-exponential-backoff.ts`, `prepare-retries.ts`); Python has no retries. A retry after the first event would repeat output the consumer already has.
Status: Agreed 2026-10-22

[MOD-006] When `StreamOptions::idle_timeout` is set, a call attempt that
receives no event for that long (for `generate`, no response) MUST end. When
`StreamOptions::timeout` is set, the whole call, retries and waits included,
MUST end once it has run that long. Either one ends the call with an
`Error::Provider` of kind `Timeout` and drops the provider stream. An idle
timeout before the first event is retryable under MOD-005. Both are unset by
default.
Falsifier: Under paused Tokio time, a provider that sends nothing for 31 s, called with `idle_timeout` 30 s and no retries, does not end with a `Timeout` provider error at 30 s, or its stream is not dropped then; a provider that sends an event every 10 s for 100 s, called with `timeout` 60 s, does not end with a `Timeout` error at 60 s; an idle timeout before the first event is not retried under the default policy; or a call with neither set ends by itself while its provider stays silent for an hour.
Mechanism: core
Rationale: An application bounds a whole agent run by dropping it; a bound on each model call, and a stall check between events, are what it cannot add from outside without middleware.
Status: Agreed 2026-10-22

[MOD-007] Dropping a `StreamHandle`, a `generate` future or an `AgentStream`
MUST stop its work: no further provider request is sent, a pending retry
wait ends, and the running tool futures are dropped.
Falsifier: After a call is dropped during a retry wait, the provider receives another request once the wait would have ended; or after an `AgentStream` is dropped while a tool runs, that tool's future has not been dropped (for a buffered run, once the runtime has polled the run's task).
Mechanism: core
Rationale: Dropping replaces the TypeScript abort signal: a stop button breaks out of the consuming loop, reads `messages()` and drops the run.
Status: Agreed 2026-10-22

[MOD-008] With the `testing` feature, `suprnova_ai::testing` MUST provide a
scripted provider. Each call takes the next scripted response (events with
optional usage, or an error), records the messages and options it received,
and sends no network request. A call after the last scripted response MUST
fail with `Error::Configuration`.
Falsifier: A test that uses only the scripted provider cannot drive an `Agent` through one tool step and a final answer; or the recorded requests differ from the messages and tool declarations the agent sent; or a scripted error reaches the caller as a different error; or a call after the last scripted response does not fail with `Error::Configuration`.
Mechanism: core
Status: Agreed 2026-10-22

[MOD-009] The client that `ReqwestTransport::new()` builds MUST give up on
opening a connection after 10 s, with `TransportError::Timeout`, and MUST
set no other time limit: once the connection is open, it MUST NOT end a
request while it waits for the response headers, or while the response body
streams or pauses.
Falsifier: A request through `ReqwestTransport::new()` to a local listener whose accept queue is full does not fail with `TransportError::Timeout` 10 s after it starts; or a request to a local server that sends the response headers only after 60 s, or that sends the headers and one body chunk, then nothing for 60 s, then a second chunk and the end, fails or does not deliver both chunks.
Mechanism: core
Rationale: A thinking model can stream nothing for minutes, and a 30 s request limit cut those responses off; an application that wants a bound on a call sets `StreamOptions::idle_timeout` or `StreamOptions::timeout` (MOD-006).
Status: Agreed 2026-10-22
