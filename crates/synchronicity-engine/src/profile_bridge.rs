//! Bridge between the profile UI and the identity/visibility CRDTs.
//!
//! - `ProfileIdentityDocument` holds user-edited fields (display name, bio).
//! - The home-realm artifact index holds per-field visibility grants that
//!   determine whether each homepage field is Public, ConnectionsOnly, or
//!   Private. This module exposes small async helpers so the SE profile
//!   modal can load+edit both without depending on gift-cycle.

use std::collections::HashMap;
use std::sync::Arc;

use indras_artifacts::{AccessGrant, AccessMode, ArtifactId, ArtifactStatus};
use indras_homepage::{fields, profile_field_artifact_id};
use indras_network::IndrasNetwork;
use indras_network::artifact_index::HomeArtifactEntry;
use indras_sync_engine::{HomepageProfileDocument, ProfileIdentityDocument};

use crate::profile_mirror::peer_profile_doc_key;

/// Document key used for the user's profile identity CRDT in the home realm.
const DOC_KEY: &str = "_profile_identity";

/// All profile field names surfaced on the homepage + visibility UI.
///
/// Ordered for presentation: user-provided first, then derived stats.
pub const ALL_FIELDS: &[&str] = &[
    fields::DISPLAY_NAME,
    fields::USERNAME,
    fields::BIO,
    fields::PUBLIC_KEY,
    fields::INTENTION_COUNT,
    fields::TOKEN_COUNT,
    fields::BLESSINGS_GIVEN,
    fields::ATTENTION_CONTRIBUTED,
    fields::CONTACT_COUNT,
    fields::HUMANNESS_FRESHNESS,
    fields::ACTIVE_QUESTS,
    fields::ACTIVE_OFFERINGS,
];

/// Visibility state for a profile field, derived from its grant list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldVisibility {
    /// Anyone can read this field (single `Public` grant).
    Public,
    /// Only current contacts can read (revocable grants per contact).
    ConnectionsOnly,
    /// No grants — nobody else can read.
    Private,
}

/// Info about a single non-public grant on a profile field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldGrantInfo {
    /// Grantee member id.
    pub grantee: [u8; 32],
    /// Human-readable grantee name (contact's display name, or hex prefix fallback).
    pub grantee_name: String,
    /// Access mode label ("Revocable", "Timed (expires …)", etc.).
    pub mode_label: String,
}

/// Full per-field visibility snapshot for the profile UI.
///
/// Bundles the field name, its current display value, the derived visibility
/// level, and the list of specific non-public grantees so the modal can
/// render an expandable "who has access" list with revoke buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileFieldVisibility {
    /// Field name constant (e.g. `fields::BIO`).
    pub field_name: &'static str,
    /// Human-readable label for the field ("Bio", "Display Name", …).
    pub display_label: &'static str,
    /// Current display value (empty string if unknown).
    pub display_value: String,
    /// Derived visibility level.
    pub visibility: FieldVisibility,
    /// Specific non-public grants (e.g. per-contact when ConnectionsOnly).
    pub specific_grants: Vec<FieldGrantInfo>,
}

/// Human-readable label for a field name.
pub fn field_label(name: &str) -> &'static str {
    match name {
        fields::DISPLAY_NAME => "Display Name",
        fields::USERNAME => "Username",
        fields::BIO => "Bio",
        fields::PUBLIC_KEY => "Public Key",
        fields::INTENTION_COUNT => "Intention Count",
        fields::TOKEN_COUNT => "Token Count",
        fields::BLESSINGS_GIVEN => "Blessings Given",
        fields::ATTENTION_CONTRIBUTED => "Attention Contributed",
        fields::CONTACT_COUNT => "Contact Count",
        fields::HUMANNESS_FRESHNESS => "Humanness Freshness",
        fields::ACTIVE_QUESTS => "Active Quests",
        fields::ACTIVE_OFFERINGS => "Active Offerings",
        _ => "Unknown",
    }
}

/// Current epoch seconds for LWW merge ordering.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Slugify a display name into a default username.
fn slugify(name: &str) -> String {
    name.trim().to_lowercase().replace(' ', "-")
}

