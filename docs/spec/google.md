Prefix: GGL

# Google Gemini API

The Gemini API provider, as the AI SDK for TypeScript 7.0.148 package
`@ai-sdk/google` sends and reads it, with every model kind it ships: language
models (`generateContent`), embeddings, Gemini image generation, Gemini
text-to-speech, transcription, live transcription and speech translation,
Veo video, the files API, batches, the Live API for realtime sessions, the
Interactions API with its agents, and evaluation. Gemini is its own
protocol, so `providers::google` has its own request builder, message
converter and stream reader on the shared HTTP layer, and the operations of
`docs/spec/operations.md` call into it. Vertex AI reuses this module with the
differences in `docs/spec/vertex.md`. Paths in rationale lines are under
`packages/google/src/`.

A model uses Gemini 3 features when its id matches `gemini-` at its start or
after `/` or `.`, and is not a Gemini 1 or 2 model (`gemini-1`, `gemini-2`
followed by `.`, `-` or the end), `gemini-pro`, `gemini-pro-vision` or
`gemini-robotics-er-1.5` (`google-model-capabilities.ts`). Provider options
are the `google` entry of the call's provider parameters, with the
TypeScript option names; an unknown option name fails with
`Error::Validation`.

[GGL-001] `providers::google::register` MUST register a provider named
`google` under `google` and `@ai-sdk/google`, restorable from a snapshot. Its
model calls MUST stream `POST <base>/<model path>:streamGenerateContent?alt=sse`,
where the base is the configured base URL or
`https://generativelanguage.googleapis.com/v1beta` and the model path is the
model id when it contains `/` and `models/<id>` otherwise, with the key in the
header `x-goog-api-key`. The key is the configuration's API key value, or the
`env` value named `GOOGLE_GENERATIVE_AI_API_KEY`. Registering it with
OpenAI, Anthropic, every host and Vertex AI into one registry MUST succeed.
Falsifier: A recorded request for the model `gemini-2.5-flash` goes to another URL than `https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse`, or for `tunedModels/t1` to another path than `tunedModels/t1:streamGenerateContent`; uses another method or key header, or omits the key from the configuration or from `env`; the provider is missing under a handle or has another name; a model restored from its snapshot streams from another URL; or registering it with `openai::register`, `anthropic::register`, every host and `google_vertex::register` fails.
Mechanism: google
Status: Agreed 2026-10-23

[GGL-002] A request MUST send the output token limit, temperature, top-p,
top-k, seed, frequency penalty and presence penalty in `generationConfig` as
`maxOutputTokens`, `temperature`, `topP`, `topK`, `seed`, `frequencyPenalty`
and `presencePenalty`, and a service tier as `serviceTier`. A call MUST fail
with `Error::Validation` before any request is sent when it sets a frequency
or presence penalty for a Gemini 2.5 model (`gemini-2.5` at the start of its
id or after `/` or `.`, followed by `.`, `-` or the end), or sets min-p, a
repetition penalty, a text verbosity, an output include list, a maximum tool
call count, parallel tool calls, a safety identifier, metadata, a cache key,
a cache retention, context management or a tool selection.
Falsifier: A recorded body for an output token limit of 100, temperature 0.5, top-p 0.9, top-k 40, seed 7, frequency penalty 0.1 and presence penalty 0.2 on `gemini-3-flash-preview` lacks any of them under those `generationConfig` names, or a service tier `flex` is not sent as `serviceTier`; a frequency penalty on `gemini-2.5-flash`, or any other setting named above, sends a request or fails with another error kind.
Mechanism: google
Rationale: `google-language-model.ts` drops Gemini 2.5's penalties with a warning and never sends the other settings.
Status: Agreed 2026-10-23

[GGL-003] A reasoning effort MUST be sent as `generationConfig.thinkingConfig`.
For a model with Gemini 3 features other than `gemini-3-pro-image` models, it
is `thinkingLevel`: `minimal` as the model's lowest level, `low`, `medium` and
`high` as given, and `xhigh` and `max` as `high`; disabled reasoning is the
lowest level, which is `low` for `gemini-flash-latest`, for
`gemini-<n>[.<m>]-pro` with n at least 3 and for `gemini-<n>.<m>-flash`
(not `-lite`) from version 3.7 on, and `minimal` otherwise. For other models
it is `thinkingBudget`: 0 for disabled reasoning and, with 65536 output tokens
and a cap of 32768 for `2.5-pro` and `gemini-3-pro-image` models and 24576
otherwise, the capped rounding of 65536 times 0.02, 0.1, 0.3, 0.6, 0.9 or 0.95
for `minimal`, `low`, `medium`, `high`, `xhigh` or `max`. A reasoning summary
MUST be sent as `thinkingConfig.includeThoughts: true`. The provider option
`thinkingConfig` MUST be merged over the derived one, and one that sets a
level or budget replaces the derived value.
Falsifier: A recorded `gemini-3-flash-preview` body for effort `medium` has a `thinkingConfig` other than `{"thinkingLevel": "medium"}`, for `xhigh` other than `high`, for disabled reasoning or `minimal` other than `minimal`; a `gemini-3-pro-preview` body for disabled reasoning has a level other than `low`; a `gemini-2.5-flash` body for disabled reasoning has a `thinkingBudget` other than 0, for `low` other than 6554, for `high` other than 24576, or a `gemini-2.5-pro` body for `max` other than 32768; a call with a reasoning summary lacks `includeThoughts: true`; or the option `{"thinkingConfig": {"thinkingBudget": 100}}` with effort `high` sends another budget.
Mechanism: google
Rationale: `resolveThinkingConfig` in `google-language-model.ts` and `mapReasoningToProviderBudget` in `provider-utils`; Gemini 3 cannot turn reasoning off.
Status: Agreed 2026-10-23

