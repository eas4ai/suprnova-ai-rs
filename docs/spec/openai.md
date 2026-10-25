Prefix: OAI

# OpenAI

OpenAI's own APIs, as the AI SDK for TypeScript 7.0.148 package
`@ai-sdk/openai` sends and reads them, with every model kind it ships: the
Responses, Chat Completions and Completions language models, embeddings,
images, speech, transcription and its streaming form, speech translation,
files, skills, batches, realtime and Live sessions, and decisions.
`OpenAICompatibleProvider::new` and models.dev metadata that names
`@ai-sdk/openai` speak the Responses API with OpenAI's rules
(`OpenAIResponsesProtocol`). The history a call sends there holds items:
messages, reasoning, function calls and their outputs, and `item_reference`
items that name an item OpenAI stored from an earlier response by its item id.
Provider options are the `openai` entry of the call's provider parameters,
with the TypeScript option names; a part's `openai` metadata carries the
options TypeScript reads from a part's provider options. A setting that OpenAI
takes for some models but not for the call's model is left out and logged at
debug (OAI-005); a setting OpenAI never takes, and an unknown option, fail
with `Error::Validation` before any request is sent. Paths in rationale lines
are under `packages/openai/src/`.

[OAI-001] On the Responses API with OpenAI's rules, each tool call of an
assistant turn in the history, which the application runs, MUST be sent as a
`function_call` item with the call's `call_id`, `name` and `arguments`, and
with the `namespace` its OpenAI metadata records when it records one. It MUST
NOT be sent as an `item_reference` or with the OpenAI item id, whether `store`
is true, false or unset. Its result MUST follow as a `function_call_output`
with the same `call_id`.
Falsifier: A recorded body for a history of an assistant tool call `call_1` named `weather` with arguments `{"city":"SF"}`, whose OpenAI metadata holds the item id `fc_1` and its raw `function_call` item, followed by its result, holds an `item_reference` to `fc_1` or a `function_call` item with `"id": "fc_1"` under `store` true, false or unset, or lacks `{"type": "function_call", "call_id": "call_1", "name": "weather", "arguments": "{\"city\":\"SF\"}"}` followed by a `function_call_output` with `call_id` `call_1`; or the same history whose raw item holds `"namespace": "tools"` is sent without that `namespace`.
Mechanism: openai
Rationale: `responses/convert-to-openai-responses-input.ts` sends these calls in full (TypeScript change 61bcdb5) because OpenAI pairs an output with its call only by `call_id`, and referenced calls failed with "No tool call found for function call output", most often after parallel calls across steps; a live two-step `gpt-5-nano` call with one referenced call succeeded on 2026-10-23.
Status: Agreed 2026-10-23

[OAI-002] On the Responses API with OpenAI's rules, a model is a reasoning
model when its id is `o` and digits followed by `-` or the end, or `gpt-<n>`
with `n` at least 5, optionally followed by `.<m>` and `-<variant>`, unless
it has no `.<m>` and its variant starts with `chat`. When `store` is not
false, reasoning and assistant text with an OpenAI item id MUST each be sent
as one `item_reference` to that id, except text on a reasoning model when no
reasoning part with an OpenAI item id comes before it in the same assistant
turn; that text, and any text when `store` is false, MUST be sent as an
assistant message holding the text, without the item id. When `store` is
false, a call to a reasoning model MUST add `reasoning.encrypted_content` to
`include` once, and a reasoning part MUST be sent only when it has encrypted
content.
Falsifier: A recorded `gpt-5.4` body with `store` unset, for an assistant turn of text `T` with item id `msg_1` and no reasoning, holds an `item_reference` to `msg_1` or lacks `T`; the same turn with a reasoning part of item id `rs_1` before `T` sends other than one `item_reference` to `rs_1` and one to `msg_1`; a `gpt-4.1` turn of `T` alone is not one `item_reference` to `msg_1`; with `store` false, a `gpt-5.4` body lacks `reasoning.encrypted_content` in `include` or holds it twice, sends `T` with `"id": "msg_1"` or as a reference, or sends a reasoning part without encrypted content; or a `gpt-4.1` body with `store` false adds `reasoning.encrypted_content` to `include`.
Mechanism: openai
Rationale: `responses/convert-to-openai-responses-input.ts` sends stored text as a reference only after its reasoning (TypeScript change 216e2c9), since a reasoning model's stored message cannot be referenced without its reasoning item, and `responses/openai-responses-language-model.ts` asks for encrypted reasoning when `store` is false; OpenAI keeps stored items for a limited time, 30 days by default by community reports, so a chat that resumes later needs `store` false.
Status: Agreed 2026-10-23

