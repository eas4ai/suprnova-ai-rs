Prefix: HST

# Hosted chat providers

Vendor APIs that speak a protocol the crate already implements: the OpenAI
Chat Completions protocol (Cerebras, DeepInfra, DeepSeek, Fireworks, Mistral,
Moonshot AI, Together AI and Z.ai), the OpenAI Responses protocol (xAI and
Hugging Face) and the Anthropic Messages protocol (MiniMax). Each one is the
existing adapter with a host profile: its endpoint, its key, and the request
and response differences the AI SDK for TypeScript 7.0.148 records for it.
The other model kinds a host's package ships, and Mistral's Conversations
API, which no other provider shares, are specified in the host's section.

| Host | Name and handles | Default base URL | Key variable |
|---|---|---|---|
| Cerebras | `cerebras`, `@ai-sdk/cerebras` | `https://api.cerebras.ai/v1` | `CEREBRAS_API_KEY` |
| DeepInfra | `deepinfra`, `@ai-sdk/deepinfra` | `https://api.deepinfra.com/v1/openai` | `DEEPINFRA_API_KEY` |
| Fireworks | `fireworks`, `@ai-sdk/fireworks` | `https://api.fireworks.ai/inference/v1` | `FIREWORKS_API_KEY` |
| Together AI | `togetherai`, `@ai-sdk/togetherai` | `https://api.together.xyz/v1` | `TOGETHER_API_KEY` |
| Z.ai | `zai`, `@ai-sdk/zai` | `https://api.z.ai/api/paas/v4` | `ZAI_API_KEY` |
| MiniMax | `minimax`, `@ai-sdk/minimax` | `https://api.minimax.io/anthropic` | `MINIMAX_API_KEY` |
| DeepSeek | `deepseek`, `@ai-sdk/deepseek` | `https://api.deepseek.com` | `DEEPSEEK_API_KEY` |
| Moonshot AI | `moonshotai`, `@ai-sdk/moonshotai` | `https://api.moonshot.ai/v1` | `MOONSHOT_API_KEY` |
| Mistral | `mistral`, `@ai-sdk/mistral` | `https://api.mistral.ai/v1` | `MISTRAL_API_KEY` |
| xAI | `xai`, `@ai-sdk/xai` | `https://api.x.ai/v1` | `XAI_API_KEY` |
| Hugging Face | `huggingface`, `@ai-sdk/huggingface` | `https://router.huggingface.co/v1` | `HUGGINGFACE_API_KEY` |

[HST-001] For each host in the table, `providers::hosts::<name>::register`
MUST register, under the host's name and handles, a provider named after the
host. Its model calls MUST stream the host's protocol from the configured
base URL, or the default base URL when none is configured: Chat Completions
at `<base>/chat/completions` with `authorization: Bearer <key>`; for xAI and
Hugging Face, Responses at `<base>/responses` with `authorization: Bearer
<key>`; and for MiniMax, Messages at `<base>/v1/messages` with `x-api-key:
<key>` and `anthropic-version: 2023-06-01`. The key is the configuration's API key
value, or the `env` value named by the host's key variable. Registering
every host, OpenAI and Anthropic into one registry MUST succeed.
Falsifier: For any host, a recorded request goes to another URL, uses another method or credential header, or omits the key; the provider is missing under one of its handles or has another name; or registering every host in the table with `openai::register`, `openai::register_factories` and `anthropic::register` into one registry fails.
Mechanism: hosts
Status: Agreed 2026-10-22

[HST-002] A Chat Completions request MUST send `stream_options:
{"include_usage": true}` for OpenAI, for an OpenAI-compatible provider
without a host profile, for DeepSeek, Fireworks, Moonshot AI and Together
AI, and MUST NOT send `stream_options` for Cerebras, DeepInfra, Mistral or
Z.ai.
Falsifier: The recorded body for Cerebras, DeepInfra, Mistral or Z.ai has a `stream_options` field; or the body for OpenAI's Chat Completions protocol, for a provider built with `OpenAICompatibleProvider::from_modelsdev_provider` for an id without a host profile, for DeepSeek, Fireworks, Moonshot AI or Together AI lacks `stream_options.include_usage` set to true.
Mechanism: hosts
Rationale: The TypeScript SDK sends it only where a host opts in (`openai-compatible-chat-language-model.ts` `includeUsage`), because some servers reject the field; custom OpenAI-compatible servers keep it, as the crate does today, since many report streamed usage only when asked and billing reads that usage.
Status: Agreed 2026-10-22

[HST-003] Chat Completions requests MUST follow each host's field rules:
Cerebras sends the output token limit as `max_completion_tokens` and an
assistant turn's reasoning as `reasoning`; DeepInfra, Fireworks, Together AI
and Z.ai send `max_tokens` and `reasoning_content`; Fireworks sends the
reasoning efforts `minimal` as `low` and `xhigh` and `max` as `high`. An
output schema MUST be sent as `response_format` of type `json_schema` for
Cerebras, DeepInfra and Fireworks, and for Together AI only for the model
`deepseek-ai/DeepSeek-V4-Flash-0731`; for other Together AI models and for
Z.ai it MUST be sent as `{"type": "json_object"}`.
Falsifier: A recorded body for a host with an output token limit of 100, an assistant turn with reasoning, a reasoning effort and an output schema has the limit under another field, the reasoning under another field, a Fireworks effort of `minimal`, `xhigh` or `max`, or a `response_format` other than the one stated for that host and model.
Mechanism: hosts
Status: Agreed 2026-10-22

[HST-004] For Z.ai, a call with tool choice `none` MUST send neither `tools`
nor `tool_choice`, and a call with tool choice `required`, a named tool or a
tool selection, or with a frequency penalty, presence penalty or seed, MUST
fail with `Error::Validation` before any request is sent. A Z.ai finish
reason MUST map `sensitive` to `content_filter`,
`model_context_window_exceeded` to `length` and `network_error` to `error`.
Falsifier: A Z.ai call with tool choice `none` sends `tools` or `tool_choice`; a Z.ai call with tool choice `required`, a named tool or a selection, or with a frequency penalty, presence penalty or seed, sends a request or fails with another error kind; or a Z.ai stream ending with finish reason `sensitive`, `model_context_window_exceeded` or `network_error` reports a finish reason other than `content_filter`, `length` or `error`.
Mechanism: hosts
Rationale: The TypeScript SDK drops these with a warning; this crate reports unsupported parameters as validation errors, as its OpenAI and Anthropic adapters do.
Status: Agreed 2026-10-22

[HST-005] A Chat Completions error response MUST give `ProviderError` the
provider's message whether the body is `{"error": {"message": ...}}`,
`{"error": "<text>"}` or `{"message": ...}`, with the error kind from the
HTTP status.
Falsifier: For an HTTP 400 response with any of the three bodies, the error message is not the body's message text, or the kind is not `BadRequest`.
Mechanism: hosts
Rationale: OpenAI and DeepInfra use the first shape, Fireworks the first two, and Cerebras and Z.ai the third (`cerebras-provider.ts`, `fireworks-provider.ts`, `zai-error.ts`).
Status: Agreed 2026-10-22

[HST-006] When a DeepInfra response reports more reasoning tokens than
completion tokens, the usage MUST report the sum as output tokens. When a
Cerebras call has an output schema and its response streams text and then
tool calls, the call MUST drop the tool calls that follow the text and, when
the response ends with finish reason `tool_calls`, report finish reason
`stop`.
Falsifier: A DeepInfra stream whose usage has `completion_tokens` 10 and `completion_tokens_details.reasoning_tokens` 30 reports output tokens other than 40; or a Cerebras stream under an output schema that streams text and a tool call and finishes with `tool_calls` delivers a tool call event or a finish reason other than `stop`.
Mechanism: hosts
Status: Agreed 2026-10-22

[HST-007] For a MiniMax model whose id names version 3.1 or later
(`MiniMax-M3.1`, `minimax-m4`), a reasoning effort MUST be sent as
`output_config.effort`, with `minimal` sent as `low`, and disabled reasoning
MUST be sent as effort `low` instead of disabled thinking. Other MiniMax
models MUST keep the Anthropic adapter's reasoning mapping.
Falsifier: A recorded MiniMax-M3.1 body for disabled reasoning has a `thinking` field or lacks `output_config.effort` of `low`; for effort `minimal` it has an effort other than `low`; for effort `high` it lacks `output_config.effort` of `high`; or a MiniMax-M2 body differs from what the Anthropic adapter sends for the same request.
Mechanism: hosts
Rationale: MiniMax 3.1 and later always reason (`minimax-chat-language-model.ts`).
Status: Agreed 2026-10-22

## DeepSeek, Moonshot AI and Mistral

These three hosts differ from Chat Completions in more than field names, so
their profiles also change how messages, schemas and stream content are
written and read.

[HST-008] A DeepSeek, Moonshot AI or Mistral call MUST fail with
`Error::Validation` before any request is sent when it sets a request
setting that the host's TypeScript provider never sends: a service tier,
metadata, a cache retention or a tool selection (`ToolChoice::Selection`)
for all three; a safety identifier for DeepSeek and Mistral; a cache key
for DeepSeek; and parallel tool calls for DeepSeek and Moonshot AI.
Falsifier: For each of the three hosts, a call that sets one of the settings named for it sends a request or fails with another error kind; or a Moonshot AI call with a cache key and a safety identifier does not send `prompt_cache_key` and `safety_identifier`, or a Mistral call with a cache key, one tool and parallel tool calls set to false does not send `prompt_cache_key` and `parallel_tool_calls: false`.
Mechanism: hosts
Rationale: The six earlier hosts still send these settings unchanged; a new host rejects what its API is not known to accept, as HST-004 does for Z.ai.
Status: Agreed 2026-10-22

A DeepSeek model is a legacy model when its id has `deepseek-chat`,
`deepseek-reasoner`, or `deepseek-v1` to `deepseek-v3` with an optional
`.<digits>` minor version, at its start or after a `/` or `.`, followed by
the end of the id or by `-`, `/`, `:` or `.`. Every other DeepSeek model is
a V4 model (`is-deepseek-v4-model.ts`).