[GGL-004] An output schema MUST be sent as `responseMimeType:
"application/json"` with `responseJsonSchema` holding the schema with every
`const` replaced by a one-value `enum`, unless the option
`structuredOutputs` is false, which sends the MIME type alone. When the call
also has function tools and the model is a `gemini-` model without Gemini 3
features, or has Gemini 3 features and a required or named tool choice, the
schema MUST instead be sent as a function tool named `json` (or `json_<n>`
when that name is taken) with the description `Respond with a JSON object.`,
chosen by the tool choice, and a call of that tool MUST be delivered as the
response's text.
Falsifier: A recorded body for an output schema with a `const` property on a call without tools lacks `responseMimeType: "application/json"` or sends the `const` instead of the `enum`; `structuredOutputs: false` still sends `responseJsonSchema`; a `gemini-2.5-flash` call with a function tool and an output schema sends `responseJsonSchema`, lacks the `json` function declaration or a tool choice of mode `ANY`; or a stream that calls `json` with arguments `{"value":1}` delivers a tool call, or a text other than `{"value":1}`.
Mechanism: google
Rationale: Gemini before 3 rejects a JSON response format together with function tools (`getArgs` in `google-language-model.ts`).
Status: Agreed 2026-10-23

[GGL-005] System messages MUST be sent as `systemInstruction` parts, and a
system message after a user, assistant or tool message MUST fail with
`Error::Validation`; for `gemma-` models the system text MUST instead start
the first user content, followed by a blank line. Assistant messages MUST be
sent with role `model`: text as `{"text"}`, reasoning as `{"text", "thought":
true}`, each with the part's `thoughtSignature` kept in its metadata under
`google`, and a tool call as `{"functionCall": {"id", "name", "args"}}` with
its signature. For a model with Gemini 3 features, a tool call without a
signature MUST be sent with the signature `skip_thought_signature_validator`,
except after a signed call of the same response. Tool results MUST be sent
with role `user` as `{"functionResponse": {"id", "name", "response": {"name",
"content"}}}`, with `error` instead of `content` for an error result; for a
model with Gemini 3 features a result of content parts MUST be one
`functionResponse` whose `content` joins its texts with newlines and whose
`parts` hold its files as `inlineData`; for other models each text MUST be its
own `functionResponse` and each file `inlineData` followed by the text `Tool
executed successfully and returned this image as a response` (`file` for a
file that is not an image).
Falsifier: A recorded body for a history with a system message, a user message, an assistant turn of reasoning with a signature, text and a tool call with a signature, and its JSON tool result differs from the stated shapes; a system message after the user message sends a request or fails with another error kind; a `gemma-3-27b-it` body has a `systemInstruction`; a `gemini-3-flash-preview` tool call without a signature is sent without `skip_thought_signature_validator`; or an error result is sent with `content`.
Mechanism: google
Rationale: Gemini 3 rejects a replayed call without its thought signature unless it carries the documented sentinel (`convert-to-google-messages.ts`).
Status: Agreed 2026-10-23

[GGL-006] A user file given as bytes MUST be sent as `inlineData` with its
media type and base64 data. A file given as a URL MUST be sent as `fileData`
with that URL when Gemini fetches it: a Gemini files API URL, a YouTube
video URL, or, for a `gemini-` model other than Gemini 2.0, an `https` URL of
one of the media types `google-provider.ts` lists. Any other URL MUST be
downloaded and sent as `inlineData` when the provider was built with a
download transport, and MUST fail with `Error::Validation` before any request
is sent when it was not.
Falsifier: A recorded body sends a PNG given as bytes in another shape; an `https` PDF URL on `gemini-2.5-flash`, a files API URL or a YouTube URL is not sent as `fileData` with that URL; an `https` PDF URL on `gemini-2.0-flash`, or an `http` image URL, is not fetched through the download transport and sent as `inlineData`, or without one sends a request or fails with another error kind.
Mechanism: google
Rationale: The TypeScript core downloads the URLs a model's `supportedUrls` does not list before the call.
Status: Agreed 2026-10-23

[GGL-007] Function tools MUST be sent as `tools: [{"functionDeclarations":
[{"name", "description", "parametersJsonSchema"}]}]`, with an empty
description when there is none, and tool choice `auto`, `none`, `required` and
a named tool as `toolConfig.functionCallingConfig` modes `AUTO`, `NONE`, `ANY`
and `ANY` with `allowedFunctionNames: [<name>]`; a strict tool makes `auto`
and an unset choice `VALIDATED`. The built-in tools
`google::tools::google_search`, `url_context`, `code_execution`,
`google_maps` and `file_search` MUST be sent as `googleSearch`, `urlContext`,
`codeExecution`, `googleMaps` and `fileSearch` with their arguments; with
function tools on a model with Gemini 3 features, the function declarations
follow them and `toolConfig` holds mode `VALIDATED` (or the tool choice's
mode) with `includeServerSideToolInvocations: true`. A built-in tool that the
model's generation does not take, a Vertex AI tool
(`enterprise_web_search`, `vertex_rag_store`), or a function and built-in
tool together on a model without Gemini 3 features, MUST fail with
`Error::Validation` before any request is sent.
Falsifier: A recorded body for two function tools and each tool choice differs from the stated shapes, or a strict tool with an unset choice is not sent with mode `VALIDATED`; a `google_search` tool with a time range filter is not sent as `googleSearch` with that filter; a `gemini-3-flash-preview` call with `google_search` and a function tool lacks the function declarations after it or `includeServerSideToolInvocations: true`; or `file_search` on `gemini-2.0-flash`, `vertex_rag_store` on this provider, or `google_search` with a function tool on `gemini-2.5-flash`, sends a request or fails with another error kind.
Mechanism: google
Rationale: `google-prepare-tools.ts` drops these with a warning, and `google-language-model.ts` only warns before sending a Vertex AI tool.
Status: Agreed 2026-10-23

