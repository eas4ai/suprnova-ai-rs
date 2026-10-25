Prefix: ANT

# Anthropic

Anthropic's Messages API, as the AI SDK for TypeScript 7.0.148 package
`@ai-sdk/anthropic` 4.0.85 sends and reads it, with every model kind it
ships: the language model with its options, history, tools, stream and prompt
caching, files, skills, batches and decisions. `AnthropicCompatibleProvider`
speaks it, and the hosts on the Messages protocol, MiniMax and Vertex AI's
Anthropic models, share its rules (ANT-016). Provider options are the
`anthropic` entry of the call's provider parameters, with the TypeScript
option names; a part's `anthropic` metadata carries what TypeScript reads from
a part's provider options, and a tool's `tool_config` arguments carry, in
snake case, what it reads from a tool's. Where TypeScript changes or leaves
out a setting or a history part with a warning, the crate makes the same
change and logs it at debug (ANT-004); a setting Anthropic never takes, and an
unknown option, fail with `Error::Validation` before any request is sent.
`list_models` and `probe` stay as `docs/recon.md` describes them. Paths in
rationale lines are under `packages/anthropic/src/`.

[ANT-001] `AnthropicCompatibleProvider::new` MUST send every request to the
configured base URL, or `https://api.anthropic.com` when none is set, with
`anthropic-version: 2023-06-01` and `x-api-key: <key>`, or with
`authorization: Bearer <token>` and no `x-api-key` when the configuration's
`env` holds `ANTHROPIC_AUTH_TOKEN`; a configuration with both that token and
an `api_key_value` MUST fail with `Error::Configuration` before any request
is sent. Each operation MUST use its path: `/v1/messages` for the language
model, `/v1/files`, `/v1/skills` and `/v1/skills/<id>/versions/<version>`,
and `/v1/messages/batches`, `/v1/messages/batches/<id>` and
`/v1/messages/batches/<id>/cancel`. The betas a request needs MUST go in one
`anthropic-beta` header, comma-separated and once each, together with the
betas that the configuration's headers and the call's extra headers name in
their own `anthropic-beta` header.
Falsifier: A recorded request of any operation goes to another URL, or lacks `anthropic-version` or the key; with `ANTHROPIC_AUTH_TOKEN` in `env` it sends `x-api-key` or lacks the bearer token; the token together with an `api_key_value` sends a request or fails with another error kind; or a call that needs two betas, with a configured header `anthropic-beta: b1`, sends other than one `anthropic-beta` header holding each of the three once.
Mechanism: anthropic
Rationale: `anthropic-provider.ts` and `getHeaders` and `getBetasFromHeaders` in `anthropic-language-model.ts`; TypeScript takes the token as a setting and the crate, which reads no environment, takes it from the configuration's `env` as it takes OpenAI's organization, and keeps the Python SDK's base URL without `/v1`.
Status: Agreed 2026-10-24

[ANT-002] The provider options of a Messages call MUST be the `anthropic`
entry of the call's provider parameters, with the option names of
`anthropic-language-model-options.ts`; an unknown name or a value outside its
schema MUST fail with `Error::Validation` before any request is sent, and so
MUST `compaction` together with `contextManagement`. The options MUST be sent
as TypeScript sends them: `thinking` as `thinking` with `type`,
`budgetTokens` as `budget_tokens`, `display` and `blockBinding` as
`block_binding.prefix_mismatch_behavior`; `effort` and `taskBudget` in
`output_config` as `effort` and `task_budget`; `speed`, `serviceTier` as
`service_tier`, `inferenceGeo` as `inference_geo`, `fallbacks` when it is
`default` or a non-empty list, `cacheControl` as `cache_control` and
`metadata.userId` as `metadata.user_id`; `mcpServers` as `mcp_servers` with
snake-case keys; `container` as its id, or as `{"id", "skills"}` with each
skill's `type`, `skill_id` (a custom skill's `anthropic` provider reference)
and `version` when it lists skills; `safeguards` with `classifier_context`;
`compaction`; `contextManagement` as `context_management` with its edits in
snake case; `disableParallelToolUse` as the tool choice's
`disable_parallel_tool_use`, with `auto` when the call sets no tool choice;
and `anthropicBeta` as betas. Each option MUST add its beta:
`mcp-client-2025-04-04` for MCP servers, `dangerous-tool-use-2026-09-03` for
safeguards, `compact-2026-09-04` for `compaction`,
`context-management-2025-06-27` for `contextManagement` and also
`compact-2026-01-12` with a `compact_20260112` edit,
`code-execution-2025-08-25`, `skills-2025-10-02` and `files-api-2025-04-14`
for container skills, `task-budgets-2026-03-13` for a task budget,
`fast-mode-2026-02-01` for speed `fast`,
`thinking-display-updates-2026-08-18` for display `updates`,
`thinking-binding-controls-2026-08-01` for a block binding, and
`server-side-fallback-2026-07-01` for `default` fallbacks or
`server-side-fallback-2026-06-01` for a list. An option MUST take precedence
over the core parameter it duplicates.
Falsifier: A recorded `claude-sonnet-4-6` body for each option set alone lacks it under its wire name, changes its value or lacks its beta; a custom container skill sends another `skill_id` than its `anthropic` reference; `disableParallelToolUse: true` without a tool choice sends other than `{"type": "auto", "disable_parallel_tool_use": true}`; the option `serviceTier: "standard_only"` with a core service tier `auto` sends `auto`; or an unknown option, `effort: "huge"`, or `compaction` with `contextManagement` sends a request or fails with another error kind.
Mechanism: anthropic
Rationale: `prepareRequest` in `anthropic-language-model.ts`; TypeScript strips unknown options, and the crate refuses them as its OpenAI port does (OAI-004).
Status: Agreed 2026-10-24