[HST-009] A DeepSeek request MUST send the output token limit as
`max_tokens` and an assistant turn's reasoning as `reasoning_content`. A
reasoning effort MUST be sent as `thinking: {"type": "enabled"}` with
`reasoning_effort`, where `minimal` and `low` are sent as `low`, `medium`
and `high` as `high`, and `xhigh` and `max` as `max`; disabled reasoning
MUST be sent as `thinking: {"type": "disabled"}` with no
`reasoning_effort`. An output schema MUST be sent as `response_format`
`{"type": "json_object"}`, with a system message first in `messages` whose
text is `Return JSON that conforms to the following schema: ` followed by
the schema as compact JSON. A call with a frequency penalty, presence
penalty or seed MUST fail with `Error::Validation` before any request is
sent. So MUST a call with a temperature or top-p while thinking is on, which
it is unless reasoning is disabled, when an effort is set or the model is
`deepseek-reasoner` or a V4 model; the error message MUST say that
disabling reasoning allows the setting.
Falsifier: A recorded DeepSeek body for an output token limit of 100, an assistant turn with reasoning and effort `medium` has the limit or the reasoning under another field, lacks `thinking` of type `enabled`, or has a `reasoning_effort` other than `high`; effort `minimal` or `xhigh` sends an effort other than `low` or `max`; disabled reasoning lacks `thinking` of type `disabled` or sends `reasoning_effort`; an output schema sends another `response_format` or another first message; a call with a penalty or a seed, or with a temperature for `deepseek-v4-flash` without disabled reasoning or for `deepseek-reasoner`, sends a request or fails with another error kind or, for the temperature, with a message that does not mention reasoning; or a temperature of 0.2 for `deepseek-v4-flash` with disabled reasoning, or for `deepseek-chat` with no effort, is not sent as `temperature: 0.2`.
Mechanism: hosts
Rationale: The TypeScript SDK drops these settings with a warning; V4 models reason by default and ignore temperature and top-p while they do.
Status: Agreed 2026-10-22

[HST-010] For a DeepSeek V4 model, every assistant message MUST carry
`reasoning_content`, the empty string when the turn has no reasoning; for a
legacy model, the reasoning of the assistant messages before the last user
message MUST NOT be sent. A user file that is not an image, or an image
whose media type is not `image/gif`, `image/jpeg`, `image/jpg`, `image/png`
or `image/webp`, MUST fail with `Error::Validation` before any request is
sent. A tool result whose output is a list of content parts with no image
MUST be sent as a string: the JSON text of that list. DeepSeek usage MUST
report `prompt_cache_hit_tokens` as cache-read tokens, or
`prompt_tokens_details.cached_tokens` when it is absent, and the finish
reason `insufficient_system_resource` MUST map to `error`.
Falsifier: A recorded `deepseek-v4-pro` body for a history with an assistant turn without reasoning lacks `"reasoning_content": ""` on that turn; a `deepseek-chat` body sends the reasoning of an assistant turn before the last user message, or drops the reasoning of one after it; a user PDF or `image/bmp` file sends a request or fails with another error kind; a tool result of two text content parts is sent as an array; or a DeepSeek stream whose usage has `prompt_cache_hit_tokens` 60 and `prompt_tokens_details.cached_tokens` 80 reports cache-read tokens other than 60, one with only `cached_tokens` 80 reports other than 80, or one that ends with finish reason `insufficient_system_resource` reports another finish reason than `error`.
Mechanism: hosts
Rationale: V4 rejects a thinking-mode request whose assistant turns lack `reasoning_content` (`convert-to-deepseek-chat-messages.ts`).
Status: Agreed 2026-10-22

A Moonshot AI model's family comes from its id: `kimi-k3`; `kimi-k2.7` for
`kimi-k2.7-code` and `kimi-k2.7-code-highspeed`; `kimi-k2.6`; `kimi-k2.5`;
`moonshot-v1` for ids that start with `moonshot-v1-`; and unknown for every
other id. The first four are the Kimi families (`moonshotai-chat-options.ts`).

[HST-011] A Moonshot AI request MUST send the output token limit as
`max_completion_tokens` and an assistant turn's reasoning as
`reasoning_content`. Reasoning MUST follow the model's family: for
`kimi-k3` and unknown models, an effort is sent as `reasoning_effort`, with
`minimal` and `low` as `low`, `medium` and `high` as `high`, and `xhigh` and
`max` as `max`, and no `thinking`; for `kimi-k2.6` and `kimi-k2.5`, an
effort is sent as `thinking: {"type": "enabled"}` and disabled reasoning as
`thinking: {"type": "disabled"}`, with no `reasoning_effort`; for
`kimi-k2.7`, an effort sends neither field. A call MUST fail with
`Error::Validation` before any request is sent when it disables reasoning
for `kimi-k3`, `kimi-k2.7` or an unknown model; sets an effort for
`moonshot-v1`; sets a seed; sets a temperature, top-p, frequency penalty or
presence penalty for a Kimi family; or sets tool choice `required` for
`kimi-k2.6`, `kimi-k2.7-code` or `kimi-k2.7-code-highspeed`.
Falsifier: A recorded Moonshot AI body has the output token limit under a field other than `max_completion_tokens`; a `kimi-k3` body for effort `xhigh` lacks `reasoning_effort: "max"` or has `thinking`; a `kimi-k2.6` body for effort `high` lacks `thinking` of type `enabled` or has `reasoning_effort`; a `kimi-k2.5` body for disabled reasoning lacks `thinking` of type `disabled`; a `kimi-k2.7-code` body for effort `high` has `thinking` or `reasoning_effort`; a call that the requirement says fails sends a request or fails with another error kind; or a `moonshot-v1-8k` call with temperature 0.5 does not send `temperature: 0.5`.
Mechanism: hosts
Rationale: The TypeScript SDK drops each of these settings with a warning (`moonshotai-chat-language-model.ts`).
Status: Agreed 2026-10-22

[HST-012] Moonshot AI tool parameter schemas and output schemas MUST be
sent as given except for three changes: a root `$schema` is removed; in
every subschema, a tuple `items` array is moved to the end of
`prefixItems`; and a string `type` next to `anyOf` moves into each `anyOf`
branch that has no `type`. An output schema MUST be sent as
`response_format` of type `json_schema` with `strict: true`. A schema whose
root is not an object with `type: "object"` MUST fail with
`Error::Validation` before any request is sent.
Falsifier: A recorded Moonshot AI body for a tool and an output schema whose schemas each have a root `$schema`, an optional property and a property with a tuple `items` array keeps the `$schema` or the `items` array, adds the optional property to `required`, or adds `additionalProperties`; a property `{"type": "string", "anyOf": [{"minLength": 1}, {"type": "null"}]}` is not sent as `{"anyOf": [{"type": "string", "minLength": 1}, {"type": "null"}]}`; the output schema's `response_format` lacks `strict: true`; or a tool whose parameter schema has the root type `string` sends a request or fails with another error kind.
Mechanism: hosts
Rationale: Moonshot validates schemas as MFJS, not as OpenAI strict JSON schema (`normalize-json-schema-for-mfjs.ts`), so the crate's strict rewrite, which makes every property required, is not applied.
Status: Agreed 2026-10-22

[HST-013] A Moonshot AI user image of type `image/jpeg`, `image/png`,
`image/gif`, `image/webp`, `image/bmp`, `image/heic` or `image/heif` MUST be
sent as `image_url`, and a video of type `video/mp4`, `video/mpeg`,
`video/mov`, `video/avi`, `video/x-flv`, `video/mpg`, `video/webm`,
`video/wmv` or `video/3gpp` as `video_url`, each as a data URL. An image or
video given as an `http` or `https` URL MUST be downloaded and sent as a
data URL when the provider was built with a download transport, and MUST
fail with `Error::Validation` before any request is sent when it was not. A `text/*`
file MUST be sent as text. Any other user file, such as a PDF or audio,
MUST fail with `Error::Validation` before any request is sent. An assistant
turn with tool calls and no text MUST send `"content": null`, and a tool
result MUST be sent as a string: its text, or the JSON text of any other
output. Usage MUST be read from a chunk's top-level `usage`, or from
`choices[0].usage` when no chunk has a top-level one, and MUST report
`usage.cached_tokens` as cache-read tokens, or
`prompt_tokens_details.cached_tokens` when it is absent.
Falsifier: A recorded Moonshot AI body sends an inline `video/mp4` file as anything other than a `video_url` data URL; an `https` image URL is not fetched through the download transport and sent as a data URL, or without a download transport sends a request or fails with another error kind; a `text/plain` file is not sent as text; a PDF or `audio/mpeg` file sends a request or fails with another error kind; an assistant turn with only a tool call lacks `"content": null`; a tool result with JSON output is sent as an array or object; or a stream whose only usage is `choices[0].usage` with 12 prompt and 5 completion tokens reports other token counts, one that also has a top-level usage of 99 and 33, before or after it, reports other than 99 and 33, or a top-level `usage.cached_tokens` of 10 reports cache-read tokens other than 10.
Mechanism: hosts
Rationale: Moonshot does not fetch external URLs (`moonshotai-chat-language-model.ts` `supportedUrls`), so the TypeScript SDK downloads them first.
Status: Agreed 2026-10-22

The Mistral models without reasoning settings are `codestral-2508`,
`codestral-latest`, `ministral-14b-2512`, `ministral-14b-latest`,
`ministral-3b-2512`, `ministral-3b-latest`, `ministral-8b-2512`,
`ministral-8b-latest`, `mistral-code-fim-latest`, `mistral-code-latest`,
`mistral-large-latest`, `mistral-large-2512`, `voxtral-small-2507` and
`voxtral-small-latest` (`mistral-chat-language-model.ts`).

[HST-014] A Mistral request MUST send the output token limit as
`max_tokens` and a seed as `random_seed`. Disabled reasoning MUST be sent as
`reasoning_effort: "none"` and every effort as `reasoning_effort: "high"`,
except that for a model without reasoning settings, an effort or disabled
reasoning MUST fail with `Error::Validation` before any request is sent. An
output schema MUST be sent as `response_format` of type `json_schema` with
`strict: false` and the schema as given. Tool choice `required` MUST be sent
as `"any"`, and a named tool as `"any"` with `tools` holding only that tool;
a call with no tools MUST send neither `tool_choice` nor
`parallel_tool_calls`.
Falsifier: A recorded Mistral body for an output token limit of 100 and seed 7 has the limit under another field, has a `seed` field or lacks `random_seed: 7`; effort `low` or `max` sends a `reasoning_effort` other than `high`, or disabled reasoning one other than `none`; an effort or disabled reasoning for `mistral-large-latest` sends a request or fails with another error kind; an output schema with an optional property is sent with `strict` true, with that property in `required`, or with `additionalProperties` added; tool choice `required` sends a `tool_choice` other than `any`; a choice of tool `b` among tools `a` and `b` sends another `tool_choice` or a `tools` list other than `b` alone; or a call with no tools and tool choice `auto` sends `tool_choice` or `parallel_tool_calls`.
Mechanism: hosts
Rationale: Mistral reasons at one level, so the TypeScript SDK sends `high` for every effort (`mistral-chat-language-model.ts`).
Status: Agreed 2026-10-22