/// Load the persisted profile identity from the home realm, if any.
pub async fn load_profile_identity(
    network: &Arc<IndrasNetwork>,
) -> Option<ProfileIdentityDocument> {
    let home = network.home_realm().await.ok()?;
    let doc = home.document::<ProfileIdentityDocument>(DOC_KEY).await.ok()?;
    let guard = doc.read().await;
    if guard.updated_at == 0 && guard.display_name.is_empty() {
        return None;
    }
    Some(ProfileIdentityDocument {
        display_name: guard.display_name.clone(),
        username: guard.username.clone(),
        bio: guard.bio.clone(),
        public_key: guard.public_key.clone(),
        updated_at: guard.updated_at,
    })
}

/// Persist a new display name into the profile identity document.
pub async fn save_display_name(network: &Arc<IndrasNetwork>, name: String) {
    let Some(doc) = identity_doc(network).await else { return };
    let public_key_hex = hex_member_id(network);
    let now = now_secs();
    let res = doc
        .update(move |d| {
            d.display_name = name.clone();
            if d.username.is_empty() {
                d.username = slugify(&name);
            }
            d.public_key = public_key_hex;
            d.updated_at = now;
        })
        .await;
    if let Err(e) = res {
        tracing::warn!("profile display_name update failed: {e}");
    }
}

/// Persist a new username into the profile identity document.
pub async fn save_username(network: &Arc<IndrasNetwork>, username: String) {
    let Some(doc) = identity_doc(network).await else { return };
    let now = now_secs();
    let cleaned = slugify(&username);
    let res = doc
        .update(move |d| {
            d.username = cleaned.clone();
            d.updated_at = now;
        })
        .await;
    if let Err(e) = res {
        tracing::warn!("profile username update failed: {e}");
    }
}

/// Persist a new bio into the profile identity document.
pub async fn save_bio(network: &Arc<IndrasNetwork>, bio: String) {
    let Some(doc) = identity_doc(network).await else { return };
    let now = now_secs();
    let trimmed = bio.trim().to_string();
    let res = doc
        .update(move |d| {
            d.bio = if trimmed.is_empty() { None } else { Some(trimmed) };
            d.updated_at = now;
        })
        .await;
    if let Err(e) = res {
        tracing::warn!("profile bio update failed: {e}");
    }
}

/// Seed the artifact index with an entry per homepage field so the
/// visibility toggles have something to write to.
///
/// Default visibility for each field is `Public`, matching the gift-cycle
/// app's onboarding behavior. Existing entries are left untouched.
pub async fn ensure_profile_artifacts(network: &Arc<IndrasNetwork>) {
    let Some(index) = artifact_index(network).await else { return };
    let member_id = network.id();
    let res = index
        .update(move |idx| {
            for field_name in ALL_FIELDS {
                let aid = field_artifact_id(&member_id, field_name);
                if idx.get(&aid).is_some() {
                    continue;
                }
                let entry = HomeArtifactEntry {
                    id: aid,
                    name: format!("profile:{field_name}"),
                    mime_type: Some("application/x-indras-profile-field".to_string()),
                    size: 0,
                    created_at: 0,
                    encrypted_key: None,
                    status: ArtifactStatus::Active,
                    grants: vec![AccessGrant {
                        grantee: [0u8; 32],
                        mode: AccessMode::Public,
                        granted_at: 0,
                        granted_by: member_id,
                    }],
                    provenance: None,
                    location: None,
                };
                idx.store(entry);
            }
        })
        .await;
    if let Err(e) = res {
        tracing::warn!("profile artifact seed failed: {e}");
    }
}

/// Read full visibility snapshot for every profile field, including
/// per-grantee info for the UI's expandable grant lists. Unknown / missing
/// entries report as `Private` with empty grants.
pub async fn list_field_visibilities(
    network: &Arc<IndrasNetwork>,
) -> Vec<ProfileFieldVisibility> {
    let contact_names = current_contact_names(network).await;
    let homepage_values = homepage_field_values(network).await;
    let Some(index) = artifact_index(network).await else {
        return ALL_FIELDS
            .iter()
            .map(|f| empty_visibility(f, &homepage_values))
            .collect();
    };
    let member_id = network.id();
    let guard = index.read().await;
    ALL_FIELDS
        .iter()
        .map(|field_name| {
            let aid = field_artifact_id(&member_id, field_name);
            let (visibility, specific_grants) = match guard.get(&aid) {
                Some(entry) => (
                    classify_grants(&entry.grants),
                    describe_specific_grants(&entry.grants, &contact_names),
                ),
                None => (FieldVisibility::Private, Vec::new()),
            };
            ProfileFieldVisibility {
                field_name: *field_name,
                display_label: field_label(field_name),
                display_value: homepage_values.get(*field_name).cloned().unwrap_or_default(),
                visibility,
                specific_grants,
            }
        })
        .collect()
}

