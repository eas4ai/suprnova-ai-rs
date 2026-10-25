//! Hosted providers that speak a protocol the crate implements: Cerebras,
//! DeepInfra, DeepSeek, Fireworks, Mistral, Moonshot AI, Together AI and Z.ai
//! over OpenAI Chat Completions, xAI and Hugging Face over OpenAI Responses,
//! and MiniMax over the Anthropic Messages protocol.
//!
//! Each host is the protocol's adapter with a host profile: its endpoint, its
//! key and the request and response differences the AI SDK for TypeScript
//! 7.0.148 records for it. Each host module has `provider`, which builds the
//! provider, and `register`, which registers it under the host's name and
//! package handle with snapshot restoration. No process environment is read:
//! the key comes from `ProviderConfig::api_key_value`, or from the `env` value
//! named by the host's key variable.
//!
//! ```
//! use std::sync::Arc;
//! use suprnova_ai::{ProviderConfig, ProviderRegistry};
//! use suprnova_ai::providers::hosts::cerebras;
//! use suprnova_ai::transport::HttpTransport;
//!
//! fn registry(transport: Arc<dyn HttpTransport>, key: String) -> Result<ProviderRegistry, suprnova_ai::Error> {
//!     let mut registry = ProviderRegistry::new();
//!     let config = ProviderConfig { api_key_value: Some(key), ..Default::default() };
//!     cerebras::register(&mut registry, config, transport)?;
//!     Ok(registry)
//! }
//! ```
//!
//! Options a host profile marks unsupported, such as Z.ai's penalties and
//! seed, fail with `Error::Validation` before any request, as the OpenAI and
//! Anthropic adapters reject theirs; the TypeScript SDK drops them with a
//! warning. An output schema goes to a host without JSON schema support as JSON
//! mode, which does not enforce the schema.
//!
//! Together AI, DeepInfra and Fireworks serve every model kind of their
//! TypeScript packages. A chat call reads the host's own options entry as
//! the compatible chat model does (HST-026); each module's
//! `completion_protocol` gives its completion models; and `ops::embed`,
//! `ops::generate_image` and, for Together AI, `ops::rerank` work with the
//! provider's models. DeepSeek reads its `deepseek` options entry, message
//! options and image parts as its package does (HST-033 to HST-036), and
//! `ops::upload_file` uploads images to its files API (HST-037). MiniMax
//! reads its `minimax` entry and 3.1's media blocks (HST-038, HST-039), and
//! `ops::generate_video` runs its video models as jobs (HST-040, HST-041).
//! `list_models` and `probe` return `Error::UnsupportedOperation` for every
//! host.
use crate::{
    Error, Provider, ProviderConfig, ProviderProtocol, ProviderRegistry,
    models::files::DownloadTransport,
    providers::{
        ModelsDevProvider,
        anthropic::protocol::{HostMessagesProtocol, MessagesProfile},
        http::ApiProfile,
        openai::{
            profile::{ChatProfile, ResponsesProfile},
            protocol::{HostChatProtocol, HostResponsesProtocol},
        },
        operations::{Item, UploadFile, UploadParams, UploadedFile},
    },
    transport::HttpTransport,
};
use async_trait::async_trait;
use std::{fmt, sync::Arc};

mod audio;
pub mod cerebras;
pub mod deepinfra;
pub mod deepseek;
mod embeddings;
mod files;
pub mod fireworks;
pub mod huggingface;
mod images;
pub mod minimax;
pub mod mistral;
mod models;
pub mod moonshotai;
mod rerank;
pub mod togetherai;
mod video;
pub mod xai;
pub mod zai;

/// Every host, for lookups by package or name.
static HOSTS: [&Host; 11] = [
    &cerebras::HOST,
    &deepinfra::HOST,
    &fireworks::HOST,
    &togetherai::HOST,
    &zai::HOST,
    &minimax::HOST,
    &deepseek::HOST,
    &moonshotai::HOST,
    &mistral::HOST,
    &xai::HOST,
    &huggingface::HOST,
];

