Prefix: VTX

# Google Vertex AI

Vertex AI, as the AI SDK for TypeScript 7.0.148 package
`@ai-sdk/google-vertex` sends and reads it: Gemini language models,
embeddings, Gemini image generation, Veo video, Gemini and Chirp
text-to-speech, Cloud Speech-to-Text and Gemini transcription, the
Interactions API, and the partner models of Anthropic, the open models
(MaaS) and xAI. The Gemini parts reuse `providers::google` (GGL) with the
differences below. The crate reads no environment and no credential files;
the application passes the values that the TypeScript package reads from the
environment variables named below, through the configuration's `env` map.
Paths in rationale lines are under `packages/google-vertex/src/`.

The Vertex host for a location is `aiplatform.googleapis.com` for `global`,
`aiplatform.<location>.rep.googleapis.com` for `eu` and `us`, and
`<location>-aiplatform.googleapis.com` otherwise. A location MUST be one DNS
label (letters, digits and hyphens), and another fails with
`Error::Configuration` before any request is sent.

[VTX-001] `providers::google_vertex::register` MUST register a provider named
`google-vertex` under `google-vertex` and `@ai-sdk/google-vertex`,
restorable from a snapshot. Its base URL MUST be the configured base URL;
with an Express Mode API key (`GOOGLE_VERTEX_API_KEY`),
`https://aiplatform.googleapis.com/v1/publishers/google`; and otherwise
`https://<host>/v1beta1/projects/<project>/locations/<location>/publishers/google`
with the project and location from `GOOGLE_VERTEX_PROJECT` and
`GOOGLE_VERTEX_LOCATION`, without `/publishers/google` for a model id
starting with `endpoints/` (a tuned model). An Express Mode key MUST be sent
as `x-goog-api-key` on every request. A tuned model, the Interactions API, a
Chirp speech model or transcription with an Express Mode key MUST fail with
`Error::Configuration` before any request is sent.
Falsifier: A recorded Gemini request for each location kind, for Express Mode and for `endpoints/123` goes to another URL or lacks the key header; a location `us-central1/x` sends a request; the provider is missing under a handle; or a tuned model, an Interactions call, `chirp-3-hd` or a transcription with an Express Mode key sends a request or fails with another error.
Mechanism: vertex
Rationale: `google-vertex-provider-base.ts`; the crate's `env` map stands in for the environment.
Status: Agreed 2026-10-23

