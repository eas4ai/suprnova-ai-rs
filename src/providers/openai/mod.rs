//! OpenAI's APIs, as the TypeScript `@ai-sdk/openai` 4.0.99 package sends
//! them, and OpenAI-compatible Chat Completions over `HttpTransport`.
//!
//! Construct a provider with explicit configuration and a shared transport. Register
//! handles and snapshot factories explicitly; no process environment is read.
//! The `env` values `OPENAI_ORG_ID` and `OPENAI_PROJECT_ID` set the
//! `OpenAI-Organization` and `OpenAI-Project` headers of OpenAI's own
//! requests; an OpenAI-compatible server gets neither.
//!
//! ```
//! use std::sync::Arc;
//! use suprnova_ai::{Model, ProviderConfig};
//! use suprnova_ai::providers::openai::OpenAICompatibleProvider;
//! use suprnova_ai::transport::HttpTransport;
//!
//! fn model(transport: Arc<dyn HttpTransport>, api_key: String) -> Model {
//!     let config = ProviderConfig { api_key_value: Some(api_key), ..Default::default() };
//!     Model::new("gpt-5.4", Arc::new(OpenAICompatibleProvider::new(config, transport)))
//! }
//! ```
//!
//! Use `OpenAICompatibleProvider::from_modelsdev_provider` for ollama or
//! custom provider metadata. Metadata that names OpenAI's own package,
//! `@ai-sdk/openai`, selects Responses, as `OpenAICompatibleProvider::new` does;
//! other metadata selects Chat Completions. Metadata for Cerebras, DeepInfra,
//! DeepSeek, Fireworks, Mistral, Moonshot AI, Together AI or Z.ai gets that
//! host's Chat Completions rules, and metadata for xAI or Hugging Face that
//! host's Responses rules, from `providers::hosts`, which also registers
//! those hosts directly. An explicit
//! protocol overrides the default. Custom providers using
//! these protocols implement `Provider::shared_transport` for owned streams.
//! `params::OpenAIParams::apply` merges `store` into the options under the
//! stable `openai` key.
//! Built-in tool arguments use snake_case JSON objects, as in the Python wire format.
//!
//! The language models are `OpenAIResponsesProtocol` (the default),
//! `OpenAIChatCompletionsProtocol` and `OpenAICompletionsProtocol`; set
//! one with `Model::with_protocol`. `OpenAIDecisionsProtocol` evaluates
//! questions with the Decisions API, and `OpenAIEmbeddingProtocol` and
//! `OpenAICompletionsProtocol` also serve OpenAI-compatible servers. The
//! `ops` front ends reach the other model kinds: embeddings, images, speech,
//! transcription and its streaming form, speech translation, files, skills,
//! batches, and realtime and Live sessions. Files, skills and batches need
//! OpenAI's own API. Provider options use the TypeScript option names, and an
//! unknown one fails with `Error::Validation`. A setting that OpenAI takes for
//! other models only is left out and logged as a `tracing` debug event with
//! target `suprnova_ai::openai`.
//!
//! `OpenAICompatibleProvider::with_download_transport` fetches the media URLs
//! that OpenAI does not fetch, and `with_websocket_transport` opens the
//! sockets of streaming audio and realtime sessions. The tools OpenAI leaves
//! to the application (`local_shell`, `shell` with a local environment,
//! `apply_patch`, `computer` and `custom`) arrive as tool calls; give an
//! agent one with `AgentTool::with_provider_declaration`. An MCP approval
//! request arrives as a pending `ToolApproval` hook whose `hook_id` is the
//! request id; a resolved hook of that id in a later message, such as the
//! one the UI adapter makes of an answered approval, goes to OpenAI as the
//! request's `mcp_approval_response`.
mod batch;
mod capabilities;
mod chat;
mod completions;
pub(crate) mod conversations;
mod decisions;
mod deepseek;
mod dialect;
mod embedding;
pub mod errors;
mod files;
mod host_options;
mod images;
mod messages;
mod mistral;
mod options;
pub mod params;
pub(crate) mod profile;
pub mod protocol;
mod provider;
mod realtime;
mod responses;
mod schema;
mod skills;
mod socket;
mod speech;
mod streaming;
pub mod tools;
mod transcription;
pub use completions::OpenAICompletionsProtocol;
pub use decisions::OpenAIDecisionsProtocol;
pub use embedding::OpenAIEmbeddingProtocol;
pub(crate) use images::media_bytes;
pub use params::OpenAIParams;
pub(crate) use transcription::extension as audio_extension;
// Kept for callers of the old name-based choice; it is deprecated.
#[allow(deprecated)]
pub use protocol::default_protocol;
pub use protocol::{
    OpenAIChatCompletionsProtocol, OpenAIChatCompletionsWithDownloads, OpenAIResponsesProtocol,
};
pub use provider::{OpenAICompatibleProvider, register, register_factories};