/// The host that models.dev metadata names, by its package handle first and
/// then by its id.
fn host_for(metadata: &ModelsDevProvider) -> Option<&'static Host> {
    HOSTS
        .iter()
        .find(|host| metadata.npm.as_deref() == Some(host.handle))
        .or_else(|| HOSTS.iter().find(|host| host.name == metadata.id))
        .copied()
}
/// The Chat Completions host profile of a provider's default protocol: a
/// provider of this module, or one built from the host's models.dev
/// metadata.
fn chat_host_of(provider: &dyn Provider) -> Option<&'static ChatProfile> {
    let protocol = provider.default_protocol()?;
    let class = protocol.protocol_class_id();
    HOSTS.iter().find_map(|host| match host.protocol {
        HostProtocol::Chat(profile) if profile.protocol_class_id == class => Some(profile),
        _ => None,
    })
}
/// The error rules of the Chat Completions host a provider is, for the
/// completion and embedding calls it makes on the compatible protocols
/// (HST-027, HST-028).
pub(crate) fn chat_api(provider: &dyn Provider) -> Option<&'static ApiProfile> {
    chat_host_of(provider).map(|profile| &profile.api)
}
/// The name of the options entry of the host a provider is, which its
/// completion and embedding calls read too (HST-027, HST-028).
pub(crate) fn entry_key(provider: &dyn Provider) -> Option<&'static str> {
    chat_host_of(provider)?.host_options.key()
}
/// The protocol of the Chat Completions host that models.dev metadata
/// names, with the other model kinds the host serves.
pub(crate) fn chat_protocol_for(metadata: &ModelsDevProvider) -> Option<Arc<dyn ProviderProtocol>> {
    let host = host_for(metadata)?;
    matches!(host.protocol, HostProtocol::Chat(_)).then(|| host.protocol(None))
}
/// The Responses profile of the host that models.dev metadata names.
pub(crate) fn responses_profile_for(
    metadata: &ModelsDevProvider,
) -> Option<&'static ResponsesProfile> {
    match host_for(metadata)?.protocol {
        HostProtocol::Responses(profile) => Some(profile),
        HostProtocol::Chat(_) | HostProtocol::Messages(_) => None,
    }
}

/// A host: its names, endpoint, key variable and protocol profile.
#[derive(Debug)]
pub(crate) struct Host {
    pub name: &'static str,
    /// The package handle, such as `@ai-sdk/cerebras`.
    pub handle: &'static str,
    pub default_base_url: &'static str,
    pub api_key_env: &'static str,
    pub protocol: HostProtocol,
    /// The model kinds beside the language model.
    pub models: Models,
}

/// The model kinds a host's TypeScript package ships beside its language
/// model, which the host provider's default protocol serves.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Models {
    /// The host's embedding model (HST-028, HST-045).
    pub embeddings: Option<Embeddings>,
    /// The host's image model (HST-029 to HST-031).
    pub images: Option<images::Images>,
    /// Together AI's reranking (HST-032).
    pub reranking: bool,
    /// DeepSeek's files API, upload only (HST-037), which the provider
    /// serves.
    pub files: bool,
    /// MiniMax's video model (HST-040, HST-041).
    pub videos: Option<video::Videos>,
    /// Mistral's speech model (HST-046).
    pub speech: bool,
    /// Mistral's transcription model (HST-047).
    pub transcription: bool,
}
/// Whose embedding API a host serves.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Embeddings {
    /// OAI-016's compatible protocol (HST-028).
    Compatible,
    /// Mistral's embeddings (HST-045).
    Mistral,
}