[ANT-003] The core parameters of a Messages call MUST be sent as TypeScript
sends its standard settings: `max_tokens` from the core output limit, or the
model's limit (ANT-004) when unset, plus the budget of enabled thinking;
temperature, top-p and top-k; the core tool choice as `auto`, `none`, `any`
for required, or `{"type": "tool", "name"}` for one tool, with
`disable_parallel_tool_use` from parallel tool calls; the service tier as
`service_tier`; the safety identifier as `metadata.user_id`; a core cache
setting as the top-level `cache_control` `{"type": "ephemeral"}` with its
retention as `ttl`; and a core compaction threshold as a `compact_20260112`
context management edit with an `input_tokens` trigger. Without an `effort`
option, a core reasoning effort MUST be mapped as
`resolveAnthropicReasoningConfig` maps a reasoning level, where a `thinking`
option keeps its thinking and a mapped effort is not sent with disabled
thinking: disabled as
`thinking` `disabled`, or `between_tools` on a model that takes it, or effort
`low` on a model that rejects disabled thinking; a level, on a model with
adaptive thinking, as `thinking` `{"type": "adaptive", "display":
"summarized"}` with `output_config.effort` (`minimal` as `low`, and `xhigh` as
`max` unless the model takes `xhigh`), and on another model as enabled
thinking with a budget of the model's output limit times 2, 10, 30, 60, 90 or
95 percent for `minimal`, `low`, `medium`, `high`, `xhigh` or `max`, rounded
and kept between 1024 and that limit. A core reasoning summary MUST set the
display of adaptive thinking, none as `omitted`, and alone MUST send adaptive
thinking with that display on a model with adaptive thinking. A call MUST
fail with `Error::Validation` before any request is sent when it sets a seed
other than random, min-p, a frequency, presence or repetition penalty, a
consideration window, a maximum tool call count, a text verbosity, an output
include list, core metadata, a cache key or a cache mode, or a tool selection
of more than one tool.
Falsifier: A recorded `claude-sonnet-4-6` body without an output limit lacks `max_tokens: 128000`, or with limit 1000 and the option of enabled thinking with budget 2000 lacks `max_tokens: 3000`; core effort `high` on `claude-sonnet-4-6` lacks adaptive thinking with display `summarized` and effort `high`; core effort `medium` on `claude-sonnet-4-5` lacks enabled thinking with `budget_tokens: 19200`; disabled reasoning on `claude-sonnet-5-5` sends other than `between_tools` thinking, and on `claude-opus-5-5` sends a `thinking` field or lacks effort `low`; the safety identifier `u1` is not `metadata.user_id`; a core cache retention `1h` lacks a top-level `{"type": "ephemeral", "ttl": "1h"}`; or each refused setting sends a request or fails with another error kind.
Mechanism: anthropic
Rationale: `prepareRequest` and `resolveAnthropicReasoningConfig` in `anthropic-language-model.ts` and `mapReasoningToProviderBudget` in `provider-utils`; Anthropic's `metadata` takes only a user id, and the Python port's default of 8192 output tokens gives way to the model's limit.
Status: Agreed 2026-10-24

