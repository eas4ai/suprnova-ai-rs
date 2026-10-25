//! Mistral over Chat Completions. A seed goes out as `random_seed`; every
//! reasoning effort as `reasoning_effort: "high"` and disabled reasoning as
//! `"none"`, except for the models without reasoning settings, such as
//! `mistral-large-latest`, where they fail with `Error::Validation`, as do
//! the settings Mistral does not take, such as a service tier. Tool choice
//! `required` and a named tool go out as `any`, the named tool alone in
//! `tools`. An output schema goes out unchanged with `strict: false`.
//! Reasoning history goes out as thinking chunks of the assistant content, a
//! final assistant message as a prefix to continue, and PDFs as document URLs;
//! other non-image files fail. The `mistral` options entry (`safePrompt`,
//! `documentImageLimit`, `documentPageLimit`, `promptCacheKey`,
//! `parallelToolCalls`, `reasoningEffort`, `structuredOutputs`,
//! `strictJsonSchema`) wins over the core settings it names; with
//! `structuredOutputs: false`, an output schema goes out as JSON mode with the
//! schema instruction in the first system message, and `strictJsonSchema`
//! sets the `strict` of the JSON schema format. A strict function tool goes
//! out with `strict: true`.
use super::{Host, HostProtocol, HostProvider, Models};
use crate::{
    Error, ProviderConfig, ProviderRegistry,
    providers::{
        http::{ApiProfile, Auth},
        openai::profile::{
            ChatProfile, Dialect, HostOptions, JsonSchemaModels, ReasoningTokens, Setting,
        },
    },
    transport::HttpTransport,
};
use std::sync::Arc;

static PROFILE: ChatProfile = ChatProfile {
    protocol_class_id: "mistral_chat_completions",
    api: ApiProfile {
        label: "Mistral",
        auth: Auth::Bearer,
        default_headers: &[],
        request_id_header: "x-request-id",
        collapse_server_errors: false,
        keep_error_object: false,
        top_level_first: false,
        code_prefixes_error: false,
        config_headers: &[],
    },
    include_usage: false,
    max_tokens_field: "max_tokens",
    reasoning_field: "reasoning_content",
    // The dialect sends every effort as `high`.
    effort_aliases: &[],
    json_schema: JsonSchemaModels::All,
    openai_params: false,
    auto_tool_choice_only: false,
    rejects_penalties_and_seed: false,
    finish_reasons: &[("model_length", "length"), ("error", "error")],
    reasoning_tokens: ReasoningTokens::InCompletion,
    json_drops_tool_calls_after_text: false,
    unsupported: &[
        Setting::ServiceTier,
        Setting::SafetyIdentifier,
        Setting::Metadata,
        Setting::CacheRetention,
        Setting::ToolSelection,
    ],
    dialect: Dialect::Mistral,
    stream_error_kinds: true,
    openai_own: false,
    host_options: HostOptions::NotRead,
};

pub(super) static HOST: Host = Host {
    name: "mistral",
    handle: "@ai-sdk/mistral",
    default_base_url: "https://api.mistral.ai/v1",
    api_key_env: "MISTRAL_API_KEY",
    protocol: HostProtocol::Chat(&PROFILE),
    models: Models::CHAT,
};

/// Build the Mistral provider. An unset base URL becomes
/// `https://api.mistral.ai/v1` and an unset key variable `MISTRAL_API_KEY`.
pub fn provider(config: ProviderConfig, transport: Arc<dyn HttpTransport>) -> HostProvider {
    HostProvider::new(&HOST, config, transport)
}

/// Register the Mistral provider under `mistral` and `@ai-sdk/mistral`,
/// with snapshot restoration that uses this transport.
pub fn register(
    registry: &mut ProviderRegistry,
    config: ProviderConfig,
    transport: Arc<dyn HttpTransport>,
) -> Result<(), Error> {
    super::register(&HOST, registry, config, transport, None)
}