[OAI-003] `OpenAICompatibleProvider::new` MUST send every OpenAI request to
the configured base URL, or `https://api.openai.com/v1` when none is set, with
`authorization: Bearer <key>`, and with `OpenAI-Organization` and
`OpenAI-Project` set from the configuration's `env` values `OPENAI_ORG_ID` and
`OPENAI_PROJECT_ID` when they are set. Each operation MUST use the path
`@ai-sdk/openai` uses: `/responses` for the default language model,
`/chat/completions` for `OpenAIChatCompletionsProtocol`, `/completions` for
the new `OpenAICompletionsProtocol`, `/embeddings`, `/images/generations`,
`/images/edits`, `/audio/speech`, `/audio/transcriptions`, `/files`,
`/skills`, `/batches`, `/realtime/client_secrets`, `/live/sessions` and
`/decisions`. An error body MUST give `ProviderError` the message of its
`error.message`, with its `error.type` and `error.code`, and the kind of the
HTTP status.
Falsifier: A recorded request of any operation named above goes to another URL than `https://api.openai.com/v1<path>`, lacks the bearer key, or, for a configuration whose `env` holds `OPENAI_ORG_ID` `org_1` and `OPENAI_PROJECT_ID` `proj_1`, lacks `OpenAI-Organization: org_1` or `OpenAI-Project: proj_1`; or an HTTP 429 body `{"error": {"message": "slow", "type": "requests", "code": "rate_limit_exceeded"}}` gives another message, type, code or a kind other than `RateLimit`.
Mechanism: openai
Rationale: `openai-provider.ts` and `openai-error.ts`; the crate reads no environment, so the organization and project come from the configuration under the names OpenAI's own SDKs read.
Status: Agreed 2026-10-23

[OAI-004] The provider options of a Responses call MUST be the `openai` entry
of the call's provider parameters, with the option names of
`openai-responses-language-model-options.ts`; an unknown name or a value
outside its schema MUST fail with `Error::Validation` before any request is
sent. The options MUST be sent as TypeScript sends them: `conversation`,
`instructions`, `maxToolCalls`, `metadata`, `parallelToolCalls`,
`previousResponseId`, `promptCacheKey`, `promptCacheRetention`,
`safetyIdentifier`, `serviceTier`, `store`, `textVerbosity` (in `text`),
`truncation` and `user` under their wire names; `logprobs` as `top_logprobs`,
`true` as 20 and a number from 1 to 20 as given; `reasoningEffort`,
`reasoningSummary`, `reasoningMode` and `reasoningContext` in `reasoning` as
`effort`, `summary`, `mode` and `context`; `contextManagement` as
`context_management` with `compact_threshold`; `compactionTrigger` as a last
input item `{"type": "compaction_trigger"}`; `strictJsonSchema` as the output
format's `strict`, true by default; and `allowedTools` as `tool_choice`
`{"type": "allowed_tools", "mode", "tools"}` with mode `auto` by default. An
option MUST take precedence over the core parameter it duplicates. A call MUST
fail with `Error::Validation` before any request is sent when it sets both
`conversation` and `previousResponseId`, or sets top-k, a seed, a frequency or
presence penalty, or stop sequences.
Falsifier: A recorded body for each option set alone on a model that takes it, `gpt-4.1`, or `gpt-4.1` with `forceReasoning: true` for `reasoningEffort`, `reasoningSummary`, `reasoningMode` and `reasoningContext`, lacks it under its wire name or changes its value; `logprobs: true` sends other than `top_logprobs: 20`; `compactionTrigger: true` does not end `input` with the trigger item; an output schema sends `strict` other than true without the option, or other than false with `strictJsonSchema: false`; `allowedTools {toolNames: ["a"]}` does not send `{"type":"allowed_tools","mode":"auto","tools":[{"type":"function","name":"a"}]}`; an option `serviceTier: "flex"` with a core service tier `priority` sends `priority`; or an unknown option, `logprobs: 21`, `conversation` with `previousResponseId`, or a seed sends a request or fails with another error kind.
Mechanism: openai
Rationale: TypeScript drops the core settings it does not send with a warning; this crate reports them as validation errors (HST-002).
Status: Agreed 2026-10-24

