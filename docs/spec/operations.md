Prefix: OPS

# Model operations

The operations beyond chat, in `suprnova_ai::ops`. The front ends keep the
names of the Python SDK 0.8.0 `ai.ops` module: `embed`, `generate_image`,
`generate_video`, `generate_audio`, `transcribe`, `rerank` and
`experimental::evaluate`. The module adds the operations that the AI SDK for
TypeScript 7.0.148 core runs and Google's and OpenAI's packages implement:
video jobs, file uploads and their info, download and delete, skill uploads,
batches, streaming transcription, speech translation, realtime sessions with
their server connections and WebRTC sessions. Each front end calls the model's protocol, the model's
override first and then the provider's. The operation results keep the
existing `Item` shape: a value, usage, warnings and provider metadata. A
warning has the TypeScript fields: `kind` (`unsupported` or `other`),
`feature`, `details` and `message`. Paths in rationale lines are under
`packages/ai/src/` in the TypeScript repository.

[OPS-001] `ops::embed`, `ops::generate_image`, `ops::generate_video`,
`ops::start_video`, `ops::video_status`, `ops::generate_audio`,
`ops::transcribe`, `ops::rerank`, `ops::upload_file`, the `ops::batch`
functions, `ops::stream_transcription`, `ops::stream_translation`, the
`ops::realtime` functions and `ops::experimental::evaluate` MUST each call the
model's protocol, or the provider for the provider-level operations, and
return its result unchanged apart from the rules below. An operation that the
protocol does not implement MUST fail with `Error::UnsupportedOperation`
naming the operation, before any request is sent. Every operation except the
streaming ones MUST retry a retryable error by the `RetryPolicy` in its
operation options, 2 retries by default, as model calls do; a streaming
operation retries only an error before its first part.
Falsifier: A scripted protocol that records its calls receives another prompt or parameters than a front end was given, or the front end returns another value, usage, warnings or metadata; a provider whose protocol lacks an operation answers that front end with another error kind or sends a request; or a protocol that fails twice with HTTP 503 and then succeeds is not called three times, or one that fails with HTTP 400 is called again.
Mechanism: operations
Rationale: The Python front ends only forward the call, and the TypeScript core retries every operation twice (`util/prepare-retries.ts`).
Status: Agreed 2026-10-23

[OPS-002] `ops::embed` MUST split its values into runs no longer than the
protocol's maximum values per call for the model and call the protocol once
per run, concurrently when the protocol allows parallel calls and one after
another otherwise. It MUST return the embeddings in the order of the values,
the sum of the runs' usage and all their warnings. An empty list of values
MUST return an empty result without a call.
Falsifier: A protocol with a maximum of 2 values per call, embedding 5 values, is called other than with the runs `[1,2]`, `[3,4]` and `[5]`, or the result is out of order, lacks a run's usage in its total or drops a warning; a protocol that disallows parallel calls has two calls in flight at once; or an empty list makes a call.
Mechanism: operations
Rationale: `embed/embed-many.ts` splits by `maxEmbeddingsPerCall`; Gemini takes 100 values per call, and Vertex takes 250 or 1.
Status: Agreed 2026-10-23

[OPS-003] `ops::generate_image` and `ops::generate_video` with a count `n`
above the protocol's maximum per call MUST make one call per batch of that
maximum, the last with the remainder, run them concurrently, and return all
images or videos in call order with the sum of their usage and all their
warnings.
Falsifier: A protocol with a maximum of 1 image per call, asked for 3 images, is called other than three times with `n` 1, or the result holds other than the three images in call order, or loses a call's usage or warnings; or a video protocol with a maximum of 4, asked for 5 videos, starts other than jobs of 4 and 1.
Mechanism: operations
Rationale: `generate-image/generate-image.ts` and `generate-video/generate-video.ts` split by the model's maximum per call; Gemini image models return one image per call and Veo up to 4 videos.
Status: Agreed 2026-10-23

[OPS-004] `ops::start_video` MUST return a `VideoJob` without waiting for
the video: the provider name, the model id and the protocol's operation
value, all of which serialize to JSON and back. `ops::video_status` with a
job MUST return `Pending`, `Completed` with the videos as file parts, or
`Failed` with the provider's message. `ops::generate_video` on a protocol
with jobs MUST start a job and check its status every poll interval (5 s by
default) until it completes. It MUST fail with an `Error::Provider` of kind
`Timeout` naming the limit when the poll timeout (600 s by default) passes
first, and with an `Error::Provider` holding the provider's message when the
job fails. Its videos MUST
be downloaded through the protocol, so that a URL is fetched with the
credentials the protocol attaches. Dropping the call MUST stop the polling.
Falsifier: A scripted job that is pending twice and then completes is checked other than three times, or at another interval than the one set; the completed videos are not fetched through the protocol's download; a job that never completes does not fail with a `Timeout` provider error after the poll timeout; a failed job gives another error kind or message; a job serialized to JSON and read back gets another status request; or a dropped call checks the status again.
Mechanism: operations
Rationale: `generate-video/generate-video.ts` polls every 5 s for up to 600 s and downloads URL results before it returns them.
Status: Agreed 2026-10-23

