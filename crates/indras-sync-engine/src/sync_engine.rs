//! SyncEngine — the app-layer entry point.
//!
//! Holds an `Arc<IndrasNetwork>` and provides domain-specific operations
//! that don't belong on individual Realms (e.g., cross-realm sentiment queries).

use std::collections::HashMap;
use std::sync::Arc;

use crate::sentiment::{SentimentRelayDocument, SentimentView};
use crate::story_auth::StoryAuth;
use indras_network::error::{IndraError, Result};
use indras_network::member::MemberId;
use indras_network::IndrasNetwork;

/// The SyncEngine app layer.
///
/// Wraps a shared `IndrasNetwork` instance and provides domain-specific
/// operations like cross-realm sentiment queries and story authentication.
///
/// # Example
///
/// ```ignore
/// use std::sync::Arc;
/// use indras_network::IndrasNetwork;
/// use indras_sync_engine::SyncEngine;
///
/// let network = Arc::new(IndrasNetwork::new("~/.myapp").await?);
/// let engine = SyncEngine::new(Arc::clone(&network));
///
/// // Use engine for cross-realm operations
/// let sentiment = engine.query_sentiment(&member_id, &relay_docs).await?;
/// ```
pub struct SyncEngine {
    network: Arc<IndrasNetwork>,
}

impl SyncEngine {
    /// Create a SyncEngine from a shared network instance.
    ///
    /// The network should already be started.
    pub fn new(network: Arc<IndrasNetwork>) -> Self {
        Self { network }
    }

    /// Access the underlying network SDK.
    pub fn network(&self) -> &IndrasNetwork {
        &self.network
    }

    /// Get a cloned Arc to the underlying network.
    pub fn network_arc(&self) -> Arc<IndrasNetwork> {
        Arc::clone(&self.network)
    }

    /// Query the aggregate sentiment view about a member.
    ///
    /// Returns direct sentiment from your own contacts who know the target,
    /// plus second-degree relayed sentiment from contacts' contacts.
    ///
    /// This is scoped to your local view — you never see sentiment from
    /// people outside your contact graph.
    pub async fn query_sentiment(
        &self,
        about: &MemberId,
        relay_documents: &HashMap<MemberId, SentimentRelayDocument>,
    ) -> Result<SentimentView> {
        let contacts = self.network.contacts_realm().await.ok_or_else(|| {
            IndraError::InvalidOperation(
                "Must join contacts realm before querying sentiment.".to_string(),
            )
        })?;

        let mut view = SentimentView::default();

        // Collect direct sentiment from our contacts
        let doc = contacts.contacts_with_sentiment();
        for (contact_id, _sentiment) in &doc {
            if contact_id == about {
                continue;
            }
            if let Some(relay_doc) = relay_documents.get(contact_id) {
                if let Some(relayed_sentiment) = relay_doc.get(about) {
                    view.direct.push((*contact_id, relayed_sentiment));
                }
            }
        }

        // Collect second-degree relayed sentiment
        for (contact_id, _sentiment) in &doc {
            if contact_id == about {
                continue;
            }
            if let Some(relay_doc) = relay_documents.get(contact_id) {
                for (rated_id, rated_sentiment) in relay_doc.iter() {
                    if rated_id == about {
                        continue;
                    }
                    let _ = (rated_id, rated_sentiment);
                }
            }
        }

        Ok(view)
    }

    /// Create a story-based account.
    ///
    /// Delegates to `StoryAuth::create_account` with the network's data directory.
    pub fn create_story_auth(
        data_dir: &std::path::Path,
        story: &indras_crypto::story_template::PassStory,
        user_id: &[u8],
        timestamp: u64,
    ) -> Result<StoryAuth> {
        StoryAuth::create_account(data_dir, story, user_id, timestamp)
    }

    /// Authenticate with a story.
    ///
    /// Delegates to `StoryAuth::authenticate`.
    pub fn authenticate_story(
        data_dir: &std::path::Path,
        story: &indras_crypto::story_template::PassStory,
    ) -> Result<(StoryAuth, crate::story_auth::AuthResult)> {
        StoryAuth::authenticate(data_dir, story)
    }
}

impl Clone for SyncEngine {
    fn clone(&self) -> Self {
        Self {
            network: Arc::clone(&self.network),
        }
    }
}