[OAI-005] On the Responses and Chat Completions APIs, a model MUST be a
reasoning model by OAI-002's rule, or when the option `forceReasoning` is
true. A reasoning model MUST send system messages with role `developer` unless
the option `systemMessageMode` is `system` or `remove`, which sends them as
`system` or leaves them out; another model MUST send `system` unless the
option says otherwise. On Responses, a reasoning effort other than `none`
without a summary option MUST send `reasoning.summary` `detailed`. A setting
that OpenAI takes for some models but not for the call's model MUST be left
out of the request and logged as a `tracing` event at level `debug` with
target `suprnova_ai::openai` that names the setting and the model, and the
call MUST still be sent. These settings are: for a reasoning model, a
temperature or top-p, and on Chat Completions logprobs, unless its effort is
`none` and the model is `gpt-6-sol`, `gpt-6-luna`, or `gpt-5.<m>` with `m` at
least 1; on Responses, logprobs for a `gpt-<n>` model with `n` at least 6
under the same exception; on Chat Completions, for a reasoning model, a
frequency or presence penalty, a logit bias or top logprobs; for another
model, a reasoning effort other than disabled, which sends nothing, or a
reasoning summary, mode or context; for a `gpt-<n>` model with `n` at least 6,
an effort other than `low`, `medium`, `high`, `xhigh` or `max` (also `none`
for `gpt-6-sol` and `gpt-6-luna`), or a prompt cache retention; service tier
`flex` for a model other than `o<n>` with `n` at least 3 and `gpt-<n>`
reasoning models with `n` at least 5; service tier `priority` or `fast` for a
model other than a `gpt-4` model, `o<n>` with `n` at least 3 and `gpt-<n>`
reasoning models with `n` at least 5 whose variant does not start with `nano`;
and the option `reasoningEffortUpdate` where it cannot apply. On a `gpt-<n>`
model with `n` at least 6 the option `reasoningEffortUpdate` MUST be sent as a
first input item `{"type": "configuration_update", "reasoning": {"effort"}}`
unless the first item already sets that effort; it cannot apply on another
model, with `reasoningMode` `pro`, `contextManagement` or `truncation` `auto`,
or for an effort the model does not take. A system message with empty content
whose `openai` metadata holds `reasoningEffortUpdate` MUST be sent as such an
item at its place, and MUST fail with `Error::Validation` before any request
is sent where the update cannot apply, on a system message with content, or
when two updates end up adjacent.
Falsifier: A recorded `gpt-5.4` body sends its system message as other than `developer`, or as other than `system` with `systemMessageMode: "system"`, or keeps it with `remove`; `gpt-4.1` with `forceReasoning: true` sends `system`; effort `high` without a summary option sends no `summary: "detailed"`; a temperature on `o3` or on `gpt-5.4` with effort `high`, an effort on `gpt-4.1`, effort `minimal` on `gpt-6-astra`, service tier `flex` on `gpt-4.1`, `priority` on `gpt-5-nano`, logprobs on `gpt-6-astra` with effort `high`, or the option `reasoningEffortUpdate` on `gpt-5.4`, fails the call, is sent, or logs no `debug` event of target `suprnova_ai::openai` naming the setting and the model; a temperature on `gpt-5.4` with effort `none`, or Responses logprobs on `gpt-5.4`, is not sent; a message-level update on `gpt-5.4`, or two adjacent updates, sends a request or fails with another error kind; or `reasoningEffortUpdate: "low"` on `gpt-6-sol` does not start the input with the update item.
Mechanism: openai
Rationale: TypeScript drops these settings with a warning, and the crate has no warning for a model call, so it drops them and logs them at debug (Shawn, 2026-10-23: "2 (log)", "just log it to debug"); settings OpenAI never takes still fail (OAI-004).
Status: Agreed 2026-10-23

[OAI-006] A Responses call MUST add to `include`, once each and after any
values the call gives: `message.output_text.logprobs` when it asks for
logprobs; `web_search_call.action.sources` when it has a web search tool,
unless the option `includeWebSearchSources` is false;
`code_interpreter_call.outputs` when it has a code interpreter tool; and
`reasoning.encrypted_content` by OAI-002, for a model `forceReasoning` makes a
reasoning model too.
Falsifier: A recorded body with `web_search` and `code_interpreter` tools lacks either include value or holds one twice; `includeWebSearchSources: false` still sends the sources value; `logprobs: 3` lacks the logprobs value; or `gpt-4.1` with `forceReasoning` and store false lacks `reasoning.encrypted_content`.
Mechanism: openai
Rationale: `prepareRequest` in `responses/openai-responses-language-model.ts`.
Status: Agreed 2026-10-23

[OAI-007] A user file on the Responses API MUST be sent as TypeScript sends
it: an image URL as `input_image` with `image_url`, image bytes as
`input_image` with a data URL, and a file whose text data starts with `file-`
as `input_image` or `input_file` with that `file_id`, each image with the
`detail` of the part's `openai` metadata `imageDetail`; a PDF given as a URL
as `input_file` with `file_url`; a PDF given as bytes as `input_file` with its
filename, or `part-<n>.pdf`, and a data URL. A `text/*` file MUST go as
`input_text` with its text, as before, unless the option
`passThroughUnsupportedFiles` is true. Other media types MUST fail with
`Error::Validation` unless that option is true, which sends them, and a
`text/*` file, as `input_file` with `file_url`, or with their filename, or
`part-<n>`, and a data URL. On Chat Completions a file id MUST be sent as
`{"type": "file", "file": {"file_id"}}`, an image as `image_url` with its
detail, WAV and MP3 bytes as `input_audio` with format `wav` or `mp3`, and a
PDF given as bytes as `file` with its filename and data; an audio or PDF URL
without a download transport, and other media types, MUST fail with
`Error::Validation`.
Falsifier: A recorded Responses body sends a PNG URL, PNG bytes with `imageDetail: "low"`, the file id `file-abc` of a PDF part, a PDF URL or PDF bytes in another shape; `text/csv` bytes are not `input_text` without the option, or are not `input_file` with it; a Chat Completions body sends `file-abc`, WAV bytes or PDF bytes in another shape; or a Chat Completions `audio/flac` part sends a request or fails with another error kind.
Mechanism: openai
Rationale: `responses/convert-to-openai-responses-input.ts` and `chat/convert-to-openai-chat-messages.ts`; the crate names an uploaded file by its id as file data, as `fileIdPrefixes` does, and passes another media type's URL on where the TypeScript core would download it first.
Status: Agreed 2026-10-24