[OPS-005] `ops::upload_file` MUST send a file's bytes, media type and
filename through the provider's files operation and return an
`UploadedFile` with the provider reference (provider name to file id or
URI), and the media type, filename, size, creation time and expiry time that
the provider reports, each absent when it does not.
Falsifier: A scripted files operation receives other bytes, another media type or filename than given, or the result has another provider reference or fills a field the provider did not report.
Mechanism: operations
Rationale: `upload-file/upload-file.ts`; Gemini fetches an uploaded file by its URI.
Status: Agreed 2026-10-23

[OPS-006] `ops::batch::start` MUST send a list of batch requests, each with
an id, a model id and either a text request (messages and model call
options) or an image request (prompt and image parameters), and return the
batch id, the warnings by request id and the batch status.
`ops::batch::status`, `ops::batch::cancel` and `ops::batch::list` (a page
size and a cursor, returning batches and the next cursor) MUST address a
batch by its id. A status MUST be `pending`, `completed` or `failed`, with
the raw provider state, the request counts (total, pending, completed,
failed) only when they add up, an error with message, type and code, and
the creation and expiry times. `ops::batch::results` MUST stream one result
per request with its id, its kind and `succeeded` with the message or
images, `failed` with an error, `cancelled` or `expired`.
Falsifier: A scripted batch receives other requests than given, or the front ends return other ids, statuses, counts, cursors or results; request counts whose parts do not add up to the total are reported; or a result stream ends before the provider's last result or reorders them.
Mechanism: operations
Rationale: `batch/batch.ts` and `normalizeBatchRequestCounts` in `provider-utils`.
Status: Agreed 2026-10-23

[OPS-007] `ops::stream_transcription` MUST take a stream of raw audio chunks
with their format (a media type and an optional sample rate) and return a
stream of transcription parts: a start with warnings, append-only deltas,
revisable partial texts and final texts, each with an optional segment id;
and a finish with the whole text, the segments, the language, the duration
and the usage the provider reports. Raw provider messages MUST be included
only when the call asks for them. Dropping the stream MUST close the
provider connection and stop reading the audio.
Falsifier: A scripted transcription stream's parts arrive in another order or with other fields; raw messages appear without the request or are missing with it; or after the stream is dropped the audio source is read again or the connection stays open.
Mechanism: operations
Rationale: `transcribe/stream-transcribe.ts`.
Status: Agreed 2026-10-23

[OPS-008] `ops::stream_translation` MUST take a stream of raw audio chunks,
their format, a target language and an optional source language and output
format, and return a stream of translation parts: a start with warnings,
output audio chunks, output text deltas and final texts, source transcript
deltas, partial and final texts, and a finish with the source text, the
output text, the duration and the usage (input and output audio seconds and
tokens, and input and output text tokens). Raw messages and dropping follow
OPS-007.
Falsifier: A scripted translation stream's parts arrive in another order or with other fields, or a dropped stream reads the audio or keeps the connection open.
Mechanism: operations
Rationale: `translate/stream-translate.ts`.
Status: Agreed 2026-10-23

[OPS-009] A protocol with realtime support MUST give, for a model,
`ops::realtime::create_token` (an ephemeral token, the WebSocket URL and the
expiry, for a client to connect with), the provider's session setup for a
normalized session configuration, and a codec per connection that turns a
normalized client event into the provider's message or nothing, and a
provider message into normalized server events. `ops::realtime::connect`
MUST open a WebSocket with a token through the WebSocket transport, send the
setup and each client event the codec turns into a message, and yield the
codec's server events in arrival order; dropping the connection MUST close
the socket.
Falsifier: A scripted WebSocket transport receives another URL, another setup or another message for a client event, or a client event the codec drops is sent; the yielded events differ from the codec's events for the received messages or arrive out of order; or a dropped connection leaves the socket open.
Mechanism: operations
Rationale: The TypeScript realtime session runs in the browser and calls the same model methods (`realtime/realtime-event-channel.ts`); a Rust server needs the token, the codec and a server-side connection.
Status: Agreed 2026-10-23