[ANT-004] A model's capabilities MUST be those `getModelCapabilities` in
`anthropic-language-model.ts` gives for its id: its output limit, whether it
has structured output, adaptive thinking and `xhigh` effort, whether it
rejects sampling parameters, disabled thinking, disabled thinking above
`high` effort, budget thinking and forced tool use, and whether it takes
`between_tools` thinking. Where TypeScript changes or leaves out a setting or
a history part with a warning, the crate MUST make the same change, MUST log
a `tracing` event at level `debug` with target `suprnova_ai::anthropic` that
names the setting and the model, and MUST still send the call. These changes
include: a temperature, top-p or top-k on a model that rejects sampling
parameters, left out; a temperature above 1 or below 0, clamped; with
enabled, adaptive or `between_tools` thinking, a temperature, top-p and top-k,
left out; without thinking, top-p with a temperature on a Claude model, left
out; disabled thinking on a model that rejects it, `between_tools` on a model
that takes it and left out otherwise; budget thinking on a model that rejects
it, adaptive; effort `xhigh` or `max` with disabled thinking on a model that
rejects that, or with `between_tools` thinking, `high`; enabled thinking
without a budget, a budget of 1024; `max_tokens` above a known model's limit,
the limit; a required or named tool choice on a model that rejects forced
tool use or with enabled thinking, `auto`, sending only the named tool; a
strict flag where strict tools are not taken, left out; cache breakpoints
after the fourth and on thinking blocks, left out; a core reasoning summary
on a model without adaptive thinking, left out; and reasoning parts when the
option `sendReasoning` is false, left out.
Falsifier: A temperature on `claude-opus-5`, a temperature of 1.5 on `claude-sonnet-4-6`, a temperature with adaptive thinking on `claude-sonnet-4-6`, top-p with a temperature on `claude-sonnet-4-6`, disabled thinking on `claude-sonnet-5-5` or `claude-fable-5-1`, budget thinking on `claude-opus-5-5`, effort `max` with disabled thinking on `claude-haiku-5-5`, `max_tokens: 200000` on `claude-sonnet-4-6`, tool choice required on `claude-opus-5-5`, or a fifth cache breakpoint, fails the call, is sent unchanged, or logs no `debug` event of target `suprnova_ai::anthropic` naming the setting and the model; or a temperature on `claude-sonnet-4-6` without thinking or top-p, or effort `max` with adaptive thinking on `claude-haiku-5-5`, is changed.
Mechanism: anthropic
Rationale: TypeScript makes these changes with a warning, and the crate has no warning for a model call, so it makes them and logs them at debug, as its OpenAI port does (OAI-005, Shawn 2026-10-23: "2 (log)", "just log it to debug").
Status: Agreed 2026-10-24

[ANT-005] An output schema MUST go out as the option `structuredOutputMode`
chooses, `auto` by default: `outputFormat` as `output_config.format`
`{"type": "json_schema", "schema"}`, which `auto` uses on a model with
structured output on a server that takes it; and otherwise the JSON tool, a
tool `json` described as `Respond with a JSON object.` with the schema as its
input schema and the tool choice `{"type": "any",
"disable_parallel_tool_use": true}`, whose input the stream delivers as the
answer's text while other text is left out, and whose `tool_use` stop is a
`stop` finish. The JSON tool on a model that rejects forced tool use and has
structured output MUST use the output format instead, logged as ANT-004 says.
The schema in `output_config.format` MUST be `sanitizeJsonSchema` of the
output schema: only the keywords it keeps, `oneOf` as `anyOf`,
`additionalProperties: false` on each object, only the string formats it
lists, and the other constraints appended to the description as it writes
them.
Falsifier: A recorded `claude-sonnet-4-6` body with an output schema lacks `output_config.format` or sends a JSON tool; with `structuredOutputMode: "jsonTool"` it lacks the JSON tool or its tool choice, or a streamed reply whose JSON tool input is `{"a":1}` gives another text or another finish than `stop`; `claude-3-haiku-20240307` uses the output format; `claude-opus-5-5` with `jsonTool` sends the JSON tool; or a schema with `minimum: 1`, `pattern`, `oneOf`, format `email` and format `regex` is sent other than `sanitizeJsonSchema` sends it.
Mechanism: anthropic
Rationale: `prepareRequest` and `sanitize-json-schema.ts`, which replaces the Python SDK's schema helper for the output format.
Status: Agreed 2026-10-24