[HST-015] A Mistral assistant turn with reasoning MUST be sent with
`content` as an array that holds, in the turn's order, a `{"type":
"thinking", "thinking": [{"type": "text", "text": <reasoning>}], "closed":
true}` part for each reasoning part and a `{"type": "text", "text": <text>}`
part for each text part, and with no reasoning field; a turn with only tool
calls MUST send `"content": ""`. When the last message is an assistant
message, it alone MUST be sent with `"prefix": true`. A tool result MUST be
sent with the tool's `name` and a string `content`: its text, or the JSON
text of any other output. A user image MUST be sent as `{"type":
"image_url", "image_url": <URL>}` with the URL or data URL as a string, and
a PDF as `{"type": "document_url", "document_url": <URL>}`, holding an
`https` URL as given or the file as a data URL. Any other user file MUST
fail with `Error::Validation` before any request is sent.
Falsifier: A recorded Mistral body for a history with an assistant turn of reasoning `R` then text `T`, an assistant turn with only a tool call, a JSON tool result of tool `t` and a final assistant message differs from the stated shapes, sends `prefix` on another message or lacks it on the last; a user image, a PDF as bytes or a PDF as an `https` URL is sent in another shape; or an `audio/mpeg` or `text/plain` file sends a request or fails with another error kind.
Mechanism: hosts
Rationale: These are the shapes `convert-to-mistral-chat-messages.ts` sends; the TypeScript SDK returns the continued text after a prefix as the server sends it, and so does the crate.
Status: Agreed 2026-10-22

[HST-016] A Mistral stream MUST read `delta.content` given as a string or
as an array of parts: `text` parts as text, the text inside `thinking`
parts as reasoning, and other parts, such as `reference`, as nothing. A text
block MUST end before a reasoning block starts, and each run of reasoning
MUST be a new reasoning block. The finish reasons `model_length` and `error`
MUST map to `length` and `error`, and usage MUST report `num_cached_tokens`
as cache-read tokens when it is present.
Falsifier: A Mistral stream whose `content` arrays carry thinking `A`, text `B`, a `reference` part, thinking `C` and text `D` delivers anything other than reasoning `A`, text `B`, reasoning `C` and text `D` as four blocks in that order, each ended before the next starts; a string `content` is lost; finish reason `model_length` or `error` maps to another reason; or usage with `num_cached_tokens` 5 reports cache-read tokens other than 5.
Mechanism: hosts
Status: Agreed 2026-10-22

## Every Chat Completions provider

[HST-017] A Chat Completions stream MUST key each tool call delta by its
`index`, or, when it has none, by its position in the chunk's `tool_calls`
array, and a delta whose `id` differs from the id of the call under its key
MUST start a new call. A tool call `index` that is present but is not a
non-negative integer MUST end the call with an `Error::Provider` of kind
`Response`.
Falsifier: A stream whose first chunk opens two calls without an index, with ids `weather_0` and `time_1`, and whose next chunk carries argument deltas without an index or id for both, does not deliver two calls, `weather_0` with arguments `{"location":"San Francisco"}` and `time_1` with `{"zone":"UTC"}`; a stream with two chunks that each carry one complete call without an index, with ids `a` and `b`, does not deliver two calls; or a call index `"not-a-number"` does not end the call with a `Response` provider error.
Mechanism: hosts
Rationale: Mistral and Moonshot AI send tool calls without an index (`mistral-tool-call.chunks.txt`, `moonshotai-stream-indexless-tool-calls.chunks.txt`), and the crate merged them into one call.
Status: Agreed 2026-10-22

| Error `code` or `type` | Status |
|---|---|
| `rate_limit_exceeded`, `rate_limit_error` | 429 |
| `server_error`, `api_error`, `internal_server_error` | 500 |
| `overloaded_error`, `service_unavailable` | 503 |
| `timeout`, `timeout_error` | 504 |
| `authentication_error`, `invalid_api_key` | 401 |
| `permission_error` | 403 |
| `not_found_error`, `model_not_found` | 404 |
| `bad_request`, `context_length_exceeded`, `invalid_request_error` | 400 |

[HST-018] For DeepSeek, Moonshot AI and Mistral, a stream event that
carries an error, as an `error` object or, for Mistral, as an object whose
`object` is `error`, MUST end the call with an `Error::Provider` that keeps
the error's message, `code` and `type`. Its kind MUST come from the first
of these rules that applies: a `code` or `type` of `insufficient_quota`
gives `RateLimit`; a `code` that is an HTTP status from 400 to 599, as a
number or a three-digit string, gives that status's kind
(`ProviderErrorKind::from_status`); a `code`, else a `type`, in the table
above gives the kind of the table's status; otherwise the kind is
`Response`. The error MUST be retryable when its status is 408, 409, 429 or
500 and above, unless it is `insufficient_quota`.
Falsifier: A DeepSeek stream error `{"error": {"message": "m", "type": "rate_limit_error", "code": "rate_limit_exceeded"}}` does not end the call with an error whose message is `m`, code `rate_limit_exceeded`, type `rate_limit_error` and kind `RateLimit`, or it is not retryable; a code `insufficient_quota` gives another kind or is retryable; a code of 503 or `"503"` gives a kind other than `ServiceUnavailable`; a Moonshot AI error of type `server_error` gives a kind other than `InternalServer` or is not retryable; a Mistral event `{"object": "error", "message": "m", "type": "invalid_request_error", "code": "3230"}` is ignored, or gives a message other than `m` or a kind other than `BadRequest`; or an error of type `unknown_error` gives a kind other than `Response` or is retryable.
Mechanism: hosts
Rationale: The table and rules are `deepseek-chat-language-model.ts`'s; an error that arrives before the first event is retried under MOD-005 when it is retryable.
Status: Agreed 2026-10-22

## xAI and Hugging Face

These two hosts speak only the Responses protocol: `@ai-sdk/xai` 5.0.0
removed its Chat Completions model. xAI is ported for text: its built-in
tools, such as web and X search, and the citations they return are not.

[HST-019] `OpenAICompatibleProvider::from_modelsdev_provider` MUST give
metadata whose package is `@ai-sdk/xai` or `@ai-sdk/huggingface`, or whose id
is `xai` or `huggingface`, that host's Responses protocol and rules.
Falsifier: A provider built from metadata with id `xai` and package `@ai-sdk/openai-compatible`, or with id `hf` and package `@ai-sdk/huggingface`, sends a request to a path other than `<base>/responses`, or a body other than the one the provider that `providers::hosts` registers for that host sends for the same call.
Mechanism: hosts
Status: Agreed 2026-10-22

[HST-020] An xAI request MUST send top-k as `top_k`, min-p as `min_p` and a
seed as `seed`. An xAI call MUST fail with `Error::Validation` before any
request is sent when it sets a frequency or presence penalty, a reasoning
summary, a text verbosity, a maximum tool call count, context management, a
cache retention, metadata or a tool selection (`ToolChoice::Selection`). A
Hugging Face call MUST fail the same way when it sets top-k, min-p, a seed, a
frequency or presence penalty, a reasoning summary, a text verbosity, an
output include list, a maximum tool call count, parallel tool calls, a
service tier, a safety identifier, a cache key, a cache retention, context
management or a tool selection.
Falsifier: For xAI and for Hugging Face, a call that sets one of the settings named for that host sends a request or fails with another error kind; or an xAI call with top-k 40, min-p 0.1 and seed 7 lacks `top_k: 40`, `min_p: 0.1` or `seed: 7`.
Mechanism: hosts
Rationale: The TypeScript packages drop these settings with a warning or never send them; DeepSeek, Moonshot AI and Mistral reject theirs the same way (HST-008).
Status: Agreed 2026-10-22

[HST-021] An xAI reasoning effort MUST be sent as `reasoning.effort`, with
`minimal` as `low`, `xhigh` and `max` as `xhigh` for the model `grok-4.6` and
as `high` for every other model, and any other effort as given; disabled
reasoning MUST be sent as `none`. For a model whose id is `grok-4.20`, then
optionally `-` and four digits, then `-reasoning` or `-non-reasoning`, an
effort or disabled reasoning MUST fail with `Error::Validation` before any
request is sent. The provider parameters under the key `xai` take `store`:
`false` MUST be sent as `"store": false` with `reasoning.encrypted_content`
added to `include`, and `true` or no setting MUST send no `store` field. A
named tool choice MUST be sent as `{"type": "function", "name": <name>}`, and
a call with a provider tool MUST fail with `Error::UnsupportedOperation`
before any request is sent.
Falsifier: A recorded xAI body for effort `minimal` has a `reasoning.effort` other than `low`; effort `xhigh` for `grok-4.7` sends other than `high`, or for `grok-4.6` other than `xhigh`; disabled reasoning sends other than `none`; an effort for `grok-4.20-0309-reasoning` or `grok-4.20-non-reasoning` sends a request or fails with another error kind; a call with `xai` store false lacks `"store": false` or lacks `reasoning.encrypted_content` in `include`, or one with store true or unset has a `store` field; a named tool `lookup` is sent in another shape; or a call with a provider tool sends a request or fails with another error kind.
Mechanism: hosts
Rationale: The effort map and the models without reasoning settings are `xai-responses-language-model.ts` and `supports-reasoning-effort.ts`; xAI stores responses by default, so only `false` needs sending.
Status: Agreed 2026-10-22

[HST-022] An xAI request MUST send no `item_reference` item and no replayed
raw item. An assistant turn MUST send each text part as `{"role":
"assistant", "content": <text>}`, with `"id"` set to the part's xAI item id
when it has one; each tool call as `{"type": "function_call", "id": <its xAI
item id, else its call id>, "call_id", "name", "arguments", "status":
"completed"}`; and each reasoning part that has an xAI item id or encrypted
content as `{"type": "reasoning", "id": <its item id, else "">, "summary":
[{"type": "summary_text", "text": <text>}], "status": "completed"}` with its
`encrypted_content`, and with an empty `summary` when the text is empty.
Reasoning with neither an item id nor encrypted content MUST NOT be sent, and
neither are built-in tool calls and results. A user image MUST be sent as
`input_image`, any other file given as a URL as `{"type": "input_file",
"file_url": <URL>}`, and any other file given as bytes MUST fail with
`Error::Validation` before any request is sent. In a tool result's content, a
file that is not an image MUST be sent as the text `[file: <media type>]`.
Falsifier: A recorded xAI body for a history whose assistant turn has reasoning with encrypted content, text and a tool call, each with an xAI item id, holds an `item_reference` or sends one of them in another shape; reasoning without an item id or encrypted content is sent; a `text/csv` file given as a URL is not sent as `input_file` with that `file_url`; a PDF given as bytes sends a request or fails with another error kind; or a PDF in a tool result's content is sent as anything other than the text `[file: application/pdf]`.
Mechanism: hosts
Rationale: These are the shapes `convert-to-xai-responses-input.ts` sends; xAI resolves no references to stored items.
Status: Agreed 2026-10-22

[HST-023] An xAI stream MUST keep each output item's id, and a reasoning
item's encrypted content, in part metadata under the key `xai`, from which
HST-022 reads them. When a usage's `input_tokens_details.cached_tokens` is
greater than its `input_tokens`, input tokens MUST be their sum. An xAI error
body `{"code": <c>, "error": <e>}` MUST give a `ProviderError` with the
message `<c>: <e>` and the code `<c>`.
Falsifier: A next request built from the history of an xAI stream with a reasoning item that carries encrypted content and a message item lacks that encrypted content or the message's item id, or the parts hold them under a key other than `xai`; usage with `input_tokens` 4142 and `cached_tokens` 4328 reports input tokens other than 8470 or cache-read tokens other than 4328; or an HTTP 400 body `{"code": "Client specified an invalid argument", "error": "Invalid request content"}` gives a message other than `Client specified an invalid argument: Invalid request content` or a code other than `Client specified an invalid argument`.
Mechanism: hosts
Rationale: Item ids under the host's own key are never sent to OpenAI when a history moves to it (`convert-xai-responses-usage.ts`, `xai-error.ts`).
Status: Agreed 2026-10-22

[HST-024] A Hugging Face output schema MUST be sent as `text.format`
`{"type": "json_schema", "name", "strict": false, "schema"}` with the schema
as given. Tool choice `required` MUST be sent as `"required"`, a named tool
as `{"type": "function", "function": {"name": <name>}}`, and tool choice
`none` MUST send neither `tools` nor `tool_choice`. A call with a provider
tool MUST fail with `Error::UnsupportedOperation` before any request is
sent. A request MUST send no `item_reference` item and no replayed raw item;
each assistant text part and each reasoning part MUST be sent as `{"role":
"assistant", "content": [{"type": "output_text", "text": <text>}]}`, tool
calls as `function_call` items and tool results as `function_call_output`
items. A user file that is not an image MUST fail with `Error::Validation`
before any request is sent.
Falsifier: A recorded Hugging Face body for an output schema with an optional property has `strict` true or a changed schema; tool choice `required` or a named tool `lookup` is sent in another shape, or tool choice `none` sends `tools` or `tool_choice`; a call with a provider tool sends a request or fails with another error kind; a body for a history with reasoning `R`, text `T`, a tool call and its result holds an `item_reference`, sends `R` or `T` in another shape, or lacks the `function_call` or `function_call_output` item; or a user PDF sends a request or fails with another error kind.
Mechanism: hosts
Rationale: The TypeScript package drops tool calls and results from the history, which breaks an agent's tool loop; Hugging Face's Responses API follows OpenAI's, whose items these are.
Status: Agreed 2026-10-22

## Every Responses provider

[HST-025] A Responses stream, for OpenAI, xAI and Hugging Face, MUST end the
call with an `Error::Provider` that keeps the message, `code` and `type` of a
`response.failed` event's `response.error`, or of an `error` event, with the
kind and retry flag that HST-018's rules give. A `response.done` event MUST be
read as `response.completed`. A `response.incomplete` event MUST report the
finish reason `length` for the reason `max_output_tokens`, `content_filter`
for `content_filter` and `other` otherwise, whether or not its response has a
`status`. A reasoning block whose summary part ends before its output item
MUST still carry the item's encrypted content.
Falsifier: An OpenAI or xAI stream that sends `response.failed` with `{"error": {"code": "server_error", "message": "m"}}` does not end the call with an error whose message is `m`, code `server_error` and kind `InternalServer`; an `error` event with code `rate_limit_exceeded` gives a kind other than `RateLimit`; a stream that ends with `response.done` reports another finish reason or usage than the same stream ending with `response.completed`; `response.incomplete` with reason `max_output_tokens` and no `status` reports a finish reason other than `length`; or an xAI stream whose `response.reasoning_summary_part.done` comes before an `output_item.done` with `encrypted_content` delivers a reasoning part without that encrypted content.
Mechanism: hosts
Rationale: The crate read `response.failed` as metadata only, so a failed OpenAI response ended with no error (`openai-responses-language-model.ts` raises one).
Status: Agreed 2026-10-22

## Together AI, DeepInfra and Fireworks, whole

The model kinds `@ai-sdk/togetherai` 3.0.76, `@ai-sdk/deepinfra` 3.0.75 and
`@ai-sdk/fireworks` 3.0.78 ship beside chat: completions and embeddings on
the OpenAI-compatible protocols, images, and Together AI's reranking. A
host's own options are the entry under its name (`togetherai`, `deepinfra`
or `fireworks`). An image call returns the warnings of TypeScript in
`Item.warnings`, and makes one image per call, so `ops::generate_image`
makes one call per image (OPS-003).

[HST-026] A Together AI, DeepInfra or Fireworks chat call MUST read the
`openaiCompatible` entry and the host's entry as
`openai-compatible-chat-language-model.ts` reads them: `user` as `user`,
`reasoningEffort` as `reasoning_effort` in place of the core effort,
`textVerbosity` as `verbosity`, and `strictJsonSchema` as the `strict` of a
JSON schema response format, true when unset. Every other key of the host's
entry MUST go into the body as given, in place of a core setting of the
same name, but MUST NOT replace `messages`, `tools`, `tool_choice`,
`reasoning_effort`, `verbosity` or the stream fields. A Fireworks call MUST send `promptCacheKey` as
`prompt_cache_key`, `serviceTier` as `service_tier`, `thinking` as
`thinking` with `budgetTokens` as `budget_tokens`, and `reasoningHistory` as
`reasoning_history`, and MUST map an effort from its options as HST-003 maps
the core effort. An entry that is not an object, a Fireworks `serviceTier`
other than `priority`, a `thinking.type` other than `enabled` or
`disabled`, a `budgetTokens` below 1024, or a `reasoningHistory` other than
`disabled`, `interleaved` or `preserved` MUST fail with `Error::Validation`
before any request.
Falsifier: A recorded body for each host with the host's entry `{"user": "u", "reasoningEffort": "low", "textVerbosity": "low", "repetition_penalty": 1.1, "temperature": 0.3, "messages": []}` and a core temperature of 0.7 lacks `user`, `reasoning_effort: "low"`, `verbosity: "low"` or `repetition_penalty: 1.1`, sends a temperature other than 0.3, or sends the entry's `messages`; `strictJsonSchema: false` under an output schema on a model that takes JSON schemas sends `strict` true; a Fireworks body for `{"promptCacheKey": "k", "serviceTier": "priority", "thinking": {"type": "enabled", "budgetTokens": 2048}, "reasoningHistory": "preserved", "reasoningEffort": "xhigh"}` sends another field name, keeps the option names, or sends an effort other than `high`; or one of the invalid entries sends a request or fails with another error kind.
Mechanism: hosts
Rationale: TypeScript spreads the host's entry into the body, so an application reaches a host's own settings this way; Fireworks renames its four options in `transformRequestBody`.
Status: Agreed 2026-10-24

[HST-027] `togetherai::completion_protocol`, `deepinfra::completion_protocol`
and `fireworks::completion_protocol` MUST return the compatible completions
protocol of OAI-015 for the host: with `stream_options.include_usage` for
Together AI and Fireworks and without it for DeepInfra. A model with that
protocol on the host's provider MUST stream `POST <base>/completions` with
the host's key, take its options from the host's entry, and read an error
body as HST-005 says.
Falsifier: A recorded completions call for each host goes to another URL or credential, sends `stream_options` for DeepInfra or lacks it for Together AI or Fireworks, or drops the host entry's `echo` or `suffix`; or a Fireworks error body `{"error": "bad prompt"}` with HTTP 400 gives another message or a kind other than `BadRequest`.
Mechanism: hosts
Rationale: `togetherai-provider.ts`, `deepinfra-provider.ts` and `fireworks-provider.ts` build their completion models on `OpenAICompatibleCompletionLanguageModel`, with `includeUsage` for Together AI and Fireworks.
Status: Agreed 2026-10-24

[HST-028] `ops::embed` with a Together AI, DeepInfra or Fireworks model
MUST embed as OAI-016's compatible protocol does: `POST <base>/embeddings`
with the host's key, `model`, `input`, `encoding_format: "float"`, and
`dimensions` and `user` from the host's entry, 2048 values per call with
parallel calls, and an error body read as HST-005 says.
Falsifier: A recorded call for each host with two values and the entry `{"dimensions": 256}` differs from the stated body or URL; 2049 values are split other than 2048 and 1; or the call fails with `UnsupportedOperation`.
Mechanism: hosts
Rationale: `OpenAICompatibleEmbeddingModel` with its defaults of 2048 values and parallel calls.
Status: Agreed 2026-10-24

[HST-029] Together AI image generation MUST send `POST
<base>/images/generations` with `model`, `prompt`, `seed`, `width` and
`height` from the size as numbers, the first input image as `image_url` (a
URL as given, bytes or a file as a data URL), `response_format: "base64"`
and the `togetherai` entry as given, and return the images of
`data[].b64_json`. For `google/gemini-3-pro-image` it MUST leave out `seed`
with a warning, and leave out the entry's `steps`, `guidance`,
`negative_prompt` and `disable_safety_checker`. An aspect ratio MUST be left
out with an `unsupported` warning for `aspectRatio`, and input images after
the first MUST be left out with a warning. A mask MUST fail with
`Error::Validation` before any request. An error body's `error.message` MUST
be the error's message.
Falsifier: A recorded call with size `1024x768`, seed 7, the entry `{"steps": 4}` and one PNG image sends another URL or body, or returns other images than the response's; `google/gemini-3-pro-image` sends `seed` or `steps`, or lacks the seed warning; an aspect ratio is sent or not warned; a second image is sent or not warned; or a mask sends a request.
Mechanism: hosts
Rationale: `togetherai-image-model.ts`. Its `aspectRatio` warning is pushed when a size is set, which its own message contradicts; the crate warns for the aspect ratio it leaves out.
Status: Agreed 2026-10-24

[HST-030] DeepInfra image generation without input images MUST send `POST
<root>/inference/<model>` with `prompt`, `num_images`, `aspect_ratio`,
`width` and `height` from the size as strings, `seed` and the `deepinfra`
entry as given, where `<root>` is the base URL without its last path
segment `openai`; a base URL whose last segment is not `openai` MUST fail
with `Error::Configuration` before any request. It MUST return the images
of `images[]` with a leading `data:image/<type>;base64,` removed. With input
images it MUST send `POST <base>/images/edits` as a multipart form with
`model`, `prompt`, each image as an `image` part (a URL fetched through the
download transport), the mask as `mask`, `n`, `size` and the entry's values
as text fields, and return the images of `data[].b64_json`. An error body's
`detail.error`, or for an edit its `error.message`, MUST be the error's
message.
Falsifier: A recorded call with size `1024x768`, aspect ratio `16:9` and seed 7 for `black-forest-labs/FLUX-1-schnell` sends another URL or body, or returns an image that keeps the data URL prefix; a call with two images and a mask sends another URL or form; an image URL without a download transport sends a request; or a base URL ending in `/v1` sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `deepinfra-image-model.ts`; TypeScript's base URL is the root that the crate's DeepInfra base URL (`https://api.deepinfra.com/v1/openai`, HST-001) extends with `openai`.
Status: Agreed 2026-10-24