[GGL-008] A stream MUST read `candidates[0].content.parts` in order: `text`
parts as text and `thought: true` parts as reasoning, each run of one kind a
new block that ends before the other kind starts, with a part's
`thoughtSignature` kept in the block's metadata under `google`, also from an
empty text part; `inlineData` parts as files (reasoning files when
`thought: true`); `functionCall` parts as tool calls with the call's `id`, or
a generated id, and its `args` as arguments; `executableCode` and
`codeExecutionResult` parts as a built-in `code_execution` call and its
result; and `toolCall` and `toolResponse` parts as built-in calls and results
named `server:<toolType>`. Grounding chunks MUST become sources, once per
URL. A finish reason `CONTINUATION` with a `continuationToken` MUST send the
same request with that token and continue the stream with its response. A
stream that ends before a chunk with a finish reason MUST send no finish
event, so that the call fails as an incomplete response.
Falsifier: A stream of thought text `A`, text `B`, an empty text part with signature `S` and a `functionCall` without an id delivers other than reasoning `A`, text `B` with `S` in its metadata, and one tool call with a generated id and the call's arguments; an `inlineData` part is not delivered as a file; an `executableCode` and a `codeExecutionResult` part are not a built-in call and its result; two grounding chunks with the same URL give two sources; a stream ending with finish reason `CONTINUATION` and token `t1` does not lead to a second request with `continuationToken: "t1"` whose text follows the first's; or a stream that stops after text `B` with no finish reason ends without an incomplete-response error.
Mechanism: google
Rationale: TypeScript ends such a stream with the finish reason `interrupted` (`google-language-model.ts`); the crate has no such reason, and its stream fails as incomplete when no finish event arrives.
Status: Agreed 2026-10-23

[GGL-009] Finish reasons MUST map `STOP` to `tool_call` after a function call
and to `stop` otherwise; `MAX_TOKENS` and `CONTINUATION` to `length`;
`IMAGE_SAFETY`, `RECITATION`, `SAFETY`, `BLOCKLIST`, `PROHIBITED_CONTENT` and
`SPII` to `content_filter`; `MALFORMED_FUNCTION_CALL` to `error`; and any
other to `other`. A `promptFeedback.blockReason` other than empty,
`BLOCK_REASON_UNSPECIFIED` and `BLOCKED_REASON_UNSPECIFIED` MUST report
`content_filter`. Usage MUST report `promptTokenCount` plus
`toolUsePromptTokenCount` as input tokens, `cachedContentTokenCount` as
cache-read tokens, `candidatesTokenCount` plus `thoughtsTokenCount` as output
tokens and `thoughtsTokenCount` as reasoning tokens, summed over continued
requests. The stream's final metadata under `google` MUST hold the last
`groundingMetadata`, `urlContextMetadata`, `safetyRatings`,
`promptFeedback`, `usageMetadata` and `finishMessage`, the service tier from
the `x-gemini-service-tier` response header, and the continuation token on a
`MAX_TOKENS` or `CONTINUATION` finish; after a continued request it MUST also
hold the first request's `promptTokenCount` as `initialPromptTokenCount` and
each request's grounding metadata, in order and `null` where absent, as
`groundingMetadataPerRequest`.
Falsifier: A stream ending with each listed finish reason, with and without a function call, reports another finish reason; a `promptFeedback` with `blockReason` `SAFETY` reports other than `content_filter`; usage with prompt 9, tool-use prompt 2, cached 4, candidates 23 and thoughts 185 reports other than input 11, cache read 4, output 208 and reasoning 185; a stream with grounding metadata and the header `x-gemini-service-tier: flex` lacks either in the final metadata; or a stream continued once, with prompt counts 10 and 40 and grounding metadata only in the first request, lacks `initialPromptTokenCount: 10` or a `groundingMetadataPerRequest` of that metadata and `null`, or a stream that is not continued holds either.
Mechanism: google
Rationale: `map-google-finish-reason.ts` and `convert-google-usage.ts`.
Status: Agreed 2026-10-23

[GGL-010] An error response MUST give `ProviderError` the message of its
`error.message`, the code of its `error.code` and the type of its
`error.status`, with the kind from the HTTP status. An `error` object inside
a stream chunk MUST end the call with an `Error::Provider` that keeps the
same fields, with the kind of the HTTP status in its `code`, retryable for
408, 409, 429 and 500 and above.
Falsifier: An HTTP 429 body `{"error": {"code": 429, "message": "m", "status": "RESOURCE_EXHAUSTED"}}` gives another message, code, type or a kind other than `RateLimit`; or a stream chunk `{"error": {"code": 503, "message": "busy", "status": "UNAVAILABLE"}}` gives a kind other than `ServiceUnavailable` or is not retryable.
Mechanism: google
Rationale: `google-error.ts` and the stream error handling in `google-language-model.ts`.
Status: Agreed 2026-10-23