[ANT-006] A function tool MUST be sent as `{"name", "description",
"input_schema"}` with the `defer_loading`, `allowed_callers`,
`input_examples` and `cache_control` of its `tool_config` arguments, with
`strict` when the tool is strict and the model has structured output on a
server that takes strict tools, and with `eager_input_streaming: true` in a
streamed call unless the option `toolStreaming` is false or the tool's
`eager_input_streaming` argument says otherwise. Function tools MUST add
`structured-outputs-2025-11-13` on a model with structured output, and
`input_examples` or `allowed_callers` MUST add
`advanced-tool-use-2025-11-20`. `anthropic::tools` MUST build each tool of
`anthropicTools`: `advisor_20260301`, `bash_20241022`, `bash_20250124`,
`code_execution_20250522`, `code_execution_20250825`,
`code_execution_20260120`, `computer_20241022`, `computer_20250124`,
`computer_20251124`, `computer_toolset_20260801`, `memory_20250818`,
`text_editor_20241022`, `text_editor_20250124`, `text_editor_20250429`,
`text_editor_20250728`, `web_fetch_20250910`, `web_fetch_20260209`,
`web_fetch_20260318`, `web_search_20250305`, `web_search_20260209`,
`web_search_20260318`, `tool_search_regex_20251119` and
`tool_search_bm25_20251119`, each from TypeScript's arguments in snake case,
and each MUST be sent as `prepareTools` sends it, with its type, its Anthropic
tool name and its beta. A provider tool that the application declares under
another name MUST be sent under its Anthropic name, and its calls and results
MUST come back under the application's name.
Falsifier: A recorded body for each tool function sends another tool object or lacks its beta; a strict tool on `claude-sonnet-4-6` lacks `strict: true`, or on `claude-3-haiku-20240307` sends it; a streamed call's function tool lacks `eager_input_streaming: true`, or has it with `toolStreaming: false`; a tool with `input_examples` lacks the advanced tool use beta; or `web_search_20250305` declared as `search` is sent under another name than `web_search`, or its streamed call or result comes back under another name than `search`.
Mechanism: anthropic
Rationale: `anthropic-prepare-tools.ts`, `anthropic-tools.ts`, `tool/*.ts` and `createToolNameMapping` in `prepareRequest`.
Status: Agreed 2026-10-24

[ANT-007] The provider tools that the application runs, the computer,
computer toolset, bash, text editor and memory tools, MUST be delivered as
tool calls under the application's tool name, and
`AgentTool::with_provider_declaration` MUST accept such a tool of the same
name as its declaration, as OAI-010 says. A call of a computer toolset member
MUST be delivered as one call of the toolset tool whose input is the member's
input with `action` set to the member's name and whose `anthropic` metadata
holds `toolsetName`; in history such a call MUST go back as a `tool_use`
named after its `action` with `toolset_name`, and its result as a
`tool_result` with `toolset_name`.
Falsifier: A streamed `bash` `tool_use` is not a tool call named after the declared bash tool; an agent with `with_provider_declaration` of `bash_20250124` does not send the provider tool or does not run its call; a toolset member `left_click` with input `{"coordinate": [1, 2]}` is not one call of the toolset tool with input `{"action": "left_click", "coordinate": [1, 2]}` and `toolsetName` `computer`; or that call and its result go back without `toolset_name` or under another name than `left_click`.
Mechanism: anthropic
Rationale: The toolset handling in `anthropic-language-model.ts` and `convert-to-anthropic-prompt.ts`.
Status: Agreed 2026-10-24

[ANT-008] The stream reader MUST deliver Anthropic's provider-run tools as
built-in calls and results under the application's tool name: a
`server_tool_use` of `web_search`, `web_fetch`, `code_execution`,
`tool_search_tool_regex`, `tool_search_tool_bm25` or `advisor`, and one of
`bash_code_execution` or `text_editor_code_execution` as a `code_execution`
call whose input starts with `"type"` set to that name; a `code_execution`
input that has `code` and no `type` as one with `"type":
"programmatic-tool-call"`; and their results in the shapes
`anthropic-language-model.ts` gives: web search results as `{url, title,
pageAge, encryptedContent, type}` each, a web fetch result as `{type, url,
retrievedAt, content: {type, title, citations, source: {type, mediaType,
data}}}`, code execution results as Anthropic sends them, tool search results
as `{type, toolName}` each, advisor results with `stopReason`, and each error
result as an error with `{type, errorCode}`. An `mcp_tool_use` and its
`mcp_tool_result` MUST be a built-in call and result whose `anthropic`
metadata holds `{"type": "mcp-tool-use", "serverName"}`. A call's or result's
`caller` MUST be kept in its `anthropic` metadata as `caller` `{type,
toolId}`, and a `tool_use` in `message_start` content MUST be delivered as a
tool call.
Falsifier: A streamed `server_tool_use` of `bash_code_execution` is not a built-in `code_execution` call with input type `bash_code_execution`; a web search result, a web fetch result, a tool search result, an advisor result or a web search error is delivered in another shape; an MCP call lacks its server name; a `tool_use` with caller `{"type": "code_execution_20260120", "tool_id": "srv_1"}` lacks the caller with `toolId` `srv_1`; or a `message_start` holding a `tool_use` delivers no tool call.
Mechanism: anthropic
Rationale: `doStream` and `doGenerate` in `anthropic-language-model.ts`; the crate's built-in call and result parts stand for TypeScript's provider-executed tool calls and results.
Status: Agreed 2026-10-24

