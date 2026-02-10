use indras_artifacts::{
    ArtifactId, Exchange, LeafType, PlayerId, Request, Story, TreeType, Vault,
    InMemoryArtifactStore, InMemoryAttentionStore, InMemoryPayloadStore,
    compute_token_value,
};

use crate::encounter;
use crate::state::{ExchangeView, RequestView, TokenView};

type Result<T> = std::result::Result<T, indras_artifacts::VaultError>;
type InMemVault = Vault<InMemoryArtifactStore, InMemoryPayloadStore, InMemoryAttentionStore>;

/// Seed the vault with demo tokens for testing.
pub fn seed_demo_tokens(vault: &mut InMemVault, now: i64) -> Result<Vec<TokenView>> {
    let tokens_data = vec![
        ("Eucalyptus Removal Token", "Helping neighbor process invasive eucalyptus", "8h", "Oct 21, 2025"),
        ("Sourdough Starter Token", "Sharing & teaching starter cultivation", "2h", "Oct 15, 2025"),
        ("Garden Mural Token", "Painting community garden entrance", "5h", "Oct 10, 2025"),
    ];

    let mut views = Vec::new();
    for (name, desc, hours, date) in tokens_data {
        let payload = format!("{}: {}", name, desc);
        let leaf = vault.place_leaf(payload.as_bytes(), LeafType::Token, now)?;

        // Store name in vault root metadata for lookup
        if let Some(artifact) = vault.get_artifact(&leaf.id)? {
            // Token is a leaf, name is in the payload
            views.push(TokenView {
                id: leaf.id,
                name: name.to_string(),
                description: desc.to_string(),
                hours: hours.to_string(),
                earned_date: date.to_string(),
                selected: false,
            });
        }
    }

    Ok(views)
}

/// Create a Request artifact from the draft fields and return a RequestView.
pub fn create_intention(
    vault: &mut InMemVault,
    title: &str,
    description: &str,
    location: &str,
    token_ids: &[ArtifactId],
    now: i64,
) -> Result<RequestView> {
    let player_id = *vault.player();
    let full_desc = format!("{}\n\n{}", title, description);
    let audience = vec![player_id];
    let request = Request::create(vault, &full_desc, audience, now)?;

    // Attach selected tokens as offers
    for token_id in token_ids {
        request.add_offer(vault, token_id.clone())?;
    }

    // Store location in the tree metadata
    if let Some(mut artifact) = vault.get_artifact(&request.id)? {
        if let Some(tree) = artifact.as_tree_mut() {
            tree.metadata.insert("title".to_string(), title.as_bytes().to_vec());
            tree.metadata.insert("location".to_string(), location.as_bytes().to_vec());
            vault.artifact_store_mut().put_artifact(&artifact)?;
        }
    }

    // Generate magic code
    let code = encounter::generate_intention_code(&player_id, &request.id);

    // Find token name for display
    let token_name = if let Some(first_id) = token_ids.first() {
        vault.get_payload(first_id)?
            .map(|p| String::from_utf8_lossy(&p).split(':').next().unwrap_or("Token").trim().to_string())
    } else {
        None
    };

    Ok(RequestView {
        id: request.id,
        title: title.to_string(),
        description: description.to_string(),
        location: location.to_string(),
        token_name,
        magic_code: Some(code),
    })
}