[GGL-011] The provider options MUST be sent as the TypeScript provider sends
them: `responseModalities`, `mediaResolution`, `audioTimestamp` and
`imageConfig` in `generationConfig`; `safetySettings` as given, or
`threshold` as one setting with that threshold for each of
`HARM_CATEGORY_HATE_SPEECH`, `HARM_CATEGORY_DANGEROUS_CONTENT`,
`HARM_CATEGORY_HARASSMENT` and `HARM_CATEGORY_SEXUALLY_EXPLICIT`;
`cachedContent`, `labels`, `serviceTier` and `continuationToken` at the top
level; and `retrievalConfig` in `toolConfig`. On this provider the Vertex AI
options `streamFunctionCallArguments`, `sharedRequestType` and
`requestType`, and `imageConfig.personGeneration`,
`imageConfig.prominentPeople` and `imageConfig.imageOutputOptions`, MUST
fail with `Error::Validation` before any request is sent.
Falsifier: A recorded body for each option lacks it at the stated place or changes its value; `threshold: "BLOCK_ONLY_HIGH"` gives other than the four settings; or a Vertex AI option on this provider sends a request or fails with another error kind.
Mechanism: google
Rationale: `google-language-model.ts` warns about the Vertex AI options and drops them.
Status: Agreed 2026-10-23

[GGL-012] A buffered call (`generate`) MUST send the same body to
`POST <base>/<model path>:generateContent` and return one message whose
parts, finish reason, usage and metadata are what the stream of GGL-008 and
GGL-009 gives for a single chunk holding the whole response, continuing a
`CONTINUATION` finish as the stream does.
Falsifier: A recorded buffered call goes to another URL or sends another body than the stream would; or its message differs from the stream's aggregated message for the same response.
Mechanism: google
Rationale: `doGenerate` in `google-language-model.ts`; the image operation and batch results read the same response shape.
Status: Agreed 2026-10-23

[GGL-013] Embedding MUST allow 100 values per call and parallel calls. One
value MUST be sent as `POST <base>/models/<id>:embedContent` with `{"model":
"models/<id>", "content": {"parts"}, "outputDimensionality", "taskType"}`,
and several as `POST <base>/models/<id>:batchEmbedContents` with one such
request per value, its content of role `user`. A value's parts MUST be its
text, when it is not empty, followed by its entry of the option `content`;
the options `outputDimensionality` and `taskType` are sent as given. A
`content` list of another length than the values MUST fail with
`Error::Validation` naming both lengths. The embeddings MUST be read from
`embedding.values` or `embeddings[].values`.
Falsifier: A recorded single embedding or a batch of two goes to another URL, sends another body, or the result holds other vectors; a `content` entry of an image part is not appended after the text; a `content` list of 1 for 2 values sends a request or fails with another error; or the protocol reports another maximum per call.
Mechanism: google
Rationale: `google-embedding-model.ts`.
Status: Agreed 2026-10-23

[GGL-014] Image generation MUST take only model ids starting with `gemini-`,
one image per call, and MUST make the GGL-012 call with the prompt text and
input images as one user message (an image URL as a file part of media type
`image/*`), the seed, `responseModalities: ["IMAGE"]`, the aspect ratio
merged into the option `imageConfig`, the other language-model options, and
a `google_search` tool with the option `googleSearch` as its arguments when
it is set. It MUST return the response's image files, a warning of kind
`unsupported` for `size`, the usage, and metadata under `google` with the
language model's metadata, the raw finish reason and one empty object per
image. Another model id or a mask MUST fail with `Error::Validation`.
Falsifier: A recorded image call sends another body or URL, keeps a `responseModalities` option, drops `imageConfig`, or omits the search tool; the result holds other images, lacks the `size` warning or the metadata; or `imagen-4.0-generate-001` or a mask sends a request or fails with another error.
Mechanism: google
Rationale: `google-image-model.ts`; Imagen is no longer in the TypeScript package.
Status: Agreed 2026-10-23

[GGL-015] Speech generation MUST send `POST <base>/models/<id>:generateContent`
with one user turn and `generationConfig` holding `responseModalities:
["AUDIO"]` and `speechConfig`: the option `multiSpeakerVoiceConfig` when set,
or `voiceConfig.prebuiltVoiceConfig.voiceName` with the voice (`Kore` by
default). For model ids starting with `gemini-2.5-` or `gemini-3.1-`
instructions MUST prefix the text as `<instructions>: <text>` (a warning
instead when multi-speaker), and the options `turns` and `speechMetadata`
MUST fail with `Error::Validation`; for other models the parts MUST be the
turns, or the text, each with `speechMetadata` holding its style (the
instructions by default) and speaker, every multi-speaker turn's speaker
MUST be a configured one, and an output format other than none MUST be sent
as `responseFormat.audio.mimeType`. Output formats MUST map `wav` to
`AUDIO_WAV` and `pcm` to `AUDIO_L16` (and for other models `mulaw` and
`alaw`, and their `audio/` names); an unknown one MUST warn and use `wav`. The
first inline audio part MUST be returned, wrapped in a 44-byte mono 16-bit
WAV header at the rate from its MIME type (24000 Hz by default) when `wav`
was asked and the audio is PCM. `speed` and `language` MUST warn, and a
custom voice or an empty transcript MUST fail with `Error::Validation`.
Falsifier: A recorded call for each model family, with and without instructions, turns and multi-speaker voices, sends another body or URL; PCM audio at `audio/L16;rate=24000` with `wav` asked is returned without the WAV header or with another rate; a stated warning is missing; or a voice `voice_x`, an empty text, or turns on `gemini-2.5-flash-preview-tts` sends a request or fails with another error.
Mechanism: google
Rationale: `google-speech-model.ts`.
Status: Agreed 2026-10-23