[OAI-008] The Responses history MUST follow
`convert-to-openai-responses-input.ts` beyond OAI-001 and OAI-002: with the
option `conversation`, assistant items with an OpenAI item id MUST be left
out; with `previousResponseId`, reasoning with an item id MUST be left out; a
built-in tool call or result with an item id MUST be one `item_reference` when
stored; a compaction item MUST be an `item_reference` when stored and
`{"type": "compaction", "id", "encrypted_content"}` when not; a resolved
`ToolApproval` hook for an MCP approval request MUST be sent as `{"type":
"mcp_approval_response", "approval_request_id", "approve"}`, preceded by an
`item_reference` to the request when stored without a conversation or previous
response; and a tool result whose `openai` metadata holds `additionalTools`
MUST be followed by `{"type": "additional_tools", "role": "developer",
"tools"}`. With a conversation or previous response, the calls expanded from
one `parallel` call (OAI-011) and their results MUST go back as that one
`function_call` and one `function_call_output` of their outputs joined by
newlines.
Falsifier: A recorded body with `conversation: "conv_1"` holds an item of a stored text, reasoning or call id; with `previousResponseId` holds a reasoning item or reference with an id; a stored web search call and result give other than one reference; a stored compaction gives other than a reference, or an unstored one another shape; an approved MCP approval request `mcpr_1` gives no `mcp_approval_response` with `approve: true`, or no reference first when stored; a result with `additionalTools` lacks the item after it; or with `previousResponseId` two expanded calls `c_0` and `c_1` with their results go back as other than the one `parallel` call `c` and one output `r0\nr1`.
Mechanism: openai
Rationale: OpenAI rejects items a conversation already holds ("Duplicate item found"); the crate's approval mechanism is its `ToolApproval` hook.
Status: Agreed 2026-10-23

[OAI-009] Function tools on the Responses API MUST be sent as `{"type":
"function", "name", "description", "parameters", "strict"}` with `strict`
false unless the tool is strict (`Tool::with_strict`), and with the
`namespace`, `defer_loading`, `async`, `output_schema` and `allowed_callers`
that the tool's `tool_config` arguments hold, and not at all when they hold
`omit_from_request_tools`, whose forced choice MUST fail with
`Error::Validation`; the text result of a tool with an `output_schema` MUST be
sent JSON-quoted; tools of one namespace MUST be grouped in one `{"type":
"namespace", "name", "description", "tools"}`, and two descriptions for one
namespace MUST fail with `Error::Validation`; `async` on a model before GPT-6
MUST be left out and logged as OAI-005 says. Provider tools MUST be sent from
their snake_case arguments as today, and `openai::tools` MUST also build
`computer`, `custom` (a name, description and grammar or text format) and
`programmatic_tool_calling`. A named tool choice MUST be sent as `{"type":
<tool>}` for `code_interpreter`, `file_search`, `image_generation`,
`web_search`, `web_search_preview`, `mcp`, `apply_patch`, `computer` and
`programmatic_tool_calling`, as `{"type": "custom", "name"}` for a custom tool
and as `{"type": "function", "name"}` otherwise. On Chat Completions a
function tool MUST be sent as `{"type": "function", "function": {"name",
"description", "parameters", "strict"}}`, with `strict` only for a strict
tool.
Falsifier: A recorded Responses body sends a plain function tool without `strict: false`, a strict one without `strict: true`, a tool with `namespace` args outside its namespace group or in another shape, or a forced choice of `image_generation` or a custom tool in another shape; two namespace descriptions send a request or fail with another error kind; `async` on `gpt-5.4` fails the call, is sent, or is not logged; `openai::tools::custom` with a grammar format sends another shape; or a Chat Completions strict tool lacks `strict: true`.
Mechanism: openai
Rationale: `responses/openai-responses-prepare-tools.ts` and `chat/openai-chat-prepare-tools.ts`; the tool options TypeScript reads from a tool's provider options live in the crate's `tool_config` arguments, as `strict` does (decision 01M5KK1PAJ3EZETWXJYC00Z33Z).
Status: Agreed 2026-10-23

[OAI-010] The provider tools that the application runs (`local_shell`, `shell`
with a local environment, `apply_patch`, `computer` and `custom`) MUST be
delivered as tool calls, not built-in calls: `local_shell_call` with input
`{"action"}`, `shell_call` with `{"action": {"commands", "timeout_ms",
"max_output_length"}}`, `apply_patch_call` with `{"call_id", "operation"}`, a
`computer_call` that has a `call_id` with `{"actions",
"pending_safety_checks", "status"}` and `custom_tool_call` with its input
text. `AgentTool::with_provider_declaration` MUST accept such a provider tool
of the same name as its declaration, so that the agent sends the provider tool
and runs its calls. In history these calls MUST go back as their own items (an
`item_reference` when stored), and their results as `local_shell_call_output
{call_id, output}`, `shell_call_output {call_id, output, max_output_length}`,
`apply_patch_call_output {call_id, status, output}`, `computer_call_output
{call_id, output: {"type": "computer_screenshot", image_url | file_id},
acknowledged_safety_checks}` and `custom_tool_call_output {call_id, output}`.
A `shell` tool with a container environment is run by OpenAI and stays a
built-in call.
Falsifier: A recorded stream of each item kind delivers a built-in tool call, or a tool call with another name or input; an agent with an `AgentTool` declared as `openai.local_shell` sends a function tool, or does not run a `local_shell_call` and send its `local_shell_call_output` in the next request; or a stored `apply_patch_call` is not one reference, or its result another shape.
Mechanism: openai
Rationale: TypeScript runs these through the application's `execute`; the crate's agent runs tool calls, so a provider declaration on an `AgentTool` is the crate's form of TypeScript's provider-defined tool with an executor.
Status: Agreed 2026-10-23