/// Revoke a single grantee's access to a profile field.
pub async fn revoke_field_access(
    network: &Arc<IndrasNetwork>,
    field_name: &str,
    grantee: [u8; 32],
) {
    let Some(index) = artifact_index(network).await else { return };
    let member_id = network.id();
    let aid = field_artifact_id(&member_id, field_name);
    let res = index
        .update(move |idx| {
            if let Some(entry) = idx.get(&aid) {
                let grants: Vec<AccessGrant> = entry
                    .grants
                    .iter()
                    .filter(|g| g.grantee != grantee)
                    .cloned()
                    .collect();
                idx.replace_grants(&aid, grants);
            }
        })
        .await;
    if let Err(e) = res {
        tracing::warn!("profile grant revoke failed: {e}");
    }
}

/// Set a field to Public — single `Public` grant with null grantee.
pub async fn set_field_public(network: &Arc<IndrasNetwork>, field_name: &str) {
    let Some(index) = artifact_index(network).await else { return };
    let member_id = network.id();
    let aid = field_artifact_id(&member_id, field_name);
    let grants = vec![AccessGrant {
        grantee: [0u8; 32],
        mode: AccessMode::Public,
        granted_at: 0,
        granted_by: member_id,
    }];
    write_grants(&index, aid, grants).await;
}

/// Set a field to ConnectionsOnly — revocable grant for each current contact.
pub async fn set_field_connections_only(network: &Arc<IndrasNetwork>, field_name: &str) {
    let Some(index) = artifact_index(network).await else { return };
    let member_id = network.id();
    let aid = field_artifact_id(&member_id, field_name);
    let contacts = current_contacts(network).await;
    let grants: Vec<AccessGrant> = contacts
        .into_iter()
        .filter(|c| *c != member_id && *c != [0u8; 32])
        .map(|grantee| AccessGrant {
            grantee,
            mode: AccessMode::Revocable,
            granted_at: 0,
            granted_by: member_id,
        })
        .collect();
    write_grants(&index, aid, grants).await;
}

/// Set a field to Private — empty grant list.
pub async fn set_field_private(network: &Arc<IndrasNetwork>, field_name: &str) {
    let Some(index) = artifact_index(network).await else { return };
    let aid = field_artifact_id(&network.id(), field_name);
    write_grants(&index, aid, Vec::new()).await;
}

/// Load a peer's mirrored identity from a shared DM realm.
///
/// Returns `None` if the peer hasn't published a mirror into this realm
/// yet. The returned doc may have empty fields where the peer hasn't
/// granted us visibility — the caller treats empty strings / `bio == None`
/// as "not shared."
pub async fn load_peer_profile_from_dm(
    network: &Arc<IndrasNetwork>,
    peer_id: [u8; 32],
    dm_realm_id: [u8; 32],
) -> Option<ProfileIdentityDocument> {
    let realm_id = indras_network::RealmId::new(dm_realm_id);
    let realm = network.get_realm_by_id(&realm_id)?;
    let key = peer_profile_doc_key(&peer_id);
    let doc = realm.document::<ProfileIdentityDocument>(&key).await.ok()?;
    let snap = doc.read().await.clone();
    if snap.updated_at == 0 {
        None
    } else {
        Some(snap)
    }
}

// ---------- internals ----------

async fn identity_doc(
    network: &Arc<IndrasNetwork>,
) -> Option<indras_network::document::Document<ProfileIdentityDocument>> {
    let home = match network.home_realm().await {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("home_realm unavailable for profile: {e}");
            return None;
        }
    };
    match home.document::<ProfileIdentityDocument>(DOC_KEY).await {
        Ok(d) => Some(d),
        Err(e) => {
            tracing::warn!("profile document unavailable: {e}");
            None
        }
    }
}