[GGL-016] Transcription of a whole file MUST take model ids without `-live`
and send `POST <base>/interactions` with `{"model": <id>, "input": [{"type":
"audio", "data", "mime_type"}]}` and, when an option is set,
`generation_config.transcription_config` with `language_codes`,
`custom_vocabulary` and `mode` (`type` the lowercased option `mode`,
`verbatim` by default, with `diarization_mode: "speaker"` and
`timestamp_granularities: ["word"]` for the options `diarization` and
`wordTimestamp`). It MUST return the steps' text contents joined, and a
segment for each `word_info` annotation with text and both offsets in
seconds. A `-live` model id MUST fail with `Error::Validation`.
Falsifier: A recorded transcription sends another URL or body for each option; the result has another text or segments for a response with two text contents and word annotations `"1.5s"` to `"2s"`; or `gemini-3.5-transcribe-live` sends a request or fails with another error.
Mechanism: google
Rationale: `transcription/google-transcription-model.ts`; whole-file transcription runs on the Interactions API.
Status: Agreed 2026-10-23

[GGL-017] Live transcription MUST take only `-live` model ids and raw 16 kHz
16-bit PCM audio (any other format fails with `Error::Validation`). It MUST
open `wss://<host>/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent`
on the base URL's host and path without its version, with the key in the
header `x-goog-api-key`, send `{"setup": {"model": <model path>,
"inputAudioTranscription": <config>}}` with the camel-case options and no
`generationConfig`, wait for `setupComplete`, then send each chunk as
`realtimeInput.audio` with MIME type `audio/pcm;rate=16000` and finally
`realtimeInput.audioStreamEnd`. Interim transcriptions MUST give partial
parts, input transcriptions delta parts, and a `finished` transcription or
`turnComplete` a final part of the segment's text (the last interim text when
no delta came), segments numbered `google-segment-<n>`. After the audio ends,
an `IDLE` or `REQUIRES_ACTION` interaction status, a `turnComplete` without a
status, the socket closing, or 3 s without a transcript MUST finish with the
final texts joined by spaces, the last language code and the usage
metadata; a server `error`, a socket error, or a close before the audio ends
MUST end the stream with an error.
Falsifier: A scripted WebSocket receives another URL, header, setup or message order, or audio before `setupComplete`; a scripted session's parts differ from the stated ones; the key appears in the URL; a stream does not finish 3 s after the last transcript under paused time; or a close before the audio ends finishes without an error.
Mechanism: google
Rationale: `transcription/google-transcription-model.ts` puts the key in the URL query; the header keeps it out of URLs that logs and errors record.
Status: Agreed 2026-10-23

[GGL-018] Speech translation MUST take raw 16 kHz 16-bit PCM audio and a
target language, warn for a source language and an output format, and open
the GGL-017 socket with the setup `{"model", "generationConfig":
{"responseModalities": ["AUDIO"], "translationConfig": {"targetLanguageCode",
"echoTargetLanguage"}}, "inputAudioTranscription": {}, "outputAudioTranscription":
{}}`. Input transcriptions MUST give source deltas, output transcriptions
output text deltas and model audio audio parts, items numbered
`google-item-<n>`; a `turnComplete` MUST close the item with its final
source and output texts. After the audio ends, 1 s of output audio whose 16-bit
samples are all within 128 of zero (at 24 kHz), or 1 s after a
`turnComplete`, MUST finish with the joined texts and the summed `AUDIO`
token counts of the prompt and response details.
Falsifier: A scripted WebSocket receives another setup; a scripted session's parts, item ids, texts or usage differ from the stated ones; text token counts enter the usage; or silent audio after the audio ends does not finish the stream.
Mechanism: google
Rationale: `speech-translation/google-speech-translation-model.ts`; Live translation never sends `turnComplete` and keeps sending silence.
Status: Agreed 2026-10-23

[GGL-019] Video generation MUST start a job with `POST
<base>/models/<id>:predictLongRunning` and `{"instances": [{"prompt",
"image", "lastFrame", "referenceImages"}], "parameters": {"sampleCount",
"aspectRatio", "resolution", "durationSeconds", "seed", "personGeneration",
"negativePrompt", ...}}`: the first-frame image (or the image) and the
last-frame image as `bytesBase64Encoded` or a `gs://` `gcsUri` with their
media type, reference images as `{"image", "referenceType": "asset"}`, the
resolutions `1280x720`, `1920x1080` and `3840x2160` as `720p`, `1080p` and
`4k`, and every other provider option copied into `parameters`; an image
URL that is not `gs://` MUST warn and be left out. The job MUST be the
operation `name`, and its status `GET <base>/<name>`: `done` with `error`
fails with `Video generation failed: <message>`, and `done` with samples
completes with each `video.uri`, which the protocol downloads with the key
header only when the URI has the base URL's origin. Up to 4 videos per call.
Falsifier: A recorded start or status request differs from the stated ones; a completed job's video is downloaded without the key header from the base URL's origin, or with it from another origin; a URL in a returned file part holds the key; or an `https` image URL is sent instead of warned.
Mechanism: google
Rationale: `google-video-model.ts` appends the key to the download URL as a query value; the header keeps it out of the URL.
Status: Agreed 2026-10-23