[OAI-011] The Responses stream reader MUST deliver, beyond what it reads
today: `response.output_text.annotation.added` annotations in the text part's
metadata under `openai.annotations`; sources in the final metadata under
`openai.sources`, once per URL or file: those of web search actions and file
search results, and the URL and file citations of annotations when the
response has no web search or file search sources of their kind; a message
item's `phase` in its text part's metadata; `program` and `program_output`
items as a built-in `programmatic_tool_calling` call and result; a compaction
item as a built-in `compaction` call whose metadata keeps its item id and
encrypted content; an MCP approval request as a built-in call and a pending
`ToolApproval` hook whose id is the approval request id; a failed or
incomplete web search as an error result; a `function_call` named `parallel`
that the call does not declare, whose `tool_uses` all name declared tools, as
one tool call per use with the id `<call id>_<n>`; and a stream that ends
without a terminal event as an incomplete response.
Falsifier: A recorded stream with a `url_citation` annotation lacks it in the text metadata or the final `openai.sources`, gives two sources for one URL, or gives a citation's source when a web search action of that response lists sources; a `phase: "commentary"` message lacks it in metadata; a `program` item, a compaction item, an MCP approval request or a failed web search is delivered in another form; a `parallel` call of two declared tools is not two tool calls `c_0` and `c_1`; or a stream cut after text does not fail as incomplete.
Mechanism: openai
Rationale: `doStream` in `responses/openai-responses-language-model.ts`; sources go to final metadata as the Gemini reader's do (decision 01M5KK631M310X7CF8TDFTM32P).
Status: Agreed 2026-10-23

[OAI-012] An error event in an OpenAI Chat Completions stream MUST end the
call with an `Error::Provider` that keeps its message, `code` and `type`, with
the kind and retry flag of HST-018's rules, as HST-025 gives a Responses
stream. A Responses stream whose first chunk is a Chat Completions chunk MUST
fail with an `Error::Provider` of kind `Response` that says the base URL
serves Chat Completions and names `OpenAIChatCompletionsProtocol`.
Falsifier: A recorded Chat Completions stream chunk `{"error": {"message": "slow", "code": "rate_limit_exceeded"}}` does not end the call with kind `RateLimit` and a retry, or `insufficient_quota` is retryable; or a Responses call answered with a Chat Completions chunk fails with another kind or a message that does not name `OpenAIChatCompletionsProtocol`.
Mechanism: openai
Rationale: `openai-stream-error.ts` gives an unknown code status 500, where HST-018 gives kind `Response`; one rule for every OpenAI stream keeps HST-025 as agreed.
Status: Agreed 2026-10-23

[OAI-013] Responses usage MUST report `input_tokens` as input tokens,
`input_tokens_details.cached_tokens` as cache-read tokens,
`input_tokens_details.cache_write_tokens` as cache-write tokens,
`output_tokens` as output tokens and `output_tokens_details.reasoning_tokens`
as reasoning tokens. The finish reason MUST be `tool_call` after a call the
application runs, `length` for `max_output_tokens`, `content_filter` for
`content_filter`, `other` for another incomplete reason and `stop` otherwise.
The final metadata under `openai` MUST hold the response id, the service tier,
the reasoning context and, when the call asked for logprobs, every text item's
logprobs.
Falsifier: A recorded completion with input 10, cached 4, cache write 2, output 20 and reasoning 5 reports other numbers; a response that ends after a function call reports other than `tool_call`; an incomplete `max_output_tokens` response reports other than `length`; or a call with `logprobs: true` lacks `openai.logprobs`.
Mechanism: openai
Rationale: `convert-openai-responses-usage.ts` and `map-openai-responses-finish-reason.ts`.
Status: Agreed 2026-10-23

[OAI-014] The provider options of a Chat Completions call to OpenAI MUST be
the `openai` entry with the names of
`chat/openai-chat-language-model-options.ts`, sent as TypeScript sends them:
`logitBias` as `logit_bias`, `logprobs` as `logprobs: true` with
`top_logprobs` (`true` as 0), `parallelToolCalls`, `user`, `reasoningEffort`
as `reasoning_effort`, `maxCompletionTokens`, `store`, `metadata`,
`prediction`, `serviceTier`, `textVerbosity` as `verbosity`, `promptCacheKey`,
`promptCacheOptions`, `promptCacheRetention` and `safetyIdentifier`; an
unknown name or a `reasoningSummary` MUST fail with `Error::Validation`. A
reasoning model MUST send the output token limit as `max_completion_tokens`.
The `gpt-4o-search-preview` and `gpt-4o-mini-search-preview` models MUST leave
out a temperature and log it as OAI-005 says. A message with audio and no
content MUST give its transcript as text. URL citations in a message or delta
MUST become sources in the final `openai.sources`; the final metadata MUST
hold `acceptedPredictionTokens`, `rejectedPredictionTokens` and the logprobs.
Falsifier: A recorded Chat Completions body for each option on a model that takes it, `gpt-4.1`, or `gpt-4.1` with `forceReasoning: true` for `reasoningEffort`, lacks it under its wire name or changes it; `logprobs: true` sends other than `top_logprobs: 0`; `gpt-5.4` with an output limit sends `max_tokens`; a `reasoningSummary` option sends a request; a temperature on `gpt-4o-search-preview` is sent or fails the call; or a stream with a `url_citation` delta and prediction token counts lacks them in the final metadata.
Mechanism: openai
Rationale: `chat/openai-chat-language-model.ts`.
Status: Agreed 2026-10-24