async fn artifact_index(
    network: &Arc<IndrasNetwork>,
) -> Option<indras_network::document::Document<indras_network::artifact_index::ArtifactIndex>> {
    let home = network.home_realm().await.ok()?;
    home.artifact_index().await.ok()
}

async fn current_contacts(network: &Arc<IndrasNetwork>) -> Vec<[u8; 32]> {
    let Some(contacts_realm) = network.contacts_realm().await else {
        return Vec::new();
    };
    let Ok(cdoc) = contacts_realm.contacts().await else {
        return Vec::new();
    };
    let data = cdoc.read().await;
    data.contacts.keys().copied().collect()
}

/// Read current contacts with their display names for grantee labeling.
async fn current_contact_names(network: &Arc<IndrasNetwork>) -> HashMap<[u8; 32], String> {
    let Some(contacts_realm) = network.contacts_realm().await else {
        return HashMap::new();
    };
    let Ok(cdoc) = contacts_realm.contacts().await else {
        return HashMap::new();
    };
    let data = cdoc.read().await;
    data.contacts
        .iter()
        .map(|(id, entry)| (*id, entry.display_name.clone().unwrap_or_default()))
        .collect()
}

/// Snapshot the current homepage display values keyed by field name.
async fn homepage_field_values(network: &Arc<IndrasNetwork>) -> HashMap<String, String> {
    let Ok(home) = network.home_realm().await else { return HashMap::new() };
    let Ok(doc) = home.document::<HomepageProfileDocument>("_homepage_profile").await else {
        return HashMap::new();
    };
    let guard = doc.read().await;
    guard
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.value.clone()))
        .collect()
}

fn empty_visibility(
    field_name: &&'static str,
    homepage_values: &HashMap<String, String>,
) -> ProfileFieldVisibility {
    ProfileFieldVisibility {
        field_name: *field_name,
        display_label: field_label(field_name),
        display_value: homepage_values
            .get(*field_name)
            .cloned()
            .unwrap_or_default(),
        visibility: FieldVisibility::Private,
        specific_grants: Vec::new(),
    }
}

fn describe_specific_grants(
    grants: &[AccessGrant],
    contact_names: &HashMap<[u8; 32], String>,
) -> Vec<FieldGrantInfo> {
    grants
        .iter()
        .filter(|g| g.grantee != [0u8; 32] && !matches!(g.mode, AccessMode::Public))
        .map(|g| {
            let name = contact_names
                .get(&g.grantee)
                .filter(|n| !n.is_empty())
                .cloned()
                .unwrap_or_else(|| {
                    g.grantee
                        .iter()
                        .take(4)
                        .map(|b| format!("{b:02x}"))
                        .collect()
                });
            let mode_label = match &g.mode {
                AccessMode::Revocable => "Revocable".to_string(),
                AccessMode::Timed { expires_at } => format!("Timed (expires {expires_at})"),
                AccessMode::Permanent => "Permanent".to_string(),
                AccessMode::Transfer => "Transfer".to_string(),
                AccessMode::Public => "Public".to_string(),
            };
            FieldGrantInfo {
                grantee: g.grantee,
                grantee_name: name,
                mode_label,
            }
        })
        .collect()
}

async fn write_grants(
    index: &indras_network::document::Document<indras_network::artifact_index::ArtifactIndex>,
    aid: ArtifactId,
    grants: Vec<AccessGrant>,
) {
    let res = index
        .update(move |idx| {
            idx.replace_grants(&aid, grants.clone());
        })
        .await;
    if let Err(e) = res {
        tracing::warn!("profile grants update failed: {e}");
    }
}

fn field_artifact_id(member_id: &[u8; 32], field_name: &str) -> ArtifactId {
    ArtifactId::Doc(profile_field_artifact_id(member_id, field_name))
}

fn classify_grants(grants: &[AccessGrant]) -> FieldVisibility {
    if grants.is_empty() {
        return FieldVisibility::Private;
    }
    if grants.iter().any(|g| matches!(g.mode, AccessMode::Public)) {
        return FieldVisibility::Public;
    }
    FieldVisibility::ConnectionsOnly
}

fn hex_member_id(network: &Arc<IndrasNetwork>) -> String {
    network.id().iter().map(|b| format!("{b:02x}")).collect()
}