impl Models {
    /// The language model alone.
    pub(crate) const CHAT: Self = Self {
        embeddings: None,
        images: None,
        reranking: false,
        files: false,
        videos: None,
        speech: false,
        transcription: false,
    };
    /// Mistral's: the language model, embeddings and speech.
    pub(crate) const MISTRAL: Self = Self {
        embeddings: Some(Embeddings::Mistral),
        speech: true,
        transcription: true,
        ..Self::CHAT
    };
    /// DeepSeek's: the language model and the files API.
    pub(crate) const DEEPSEEK: Self = Self {
        files: true,
        ..Self::CHAT
    };
    /// MiniMax's: the language model and the video model.
    pub(crate) const MINIMAX: Self = Self {
        videos: Some(video::Videos::MiniMax),
        ..Self::CHAT
    };
    /// DeepInfra's: embeddings and images.
    pub(crate) const DEEPINFRA: Self = Self {
        embeddings: Some(Embeddings::Compatible),
        images: Some(images::Images::DeepInfra),
        ..Self::CHAT
    };
    /// Fireworks': embeddings and images.
    pub(crate) const FIREWORKS: Self = Self {
        embeddings: Some(Embeddings::Compatible),
        images: Some(images::Images::Fireworks),
        ..Self::CHAT
    };
    /// Together AI's: embeddings, images and reranking.
    pub(crate) const TOGETHER_AI: Self = Self {
        embeddings: Some(Embeddings::Compatible),
        images: Some(images::Images::TogetherAI),
        reranking: true,
        ..Self::CHAT
    };
    /// Whether the host serves a model kind beside its language model.
    fn beside_chat(self) -> bool {
        self.embeddings.is_some()
            || self.images.is_some()
            || self.reranking
            || self.videos.is_some()
            || self.speech
            || self.transcription
    }
}
impl Host {
    /// The error and credential rules of the host's API.
    fn api(&self) -> &'static ApiProfile {
        match self.protocol {
            HostProtocol::Chat(profile) => &profile.api,
            HostProtocol::Responses(profile) => &profile.api,
            HostProtocol::Messages(profile) => &profile.api,
        }
    }
    /// The protocol of the host's models: its language model's, with the
    /// other model kinds it serves.
    fn protocol(
        &'static self,
        downloads: Option<Arc<dyn DownloadTransport>>,
    ) -> Arc<dyn ProviderProtocol> {
        match self.protocol {
            HostProtocol::Chat(profile) if self.models.beside_chat() => Arc::new(
                models::HostModels::new(HostChatProtocol(profile, downloads), self.models),
            ),
            HostProtocol::Messages(profile) if self.models.beside_chat() => Arc::new(
                models::MessagesModels::new(HostMessagesProtocol(profile), self.models),
            ),
            _ => self.protocol.protocol(downloads),
        }
    }
}