[OAI-015] `OpenAICompletionsProtocol` MUST stream `POST <base>/completions`
with `stream_options.include_usage`, a prompt that starts with the first
system message and a blank line, then `user:\n<text>\n\n` and
`assistant:\n<text>\n\n` per turn and ends with `assistant:\n`, the stop
sequence `\nuser:` before the call's own, and the options `echo`, `logitBias`,
`suffix`, `user` and `logprobs` (`true` as 0) of the `openai` entry. A later
system message, a tool call or result, tools, a tool choice or an output
schema MUST fail with `Error::Validation`. It MUST read `choices[0].text` as
text, the finish reasons of Chat Completions, the logprobs into the final
`openai.logprobs`, and `prompt_tokens` and `completion_tokens` as usage. The
same protocol MUST be constructible for an OpenAI-compatible server, which
sends the call's other provider options in the body, sends no `logprobs`, and
sends `stream_options` only when its profile asks for usage.
Falsifier: A recorded body for a system message, a user turn and an assistant turn sends another prompt, stop list or path; an option is missing or changed; a tool sends a request; or a stream of texts `a` and `b` with finish `length` and usage 3 and 2 delivers other text, finish reason or usage.
Mechanism: openai
Rationale: `completion/openai-completion-language-model.ts` and `openai-compatible-completion-language-model.ts`, which DeepInfra, Fireworks and Together AI use in TypeScript.
Status: Agreed 2026-10-23

[OAI-016] Embedding with OpenAI MUST send `POST <base>/embeddings` with
`model`, `input`, `encoding_format: "float"`, `dimensions` and `user` from the
`openai` options, allow 2048 values per call and parallel calls, and report
`usage.prompt_tokens` as input tokens. The same embedding protocol MUST be
constructible for an OpenAI-compatible server, with its own maximum and
parallel setting and its `providerMetadata` passed through.
Falsifier: A recorded body for two values with `dimensions: 256` differs from the stated shape; 2049 values are split other than 2048 and 1; or usage 7 is reported otherwise.
Mechanism: openai
Rationale: `embedding/openai-embedding-model.ts` and `openai-compatible-embedding-model.ts`.
Status: Agreed 2026-10-23

[OAI-017] Image generation with OpenAI MUST send `POST
<base>/images/generations` with `model`, `prompt`, `n`, `size` and the options
`quality`, `style`, `background`, `moderation`, `outputFormat`,
`outputCompression` and `user`, and `response_format: "b64_json"` except for
`gpt-image-` and `chatgpt-image-` models; a prompt with images MUST send `POST
<base>/images/edits` as multipart with the images (`image`, or `image[]` for
several), the mask, `n`, `size` and the options `quality`, `background`,
`output_format`, `output_compression`, `input_fidelity` and `user`, fetching
image URLs through the provider's download transport
(`OpenAICompatibleProvider::with_download_transport`) and failing with
`Error::Validation` without one. It MUST allow 10 images per call for
`dall-e-2`, `chatgpt-image-latest` and `gpt-image-` models, and 1 otherwise.
An aspect ratio or a seed, and images for `dall-e-3`, MUST fail with
`Error::Validation`. The result MUST hold the `b64_json` images, the usage,
and per image under `openai.images` the revised prompt, size, quality,
background, output format and image and text tokens divided as TypeScript
divides them.
Falsifier: A recorded generation body for `dall-e-3` lacks `response_format: "b64_json"`, or one for `gpt-image-1` has it; an edit with two images sends other than two `image[]` parts; `n: 3` on `dall-e-3` makes other than three calls; an aspect ratio sends a request; or two images with 101 image tokens report other than 50 and 51.
Mechanism: openai
Rationale: `image/openai-image-model.ts`.
Status: Agreed 2026-10-23

[OAI-018] Speech with OpenAI MUST send `POST <base>/audio/speech` with
`model`, `input`, `voice` (`alloy` by default), `response_format` (`mp3` by
default; `opus`, `aac`, `flac`, `wav` or `pcm` as given), `speed` and
`instructions` from the prompt or the options, and return the audio bytes with
the media type `audio/mpeg`, `audio/opus`, `audio/aac`, `audio/flac`,
`audio/wav` or `audio/pcm` of its format. Another output format or a language
MUST fail with `Error::Validation`.
Falsifier: A recorded body without a voice or format lacks `alloy` or `mp3`; `wav` is not sent; an `ogg` format or a language sends a request; or the audio is returned with another media type.
Mechanism: openai
Rationale: `speech/openai-speech-model.ts`.
Status: Agreed 2026-10-23