[ANT-009] In history, the built-in calls and results of ANT-008 MUST go back
as `convert-to-anthropic-prompt.ts` sends them: as `server_tool_use` with the
Anthropic name and input (a code execution subtool named from its input
`type`, which is removed, and the `programmatic-tool-call` type removed); as
`web_search_tool_result`, `web_fetch_tool_result`,
`code_execution_tool_result`, `bash_code_execution_tool_result`,
`text_editor_code_execution_tool_result`, `tool_search_tool_result` and
`advisor_tool_result` in Anthropic's shapes, an error result with its
`errorCode`, or `unavailable` when it has none; and as `mcp_tool_use` and
`mcp_tool_result` for MCP. A call or result whose metadata holds a `caller`
MUST go back with `caller` `{type, tool_id}`, except that a caller whose code
execution call is missing from the history before the last user message MUST
be left out and logged. Tool call arguments that are not a JSON object MUST go
as `{"rawInvalidInput": <value>}`, with the text itself when it is not JSON.
Falsifier: A history of each streamed built-in call and result of ANT-008 is not sent back as the stream received it; a web search error without an error code lacks `error_code: "unavailable"`; a caller whose code execution call is in the history before the last user message is left out, or one whose call is missing is sent; or a call with arguments `[1]` is not sent with input `{"rawInvalidInput": [1]}`.
Mechanism: anthropic
Rationale: `convert-to-anthropic-prompt.ts`, including its caller pruning and `toAnthropicToolInput`; the Python port sent invalid arguments as an empty object.
Status: Agreed 2026-10-24

[ANT-010] A user file MUST be sent as TypeScript sends it: an image as
`image` with a `url` source for an HTTP(S) URL or a `base64` source with its
media type, a wildcard type resolved from the bytes; a PDF as `document` with
a `url` or `base64` source, adding `pdfs-2024-09-25`; a `text/plain` file as
`document` with a `text` source of its UTF-8 text, a URL fetched through the
provider's download transport; and a file whose text data starts with
`file_` as `image` or `document` with a `file` source of that id, adding
`files-api-2025-04-14`, or as `container_upload` when the part's `anthropic`
metadata sets `containerUpload`. A document MUST carry the `title` of the
part's `anthropic` metadata, else its filename, the metadata's `context`, and
`citations: {"enabled": true}` when the metadata enables citations. Other
media types, and a `text/plain` URL without a download transport, MUST fail
with `Error::Validation`. A tool result MUST be a `tool_result` whose content
is the text of a text result, the JSON text of a JSON result, or the parts of
a content result: text, and images and PDFs as above by file id, URL or
bytes, with other parts left out and logged; an error result MUST set
`is_error`.
Falsifier: A recorded body sends a PNG URL, PNG bytes typed `image/*`, a PDF URL, PDF bytes, a text file, a `file_abc` PDF, or a `file_abc` file with `containerUpload` in another shape or without its beta; a PDF with `citations`, `title` and `context` metadata lacks them; an `audio/wav` file sends a request or fails with another error kind; or a tool result with text, PNG bytes and a PDF URL is sent in another shape.
Mechanism: anthropic
Rationale: `convert-to-anthropic-prompt.ts`; TypeScript's core downloads a URL whose media type the provider does not list, and the crate does it through `AnthropicCompatibleProvider::with_download_transport`, as OpenAI's port does.
Status: Agreed 2026-10-24