[HST-031] Fireworks image generation MUST send `POST` with `prompt`,
`aspect_ratio`, `seed`, `samples`, the first input image as `input_image`
(a URL as given, bytes or a file as a data URL), `width` and `height` from
the size as strings, and the `fireworks` entry as given, to
`<base>/image_generation/<model>` for the size models
(`accounts/fireworks/models/` followed by
`playground-v2-5-1024px-aesthetic`, `japanese-stable-diffusion-xl`,
`playground-v2-1024px-aesthetic`, `stable-diffusion-xl-1024-v1-0` or
`SSD-1B`), to `<base>/workflows/<model>` for the Kontext models
(`accounts/fireworks/models/flux-kontext-pro` and `flux-kontext-max`), and
to `<base>/workflows/<model>/text_to_image` for every other model, and
return the response bytes as the image. For the two Kontext models it MUST
fail with `Error::Validation` before any request when the provider has no
download transport; with one, it MUST read `request_id`, then `POST
<base>/workflows/<model>/get_result` with `{"id"}` every 500 ms until
`status` is `Ready`, when it MUST download `result.sample` with the key only
from the base URL's origin and through the download transport without it
from another; `Error` or `Failed`, or 120 s without `Ready`, MUST fail the
call. A size on a model that is not a size
model, an aspect ratio on a size model, input images after the first and a
mask MUST be warned and left out. The entry's `output_format` other than
`jpeg` or `png`, or a `safety_tolerance` outside 0 to 6, MUST fail with
`Error::Validation` before any request.
Falsifier: A recorded call for a size model, for `accounts/fireworks/models/flux-1-dev-fp8` and for `accounts/fireworks/models/flux-kontext-pro` goes to another URL or sends another body; the Kontext call does not poll `get_result` until `Ready`, downloads the sample from another origin with the key or from the base origin without it, sends a request when the provider has no download transport, or does not fail on `Failed` or after 120 s; a stated warning is missing or its setting is sent; or an invalid entry sends a request.
Mechanism: hosts
Rationale: `fireworks-image-model.ts`, whose `getFromApi` sends the headers only to the credentialed origin and whose sample "typically points at a CDN"; the crate fetches other origins through the download transport, as GGL-019 does for Veo videos, and checks for one before a job is started and billed.
Status: Agreed 2026-10-24