[OAI-019] Transcription of a file with OpenAI MUST send `POST
<base>/audio/transcriptions` as multipart with `model`, `file` named
`audio.<extension of its media type>`, `response_format` (`verbose_json` for
`whisper-1`; else the option, or `diarized_json` for
`gpt-4o-transcribe-diarize`, `json` for `gpt-4o-transcribe` and
`gpt-4o-mini-transcribe`, else `verbose_json`), `include[]`, `language`,
`prompt`, `temperature`, `timestamp_granularities[]` and `chunking_strategy`
(`auto` by default for the diarize model). It MUST read the text, the segments
(or words), the language as a two-letter code or by the TypeScript name map,
the duration, and the diarized segments and languages under `openai`. A
`gpt-realtime-whisper` model MUST fail with `Error::Validation` for a file and
transcribe only streams (OAI-020).
Falsifier: A recorded multipart for `whisper-1` lacks `response_format=verbose_json`, or for the diarize model lacks `chunking_strategy=auto`; `timestamp_granularities: ["word"]` is not sent as `timestamp_granularities[]=word`; a response with language `english` reports other than `en`; or `gpt-realtime-whisper` sends a request for a file.
Mechanism: openai
Rationale: `transcription/openai-transcription-model.ts`.
Status: Agreed 2026-10-23

[OAI-020] Streaming transcription with a `gpt-realtime-whisper` model MUST
open `<base as wss>/realtime?intent=transcription` with the provider's
headers, send `{"type": "session.update", "session": {"type": "transcription",
"audio": {"input": {"format", "transcription": {"model", "language", "delay"},
"turn_detection": null}}, "include"}}`, each audio chunk as
`input_audio_buffer.append` with base64 audio and `input_audio_buffer.commit`
after the last, and read `conversation.item.input_audio_transcription.delta`
as deltas and `.completed` as the final text and finish. Another model, and
the options `include`, `prompt`, `temperature` and `timestampGranularities`,
MUST fail with `Error::Validation`. Speech translation with
`gpt-realtime-translate` MUST open `<base as
wss>/realtime/translations?model=<id>`, send the session update with input
transcription model `gpt-realtime-whisper`, `noise_reduction: null` and the
target language, audio as `session.input_audio_buffer.append`, `session.close`
after the last, and read `session.output_audio.delta`,
`session.output_transcript.delta`, `session.input_transcript.delta` and
`session.closed`; audio other than 24 kHz 16-bit PCM, a source language or an
output format MUST fail with `Error::Validation`.
Falsifier: A scripted WebSocket transport receives another URL, session update or audio messages, or no commit or close; delivered parts differ from the recorded server messages; or a `whisper-1` stream, a `prompt` option, 16 kHz translation audio or a source language connects or fails with another error kind.
Mechanism: openai
Rationale: `transcription/openai-transcription-model.ts` and `speech-translation/openai-speech-translation-model.ts`; a server sends the key in the `authorization` header, where a browser must use the `openai-insecure-api-key` subprotocol.
Status: Agreed 2026-10-23

[OAI-021] File upload with OpenAI MUST send `POST <base>/files` as multipart
with the file, `purpose` (`assistants` unless the option `purpose` says
otherwise) and, with the option `expiresAfter`,
`expires_after[anchor]=created_at` and `expires_after[seconds]`. It MUST
return the reference `{"openai": <id>}`, and the filename, size and creation
and expiry times that OpenAI reports (OPS-005), with the purpose and status
under `openai`. File info, download and delete (OPS-012) MUST use `GET
<base>/files/<id>`, `GET <base>/files/<id>/content` and `DELETE
<base>/files/<id>`, and a reference without an `openai` id MUST fail with
`Error::Validation`.
Falsifier: A recorded upload lacks `purpose=assistants` or the expiry fields with `expiresAfter: 3600`; the result has another reference, a media type, or a field OpenAI did not report; a download returns other bytes or media type; or a delete of a reference without an `openai` id sends a request.
Mechanism: openai
Rationale: `files/openai-files.ts`.
Status: Agreed 2026-10-23

[OAI-022] Skill upload with OpenAI (OPS-013) MUST send `POST <base>/skills` as
multipart with each file as `files[]` named by its path, fail with
`Error::Validation` for a display title, and return the reference `{"openai":
<id>}`, the name, description and latest version, and the default version and
creation and update times under `openai`.
Falsifier: A recorded upload of two files sends other parts or names; a display title sends a request; or the result lacks a reported field or has another reference.
Mechanism: openai
Rationale: `skills/openai-skills.ts`.
Status: Agreed 2026-10-23