[ANT-011] System messages MUST be sent as TypeScript sends them, except
where Anthropic refuses the place: the first run of system messages, when no
system text came before it and it sets no tool changes, clear time or
effort, as the top-level `system` list of text blocks; a later run as
`{"role": "system", "content"}` messages, adding
`mid-conversation-system-2026-04-07`. A later run with content MUST stay in
place when it follows a user message, or an assistant message that ends in
a server tool result, and comes before an assistant message or ends the
history; otherwise it MUST move to just after the last user message before
the next assistant message or the end, the user messages it separated MUST
go as one, and the move MUST be logged; when no user message comes between
its place and that point, the call MUST fail with `Error::Validation`
before any request. A later run whose messages hold only an effort MUST stay
in place. A system message's `anthropic` metadata MUST be read as
`anthropicSystemMessageProviderOptions`: `clearAt` as `clear_at` with
`mid-conversation-system-clear-at-2026-08-21`, `effort` as
`output_config.effort` with `mid-conversation-output-config-2026-07-01`, and
`toolChanges` as `tool_addition` and `tool_removal` blocks that name a tool
by its Anthropic name, with `mid-conversation-tool-changes-2026-07-01`. An
effort-only first system message with empty text MUST be sent as `{"role":
"system", "content": [], "output_config": {"effort"}}`, and tool changes, a
clear time or an effort on the top-level system text MUST be left out and
logged.
Falsifier: A recorded body for the messages system, user, assistant text, system, user sends the second system message in the top-level `system`, or other than as a system message just after the last user message, or without its beta, or logs no move; one for system, user, system, user, assistant, user sends other than one user message holding both first user texts, then the second system message, then the assistant; one for system, user, system, assistant moves the second system message; the history system, user, assistant text, system sends a request or fails with another error kind; a later system message that ends the history after an assistant message ending in a web search result, or one with only an effort between assistant text and a user message, is moved; a mid-conversation system message with `clearAt`, `effort` or a tool addition lacks its field or beta; an empty first system message with effort `low` is not that system message; or tool changes on the first system message are sent.
Mechanism: anthropic
Rationale: `convert-to-anthropic-prompt.ts`; the Python port joined every system message into the top-level system text; calls on `claude-haiku-5-5` (2026-10-24) showed Anthropic refuses a system message with content after assistant text ("role 'system' must follow a 'user' message or an 'assistant' message ending in a server tool result") or before a user message ("role 'system' must precede an 'assistant' message or end the array"), both of which TypeScript sends in place.
Status: Agreed 2026-10-24

[ANT-012] An assistant turn in history MUST be sent as
`convert-to-anthropic-prompt.ts` sends it: consecutive user and tool messages
as one user message and consecutive assistant messages as one assistant
message; text with the citations its `anthropic` metadata holds, the last
text of a history that ends with the assistant trimmed; a text part whose
metadata `type` is `compaction` as a `compaction` block with its signature,
adding `compact-2026-09-04` when it has one, and nothing when it is empty;
reasoning with a `signature` as `thinking`, with `redactedData` as
`redacted_thinking`, and other reasoning left out and logged; a text part
whose metadata `type` is `fallback` as a `fallback` block `{from, to}` with
`server-side-fallback-2026-06-01`, and one whose type is `container_upload`
as nothing; and the `tool_use` blocks of each run between thinking and
fallback blocks after that run's other blocks.
Falsifier: A recorded body for an assistant turn of text, a tool call and more text sends the tool call before the second text; a history that ends with the assistant text `"hi  "` sends it untrimmed; a text part with citations metadata loses them; a compaction part, a signed reasoning part, a redacted reasoning part or a fallback part is sent in another shape or without its beta; a container upload part is sent; or two consecutive assistant messages go as two messages.
Mechanism: anthropic
Rationale: `convert-to-anthropic-prompt.ts` (`groupIntoBlocks`, `moveToolUseBlocksToEnd`).
Status: Agreed 2026-10-24

[ANT-013] The Messages stream reader MUST deliver, beyond ANT-008: text, with
the citations of its `citations_delta` events as `citations` in its
`anthropic` metadata at its end; thinking with its signature, and
`redacted_thinking` as reasoning whose `anthropic` metadata holds
`redactedData`; a `compaction` block as text whose metadata holds `type:
"compaction"` and its signature, from `compaction_delta` events or from the
block start when it arrives complete; a `fallback` block as an empty text
part whose metadata holds `{type: "fallback", from, to}`, and a
`container_upload` block as one whose metadata holds `{type:
"container_upload", fileId}`; and the sources in the final metadata under
`anthropic.sources`, as `{sourceType, id, url, title, mediaType, filename}`
once each: a URL source for each web search result, the web search citations
when the response has no web search results, and a document source for each
citation of a document that the call sent with citations enabled or that a
web fetch returned. It MUST ignore `ping` events and a repeated
`message_start` of the open message, and MUST end the call with an
`Error::Provider` of kind `Response` for the `message_start` of another
message while one is open. An `error` event MUST end the call with an
`Error::Provider` that keeps its message and type, with the kind of the
status its type gives (`ProviderErrorKind::from_status`), retryable for the
first three: `api_error` 500, `overloaded_error` 529, `rate_limit_error` 429,
`request_too_large` 413, `authentication_error` 401, `permission_error` 403,
`not_found_error` 404, and `billing_error` and `invalid_request_error` 400;
another type gives kind `Response`.
Falsifier: A recorded stream with a text block and two `citations_delta` events lacks both citations in the text's metadata; a citation of a cited PDF, or a web search result, lacks its source in `anthropic.sources`, or a URL appears there twice; a `redacted_thinking`, `compaction`, `fallback` or `container_upload` block is delivered in another shape; a `ping` or a repeated `message_start` changes the events; or an `overloaded_error` event is not a retryable provider error with the kind of status 529, or an `invalid_request_error` event is retryable.
Mechanism: anthropic
Rationale: `doStream` and `getAnthropicStreamErrorMetadata` in `anthropic-language-model.ts`; sources go in the final metadata as Gemini's do (decision of 2026-10-23), since the crate has no source part, and a stream that ends without `message_stop` stays an incomplete response, as OAI-011 has it.
Status: Agreed 2026-10-24