[HST-032] Together AI reranking MUST send `POST <base>/rerank` with
`model`, `query`, `documents` as the texts or JSON objects given, `top_n`,
`rank_fields` from the `togetherai` option `rankFields` and
`return_documents: false`, and return each result's `index` and
`relevance_score` in the response's order, with the response `id` and
`model` in the metadata. An unknown `togetherai` option MUST fail with
`Error::Validation` before any request, and an error body's `error.message`
MUST be the error's message.
Falsifier: A recorded call with two JSON documents, `top_n` 1 and `rankFields: ["title"]` sends another URL or body; the result has another order, index or score, or lacks the id or model; or an unknown option sends a request.
Mechanism: hosts
Rationale: `reranking/togetherai-reranking-model.ts`.
Status: Agreed 2026-10-24

## DeepSeek and MiniMax, whole

The rest of `@ai-sdk/deepseek` 3.0.70 and `@ai-sdk/minimax` 3.0.62: DeepSeek's
own chat options, message options, strict tools, image parts and files API,
and MiniMax's own options, media parts and video models. A host's own
options are the entry under its name (`deepseek` or `minimax`) in a call's
provider params, in a message's or part's provider metadata, or in an
operation's provider options. A DeepSeek file id is `file-` followed by the
id, as the files API returns it; a file part names an uploaded file by that
id as its text data. A MiniMax uploaded video is named by `mm_file://`
followed by its id the same way.

[HST-033] A DeepSeek chat call MUST read the `deepseek` entry as
`deepseek-chat-language-model.ts` reads it: `logprobs` true, or any
`topLogprobs`, MUST send `logprobs: true`, and `topLogprobs` MUST be sent as
`top_logprobs`; `userId` MUST be sent as `user_id`; `thinking.type` MUST be
sent as `thinking.type`, with `adaptive` sent as `enabled`, in place of the
core reasoning setting, and it decides whether thinking is on for HST-009's
temperature and top-p rule; `reasoningEffort` MUST be sent as
`reasoning_effort`, with `medium` sent as `high` and `xhigh` as `max`, in
place of the core effort, and MUST NOT be sent while thinking is disabled. A
`topLogprobs` that is not an integer from 0 to 20, a `userId` longer than
512 characters or with a character other than ASCII letters, digits, `_` and
`-`, a `thinking.type` other than `adaptive`, `enabled` or `disabled`, a
`reasoningEffort` other than `low`, `medium`, `high`, `xhigh` or `max`, or
an entry that is not an object MUST fail with `Error::Validation` before any
request.
Falsifier: A recorded `deepseek-v4-flash` body for the entry `{"topLogprobs": 3, "userId": "user_1", "reasoningEffort": "xhigh"}` and core effort `low` lacks `logprobs: true`, `top_logprobs: 3`, `user_id: "user_1"` or `reasoning_effort: "max"`; one for `{"logprobs": true}` lacks `logprobs: true` or sends `top_logprobs`; one for `{"thinking": {"type": "adaptive"}}` sends a `thinking.type` other than `enabled`; one for `{"thinking": {"type": "disabled"}, "reasoningEffort": "high"}` with temperature 0.2 lacks `thinking.type: "disabled"` or `temperature: 0.2`, or sends `reasoning_effort`; one for `deepseek-chat` with `{"thinking": {"type": "enabled"}}` and a temperature sends a request or fails with another error kind; or one of the invalid entries sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `deepseek-chat-language-model-options.ts` and `getArgs`; `strictJsonSchema` is not read, since DeepSeek's provider never turns on JSON schema output.
Status: Agreed 2026-10-24

[HST-034] A DeepSeek message whose `deepseek` metadata holds `name` MUST
send it as the message's `name` for a system, user or assistant message,
and leave it out for a tool message. An assistant message whose `deepseek`
metadata holds `prefix: true` MUST be sent with `prefix: true` when it is
the last message and the base URL ends in `/beta`; a `prefix` other than
`true`, on another role, on an assistant message that is not last, or on a
base URL that does not end in `/beta` MUST fail with `Error::Validation`
before any request. A strict function tool (`Tool::is_strict`) MUST be sent
with `strict: true` when the base URL ends in `/beta`; a strict tool on
another base URL, or a request with a strict and a non-strict function tool,
MUST fail with `Error::Validation` before any request.
Falsifier: A recorded body with `name` on a system, a user and an assistant message lacks one of them or sends one on a tool message; a body for a base URL ending in `/beta` whose last assistant message sets `prefix: true` lacks `prefix: true`, or one whose strict tool lacks `strict: true`; or a prefix on a user message, on an assistant message before the last, or on the default base URL, a strict tool on the default base URL, or a strict tool beside a non-strict one on a `/beta` base URL, sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `convert-to-deepseek-chat-messages.ts` and `deepseek-prepare-tools.ts`; `deepseek-provider.ts` turns both beta features on only for a base URL ending in `/beta`.
Status: Agreed 2026-10-24

[HST-035] A DeepSeek user image part MUST be sent as `image_url` with the
URL as given, or a data URL for bytes (with `image/jpg` sent as
`image/jpeg`), and the part's `deepseek` metadata `imageDetail` as `detail`;
a part whose text data is a DeepSeek file id MUST be sent as `{"type":
"file", "file_id"}`; and a part with bytes whose metadata holds `fileData:
true` MUST be sent as `{"type": "file", "file_data": <the data URL>}` with
its filename as `filename` when it has one. An image URL longer than 8192
characters, `fileData` on a URL, `fileData` with `imageDetail`, a `fileData`
other than `true`, or an `imageDetail` other than `low`, `high`, `original`
or `auto` MUST fail with `Error::Validation` before any request. An image in
a tool result's content MUST be sent the same way, except that `fileData` is
not read there.
Falsifier: A recorded body for a user message with a 100-character image URL and `imageDetail: "high"`, a PNG given as bytes, a PNG given as bytes with `fileData: true` and a filename, and a part with text `file-api-abc123` of type `image/png` sends the URL part other than `{"type": "image_url", "image_url": {"url": <the URL>, "detail": "high"}}`, the bytes other than an `image_url` data URL, the `fileData` part other than `{"type": "file", "file_data": <the data URL>, "filename": <the name>}`, or the id other than `{"type": "file", "file_id": "file-api-abc123"}`; a tool result with such an id and an image URL with `imageDetail: "low"` sends other parts; or one of the invalid parts sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `convert-to-deepseek-chat-messages.ts` and `deepseek-file-part-options.ts`; HST-010 keeps the media types.
Status: Agreed 2026-10-24

[HST-036] The final message of a DeepSeek call MUST carry a `deepseek`
metadata entry with `promptCacheHitTokens` and `promptCacheMissTokens` from
the usage, `responseObject`, `choiceIndex`, `messageRole`, `toolCallTypes`
(the `type` of each tool call, in index order), `logprobs` with the
`content` and `reasoning_content` entries the choices carried, and
`systemFingerprint`, each absent when the response did not report it.
Falsifier: A recorded stream whose chunks carry `object: "chat.completion.chunk"`, `system_fingerprint: "fp_1"`, a choice index 0, a delta role `assistant`, one function tool call, two content logprobs and a usage with `prompt_cache_hit_tokens` 60 and `prompt_cache_miss_tokens` 40 ends with a final message whose `deepseek` metadata lacks `promptCacheHitTokens: 60`, `promptCacheMissTokens: 40`, `responseObject: "chat.completion.chunk"`, `choiceIndex: 0`, `messageRole: "assistant"`, `toolCallTypes: ["function"]`, the two logprobs under `logprobs.content` or `systemFingerprint: "fp_1"`; or a stream without logprobs or a fingerprint reports them.
Mechanism: hosts
Rationale: The `finish` part of `doStream` in `deepseek-chat-language-model.ts`.
Status: Agreed 2026-10-24