#[derive(Debug)]
pub(crate) enum HostProtocol {
    Chat(&'static ChatProfile),
    Responses(&'static ResponsesProfile),
    Messages(&'static MessagesProfile),
}
impl HostProtocol {
    fn protocol(&self, downloads: Option<Arc<dyn DownloadTransport>>) -> Arc<dyn ProviderProtocol> {
        match self {
            Self::Chat(profile) => Arc::new(HostChatProtocol(profile, downloads)),
            Self::Responses(profile) => Arc::new(HostResponsesProtocol(profile)),
            Self::Messages(profile) => Arc::new(HostMessagesProtocol(profile)),
        }
    }
    fn class_id(&self) -> &'static str {
        match self {
            Self::Chat(profile) => profile.protocol_class_id,
            Self::Responses(profile) => profile.protocol_class_id,
            Self::Messages(profile) => profile.protocol_class_id,
        }
    }
}

/// A hosted provider. Build one with a host module's `provider` function.
/// Its name and its snapshot class id are the host's name.
#[derive(Clone)]
pub struct HostProvider {
    host: &'static Host,
    config: ProviderConfig,
    transport: Arc<dyn HttpTransport>,
    downloads: Option<Arc<dyn DownloadTransport>>,
}
impl fmt::Debug for HostProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostProvider")
            .field("name", &self.host.name)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
impl HostProvider {
    /// Fill in the host's default base URL and key variable where the
    /// configuration leaves them unset.
    fn new(
        host: &'static Host,
        mut config: ProviderConfig,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        if config.default_base_url.is_empty() {
            config.default_base_url = host.default_base_url.into();
        }
        if config.api_key_env.is_none() {
            config.api_key_env = Some(host.api_key_env.into());
        }
        Self {
            host,
            config,
            transport,
            downloads: None,
        }
    }
    /// Download the media URLs that a Chat Completions host cannot fetch
    /// before each request: images, videos and text for Moonshot AI, audio
    /// and PDFs for the hosts that take them as OpenAI does; for image
    /// calls, the image URLs DeepInfra edits and a Fireworks Kontext result
    /// on another origin than the base URL, which comes without the key;
    /// and for MiniMax video calls, the video, which comes the same way.
    /// Without one, such a URL fails with `Error::Validation`, and so does
    /// every Fireworks Kontext call and every MiniMax video call, before its
    /// job starts, wherever its result would be. Responses hosts send no
    /// downloaded media and ignore it. Download transports must honor the
    /// core DNS-pinning contract. A snapshot does not keep the transport; a
    /// restore through a registry made with the host module's
    /// `register_with_downloads` (Moonshot AI, DeepInfra, Fireworks and
    /// MiniMax) gets it back.
    #[must_use]
    pub fn with_download_transport(mut self, downloads: Arc<dyn DownloadTransport>) -> Self {
        self.downloads = Some(downloads);
        self
    }
}
#[async_trait]
impl Provider for HostProvider {
    fn name(&self) -> &str {
        self.host.name
    }
    fn config(&self) -> &ProviderConfig {
        &self.config
    }
    fn provider_class_id(&self) -> &str {
        self.host.name
    }
    fn default_protocol(&self) -> Option<Arc<dyn ProviderProtocol>> {
        Some(self.host.protocol(self.downloads.clone()))
    }
    fn transport(&self) -> Option<&dyn HttpTransport> {
        Some(self.transport.as_ref())
    }
    fn shared_transport(&self) -> Option<Arc<dyn HttpTransport>> {
        Some(self.transport.clone())
    }
    fn is_configured(&self) -> bool {
        self.api_key().is_some_and(|key| !key.is_empty()) && self.config.is_configured()
    }
    fn download_transport(&self) -> Option<Arc<dyn DownloadTransport>> {
        self.downloads.clone()
    }
    /// Upload a file to a host with a files API (DeepSeek, HST-037). File
    /// info, download and delete stay unsupported, as the TypeScript package
    /// has them.
    async fn upload_file(
        &self,
        file: UploadFile,
        params: UploadParams,
    ) -> Result<Item<UploadedFile>, Error> {
        if !self.host.models.files {
            return Err(Error::UnsupportedOperation("upload_file".into()));
        }
        files::deepseek_upload(self.host.api(), self, file, params).await
    }
}

/// Register the host's provider under its name and handle, with its provider
/// and protocol snapshot factories. Nothing is registered when a handle or
/// factory is already taken.
fn register(
    host: &'static Host,
    registry: &mut ProviderRegistry,
    config: ProviderConfig,
    transport: Arc<dyn HttpTransport>,
    downloads: Option<Arc<dyn DownloadTransport>>,
) -> Result<(), Error> {
    let mut next = registry.clone();
    let protocol_downloads = downloads.clone();
    let build = move |config| {
        let provider = HostProvider::new(host, config, transport.clone());
        match &downloads {
            Some(downloads) => provider.with_download_transport(downloads.clone()),
            None => provider,
        }
    };
    let provider = Arc::new(build(config));
    for handle in [host.name, host.handle] {
        next.register(handle, provider.clone())?;
    }
    // A restore takes the endpoint and the credentials from the registered
    // provider; the snapshot keeps only the rest of the configuration.
    next.register_provider_factory(host.name, move |value, _| {
        let config = serde_json::from_value(value)?;
        Ok(Arc::new(build(config)))
    })?;
    // A model's own protocol gets the registry's download transport back
    // too.
    next.register_protocol_factory(host.protocol.class_id(), move |_| {
        Ok(host.protocol(protocol_downloads.clone()))
    })?;
    *registry = next;
    Ok(())
}