[ANT-014] Messages usage MUST be reported as `convertAnthropicUsage` reports
it: input tokens as `input_tokens` plus the cache creation and cache read
tokens, those two as cache-write and cache-read tokens, output tokens, and
reasoning tokens from `output_tokens_details.thinking_tokens`; with
`iterations` and no `fallback_message` iteration, the input and output
tokens MUST be the sums over the `compaction` and `message` iterations. The
finish reason MUST be `stop` for `end_turn`, `stop_sequence` and
`pause_turn`, `tool_call` for `tool_use` unless ANT-005's JSON tool answered,
`length` for `max_tokens` and `model_context_window_exceeded`,
`content_filter` for `refusal`, and `other` otherwise. The final metadata
under `anthropic` MUST hold `stopReason`, the raw `usage`, `stopSequence`,
`stopDetails` with `recommendedModel`, `inputTransformations`,
`safeguardResults` (the last value that is not null), `iterations` with
camel-case keys, `container` as `{expiresAt, id, skills}` and
`contextManagement` as `{appliedEdits}`, as `anthropic-language-model.ts`
gives them. `anthropic::container_id(messages)` MUST give the container id of
the latest message whose metadata holds one.
Falsifier: A recorded stream with input 10, cache creation 2, cache read 3, output 20 and 5 thinking tokens reports other usage; one with a `compaction` iteration of input 100 and a `message` iteration of input 30 reports input other than 130 plus its cache tokens; `pause_turn` gives another finish than `stop`; `refusal` with `stop_details` lacks `stopDetails`; a stream with a container and an applied `clear_tool_uses_20250919` edit lacks them in the final metadata; or `container_id` gives another id.
Mechanism: anthropic
Rationale: `convert-anthropic-usage.ts`, `map-anthropic-stop-reason.ts` and `anthropic-message-metadata.ts`; `container_id` is `forward-anthropic-container-id-from-last-step.ts` for the crate's messages.
Status: Agreed 2026-10-24

[ANT-015] A part whose `anthropic` metadata holds `cacheControl` or
`cache_control` MUST send it as its block's `cache_control`: system texts,
user text and file parts, tool results (from the part, else from the first of
its content parts that has metadata), assistant text, tool calls and built-in
parts, and function tools (their `cache_control` argument); a message's own
metadata MUST apply to its last part when that part sets none. Only the first
four breakpoints of a request MUST be sent, and none on a thinking or
redacted thinking block; the others are left out and logged (ANT-004). The
option `cacheControl` and the core cache setting go on the top level
(ANT-002, ANT-003).
Falsifier: A recorded body lacks `cache_control` on a system text, a user text, a PDF, a tool result or a function tool whose metadata or argument sets it, or on the last part of a user message whose message metadata sets it; or a request with five breakpoints sends the fifth.
Mechanism: anthropic
Rationale: `get-cache-control.ts` and its calls in `convert-to-anthropic-prompt.ts` and `anthropic-prepare-tools.ts`.
Status: Agreed 2026-10-24

[ANT-016] MiniMax and Vertex AI's Anthropic models MUST take the Messages
rules of this file as TypeScript's `AnthropicLanguageModel` gives them to
those hosts, keeping HST-001, HST-007 and VTX-013: a MiniMax model of version
3.1 or later MUST clamp temperature at 2 instead of 1 and keep temperature
and top-p with thinking, and Vertex AI MUST use the JSON tool and send no
strict flag. A model whose id does not name a Claude model MUST take
TypeScript's defaults for one: an output limit of 4096 and no structured
output.
Falsifier: A recorded `MiniMax-M3.1` body with temperature 1.5 sends another temperature, or with thinking leaves out its temperature or top-p; a `MiniMax-M2` body with temperature 1.5 sends other than 1; a Vertex AI Anthropic body with an output schema uses the output format or sends a strict flag; or a `MiniMax-M2` body without an output limit lacks `max_tokens: 4096`.
Mechanism: anthropic
Rationale: `minimax-chat-language-model.ts` (`maxTemperature`, `supportsTemperatureAndTopPWithThinking`) and `google-vertex-anthropic-provider.ts` configure the shared model; MiniMax's own options and media come with its host commitment.
Status: Agreed 2026-10-24