[HST-037] `ops::upload_file` with the DeepSeek provider MUST send `POST
<base>/files` as multipart with the file (and its filename when it has
one), `purpose: user_data`, and, for a `deepseek` option `expiresAfter`,
`expires_after[anchor]: created_at` and `expires_after[seconds]`, and return
the `deepseek` provider reference of the response's `id`, the response's
filename else the given one, the given media type, the size (`bytes`), the
creation and expiry times (`created_at` and `expires_at` seconds, as RFC
3339) and the metadata `{object, filename, purpose, bytes, createdAt,
expiresAt}`, each absent when the response lacks it. Before any request it
MUST fail with `Error::Validation` when the file is larger than 64 MiB, the
filename is longer than 512 characters, the file's detected media type is
not GIF, JPEG, PNG or WebP, the declared media type (before any `;`,
trimmed, in lower case) is neither one of those nor generic (empty,
`application/binary`, `application/octet-stream`, `binary/octet-stream`,
`image` or `image/*`), or it is generic while no type is detected and the
filename does not end in `.gif`, `.jpg`, `.jpeg`, `.png` or `.webp`; and
when `expiresAfter` is not an integer from 3600 to 2592000. File info,
download and delete MUST fail with `Error::UnsupportedOperation` before any
request.
Falsifier: A recorded upload of a PNG named `a.png` with `expiresAfter: 3600` goes to another URL, sends another form, or returns other than the reference `deepseek` to the response's id, the response's filename, `image/png`, the response's `bytes`, `created_at` and `expires_at`, or the stated metadata; a 65 MiB file, a 513-character filename, a PNG declared `application/pdf`, a file declared `image/jpeg` whose bytes are a PDF, `application/octet-stream` bytes with no detectable type named `a.bin`, or `expiresAfter: 60` sends a request or fails with another error kind; `application/octet-stream` PNG bytes, or bytes with no detectable type named `a.webp`, fail; or file info, download or delete sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `deepseek-files.ts`, which uploads only.
Status: Agreed 2026-10-24

[HST-038] A MiniMax call MUST read the `minimax` entry with the rules of
the `anthropic` entry, each key of `minimax` over the same key of
`anthropic`, and an `effort` in either entry MUST win over the core
reasoning setting of HST-007. When the call's provider params hold a
`minimax` entry, the final message's `anthropic` metadata MUST also be given
under `minimax`.
Falsifier: A recorded `MiniMax-M3.1` body for `{"anthropic": {"effort": "high"}, "minimax": {"effort": "low"}}` sends an effort other than `low`; one for `{"minimax": {"effort": "medium"}}` and disabled core reasoning sends an effort other than `medium` or a `thinking` field; a `MiniMax-M2` body for `{"minimax": {"thinking": {"type": "disabled"}}}` lacks `thinking.type: "disabled"`; or a stream for a call with a `minimax` entry ends with a final message without a `minimax` metadata entry equal to its `anthropic` one, or one for a call without the entry ends with a `minimax` metadata entry.
Mechanism: hosts
Rationale: `anthropic-language-model.ts` reads a host's own key beside `anthropic` and names its metadata after it when the key was used; `minimax-chat-language-model.ts` skips its effort mapping when either entry sets `effort`.
Status: Agreed 2026-10-24

[HST-039] For a MiniMax model of version 3.1 or later, a user file part of
an `image/*` or `video/*` media type MUST be sent as `{"type": "image" |
"video", "source"}` with the source `{"type": "base64", "media_type",
"data"}` for bytes, `{"type": "url", "url"}` for a URL, and `{"type": "url",
"url": <the text>}` for a video whose text data starts with `mm_file://`;
the part's `minimax` metadata `detail` (`low`, `default` or `high`) for an
image and `fps` (0.2 to 5) for a video MUST go into the source; and such a
model MUST read a part's or message's cache control (ANT-015) from its
`minimax` metadata before its `anthropic` metadata. An image whose text data
starts with `mm_file://`, or a `detail` or `fps` outside those values, MUST
fail with `Error::Validation` before any request. Other media types, and
every file part for an earlier model, MUST be sent as the Anthropic adapter
sends them.
Falsifier: A recorded `MiniMax-M3.1` body for a user message with a PNG given as bytes and `detail: "high"`, an `https` MP4 URL with `fps: 2`, a video part with text `mm_file://abc`, and a text part with `minimax.cacheControl` sends other blocks than `{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": <base64>, "detail": "high"}}`, `{"type": "video", "source": {"type": "url", "url": <the URL>, "fps": 2}}` and `{"type": "video", "source": {"type": "url", "url": "mm_file://abc"}}`, or lacks `cache_control` on the text; a `MiniMax-M2` body with an MP4 sends a request or fails with another error kind; or an image with text `mm_file://abc`, `detail: "auto"` or `fps: 10` sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `convert-to-minimax-media-part.ts`, `minimax-media-options.ts`, and the `cacheControlProviderOptionsName` of `minimax-chat-language-model.ts`.
Status: Agreed 2026-10-24

MiniMax's video API takes a resolution tier, so `minimax-video-model.ts`
maps a core resolution to one through a closed table: `480x480`,
`1120x480`, `854x480`, `640x480`, `480x854` and `480x640` are `480P`;
`768x768`, `1792x768`, `1366x768`, `1024x768`, `768x1366` and `768x1024`
are `768P`; and `2048x2048`, `2560x1080`, `2560x1440`, `2048x1536`,
`1440x2560` and `1536x2048` are `2K`. A core resolution that names a tier
in any case is that tier. A text-to-video call is one whose content holds
the prompt alone.

[HST-040] `ops::generate_video` and `ops::start_video` with the MiniMax
provider MUST send `POST <root>/v2/video_generation` with `authorization:
Bearer <key>`, where the root is the base URL without its trailing
`/anthropic`, and `{"model", "content", "resolution", "duration", "ratio",
"aigc_watermark"}`. `content` MUST start with `{"type": "text", "text": <the
prompt, or "">}`, then the first frame (the `first_frame` frame image, else
the image) and the last frame as `{"type": "image_url", "image_url":
{"url"}, "role": "first_frame" | "last_frame"}`, or, without frames and for a
model other than `MiniMax-H3-Max`, the references by media type as
`image_url` with role `reference_image` (at most 9) and `video_url` with
role `reference_video` (at most 3), then the `minimax` option
`referenceAudioUrls` as `audio_url` with role `reference_audio` (at most 3,
only beside a reference image or video); a URL goes as given and bytes as a
data URL of their media type. The `minimax` options `resolution` (`480P`,
`768P` or `2K`) and `ratio` (`adaptive`, `21:9`, `16:9`, `4:3`, `1:1`, `3:4`
or `9:16`) MUST win over the core resolution and aspect ratio; the
resolution MUST be one the model supports (`MiniMax-H3`: `768P` and `2K`,
default `2K`; `MiniMax-H3-Max`: `480P` and `768P`, default `768P`; other
models: all three, default `2K`); a text-to-video call MUST default its
ratio to `16:9` and MUST NOT send `adaptive`; a call with frames MUST send no
ratio; the duration MUST default to 5 and be clamped to 4 to 15 for
`MiniMax-H3` and 5 to 15 otherwise; and `aigcWatermark` MUST be sent as
`aigc_watermark`. What TypeScript warns about MUST be returned in
`Item.warnings` and left out: a frame or reference that is not an image or
video, a last frame without a first, references with frames or on
`MiniMax-H3-Max`, references past a limit, a reference URL without a media
type (sent as an image), audio without a reference image or video, a
resolution the model does not support or the table does not name, an
aspect ratio outside the list or ignored, a clamped duration, and `fps`,
`seed` and `generate_audio`. Before any request the call MUST fail with
`Error::Validation` when the provider has no download transport, bare bytes
have no detectable image media type, a `minimax` option has another type or
value than stated, or the entry holds `pollIntervalMs` or `pollTimeoutMs`,
which `VideoOptions` sets in this crate; and with `Error::Configuration`
when the base URL does not end in `/anthropic`.
Falsifier: A recorded `MiniMax-H3` call with the prompt `a cat`, no media and no options goes to another URL or credential, or sends a body other than `{"model": "MiniMax-H3", "content": [{"type": "text", "text": "a cat"}], "resolution": "2K", "duration": 5, "ratio": "16:9"}`; a `MiniMax-H3-Max` call with a PNG first frame, a JPEG last frame, a duration of 20, the core resolution `1366x768` and aspect ratio `1:1` sends other than two `image_url` data URL parts with roles `first_frame` and `last_frame`, `resolution: "768P"` and `duration: 15`, sends a `ratio`, or lacks the duration and aspect ratio warnings; a `MiniMax-H3` call with two image URL references, one `video/mp4` URL reference, four `referenceAudioUrls`, the options `ratio: "adaptive"` and `aigcWatermark: true`, and a duration of 3 sends other than two `reference_image`, one `reference_video` and three `reference_audio` parts, `ratio: "adaptive"`, `aigc_watermark: true` and `duration: 4`, or lacks the audio and duration warnings; a `MiniMax-H3-Max` call with a reference sends it or lacks its warning; a call with `fps`, `seed` and `generate_audio` sends one of them or lacks their warnings; or a call on a provider without a download transport, on the base URL `https://proxy.test/v1`, with bytes that are no image, with the option `resolution: "1080P"`, or with `pollTimeoutMs: 1000`, sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `getArgs` and `doStart` in `minimax-video-model.ts`; `minimax-provider.ts` roots video calls at `https://api.minimax.io`, the Messages base URL without `/anthropic`; the download transport is checked before a job is started and billed, as HST-031 checks it; TypeScript's `webhookUrl` has no core counterpart.
Status: Agreed 2026-10-24

[HST-041] A MiniMax video job MUST be the start response's `task_id`, and
its status `GET <root>/v2/query/video_generation/<task_id>` with the bearer
key: `task.status` `succeeded` completes with one `video/mp4` file part at
`task.content.url`, which `generate_video` downloads through the download
transport without the key; `failed`, `cancelled` and `expired` fail with a
message naming the task id and, for `failed`, the task's error message and
code; any other status is pending. The completed item's `minimax` metadata
MUST hold `taskId`, `videoUrl`, `resolvedInputs` (`imageCount`, the frame
and reference images sent, and `referenceVideoUrls`, the URL references
sent as videos), and `duration`, `ratio`, `resolution` and `usage`
(`totalSeconds`, `inputSeconds` and `outputSeconds`) when the task reports
them. A start response without `task_id`, or a `succeeded` task without a
URL, MUST fail with `Error::Provider`, and an error body's `error.message`
MUST be the error's message. A MiniMax video model makes one video per call.
Falsifier: A scripted job for a call with one first frame whose start returns `task_id: "t1"` and whose status is `running`, then `succeeded` with a `content.url` on another origin, `duration` 5, `ratio` `16:9`, `resolution` `2K` and `usage.total_seconds` 5, is checked at another URL or without the key, is reported other than pending then completed, fetches the URL with the key or not through the download transport, or completes with other `minimax` metadata than `{taskId: "t1", videoUrl: <the URL>, resolvedInputs: {imageCount: 1, referenceVideoUrls: []}, duration: 5, ratio: "16:9", resolution: "2K", usage: {totalSeconds: 5}}`; a `failed` task with error message `m` and code 7 does not fail with a message holding `m`, `7` and `t1`; a `cancelled` or `expired` task does not fail; a start response without `task_id` or a `succeeded` task without a URL gives another error kind; an HTTP 400 body `{"error": {"message": "bad"}}` gives another message; or `n` of 2 makes other than two jobs.
Mechanism: hosts
Rationale: `doStatus` and `doGenerate` in `minimax-video-model.ts`; TypeScript's core fetches a result URL without credentials, and its `pollIntervalMs` and `pollTimeoutMs` are `VideoOptions` here (OPS-004).
Status: Agreed 2026-10-24