[GGL-020] File upload MUST start a resumable upload with `POST
<origin>/upload/v1beta/files` (the base URL without `/v1beta`), the headers
`X-Goog-Upload-Protocol: resumable`, `X-Goog-Upload-Command: start`,
`X-Goog-Upload-Header-Content-Length` and
`X-Goog-Upload-Header-Content-Type`, and `{"file": {"display_name"}}` from
the option `displayName`; send the bytes to the returned
`x-goog-upload-url` in chunks of 8 MiB, each with its byte offset as
`X-Goog-Upload-Offset` and `X-Goog-Upload-Command: upload`, the last with
`upload, finalize`; send a chunk again, after 1 s and then doubling, when its
response lacks `X-Goog-Upload-Status`, at most 3 times in all; and while the file's `state` is
`PROCESSING`, check `GET <base>/files/<id>` every 2 s (option
`pollIntervalMs`) for up to 300 s (option `pollTimeoutMs`). It MUST return the
file's `uri` as the provider reference under `google`, its MIME type, and its
metadata; a `FAILED` state, a missing upload URL, an upload status other than
`active` before the last chunk or `final` after it, a chunk still without a
status after its third send, or the timeout MUST fail with
`Error::Provider`, and a filename MUST warn.
Falsifier: A recorded upload sends another URL, header or body at either step, or polls at another interval; a 20 MiB file is sent other than as chunks of 8, 8 and 4 MiB at offsets 0, 8388608 and 16777216 with only the last finalizing; a chunk answered without `X-Goog-Upload-Status` is not sent again, or is sent a fourth time; the result has another reference or metadata; or a `FAILED` file, a missing `x-goog-upload-url`, a finalize answered with status `active`, or a file still processing after the timeout does not fail.
Mechanism: google
Rationale: `google-files.ts` sends the whole file in one request; Google's Go SDK sends 8 MiB chunks and checks `X-Goog-Upload-Status` (`api_client.go`), so a failure resends one chunk instead of a file of up to 2 GB.
Status: Agreed 2026-10-23

[GGL-021] Starting a batch MUST take text and image requests that all name
the same model (another model, an empty list or a mask fail with
`Error::Validation`), build each request's body as GGL-001 and GGL-014 do,
and send `POST <base>/<model path>:batchGenerateContent` with `{"batch":
{"displayName": "ai-sdk-batch-<id>", "webhookConfig", "inputConfig":
{"requests": {"requests": [{"request", "metadata": {"key": <id>}}]}}}}`
while the body stays under 20,000,000 bytes. A larger batch MUST instead be
uploaded as JSON lines of `{"key", "request"}` (at most 2 GiB) through the
GGL-020 resumable upload with media type `application/jsonl` and sent with
`inputConfig.fileName`, the input file's name and expiry in the metadata.
Falsifier: A recorded batch of one text and one image request sends another body or URL; a batch of mixed models or an image request with `n` 2 sends a request; or a batch whose inline body would reach 20,000,000 bytes is not uploaded as JSON lines and sent by file name.
Mechanism: google
Rationale: `google-batch.ts`.
Status: Agreed 2026-10-23

[GGL-022] A batch's status MUST come from `GET <base>/<batch id>`, `cancel`
from `POST <base>/<batch id>:cancel` and `list` from `GET
<base>/batches?pageSize&pageToken`. A state, without its `BATCH_STATE_` or
`JOB_STATE_` prefix, of `SUCCEEDED` MUST be `completed`, `FAILED`,
`CANCELLED` or `EXPIRED` `failed`, any other `pending`, a missing state the
operation's `done`, and an operation error `failed`. Results of a pending
batch MUST fail with `Error::Validation`; they MUST be read from the inline
responses or streamed from `GET <origin>/download/v1beta/<file>:download?alt=media`
as JSON lines of at most 64 MiB, each line a request's error (`cancelled`
for status `CANCELLED` or code 1, else `failed`), a prompt block (`failed`,
code `prompt_blocked`), a response read as GGL-012 reads one (an image
result when it holds images), or `failed` for a missing or malformed
response.
Falsifier: A recorded status, cancel or list request differs; a listed state maps to another status; results of a pending batch are fetched; a JSON-lines file of an error line, a blocked line, a text line and an image line gives other results; or a line over 64 MiB is read whole.
Mechanism: google
Rationale: `google-batch.ts`; the result file can be larger than memory, so it is read line by line.
Status: Agreed 2026-10-23