[OAI-023] Batches with OpenAI MUST take only text requests that all name one
model, upload their Responses request bodies as JSON lines `{"custom_id",
"method": "POST", "url": "/v1/responses", "body"}` to `/files` with purpose
`batch` and an expiry of 48 hours (or the option `inputFileExpiresAfter`, 3600
to 2592000 seconds), and create the batch with `{"input_file_id", "endpoint":
"/v1/responses", "completion_window": "24h"}`. Status MUST map `completed` to
completed, `failed`, `expired` and `cancelled` to failed and the rest to
pending, with request counts, the first error and the times; cancel MUST send
`POST /batches/<id>/cancel`; list MUST send `limit` and `after` and return
`last_id` as the next cursor when there are more. Results MUST read the output
file and then the error file as JSON lines, giving `batch_cancelled` and
`batch_expired` errors as cancelled and expired, other errors and non-2xx
lines as failed with OpenAI's error, and 2xx bodies as the Responses result.
Falsifier: A recorded batch with an image request or two models sends a request; the uploaded lines or batch body differ; statuses, counts or cursors map otherwise; or result lines of each kind give other outcomes.
Mechanism: openai
Rationale: `openai-batch.ts`.
Status: Agreed 2026-10-23

[OAI-024] Realtime with OpenAI MUST create a token with `POST
<base>/realtime/client_secrets` and the session setup and expiry, returning
the secret, `wss://<host>/v1/realtime?model=<id>` and the expiry; the session
setup MUST be TypeScript's `buildOpenAISessionConfig`; the codec MUST encode
and decode the events of `openai-realtime-event-mapper.ts`; and a server
connection (OPS-014) MUST open that URL with the provider's key. A `gpt-live-`
model MUST use the Live API instead: a server connection to `<base as
wss>/live/sessions` with the provider's key that sends `session.start` with
the Live session setup; a codec that encodes `session.close`, input audio,
mute, unmute and `context-append` and decodes the server events of
`openai-live-event-mapper.ts`; and a WebRTC session (OPS-014) created with
`POST <base>/live/sessions` from an SDP offer. A token, a session update, and
the settings the Live setup refuses, MUST fail with `Error::Validation` on the
Live API.
Falsifier: A recorded token request has another body, or the token another URL; a scripted socket receives another setup or message for a client event, or yields other events for a server message; a `gpt-live-1` server connection opens another URL or sends another first message; a WebRTC session posts another body or returns another answer; or a `gpt-live-1` token or session update sends anything.
Mechanism: openai
Rationale: `realtime/openai-realtime-model.ts`, `openai-realtime-event-mapper.ts` and `live/`; TypeScript chooses the Live API for `gpt-live-1` or an explicit option, and the Live API takes no client secrets.
Status: Agreed 2026-10-23

[OAI-025] `OpenAIDecisionsProtocol` MUST evaluate with `POST
<base>/decisions`: the state as one user input of one `input_text` part, JSON
as its text, a choice question as `{"type": "choice", "name", "instructions",
"choices"}`, a score question as `{"type": "score", "name", "instructions",
"levels"}` labelled by index, a Noul question as `{"type": "predicate",
"name", "instructions"}` with its true and false criteria appended, and the
option `safetyIdentifier`. It MUST answer as OPS-011 answers, a refusal as
`{"type": "refusal"}`, and keep the probabilities and confidence under
`openai`; a duplicate, missing or extra answer MUST fail with
`Error::Provider` of kind `Response`. The Responses protocol's evaluation MUST
be the OPS-011 adapter on its language model.
Falsifier: A recorded body for a choice, a score and a Noul question differs from the stated shapes; an answer `{"type": "predicate", "probability": 0.7}` gives other than a Noul of 0.7; a refusal gives another answer; or a missing or duplicate answer does not fail.
Mechanism: openai
Rationale: `openai-decision-model.ts`; the crate's Noul question is TypeScript's boolean question.
Status: Agreed 2026-10-23

[OAI-026] Prompt caching on OpenAI MUST send the core cache key as
`prompt_cache_key` and retention as `prompt_cache_retention`, the option
`promptCacheOptions` as `prompt_cache_options` (`mode`, `ttl` and, on
Responses, `prewarm`), and a part whose `openai` metadata holds
`promptCacheBreakpoint` with `prompt_cache_breakpoint` on its text, image or
file content, on a system message as one `input_text` part, and on a tool
result's output; a core cache mode MUST fail with `Error::Validation`.
Falsifier: A recorded body lacks `prompt_cache_options` for the option, or a breakpoint on a user text part, a system message or a tool result; or a cache mode sends a request.
Mechanism: openai
Rationale: `convert-to-openai-responses-input.ts` and the chat converter; the crate dropped a cache mode without a word.
Status: Agreed 2026-10-23

[OAI-027] An output schema and the parameters of a function tool MUST be sent
normalized as `normalize-openai-json-schema.ts` normalizes them: a
`propertyNames` that is a string schema and a `pattern` with a regex
lookaround are removed and logged as OAI-005 says, another `propertyNames`
fails with `Error::Validation`, and a one-item `allOf` of a `$ref` becomes the
`$ref`, expanded at the root. The output schema MUST keep the crate's strict
form on Responses and Chat Completions: every property required and
`additionalProperties: false` on objects.
Falsifier: A recorded body for a schema with `{"allOf": [{"$ref": "#/$defs/A"}]}` at the root is not the expanded `A`; a nested one is not `{"$ref": "#/$defs/A"}`; a schema with a string `propertyNames` or a `(?=` pattern is sent with it, fails, or is not logged; or a `propertyNames` of `{"type": "number"}` sends a request.
Mechanism: openai
Rationale: TypeScript removes these with a warning and throws for another `propertyNames`; the crate logs what it removes (OAI-005).
Status: Agreed 2026-10-23