## Mistral, whole

The rest of `@ai-sdk/mistral` 4.0.69: the `mistral` options of its chat
model, its Conversations API language model with the web search tools, and
its embedding, speech and transcription models. Mistral's own options are
the `mistral` entry in a call's provider params or an operation's provider
options. `mistral::conversations()` gives the Conversations protocol, which
a model takes through `Model::with_protocol`, as a model takes Google's
Interactions protocol; `mistral::web_search()` and
`mistral::web_search_premium()` give that API's provider-executed tools,
with the ids `mistral.web_search` and `mistral.web_search_premium`. A speech
call returns the warnings of TypeScript in `Item.warnings`, as the image and
video calls of HST-029 to HST-031 and HST-040 do; the language models, the
embeddings and the transcription fail with `Error::Validation` on a setting
TypeScript drops with a warning or refuses, as HST-008 does.

[HST-042] A Mistral chat call MUST read the `mistral` entry as
`mistral-chat-language-model.ts` reads it: `safePrompt` MUST be sent as
`safe_prompt`, `documentImageLimit` as `document_image_limit`,
`documentPageLimit` as `document_page_limit`, `promptCacheKey` as
`prompt_cache_key` and `parallelToolCalls` as `parallel_tool_calls` (only
with tools, HST-014), the last two in place of the core cache key and
parallel tool calls;
`reasoningEffort` (`high` or `none`) MUST be sent as `reasoning_effort` in
place of HST-014's mapping of the core setting, and MUST fail with
`Error::Validation` for a model without reasoning settings; under an output
schema, `strictJsonSchema` MUST be the `strict` of the `json_schema`
response format, false when unset, and `structuredOutputs: false` MUST send
`response_format: {"type": "json_object"}` instead and put the instruction
`JSON schema:`, the schema as compact JSON and `You MUST answer with a JSON
object that matches the JSON schema above.`, each on its own line, at the
end of the first system message after a blank line, or as a new first system
message. A strict function tool (`Tool::is_strict`) MUST be sent with
`strict: true` in its `function`. A key other than these, a value of another
type, a `reasoningEffort` other than `high` or `none`, or an entry that is
not an object MUST fail with `Error::Validation` before any request. Usage
MUST report `prompt_tokens_details.cached_tokens` as cache-read tokens when
`num_cached_tokens` is absent.
Falsifier: A recorded `mistral-small-latest` body for the entry `{"safePrompt": true, "documentImageLimit": 4, "documentPageLimit": 16, "promptCacheKey": "k", "parallelToolCalls": false, "reasoningEffort": "none"}` with one tool and core effort `high` lacks `safe_prompt: true`, `document_image_limit: 4`, `document_page_limit: 16`, `prompt_cache_key: "k"`, `parallel_tool_calls: false` or `reasoning_effort: "none"`; one with an output schema and `{"strictJsonSchema": true}` sends `strict` false; one with an output schema, `{"structuredOutputs": false}` and a system message `S` sends a `response_format` other than `{"type": "json_object"}` or a first system message other than `S`, a blank line, `JSON schema:`, the schema and `You MUST answer with a JSON object that matches the JSON schema above.` on five lines; a strict tool lacks `strict: true`; `{"reasoningEffort": "high"}` for `mistral-large-latest`, `{"reasoningEffort": "medium"}`, `{"safePrompt": "yes"}`, `{"topK": 3}` or the entry `[]` sends a request or fails with another error kind; or a stream whose usage has `prompt_tokens_details: {"cached_tokens": 4}` and no `num_cached_tokens` reports cache-read tokens other than 4.
Mechanism: hosts
Rationale: `mistral-chat-language-model-options.ts` and `getArgs`; the JSON instruction is `injectJsonInstructionIntoMessages` of `provider-utils`, and `convertMistralUsage` reads `prompt_tokens_details` before `prompt_token_details`.
Status: Agreed 2026-10-24

The Conversations API reasons only on these models
(`mistral-conversation-language-model.ts`): `glm-5-2`, `labs-leanstral-1-5`,
`labs-leanstral-1-5-1`, `magistral-medium-latest`, `magistral-small-latest`,
`mistral-large-4`, `mistral-large-4-0`, `mistral-medium`,
`mistral-medium-2604`, `mistral-medium-3`, `mistral-medium-3-5`,
`mistral-medium-3.5`, `mistral-medium-latest`, `mistral-small-2603`,
`mistral-small-latest`, `mistral-vibe-cli-fast`, `mistral-vibe-cli-latest`,
`mistral-vibe-cli-with-tools` and `zai-glm-5-2`. A built-in tool call part
of that API names its tool after the call's tool whose id is `mistral.` and
the wire name (`web_search` or `web_search_premium`), else after the wire
name.

[HST-043] A call on the Conversations protocol MUST send `POST
<base>/conversations` with the Mistral key and `{"model", "store": false,
"stream": true, "instructions", "inputs", "tools", "completion_args"}`.
`instructions` MUST be the system messages' text joined by a blank line,
absent without one, and hold HST-042's JSON instruction under
`structuredOutputs: false`; `completion_args` MUST hold the limit, sampling,
penalty, stop, seed, reasoning and response format fields HST-014 and
HST-042 give a chat body, and `tool_choice` as HST-014 maps it; `inputs`
MUST hold, in order, each user message as `{"type": "message.input", "role":
"user", "content": <HST-015's user parts>}`, each assistant turn's text and
reasoning as `{"type": "message.input", "role": "assistant", "content":
<HST-015's text and thinking parts>}`, cut before each tool call and with no
`prefix`, each tool call as `{"type": "function.call", "tool_call_id",
"name", "arguments": <the JSON text>}`, and each tool result as `{"type":
"function.result", "tool_call_id", "result": <HST-015's string content>}`. A
built-in tool call part MUST go as `function.call` with `name` from its
`mistral` metadata's `function`, else `name`, else its tool's wire name, and
`arguments` the `arguments` text of its arguments, else their JSON; its
return MUST go as `function.result` whose `result` is its value's
`info.result` when that is a string, else the JSON of `info.result`, else of
`info`, else `null`. `tools` MUST hold each function tool as HST-042 sends
it and each web search tool as `{"type": "web_search"}` or `{"type":
"web_search_premium"}`; a named tool choice MUST keep only the named
function or web search tool, as HST-014 does. Before any request the call
MUST fail with `Error::Validation` when the `mistral` entry holds
`safePrompt`, `documentImageLimit`, `documentPageLimit`, `promptCacheKey` or
`parallelToolCalls`, when a core cache key, parallel tool calls or a setting
HST-008 names for Mistral is set, when another provider tool is declared, or
when a reasoning setting or `reasoningEffort` is set for a model the
Conversations API does not reason on. A snapshot of a model with this
protocol MUST restore it in a registry where `mistral::register` ran.
Falsifier: A recorded `mistral-medium-latest` body for a history of system `S`, user `U`, an assistant turn of reasoning `R`, text `T` and a call `c1` of tool `t` with arguments `{"a": 1}`, a JSON result `{"b": 2}` of `c1`, a built-in call `e1` of `web_search` with arguments `{"arguments": "q"}` and metadata `{"type": "tool.execution", "name": "web_search", "function": "web_search"}`, its return `{"info": {"result": "r"}}`, and a user message `V`, with the strict tool `t` and `mistral::web_search()`, output limit 100, seed 7, core effort `low` and an output schema, goes to another URL or credential, or differs from `{"model": "mistral-medium-latest", "store": false, "stream": true, "instructions": "S", "inputs": [{"type": "message.input", "role": "user", "content": [{"type": "text", "text": "U"}]}, {"type": "message.input", "role": "assistant", "content": [{"type": "thinking", "thinking": [{"type": "text", "text": "R"}], "closed": true}, {"type": "text", "text": "T"}]}, {"type": "function.call", "tool_call_id": "c1", "name": "t", "arguments": "{\"a\":1}"}, {"type": "function.result", "tool_call_id": "c1", "result": "{\"b\":2}"}, {"type": "function.call", "tool_call_id": "e1", "name": "web_search", "arguments": "q"}, {"type": "function.result", "tool_call_id": "e1", "result": "r"}, {"type": "message.input", "role": "user", "content": [{"type": "text", "text": "V"}]}], "tools": [{"type": "function", "function": {"name": "t", "description": <t's>, "parameters": <t's>, "strict": true}}, {"type": "web_search"}], "completion_args": {"max_tokens": 100, "random_seed": 7, "reasoning_effort": "high", "response_format": {"type": "json_schema", "json_schema": {"name": "output", "strict": false, "schema": <the schema>}}}}`; a tool choice of `web_search` sends `tools` other than `[{"type": "web_search"}]` or a `tool_choice` other than `any`; a call with no system message and `{"structuredOutputs": false}` under an output schema lacks `instructions` starting with `JSON schema:`; a call for `mistral-large-latest` with effort `low`, one with the entry `{"safePrompt": true}`, one with a core cache key, or one with an Anthropic web search tool, sends a request or fails with another error kind; or a restored snapshot of a model with the protocol streams to another URL than `<base>/conversations`.
Mechanism: hosts
Rationale: `convert-to-mistral-conversation-inputs.ts`, `mistral-conversation-prepare-tools.ts` and `getArgs` in `mistral-conversation-language-model.ts`, which warns about the five options and sends `store: false`; a built-in execution is replayed as a call and a result because entry ids cannot be sent as inputs.
Status: Agreed 2026-10-24