[GGL-023] A realtime token MUST be created with `POST
<realtime base>/v1alpha/auth_tokens`, where the realtime base is the base URL
without its trailing `v1beta` or `v1alpha`, with the key in the header
`x-goog-api-key` and `{"uses": 0, "expireTime", "newSessionExpireTime",
"bidiGenerateContentSetup"}`: the session window the requested seconds (60
by default) from now, the token's expiry 30 minutes later, and the setup.
It MUST return the token's `name`, the URL
`wss://<host>/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContentConstrained`,
whose WebSocket config adds `access_token=<token>` for a browser, and the
expiry in seconds. `ops::realtime::connect` with such a token MUST send it as
the header `Authorization: Token <token>` and keep it out of the URL, send
the setup, and fail with `Error::Provider` when the first server message is
not `setupComplete`, sending no client event before it. The setup MUST hold the model path, `generationConfig` with the
upper-cased output modalities (`AUDIO` by default) and the voice as a
prebuilt voice, the instructions as `systemInstruction`, the tools as one
`functionDeclarations` list with `parametersJsonSchema` and the option
`defaultToolBehavior` as each `behavior`, empty input and output
transcription configs when asked, the other provider options at the top
level, then the options `translationConfig` and `thinkingConfig` in
`generationConfig`, with `thinkingLevel: "low"` added for a
`gemini-<n>.<m>-live` thinking model that sets neither a level nor a budget.
Falsifier: A recorded token request sends another URL, puts the key in the URL, or sends another body; the token, URL or expiry differ; a setup for each session field and option differs from the stated one; or a scripted connection with a token puts it in the URL or lacks `Authorization: Token <token>`, sends a client event before `setupComplete`, or does not fail when the first message is another.
Mechanism: google
Rationale: `realtime/google-realtime-model.ts` and `buildGoogleSessionConfig`; the TypeScript token request puts the key in the URL query, and a browser can only pass the token in the URL, while Google's Go SDK connects with `Authorization: Token` and waits for `setupComplete` (`live.go`).
Status: Agreed 2026-10-23

[GGL-024] The realtime codec MUST send a session update as `{"setup"}` and
remember its input audio rate (16000 by default), input audio as
`realtimeInput.audio` with MIME type `audio/pcm;rate=<rate>`, an audio
commit as `realtimeInput.audioStreamEnd: true`, a user text as
`realtimeInput.text`, and a function result as
`toolResponse.functionResponses[{"id", "name", "response"}]`, the response
the output's JSON object as it is, any other JSON wrapped as `{"output"}`,
and text that is not JSON as `{"output": <text>}`. Audio clears, response
creation and cancellation, truncation and audio messages MUST send nothing.
Falsifier: The codec's message for any listed client event differs from the stated one, or one of the ignored events produces a message.
Mechanism: google
Rationale: `realtime/google-realtime-event-mapper.ts`.
Status: Agreed 2026-10-23

[GGL-025] The realtime codec MUST read `setupComplete` as session created; a
`toolCall` as an arguments delta and done event per function call with the
call's id, name and JSON arguments; `toolCallCancellation`, `goAway`,
`sessionResumptionUpdate`, `generationComplete`, `interactionStatus` and
`waitingForInput` as custom events of those names; `interrupted` as speech
started; model audio and text parts as audio and text deltas; output
transcriptions as transcript deltas; input transcriptions as one running
transcript per utterance, numbered `google-input-<k>` and closed by
`finished`, by an interruption, or by a completed turn when `finished` never
comes; and `turnComplete` as audio, text and transcript done events for the
kinds the turn had and a completed response. Responses and items MUST be
numbered `google-resp-<n>` and `google-item-<n>`, the number advancing when
the next turn's content arrives.
Falsifier: The codec's events for a recorded Live session (tool calls, an interruption, audio, text, transcripts with and without `finished`, and two turns) differ from the stated ones in kind, order, ids or text.
Mechanism: google
Rationale: `realtime/google-realtime-event-mapper.ts`.
Status: Agreed 2026-10-23

[GGL-026] `providers::google::interactions` MUST give a protocol for a
model id or an agent (built-in or managed) that sends `POST
<base>/interactions` with `model` or `agent`, the converted `input`, the
system text as `system_instruction` (the option `systemInstruction` when
there is none), the tools, `response_format` (a JSON schema as a
`text`/`application/json` entry, then the option `responseFormat` entries in
snake case, then a deprecated `imageConfig` image entry), the option
`responseModalities`, `previous_interaction_id`, `service_tier`, `store`,
`background`, and for a model `generation_config` with the sampling
settings, the stop sequences, the output limit, `thinking_level`,
`thinking_summaries` and `tool_choice`, or for an agent `agent_config` and
`environment`. A protocol stream MUST add `"stream": true`. Every
Interactions request, polls, stream reconnections and cancellations included,
MUST send the header `Api-Revision: 2026-05-20` unless the configured headers
set `Api-Revision`. Frequency and
presence penalties and the settings GGL-002 rejects MUST fail with
`Error::Validation`, and so MUST a JSON schema and the generation settings
for an agent, and `environment` for a model. Function tools MUST be `{"type": "function", "name",
"description", "parameters"}`, the built-in tools of GGL-007 and
`computer_use`, `mcp_server` and `retrieval` their Interactions shapes, and
the tool choice `auto`, `any`, `none` or `{"allowed_tools": {"mode":
"validated", "tools": [<name>]}}`, sent only with a function tool.
Falsifier: A recorded request for a model and for an agent, with each setting and tool, differs from the stated body; a setting that does not apply to the model or agent sends a request; a tool choice is sent without a function tool; or a recorded create, poll, stream reconnection or cancel request lacks `Api-Revision: 2026-05-20`, or replaces a configured `Api-Revision: 2026-01-01`.
Mechanism: google
Rationale: `interactions/google-interactions-language-model.ts` and `prepare-google-interactions-tools.ts`; the TypeScript model warns and drops what this requirement rejects, and sends no `Api-Revision`, which Google's Go SDK sends on every Interactions request (`interactions/internal/hooks/google_genai_auth.go`) so that a change of the default revision cannot change the response shape.
Status: Agreed 2026-10-23