/// Simulate receiving a request as a provider (for demo/single-instance testing).
/// In a real P2P scenario, this would come over the network.
pub fn receive_request_as_provider(
    vault: &InMemVault,
    request_id: &ArtifactId,
) -> Result<Option<RequestView>> {
    let artifact = match vault.get_artifact(request_id)? {
        Some(a) => a,
        None => return Ok(None),
    };
    let tree = match artifact.as_tree() {
        Some(t) => t,
        None => return Ok(None),
    };

    let title = tree.metadata.get("title")
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    let location = tree.metadata.get("location")
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();

    // Get description from the description leaf
    let request = Request::from_id(request_id.clone());
    let description = if let Some(desc_artifact) = request.description(vault)? {
        if let Some(leaf) = desc_artifact.as_leaf() {
            vault.get_payload(&leaf.id)?
                .map(|p| String::from_utf8_lossy(&p).to_string())
                .unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Get first offer token name
    let offers = request.offers(vault)?;
    let token_name = offers.first().and_then(|(r, _)| {
        vault.get_payload(&r.artifact_id).ok().flatten()
            .map(|p| String::from_utf8_lossy(&p).split(':').next().unwrap_or("Token").trim().to_string())
    });

    Ok(Some(RequestView {
        id: request_id.clone(),
        title,
        description,
        location,
        token_name,
        magic_code: None,
    }))
}

/// Provider submits proof and proposes an exchange.
/// Returns the Exchange artifact ID.
pub fn submit_proof_and_propose(
    vault: &mut InMemVault,
    request_id: &ArtifactId,
    proof_title: &str,
    proof_description: &str,
    provider_id: PlayerId,
    now: i64,
) -> Result<ArtifactId> {
    let proof_text = format!("{}: {}", proof_title, proof_description);
    let proof = vault.place_leaf(proof_text.as_bytes(), LeafType::Attestation, now)?;

    // Get the first offered token from the request
    let request = Request::from_id(request_id.clone());
    let offers = request.offers(vault)?;
    let token_id = offers
        .first()
        .map(|(r, _)| r.artifact_id.clone())
        .ok_or(indras_artifacts::VaultError::ArtifactNotFound)?;

    let requester_id = *vault.player();
    let audience = vec![requester_id, provider_id];

    let exchange = Exchange::propose(
        vault,
        proof.id.clone(),
        token_id,
        audience,
        now,
    )?;

    // Provider accepts their side
    exchange.accept(vault)?;

    Ok(exchange.id)
}

/// Requester reviews and releases the token (accepts + completes the exchange).
pub fn release_token(
    vault: &mut InMemVault,
    exchange_id: &ArtifactId,
    now: i64,
) -> Result<()> {
    let exchange = Exchange::from_id(exchange_id.clone());
    exchange.accept(vault)?;
    exchange.complete(vault, now)?;
    Ok(())
}

/// Build an ExchangeView from an exchange artifact ID.
pub fn build_exchange_view(
    vault: &InMemVault,
    exchange_id: &ArtifactId,
    provider_name: &str,
) -> Result<ExchangeView> {
    let exchange = Exchange::from_id(exchange_id.clone());

    let offered = exchange.offered(vault)?;
    let proof_text = offered.as_ref().and_then(|a| {
        a.as_leaf().and_then(|leaf| {
            vault.get_payload(&leaf.id).ok().flatten()
                .map(|p| String::from_utf8_lossy(&p).to_string())
        })
    });

    let (proof_title, proof_description) = match proof_text {
        Some(text) => {
            let mut parts = text.splitn(2, ':');
            let title = parts.next().unwrap_or("").trim().to_string();
            let desc = parts.next().unwrap_or("").trim().to_string();
            (Some(title), Some(desc))
        }
        None => (None, None),
    };

    let requested = exchange.requested(vault)?;
    let token_name = requested.as_ref().and_then(|a| {
        a.as_leaf().and_then(|leaf| {
            vault.get_payload(&leaf.id).ok().flatten()
                .map(|p| String::from_utf8_lossy(&p).split(':').next().unwrap_or("Token").trim().to_string())
        })
    }).unwrap_or_else(|| "Token".to_string());

    let completed = exchange.is_closed(vault)?;

    Ok(ExchangeView {
        id: exchange_id.clone(),
        request_title: proof_title.clone().unwrap_or_else(|| "Exchange".to_string()),
        provider_name: provider_name.to_string(),
        proof_title,
        proof_description,
        token_name,
        completed,
    })
}