[HST-044] A Conversations stream MUST read its events:
`conversation.response.started` gives the response id (`conversation_id`);
`message.output.delta` gives the response model from its first `model`, and
its `content`, a string or one part, as text for a string or a `text` part
and as reasoning for a `thinking` part, in a block keyed by the event's
`id`, its `content_index` (0 when absent) and the block kind, where a delta
for another key ends the open block first; a `tool_reference` part with a
URL MUST become a source `{"sourceType": "url", "id", "url", "title": <the
title, else the URL>}`, once per URL, in the final message's `mistral`
metadata `sources`; `function.call.delta` gives a tool call delta keyed by
`tool_call_id`, with `name` and `arguments`, after ending any open block;
`tool.execution.started` and `tool.execution.delta` open or continue a
built-in tool call by the event's `id`, named as the paragraph above says,
with their `arguments` joined; `tool.execution.done` MUST end it with the
arguments `{"arguments": <the joined text>}` and the `mistral` metadata
`{"type": "tool.execution", "name", "function"}` (`function` when given),
followed by a built-in tool return whose value is `{"info": <info>}`, or
`{}` without `info`; and `conversation.response.done` gives the usage
(`prompt_tokens` and `completion_tokens`, 0 when absent, with the cache-read
tokens of HST-016 and HST-042). A `conversation.response.error` event MUST
end the call with an `Error::Provider` holding its `message` and `code`,
classified as HST-018 classifies a Mistral stream error, and a
`tool.execution.done` for an id not open MUST end it with an
`Error::Provider` of kind `Response`. The finish reason MUST be `error` when
the stream ends without `conversation.response.done` or with an open
execution, else `tool_call` when a function call was read, else `stop`.
Falsifier: A recorded stream of `conversation.response.started` with `conversation_id` `conv_1`; a `message.output.delta` with `id` `m1`, `content_index` 0, `model` `mistral-medium-2604` and a `thinking` part `A`; one with `id` `m1`, `content_index` 1 and a `text` part `B`; a `tool.execution.started` with `id` `e1`, `name` `web_search` and `arguments` `{"q`; a `tool.execution.delta` for `e1` with `arguments` `":1}`; a `tool.execution.done` for `e1` with `function` `web_search` and `info` `{"result": "r"}`; a `message.output.delta` with `id` `m2`, `content_index` 0 and a `tool_reference` part with `url` `https://a.test` and `title` `A`, then one with the same URL and no title; one with `id` `m2`, `content_index` 0 and the string content `C`; a `function.call.delta` with `tool_call_id` `c1`, `name` `t`, `arguments` `{}` and `output_index` 0; and a `conversation.response.done` with `prompt_tokens` 12, `completion_tokens` 5 and `num_cached_tokens` 3, for a call with the tools `t` and `mistral::web_search()`, delivers other than reasoning `A`, text `B`, a built-in call `e1` of tool `web_search` with the arguments `{"arguments": "{\"q\":1}"}` and the metadata `{"type": "tool.execution", "name": "web_search", "function": "web_search"}`, its return `{"info": {"result": "r"}}`, text `C` and a call `c1` of `t`, each block ended before the next starts; ends with a response id other than `conv_1`, a response model other than `mistral-medium-2604`, a finish reason other than `tool_call`, usage other than 12 input, 5 output and 3 cache-read tokens, or `mistral.sources` other than one source for `https://a.test` titled `A`; a stream that ends after `started` and a text delta gives a finish reason other than `error`; a `conversation.response.error` with `message` `m` and `code` 429 does not end the call with a `RateLimit` provider error whose message is `m`; or a `tool.execution.done` for `e9` does not end it with a `Response` provider error.
Mechanism: hosts
Rationale: `doStream` and `createContentConverter` in `mistral-conversation-language-model.ts`; sources go in the final metadata as Google's do (decision 01M5KK631M310X7CF8TDFTM32P), since the core has no source part.
Status: Agreed 2026-10-24

[HST-045] `ops::embed` with a Mistral model MUST send `POST
<base>/embeddings` with the Mistral key and `{"model", "input": <the
values>, "encoding_format": "float"}`, with the `mistral` options `metadata`
as `metadata`, `outputDimension` as `output_dimension` and `outputDtype` as
`output_dtype`, at most 32 values per call and one call at a time, and
return each `data[].embedding` with `usage.prompt_tokens` as input tokens. A
`metadata` that is not an object, an `outputDimension` that is not a
positive integer, an `outputDtype` other than `float`, `int8`, `uint8`,
`binary` or `ubinary`, or another key MUST fail with `Error::Validation`
before any request.
Falsifier: A recorded `mistral-embed` call with the values `a` and `b` and the entry `{"metadata": {"k": "v"}, "outputDimension": 256, "outputDtype": "int8"}` goes to another URL or credential or differs from `{"model": "mistral-embed", "input": ["a", "b"], "encoding_format": "float", "metadata": {"k": "v"}, "output_dimension": 256, "output_dtype": "int8"}`; 33 values are split other than 32 and 1, or the second call starts before the first answers; a response with two embeddings and `usage.prompt_tokens` 7 returns other than those embeddings and 7 input tokens; or `{"outputDimension": 0}`, `{"outputDtype": "fp16"}` or `{"dimensions": 256}` sends a request or fails with another error kind.
Mechanism: hosts
Rationale: `mistral-embedding-model.ts`: 32 values per call and no parallel calls.
Status: Agreed 2026-10-24

[HST-046] `ops::generate_audio` with a Mistral model MUST send `POST
<base>/audio/speech` with the Mistral key and `{"model", "input": <the
text>, "voice_id": <the voice>, "response_format": <the format>, "stream":
false}`, with the `mistral` options `refAudio` as `ref_audio` in place of
`voice_id`, `expressiveness` as `expressiveness` and `phonemeHints` as
`phoneme_hints`, and return the response's `audio_data`, decoded from
base64, as one file part of the format's media type: `mp3` `audio/mpeg`,
`wav` `audio/wav`, `pcm` `audio/pcm`, `flac` `audio/flac` and `opus`
`audio/opus`. The format MUST default to `mp3`; another format MUST be sent
as `mp3` with a warning in `Item.warnings`, and the prompt's instructions, a
speed and a language MUST be left out with a warning each. An empty
`refAudio`, an `expressiveness` outside 1 to 5, a `phonemeHints` that is
not an object of non-empty keys to non-empty lists of non-empty strings, or
another key MUST fail with `Error::Validation` before any request, and a
response without `audio_data` MUST fail with `Error::Provider`.
Falsifier: A recorded `voxtral-mini-tts-latest` call for the text `hi`, voice `v1` and format `wav` goes to another URL or credential, differs from `{"model": "voxtral-mini-tts-latest", "input": "hi", "voice_id": "v1", "response_format": "wav", "stream": false}`, or returns other than the decoded `audio_data` bytes as one `audio/wav` part with no warning; one with `{"refAudio": "AAAA", "expressiveness": 3, "phonemeHints": {"hi": ["haɪ"]}}` sends `voice_id`, or lacks `ref_audio: "AAAA"`, `expressiveness: 3` or `phoneme_hints: {"hi": ["haɪ"]}`; one with format `aac`, instructions, speed 1.5 and language `en` sends a `response_format` other than `mp3`, sends an instruction, speed or language field, or returns other than four warnings; `{"expressiveness": 6}`, `{"refAudio": ""}`, `{"phonemeHints": {"hi": []}}` or `{"voice": "v"}` sends a request or fails with another error kind; or a response `{}` gives another error kind.
Mechanism: hosts
Rationale: `mistral-speech-model.ts`, which warns about the format, instructions, speed and language and falls back to `mp3`.
Status: Agreed 2026-10-24

The transcription models that cannot take a language with timestamps are
`voxtral-mini-2507`, `voxtral-mini-2602` and `voxtral-mini-latest`
(`mistral-transcription-model.ts`).

[HST-047] `ops::transcribe` with a Mistral model MUST send `POST
<base>/audio/transcriptions` with the Mistral key as multipart with
`model`, the audio as `file` named `audio.<extension>`, with OAI-019's
extension for its media type, and the `mistral` options `language`,
`returnLanguage` as `return_language`, `temperature`,
`timestampGranularities` as one `timestamp_granularities` field per value,
`diarize`, and `contextBias` as one `context_bias` field per value; and
return the response's `text`, its `segments` (`text`, `start`, `end`), its
`language`, the duration `usage.prompt_audio_seconds`, else the last
segment's end, the usage (`prompt_tokens` as input and `completion_tokens`
as output tokens) and the `mistral` metadata `usage` (`promptTokens`,
`completionTokens`, `totalTokens`, `promptAudioSeconds` and `requestCount`,
those the response holds) and `segments` (those with a `type`, `score` or
`speaker_id`, as `{"text", "startSecond", "endSecond", "type", "score",
"speakerId"}`), each absent when the response lacks it. Before any request
the call MUST fail with `Error::Validation` when `language` is given with
`returnLanguage: true`, or with `timestampGranularities` on a model named
above; when `language` or a `contextBias` entry is empty,
`timestampGranularities` is empty or holds other than `segment` or `word`,
or another key is given; and a response without `text` MUST fail with
`Error::Provider`.
Falsifier: A recorded `voxtral-mini-transcribe-latest` call with MP3 bytes and the entry `{"language": "en", "temperature": 0.2, "timestampGranularities": ["segment"], "diarize": true, "contextBias": ["Mistral", "Voxtral"]}` goes to another URL or credential, or its form lacks `model`, the file as `audio.mp3` of type `audio/mpeg`, `language` `en`, `temperature` `0.2`, `timestamp_granularities` `segment`, `diarize` `true` or the two `context_bias` fields; a response with text `t`, language `fr`, the segment `{"type": "transcription_segment", "text": "t", "start": 0, "end": 1.5, "speaker_id": "s1"}` and usage `{"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12, "prompt_audio_seconds": 1.5, "request_count": 1}` returns other than text `t`, one segment from 0 to 1.5, language `fr`, duration 1.5, 10 input and 2 output tokens, or `mistral` metadata other than `{"usage": {"promptTokens": 10, "completionTokens": 2, "totalTokens": 12, "promptAudioSeconds": 1.5, "requestCount": 1}, "segments": [{"text": "t", "startSecond": 0, "endSecond": 1.5, "type": "transcription_segment", "speakerId": "s1"}]}`; a response without `usage` whose last segment ends at 2 gives a duration other than 2, or one whose segments have no `type`, `score` or `speaker_id` gives `mistral` metadata with `segments`; `{"language": "en", "returnLanguage": true}`, `{"language": "en", "timestampGranularities": ["segment"]}` on `voxtral-mini-latest`, `{"timestampGranularities": ["sentence"]}`, `{"contextBias": [""]}` or `{"prompt": "p"}` sends a request or fails with another error kind; or a response `{"model": "m"}` gives another error kind.
Mechanism: hosts
Rationale: `mistral-transcription-model.ts` and `mistral-transcription-model-options.ts`; the two conflicts are its `InvalidArgumentError`s.
Status: Agreed 2026-10-24