[GGL-027] The Interactions input MUST be one step per turn: user text and
files as a `user_input` step (adjacent texts joined by a blank line), each
assistant text and file run as a `model_output` step, reasoning as a
`thought` step with its signature and summary, a tool call as a
`function_call` step with its signature, a Gemini processing part as a
`processing_call` or `processing_result` step, and tool results as a
`user_input` step of `function_result` blocks (`is_error` for an error or a
denial, JSON results as JSON text, content results as text and image
blocks). Files MUST be `image`, `audio`, `video` or `document` blocks by top
media type, with base64 data or a URI, the option `mediaResolution` on images
and videos and a video file's `processing` option. With the option
`previousInteractionId` and `store` not false, assistant turns of that
interaction and their tool results MUST be left out.
Falsifier: A recorded input for a history with a system message, user text and files, an assistant turn of reasoning, text, a tool call and a processing call, and its tool results differs from the stated steps; or a history continued with `previousInteractionId` still sends that interaction's turns.
Mechanism: google
Rationale: `interactions/convert-to-google-interactions-input.ts`; the crate keeps Gemini processing steps as built-in tool parts named `processing`.
Status: Agreed 2026-10-23

[GGL-028] An Interactions response MUST give text with its annotations as
sources, images and videos as files (inline data or URI), `thought` steps as
reasoning with their signature, `function_call` steps as tool calls,
processing steps as built-in parts named `processing`, and built-in `*_call`
and `*_result` steps as built-in calls and results named without the suffix,
with their sources; every part keeps the interaction id and its signature in
its metadata under `google`. The finish reason MUST be `tool_call` for
`requires_action` or a completed response with a function call, `stop` for
`completed`, `error` for `failed`, `length` for `incomplete` and `other`
otherwise; usage MUST report the total input, cached, output plus thought and
thought tokens; and the metadata MUST hold the interaction id, the service
tier and the output tokens by modality.
Falsifier: A recorded response with each step kind gives other parts, sources, metadata, finish reason or usage than stated.
Mechanism: google
Rationale: `interactions/parse-google-interactions-outputs.ts`, `extract-google-interactions-sources.ts` and `convert-google-interactions-usage.ts`.
Status: Agreed 2026-10-23

[GGL-029] An Interactions stream MUST read `interaction.created`, `step.start`,
`step.delta`, `step.stop`, the status events, `interaction.completed` and
`error` by step index: text, reasoning, function-call argument deltas,
annotation sources (once per URL across the stream), images and videos,
built-in calls and results and processing steps as GGL-028 gives them, a
function call's id and signature taken from its deltas. An `error` event MUST
end the call with an `Error::Provider` holding its message and code, and a
stream without `interaction.completed` whose last status is in progress MUST
report no finish reason.
Falsifier: A recorded event stream with each step kind, interleaved by index, delivers other blocks, deltas or parts; a source repeated in two deltas is delivered twice; an `error` event gives another error; or an interrupted stream reports a finish reason.
Mechanism: google
Rationale: `interactions/build-google-interactions-stream-transform.ts`.
Status: Agreed 2026-10-23

[GGL-030] An agent call or a call with the option `background` MUST send the
request without `stream` and, while the status is not terminal (`completed`,
`failed`, `cancelled`, `incomplete`), poll `GET <base>/interactions/<id>`
after 1 s, doubling to at most 10 s, for up to the option `pollingTimeoutMs`
(30 minutes by default), and a stream MUST instead follow `GET
<base>/interactions/<id>?stream=true`, reconnecting with `last_event_id`
after a dropped connection up to 3 times in a row, or replay a terminal first
response as one stream. A call that is dropped or times out before a terminal
status MUST send `POST <base>/interactions/<id>/cancel`.
Falsifier: A scripted agent that is in progress twice and then completes is polled other than at 1 s and 2 s; a stream that drops after event `e2` does not reconnect with `last_event_id=e2`, or retries a fourth time; a terminal first response is not replayed; or a dropped call does not send the cancel request.
Mechanism: google
Rationale: `interactions/poll-google-interactions.ts`, `stream-google-interactions.ts` and `cancel-google-interaction.ts`; agents run for minutes on Google's side.
Status: Agreed 2026-10-23

[GGL-031] The Gemini protocol's evaluation MUST be the OPS-011 adapter on
its own buffered language model call, so that `ops::experimental::evaluate`
on a `google` model sends one GGL-012 request with the decision schema as
`responseJsonSchema` and `thinkingConfig` for disabled reasoning.
Falsifier: A recorded evaluation on `gemini-3-flash-preview` sends another URL or a body without the decision schema or the disabled-reasoning `thinkingConfig`, or its answers differ from OPS-011's for the recorded response.
Mechanism: google
Rationale: `google-provider.ts` builds `decisionModel` from its chat model.
Status: Agreed 2026-10-23

[GGL-032] The Gemini API and Vertex AI responses of generate content, image,
speech, video and Interactions calls MUST be read up to 256 MiB per response
body and per stream event, and one larger MUST fail with an `Error::Provider`
naming the limit without reading past it; the crate's 8 MiB default MUST
still apply to the other Google responses.
Falsifier: A recorded stream event of 12 MiB holding an image, or a 20 MiB image or speech response body, fails or loses bytes; a 300 MiB response body is read past 256 MiB or does not fail with an error naming the limit; or a 12 MiB file metadata or batch status response is read.
Mechanism: google
Rationale: A 4K image, long speech or inline video arrives as base64 in one response or stream event, which the crate's 8 MiB limit (`src/providers/http.rs`, `src/transport/sse.rs`) rejects; Google's Go SDK reads stream lines up to 256 MB (`api_client.go`).
Status: Agreed 2026-10-23
