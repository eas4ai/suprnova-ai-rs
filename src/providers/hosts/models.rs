//! The model kinds a host serves beside chat, on the protocol its provider
//! gives every model: Together AI's, DeepInfra's and Fireworks' embeddings
//! on the compatible embedding protocol (HST-028) and their image models
//! (HST-029 to HST-031), Together AI's reranking (HST-032), MiniMax's
//! video model on its Messages protocol (HST-040, HST-041), and Mistral's
//! embeddings (HST-045).
use super::{Embeddings, Models, embeddings, images, rerank, video};
use crate::{
    Error, Model, Provider, ProviderProtocol, StreamOptions,
    providers::{
        EventStream,
        anthropic::protocol::HostMessagesProtocol,
        openai::{OpenAIEmbeddingProtocol, protocol::HostChatProtocol},
        operations::{
            CallLimits, Documents, EmbedParams, EvaluationInput, EvaluationParams, ImageParams,
            ImagePrompt, Item, Question, RankedDocument, RerankParams, VideoParams, VideoPrompt,
            VideoStatus,
        },
    },
    types::{
        Message,
        messages::{FilePart, Metadata},
    },
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;

/// The values per call and the parallel calls of the hosts' embeddings,
/// `OpenAICompatibleEmbeddingModel`'s defaults.
const EMBEDDINGS_PER_CALL: usize = 2048;

/// A host's chat protocol with the other model kinds it serves.
pub(super) struct HostModels {
    chat: HostChatProtocol,
    models: Models,
}
impl HostModels {
    pub(super) fn new(chat: HostChatProtocol, models: Models) -> Self {
        Self { chat, models }
    }
    /// The compatible embedding protocol of the hosts that embed on it.
    fn compatible_embeddings() -> OpenAIEmbeddingProtocol {
        OpenAIEmbeddingProtocol::compatible(EMBEDDINGS_PER_CALL, true)
    }
}

#[async_trait]
impl ProviderProtocol for HostModels {
    fn protocol_class_id(&self) -> &str {
        self.chat.protocol_class_id()
    }
    fn stream(
        &self,
        model: &Model,
        messages: Vec<Message>,
        options: StreamOptions,
        provider: &dyn Provider,
    ) -> EventStream {
        self.chat.stream(model, messages, options, provider)
    }
    async fn embed(
        &self,
        model: &Model,
        values: Vec<String>,
        params: EmbedParams,
        provider: &dyn Provider,
    ) -> Result<Item<Vec<Vec<f64>>>, Error> {
        match self.models.embeddings {
            None => Err(Error::UnsupportedOperation("embed".into())),
            Some(Embeddings::Compatible) => {
                Self::compatible_embeddings()
                    .embed(model, values, params, provider)
                    .await
            }
            Some(Embeddings::Mistral) => {
                embeddings::mistral(&self.chat.0.api, model, values, params, provider).await
            }
        }
    }
    fn embed_limits(&self, model: &Model) -> CallLimits {
        match self.models.embeddings {
            None => CallLimits::default(),
            Some(Embeddings::Compatible) => Self::compatible_embeddings().embed_limits(model),
            Some(Embeddings::Mistral) => embeddings::MISTRAL_LIMITS,
        }
    }
    async fn generate_image(
        &self,
        model: &Model,
        prompt: ImagePrompt,
        params: ImageParams,
        provider: &dyn Provider,
    ) -> Result<Item<Vec<FilePart>>, Error> {
        let Some(kind) = self.models.images else {
            return Err(Error::UnsupportedOperation("generate_image".into()));
        };
        // A models.dev provider holds its download transport itself.
        let downloads = self
            .chat
            .1
            .clone()
            .or_else(|| provider.download_transport());
        images::generate(
            kind,
            &self.chat.0.api,
            downloads.as_deref(),
            model,
            prompt,
            params,
            provider,
        )
        .await
    }
    async fn rerank(
        &self,
        model: &Model,
        documents: Documents,
        query: String,
        params: RerankParams,
        provider: &dyn Provider,
    ) -> Result<Item<Vec<RankedDocument>>, Error> {
        if !self.models.reranking {
            return Err(Error::UnsupportedOperation("rerank".into()));
        }
        rerank::together(&self.chat.0.api, model, documents, query, params, provider).await
    }
    /// Each host's image model makes one image per call.
    fn image_limits(&self, _model: &Model) -> CallLimits {
        CallLimits {
            max_per_call: Some(1),
            parallel: true,
        }
    }
}

/// A host's Messages protocol with the other model kinds it serves.
pub(super) struct MessagesModels {
    chat: HostMessagesProtocol,
    models: Models,
}
impl MessagesModels {
    pub(super) fn new(chat: HostMessagesProtocol, models: Models) -> Self {
        Self { chat, models }
    }
    fn videos(&self, what: &str) -> Result<video::Videos, Error> {
        self.models
            .videos
            .ok_or_else(|| Error::UnsupportedOperation(what.into()))
    }
}

#[async_trait]
impl ProviderProtocol for MessagesModels {
    fn protocol_class_id(&self) -> &str {
        self.chat.protocol_class_id()
    }
    fn stream(
        &self,
        model: &Model,
        messages: Vec<Message>,
        options: StreamOptions,
        provider: &dyn Provider,
    ) -> EventStream {
        self.chat.stream(model, messages, options, provider)
    }
    async fn evaluate(
        &self,
        model: &Model,
        state: EvaluationInput,
        questions: BTreeMap<String, Question>,
        params: EvaluationParams,
        provider: &dyn Provider,
    ) -> Result<Item<Metadata>, Error> {
        self.chat
            .evaluate(model, state, questions, params, provider)
            .await
    }
    fn video_limits(&self, _model: &Model) -> CallLimits {
        match self.models.videos {
            Some(video::Videos::MiniMax) => video::limits(),
            None => CallLimits::default(),
        }
    }
    async fn start_video(
        &self,
        model: &Model,
        prompt: VideoPrompt,
        params: VideoParams,
        provider: &dyn Provider,
    ) -> Result<Item<Value>, Error> {
        match self.videos("start_video")? {
            video::Videos::MiniMax => video::minimax_start(model, prompt, params, provider).await,
        }
    }
    async fn video_status(
        &self,
        model: &Model,
        operation: &Value,
        provider: &dyn Provider,
    ) -> Result<VideoStatus, Error> {
        match self.videos("video_status")? {
            video::Videos::MiniMax => video::minimax_status(model, operation, provider).await,
        }
    }
    async fn download_video(
        &self,
        _model: &Model,
        video: FilePart,
        provider: &dyn Provider,
    ) -> Result<FilePart, Error> {
        match self.videos("download_video")? {
            video::Videos::MiniMax => video::minimax_download(video, provider).await,
        }
    }
}