[VTX-002] Without an Express Mode key, every Vertex request MUST carry the
Vertex auth headers, the configured headers applying after them. They MUST
come from the credentials the application set on the provider: an access
token source, sent as `authorization: Bearer <token>`; or, with the feature
`google-auth`, a `google_cloud_auth::credentials::Credentials` value, every
header of its `headers` sent, `x-goog-user-project` included. With that
feature and no credentials set, they MUST come from a service account built
with `google_cloud_auth::credentials::service_account::Builder` from the
`env` values `GOOGLE_CLIENT_EMAIL`, `GOOGLE_PRIVATE_KEY` (a PKCS #8 PEM key)
and the optional `GOOGLE_PRIVATE_KEY_ID`, with the scope
`https://www.googleapis.com/auth/cloud-platform`. The crate MUST ask the
credentials for the headers on each request and keep no token itself.
Without credentials, a call MUST fail with `Error::Configuration` before any
request is sent, naming the `google-auth` feature when it is off; a
credentials error MUST fail with an `Error::Provider` that contains neither
the private key nor a token. A snapshot MUST NOT hold a credential, a token
or the token source.
Falsifier: A recorded request with a token source giving `t1`, with credentials whose headers are `authorization: Bearer t2` and `x-goog-user-project: billing-1`, or with a service account in the `env` map lacks those headers, or keeps them over a configured `authorization` header; the service account's bearer token is not a JWT whose header holds the key id and whose claims hold the email as `iss` and the cloud-platform scope; two requests ask the credentials for headers fewer than twice; a call without credentials sends a request, or does not name the feature when it is off; an error's text contains the private key or a token; or a snapshot holds any of them.
Mechanism: vertex
Rationale: TypeScript signs and exchanges a service-account JWT itself (`edge/google-vertex-auth-edge.ts`) or uses `google-auth-library`; Google's Rust crate `google-cloud-auth` 1.18.0 gives application default credentials, service accounts with locally signed tokens, and the quota project header that Google's Go SDK also sends.
Status: Agreed 2026-10-23

[VTX-003] Gemini on Vertex MUST follow GGL-001 to GGL-012 except: provider
options MUST be read from `googleVertex`, then `vertex`, then `google`, and
written into part metadata under both `googleVertex` and `vertex`; function
calls and responses MUST NOT carry an `id`; frequency and presence penalties
MUST be sent for Gemini 2.5; `serviceTier` MUST fail with
`Error::Validation`; `sharedRequestType` and `requestType` MUST be sent as
the headers `X-Vertex-AI-LLM-Shared-Request-Type` and
`X-Vertex-AI-LLM-Request-Type`; `imageConfig.personGeneration`,
`prominentPeople` and `imageOutputOptions` MUST be sent; Gemini 3 function
and built-in tools together MUST NOT send
`includeServerSideToolInvocations`; every `http`, `https` and `gs` file URL
MUST be sent as `fileData`; and `enterprise_web_search` and
`vertex_rag_store` MUST be sent as `enterpriseWebSearch: {}` and
`retrieval.vertex_rag_store` with `rag_resources.rag_corpus` and
`similarity_top_k`. Its responses and those of VTX-007 to VTX-009 MUST
follow the size limit of GGL-032.
Falsifier: A recorded Vertex body for options under each key, a tool call and its result, penalties on `gemini-2.5-flash`, the request types, the Vertex image options, mixed tools on `gemini-3-flash-preview`, an `https` and a `gs` file URL and both Vertex tools differs from the stated one; a part's signature is not written under both keys; `serviceTier` sends a request; or a 20 MiB response to a Vertex Gemini, image, video or speech call fails.
Mechanism: vertex
Rationale: The Vertex branches in `google-language-model.ts`, `convert-to-google-messages.ts` and `google-prepare-tools.ts` of `@ai-sdk/google`.
Status: Agreed 2026-10-23

[VTX-004] With the option `streamFunctionCallArguments` on a streaming
Vertex call, the request MUST set
`toolConfig.functionCallingConfig.streamFunctionCallArguments: true`, and
`functionCall` parts carrying `partialArgs` (each a JSON path with a string,
number, boolean or null value, a string continuing while `willContinue` is
true) MUST be delivered as one tool call whose argument deltas concatenate
to the JSON text of the final arguments object, keys in arrival order.
Falsifier: A recorded body lacks the flag; or a stream of `partialArgs` for `$.location` in two string pieces, `$.units` and `$.recipe.ingredients[0].name` delivers deltas that do not join to the JSON of the final object, or more than one tool call.
Mechanism: vertex
Rationale: `google-json-accumulator.ts` of `@ai-sdk/google`; only Vertex streams function call arguments.
Status: Agreed 2026-10-23

[VTX-005] A Vertex Gemini call MUST download a tool result's file URL, up to
7 MiB (or the provider's set limit), and send it as inline data, except that
for a model with Gemini 3 features a `gs://` URL of an `image/png`,
`image/jpeg`, `image/webp`, `application/pdf` or `text/plain` file MUST be
sent as `fileData`. A download over the limit MUST fail before the model
request is sent.
Falsifier: A recorded Vertex body sends a tool result's `https` image URL instead of its downloaded bytes, or downloads a `gs://` PNG on `gemini-3-flash-preview`; or a download of 8 MiB sends the model request.
Mechanism: vertex
Rationale: `download-tool-result-files.ts` of `@ai-sdk/google`, set up by `google-vertex-provider-base.ts`.
Status: Agreed 2026-10-23

[VTX-006] Vertex embedding MUST use `POST <base>/models/<id>:embedContent`
with `{"content": {"parts": [{"text"}]}, "embedContentConfig":
{"outputDimensionality", "taskType", "title", "autoTruncate"}}`, one value per
call, for `gemini-embedding-2` and `gemini-embedding-2-preview`, and `POST
<base>/models/<id>:predict` with `{"instances": [{"content", "task_type",
"title"}], "parameters": {"outputDimensionality", "autoTruncate"}}`, up to
250 values per call, for other models. Usage MUST be the prompt token count
or the sum of the predictions' token counts.
Falsifier: A recorded embedding for each endpoint differs from the stated body or URL; the reported maximum per call differs; or the result has other vectors or usage.
Mechanism: vertex
Rationale: `google-vertex-embedding-model.ts`.
Status: Agreed 2026-10-23

[VTX-007] Vertex image generation MUST follow GGL-014 on the Vertex Gemini
call with the options under `googleVertex` and `vertex`, without the
`googleSearch` option, and return metadata under `googleVertex` and `vertex`
with one empty object per image.
Falsifier: A recorded Vertex image call sends another body or URL, or the result's metadata differs.
Mechanism: vertex
Rationale: `google-vertex-image-model.ts`.
Status: Agreed 2026-10-23

[VTX-008] Vertex video generation MUST follow GGL-019's start request, with
reference images from the provider options sent as given, the call's or the
option's `generateAudio` and the option `gcsOutputDirectory` in
`parameters`, and its status from `POST <base>/models/<id>:fetchPredictOperation`
with `{"operationName"}`. A completed job MUST return each of
`response.videos` as its base64 bytes or its `gcsUri`, with its MIME type
(`video/mp4` by default) and metadata under `googleVertex`, `google-vertex`
and `vertex`.
Falsifier: A recorded start or status request differs from the stated one; or a completed job with a base64 video and a `gs://` video returns other files or metadata.
Mechanism: vertex
Rationale: `google-vertex-video-model.ts`.
Status: Agreed 2026-10-23

[VTX-009] Vertex speech MUST send a model id starting with `chirp` to Cloud
Text-to-Speech, `POST https://texttospeech.googleapis.com/v1/text:synthesize`
with `{"input": {"text"}, "voice": {"languageCode", "name"}, "audioConfig":
{"audioEncoding": "LINEAR16", "speakingRate"}}`: a voice containing
`Chirp3-HD` used as it is, with the language before it (or the language
setting, or `en-US`), and any other voice named
`<language>-Chirp3-HD-<voice>` (`Kore` and `en-US` by default); it MUST warn
for instructions and for an output format other than `wav`, and return the
decoded `audioContent` as `audio/wav`. Other speech models MUST follow GGL-015
on `<base>/models/<id>:generateContent` with the options under `googleVertex`,
then `vertex`, then `google`.
Falsifier: A recorded Chirp call for each voice form differs from the stated body or URL, or lacks a stated warning; or a Gemini TTS call on Vertex goes to another URL or reads its options from another key.
Mechanism: vertex
Rationale: `google-vertex-cloud-tts-speech-model.ts`.
Status: Agreed 2026-10-23

[VTX-010] Vertex transcription MUST send a model id that does not start with
`gemini` to Cloud Speech-to-Text, `POST
https://<region host>/v2/projects/<project>/locations/<region>/recognizers/_:recognize`
(`speech.googleapis.com` for `global`, else `<region>-speech.googleapis.com`;
the region the option `region` or the location) with `{"config": {"model",
"languageCodes" (["auto"] by default), "autoDecodingConfig": {}, "features":
{"enableWordTimeOffsets", "enableAutomaticPunctuation"} (both true by
default)}, "content"}`, and return the results' first transcripts joined by
spaces, a segment per timed word, the first result's language as a two-letter
code and `totalBilledDuration` as the duration.
Falsifier: A recorded Cloud Speech-to-Text call differs from the stated body or URL for each option; or a recorded response gives another text, segments, language or duration.
Mechanism: vertex
Rationale: `google-vertex-transcription-model.ts`.
Status: Agreed 2026-10-23

[VTX-011] Gemini transcription on Vertex MUST send a whole file to `POST
<base>/models/<id>:generateContent` with the audio as `inlineData` and the
camel-case options as `generationConfig.audioTranscriptionConfig`, and return
the parts' text, or else their `audioTranscription` text, the timed words as
segments and the first transcription's language. Live transcription MUST
follow GGL-017 on
`wss://<host>/ws/google.cloud.aiplatform.v1.LlmBidiService/BidiGenerateContent`
with the Vertex auth headers and the model
`projects/<project>/locations/<location>/publishers/google/models/<id>`.
Falsifier: A recorded whole-file call or a scripted live session differs from the stated URL, headers, setup, parts or result.
Mechanism: vertex
Rationale: `gemini-transcription/google-vertex-gemini-transcription-model.ts`.
Status: Agreed 2026-10-23

[VTX-012] The Interactions protocol on Vertex MUST follow GGL-026 to GGL-030
at `https://<host>/v1beta1/projects/<project>/locations/<location>/interactions`
with the Vertex auth headers.
Falsifier: A recorded Vertex Interactions request goes to another URL or lacks the Vertex auth headers.
Mechanism: vertex
Rationale: `google-vertex-provider-base.ts` gives the Interactions model the location's base URL.
Status: Agreed 2026-10-23

[VTX-013] `providers::google_vertex::anthropic::register` MUST register a
provider named `google-vertex-anthropic` under `google-vertex-anthropic` and
`@ai-sdk/google-vertex/anthropic` on the Anthropic Messages protocol at
`https://<host>/v1/projects/<project>/locations/<location>/publishers/anthropic/models/<id>:streamRawPredict`
(`:rawPredict` for a buffered call), with the Vertex auth headers, a body without
`model` and with `anthropic_version: "vertex-2023-10-16"`, and the header
`x-vertex-ai-labels` holding the base64 JSON of the option `labels` under
`vertex` unless the call sets that header. Every file URL MUST be downloaded
and sent as base64, an output schema MUST use the JSON tool, strict tools
MUST NOT be sent as strict, and Anthropic tools other than the bash,
text editor, computer, web search and tool search versions that
`googleVertexAnthropicTools` lists MUST fail with `Error::Validation`.
Falsifier: A recorded request goes to another URL, holds `model`, lacks `anthropic_version` or the labels header, sends a URL file, uses native structured output or a strict flag; or an unlisted Anthropic tool sends a request.
Mechanism: vertex
Rationale: `anthropic/google-vertex-anthropic-provider.ts`.
Status: Agreed 2026-10-23

[VTX-014] `providers::google_vertex::maas::register` MUST register a provider
named `google-vertex-maas` under `@ai-sdk/google-vertex/maas` on the Chat
Completions protocol at
`https://<host>/v1/projects/<project>/locations/<location>/endpoints/openapi/chat/completions`,
the location `global` by default, with the Vertex auth headers after the
configured headers, and `max_tokens: 8192` when unset for
`meta/llama-4-maverick-17b-128e-instruct-maas` and
`meta/llama-4-scout-17b-16e-instruct-maas`.
Falsifier: A recorded request goes to another URL, its Vertex auth headers are replaced by a configured header, or a Llama 4 request without a limit lacks `max_tokens: 8192`.
Mechanism: vertex
Rationale: `maas/google-vertex-maas-provider.ts`.
Status: Agreed 2026-10-23

[VTX-015] `providers::google_vertex::xai::register` MUST register a provider
named `google-vertex-xai` under `@ai-sdk/google-vertex/xai` on the Chat
Completions protocol at
`https://aiplatform.googleapis.com/v1/projects/<project>/locations/<location>/endpoints/openapi/chat/completions`,
the location `global` by default, with the Vertex auth headers after the
configured headers, `stream_options.include_usage`, an output schema as `response_format`
of type `json_schema`, no `reasoning_effort`, only image file URLs sent as
URLs, and usage of prompt tokens as input, cached tokens as cache reads, and
completion plus reasoning tokens as output.
Falsifier: A recorded request goes to another URL, sends `reasoning_effort`, lacks `stream_options.include_usage` or the schema format, or sends a PDF URL as a URL; or recorded usage of prompt 10, cached 4, completion 6 and reasoning 3 reports other than input 10, cache read 4, output 9 and reasoning 3.
Mechanism: vertex
Rationale: `xai/google-vertex-xai-provider.ts`.
Status: Agreed 2026-10-23
