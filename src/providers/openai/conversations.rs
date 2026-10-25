//! Mistral's Conversations API (`mistral-conversation-language-model.ts`):
//! a second language protocol of the Mistral provider, which a model takes
//! through `Model::with_protocol` (HST-043, HST-044).

use serde_json::Map;

use super::profile::ChatProfile;
use crate::{
    ProviderProtocol,
    types::tools::{Tool, ToolConfig},
};

/// The protocol's class id in snapshots.
pub(crate) const CLASS_ID: &str = "mistral_conversations";
/// The ids of the API's provider-executed tools (`mistralProviderToolNames`).
pub(crate) const WEB_SEARCH: &str = "mistral.web_search";
pub(crate) const WEB_SEARCH_PREMIUM: &str = "mistral.web_search_premium";

/// The Conversations protocol of the Mistral host whose chat profile this
/// is: the profile gives the API's credentials and error rules.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConversationsProtocol(pub(crate) &'static ChatProfile);

impl ProviderProtocol for ConversationsProtocol {
    fn protocol_class_id(&self) -> &str {
        CLASS_ID
    }
}

/// A web search tool of the Conversations API, named after its wire name.
pub(crate) fn web_search_tool(id: &'static str) -> Tool {
    let name = id.strip_prefix("mistral.").unwrap_or(id);
    Tool::provider(
        name,
        ToolConfig {
            id: Some(id.to_owned()),
            args: Map::new(),
        },
    )
    .expect("a provider tool with an id is well formed")
}