[ANT-017] File upload with Anthropic MUST send `POST <base>/v1/files` as
multipart with the file, and its filename when it has one, adding
`files-api-2025-04-14`, and return the `anthropic` provider reference of its
id, its media type, filename, size and creation time, and the metadata
`{filename, mimeType, sizeBytes, createdAt, downloadable}`. File info,
download and delete MUST fail with `Error::UnsupportedOperation` before any
request is sent.
Falsifier: A recorded upload goes to another URL, sends another form or lacks the beta, or its result lacks the reference, size or creation time; or file info, download or delete sends a request or fails with another error kind.
Mechanism: anthropic
Rationale: `anthropic-files.ts`, which uploads only.
Status: Agreed 2026-10-24

[ANT-018] Skill upload with Anthropic (OPS-013) MUST send `POST
<base>/v1/skills` as multipart with `display_title` when one is given and
each file as `files[]` named by its path, adding `skills-2025-10-02`; when
the reply names a latest version it MUST then send `GET
<base>/v1/skills/<id>/versions/<version>` with the same beta. It MUST return
the `anthropic` reference of the skill id, its display title, the name and
description of the version or else of the skill, the latest version, and the
metadata `{source, createdAt, updatedAt}`.
Falsifier: A recorded upload sends another form or lacks the beta; a reply with a latest version leads to no version request; or the result lacks the version's name and description, or the reference.
Mechanism: anthropic
Rationale: `skills/anthropic-skills.ts`.
Status: Agreed 2026-10-24

[ANT-019] Batches with Anthropic MUST take only text requests whose ids are 1
to 64 letters, digits, `_` or `-` and unique in the batch, and send `POST
<base>/v1/messages/batches` with `{"requests": [{"custom_id", "params"}]}`,
each `params` the buffered Messages body of its request, with the betas of
every request and of the batch's `anthropicBeta` option in one header, and
return each request's ANT-004 changes as warnings with its id and a webhook
URL as a warning. A request with its own `anthropicBeta`, the JSON tool, a
provider tool under another name than its Anthropic name, `speed`, or a
fallback with `speed` MUST fail with `Error::Validation` before any request
is sent. A status MUST be `completed` for `ended` and `pending` otherwise,
with the counts total, pending (`processing`), completed (`succeeded`) and
failed (`errored`, `canceled` and `expired`), the times, and metadata
`{archivedAt, cancelInitiatedAt, endedAt, requestCounts, resultsUrl}`; cancel
MUST send `POST .../<id>/cancel`, and list MUST send `limit` and `after_id`
and return `last_id` as the next cursor when `has_more`. Results MUST fail
with `Error::Validation` for a pending or archived batch, and with an
`Error::Provider` of kind `Response`, before the results are requested, for a
batch without `results_url` or one whose `results_url` has another origin
than the base URL; otherwise they MUST read its JSON lines as they arrive, at
most 64 MiB a line: `succeeded` with the message read as a buffered Messages
response, leaving out content blocks of unknown types; `failed` with the
error's message and type and its request id in metadata; `cancelled`;
`expired`; and `failed` with code `invalid_response` for a line it cannot
read.
Falsifier: A recorded batch with an image request, a bad or repeated id, a per-request beta, a JSON tool, an aliased provider tool or `speed` sends a request; the batch body or its beta header differs; a temperature of 1.5 lacks a warning for its request; statuses, counts or cursors map otherwise; a `results_url` on another origin is requested; or result lines of each kind give other outcomes.
Mechanism: anthropic
Rationale: `anthropic-batch.ts`; TypeScript fetches a `results_url` on another origin without credentials, and the crate refuses it instead, so the key never leaves the base URL's origin.
Status: Agreed 2026-10-24

[ANT-020] `ops::experimental::evaluate` on the Anthropic Messages protocol
MUST evaluate as OPS-011 says, with one buffered Messages call of this file.
Falsifier: An evaluation on `claude-sonnet-4-6` sends other than one Messages call with OPS-011's system text, user turn and output schema, or fails with `Error::UnsupportedOperation`.
Mechanism: anthropic
Rationale: `decisionModel` in `anthropic-provider.ts` is `DecisionLanguageModel` over the Messages model.
Status: Agreed 2026-10-24