[OPS-010] The crate MUST define a `WebSocketTransport` trait that opens a
connection to a `wss://` or `ws://` URL with request headers and exchanges
text messages in both directions, with close codes and reasons reported, and
the `websocket` feature MUST provide an implementation over
`tokio-tungstenite` with rustls and a 10 s connect limit. Debug output of a
connection request MUST NOT show its headers or its URL query.
Falsifier: The implementation fails to connect to a local test WebSocket server, sends other headers than given, does not deliver a text message in either direction, reports another close code, or takes longer than 10 s to fail against a server that never answers the handshake; or a request's debug output contains a header value or a query string.
Mechanism: operations
Rationale: Live transcription, translation and realtime need WebSockets, and the HTTP transport cannot carry them.
Status: Agreed 2026-10-23

[OPS-011] `ops::experimental::evaluate` on a protocol whose evaluation is
built on its language model MUST make one buffered model call with no
reasoning by default, the fixed system text of the TypeScript decision
adapter, a user turn that holds the questions as JSON (each question under
`q<i>`, choice options as `c<i>` with their labels and descriptions) followed
by `Shared state:` and the state, and an output schema with one property per
question: a string enum of the option codes for a choice, and a number for a
score or a Noul. It MUST answer a choice as `{"type": "choice", "choice":
<label>}` (from the option code, or the label itself), a score as `{"type":
"score", "score": <n>}` and a Noul as `{"type": "noul", "noul": <p>}`, keyed
by the question ids, and MUST fail with `Error::Provider` of kind `Response`
when the call does not finish with `stop`, its text is not JSON, it holds
another set of keys, a choice is unknown, a score lies outside 0 to its
level count minus one, or a probability lies outside 0 to 1.
Falsifier: A scripted language model receives another prompt, schema or reasoning setting; an answer of `c1` for a choice of options `a` and `b` gives other than `b`; a score of 3 on a two-level question, a probability of 1.5, a missing key, non-JSON text or a `length` finish does not fail with that error kind.
Mechanism: operations
Rationale: `DecisionLanguageModel` in `provider-utils`; the crate's question types and answer shapes are the Python SDK's (`ai/ops/experimental/evaluation.py`).
Status: Agreed 2026-10-23

[OPS-012] `ops::file_info`, `ops::download_file` and `ops::delete_file` MUST
take a provider and a file's provider reference, as `UploadedFile` holds it,
and call the provider's files operation: info MUST return an `UploadedFile`
with the fields the provider reports, download the file's bytes and the media
type the provider reports, read up to 256 MiB, and delete whether the provider
deleted the file. A provider without the operation MUST fail with
`Error::UnsupportedOperation` naming it before any request is sent, and each
MUST retry as OPS-001 says.
Falsifier: A scripted files operation receives another reference, or a front end returns other fields, bytes, media type or deleted flag; a download over 256 MiB is not refused; or a provider without the operation sends a request or fails with another error kind.
Mechanism: operations
Rationale: `FilesV4` in `@ai-sdk/provider` has `getFileMetadata`, `downloadFile` and `deleteFile`, which OpenAI and xAI implement.
Status: Agreed 2026-10-23

[OPS-013] `ops::upload_skill` MUST take a provider, the skill's files, each a
path and its bytes, and an optional display title, call the provider's skills
operation and return an `UploadedSkill` with the provider reference and the
name, description, latest version and metadata the provider reports. A
provider without the operation MUST fail with `Error::UnsupportedOperation`
before any request is sent.
Falsifier: A scripted skills operation receives other files, paths or title, or the front end returns another reference or a field the provider did not report; or a provider without the operation sends a request or fails with another error kind.
Mechanism: operations
Rationale: `upload-skill/upload-skill.ts` and `SkillsV4`, which OpenAI and Anthropic implement.
Status: Agreed 2026-10-23

[OPS-014] `ops::realtime::connect_server` MUST open a session with the
provider's own credentials through its WebSocket transport, for a server that
holds the key, and send the session setup as the protocol starts a session: a
session start for a protocol whose sessions start once, a session update
otherwise. `ops::realtime::create_webrtc_session` MUST send an SDP offer and a
session configuration through the protocol and return the session id and the
SDP answer. A protocol without either MUST fail with
`Error::UnsupportedOperation` before any request is sent, and a dropped
connection MUST close its socket as OPS-009 says.
Falsifier: A scripted WebSocket transport receives another URL, headers or first message; a scripted protocol's WebRTC session receives another offer or configuration, or the front end returns another id or answer; or a protocol without either sends a request or fails with another error kind.
Mechanism: operations
Rationale: OpenAI's Live API is opened by a server with its key (`getServerWebSocketConfig`) or by a browser through WebRTC, and takes no client secrets.
Status: Agreed 2026-10-23
