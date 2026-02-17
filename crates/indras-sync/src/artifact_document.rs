//! Per-tree Automerge document wrapping AutoCommit.
//!
//! Stores artifact metadata, references, grants, and metadata key-value pairs
//! with CRDT semantics for conflict-free distributed sync.
//!
//! Document structure:
//! ```text
//! ROOT/
//!   artifact_id: String ("blob:hex" | "doc:hex")
//!   steward:     String (hex)
//!   artifact_type: String
//!   status:      String ("active" | "recalled:<ts>" | "transferred:<hex>:<ts>")
//!   created_at:  i64
//!   references:  List[Map{ artifact_id, position, label }]
//!   grants:      List[Map{ grantee, mode, granted_at, granted_by }]
//!   metadata:    Map{ key -> Bytes }
//! ```

use std::collections::BTreeMap;

use automerge::transaction::Transactable;
use automerge::{AutoCommit, ChangeHash, ObjId, ObjType, ReadDoc, ScalarValue, Value, ROOT};
use indras_artifacts::{
    AccessGrant, AccessMode, ArtifactId, ArtifactRef, ArtifactStatus, PlayerId, TreeType,
};

use crate::error::SyncError;

// ===== Schema key constants =====

mod keys {
    pub const ARTIFACT_ID: &str = "artifact_id";
    pub const STEWARD: &str = "steward";
    pub const ARTIFACT_TYPE: &str = "artifact_type";
    pub const STATUS: &str = "status";
    pub const CREATED_AT: &str = "created_at";
    pub const REFERENCES: &str = "references";
    pub const GRANTS: &str = "grants";
    pub const METADATA: &str = "metadata";

    // Reference map fields
    pub const REF_ARTIFACT_ID: &str = "artifact_id";
    pub const REF_POSITION: &str = "position";
    pub const REF_LABEL: &str = "label";

    // Grant map fields
    pub const GRANT_GRANTEE: &str = "grantee";
    pub const GRANT_MODE: &str = "mode";
    pub const GRANT_GRANTED_AT: &str = "granted_at";
    pub const GRANT_GRANTED_BY: &str = "granted_by";
}

// ===== Private encoding helpers =====

fn artifact_id_to_str(id: &ArtifactId) -> String {
    match id {
        ArtifactId::Blob(b) => format!("blob:{}", hex::encode(b)),
        ArtifactId::Doc(d) => format!("doc:{}", hex::encode(d)),
    }
}

fn str_to_artifact_id(s: &str) -> ArtifactId {
    if let Some(hex_part) = s.strip_prefix("blob:") {
        let bytes = hex::decode(hex_part).expect("invalid blob hex");
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        ArtifactId::Blob(arr)
    } else if let Some(hex_part) = s.strip_prefix("doc:") {
        let bytes = hex::decode(hex_part).expect("invalid doc hex");
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        ArtifactId::Doc(arr)
    } else {
        panic!("unknown artifact_id prefix in: {s}")
    }
}

fn tree_type_to_str(t: &TreeType) -> String {
    match t {
        TreeType::Vault => "vault".to_string(),
        TreeType::Story => "story".to_string(),
        TreeType::Gallery => "gallery".to_string(),
        TreeType::Document => "document".to_string(),
        TreeType::Request => "request".to_string(),
        TreeType::Exchange => "exchange".to_string(),
        TreeType::Collection => "collection".to_string(),
        TreeType::Inbox => "inbox".to_string(),
        TreeType::Quest => "quest".to_string(),
        TreeType::Need => "need".to_string(),
        TreeType::Offering => "offering".to_string(),
        TreeType::Intention => "intention".to_string(),
        TreeType::Custom(s) => format!("custom:{s}"),
    }
}

fn str_to_tree_type(s: &str) -> TreeType {
    match s {
        "vault" => TreeType::Vault,
        "story" => TreeType::Story,
        "gallery" => TreeType::Gallery,
        "document" => TreeType::Document,
        "request" => TreeType::Request,
        "exchange" => TreeType::Exchange,
        "collection" => TreeType::Collection,
        "inbox" => TreeType::Inbox,
        "quest" => TreeType::Quest,
        "need" => TreeType::Need,
        "offering" => TreeType::Offering,
        "intention" => TreeType::Intention,
        _ => {
            if let Some(custom) = s.strip_prefix("custom:") {
                TreeType::Custom(custom.to_string())
            } else {
                TreeType::Custom(s.to_string())
            }
        }
    }
}

fn access_mode_to_str(m: &AccessMode) -> String {
    match m {
        AccessMode::Revocable => "revocable".to_string(),
        AccessMode::Permanent => "permanent".to_string(),
        AccessMode::Timed { expires_at } => format!("timed:{expires_at}"),
        AccessMode::Transfer => "transfer".to_string(),
    }
}

fn str_to_access_mode(s: &str) -> AccessMode {
    match s {
        "revocable" => AccessMode::Revocable,
        "permanent" => AccessMode::Permanent,
        "transfer" => AccessMode::Transfer,
        _ => {
            if let Some(ts_str) = s.strip_prefix("timed:") {
                let expires_at: i64 = ts_str.parse().expect("invalid timed timestamp");
                AccessMode::Timed { expires_at }
            } else {
                AccessMode::Revocable
            }
        }
    }
}

fn status_to_str(s: &ArtifactStatus) -> String {
    match s {
        ArtifactStatus::Active => "active".to_string(),
        ArtifactStatus::Recalled { recalled_at } => format!("recalled:{recalled_at}"),
        ArtifactStatus::Transferred { to, transferred_at } => {
            format!("transferred:{}:{transferred_at}", hex::encode(to))
        }
    }
}

fn str_to_status(s: &str) -> ArtifactStatus {
    if s == "active" {
        return ArtifactStatus::Active;
    }
    if let Some(ts_str) = s.strip_prefix("recalled:") {
        let recalled_at: i64 = ts_str.parse().expect("invalid recalled timestamp");
        return ArtifactStatus::Recalled { recalled_at };
    }
    if let Some(rest) = s.strip_prefix("transferred:") {
        // format: "<hex>:<timestamp>"
        // hex is 64 chars (32 bytes), then colon, then timestamp
        let (hex_part, ts_part) = rest.split_at(64);
        let ts_str = ts_part.strip_prefix(':').expect("missing colon in transferred");
        let transferred_at: i64 = ts_str.parse().expect("invalid transferred timestamp");
        let bytes = hex::decode(hex_part).expect("invalid transferred hex");
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        return ArtifactStatus::Transferred {
            to: arr,
            transferred_at,
        };
    }
    ArtifactStatus::Active
}

// ===== Helper to extract string scalar from doc =====

fn get_string(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<String> {
    match doc.get(obj, key) {
        Ok(Some((Value::Scalar(cow), _))) => match cow.as_ref() {
            ScalarValue::Str(s) => Some(s.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn get_i64(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<i64> {
    match doc.get(obj, key) {
        Ok(Some((Value::Scalar(cow), _))) => match cow.as_ref() {
            ScalarValue::Int(n) => Some(*n),
            ScalarValue::Uint(n) => Some(*n as i64),
            _ => None,
        },
        _ => None,
    }
}

// ===== ArtifactDocument =====

/// Per-tree Automerge document wrapping `AutoCommit`.
///
/// Each artifact tree gets its own `ArtifactDocument` that tracks metadata,
/// references to child artifacts, access grants, and arbitrary metadata blobs.
/// CRDT semantics allow conflict-free concurrent edits across peers.
pub struct ArtifactDocument {
    doc: AutoCommit,
}

impl ArtifactDocument {
    // ===== Dynamic ObjId helpers =====
    // CRITICAL: Never cache ObjIds — they go stale after sync/merge.

    fn references_obj(&self) -> ObjId {
        self.doc
            .get(ROOT, keys::REFERENCES)
            .expect("references lookup failed")
            .expect("references list missing")
            .1
    }

    fn grants_obj(&self) -> ObjId {
        self.doc
            .get(ROOT, keys::GRANTS)
            .expect("grants lookup failed")
            .expect("grants list missing")
            .1
    }

    fn metadata_obj(&self) -> ObjId {
        self.doc
            .get(ROOT, keys::METADATA)
            .expect("metadata lookup failed")
            .expect("metadata map missing")
            .1
    }

    // ===== Construction / persistence =====

    /// Create a new document with pre-initialized schema.
    pub fn new(
        artifact_id: &ArtifactId,
        steward: &PlayerId,
        tree_type: &TreeType,
        now: i64,
    ) -> Self {
        let mut doc = AutoCommit::new();

        doc.put(ROOT, keys::ARTIFACT_ID, artifact_id_to_str(artifact_id))
            .expect("Failed to write artifact_id");
        doc.put(ROOT, keys::STEWARD, hex::encode(steward))
            .expect("Failed to write steward");
        doc.put(ROOT, keys::ARTIFACT_TYPE, tree_type_to_str(tree_type))
            .expect("Failed to write artifact_type");
        doc.put(ROOT, keys::STATUS, status_to_str(&ArtifactStatus::Active))
            .expect("Failed to write status");
        doc.put(ROOT, keys::CREATED_AT, now)
            .expect("Failed to write created_at");
        doc.put_object(ROOT, keys::REFERENCES, ObjType::List)
            .expect("Failed to create references list");
        doc.put_object(ROOT, keys::GRANTS, ObjType::List)
            .expect("Failed to create grants list");
        doc.put_object(ROOT, keys::METADATA, ObjType::Map)
            .expect("Failed to create metadata map");

        Self { doc }
    }

    /// Create an empty document shell with no pre-initialized schema.
    ///
    /// Used when bootstrapping a document from a received sync payload.
    /// The schema will be populated by `load_incremental()` from the sender's changes.
    pub fn empty() -> Self {
        Self {
            doc: AutoCommit::new(),
        }
    }

    /// Load a document from saved bytes.
    pub fn load(bytes: &[u8]) -> Result<Self, SyncError> {
        let doc =
            AutoCommit::load(bytes).map_err(|e| SyncError::DocumentLoad(e.to_string()))?;
        Ok(Self { doc })
    }

    /// Export the full document state as bytes.
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    /// Fork this document into an independent copy.
    pub fn fork(&mut self) -> Result<Self, SyncError> {
        let bytes = self.save();
        Self::load(&bytes)
    }

    // ===== Raw sync support =====

    /// Get the current change heads for incremental sync.
    pub fn get_heads(&mut self) -> Vec<ChangeHash> {
        self.doc.get_heads()
    }

    /// Export only the changes since the given heads.
    ///
    /// Returns bytes representing changes that happened after `heads`,
    /// suitable for sending to a peer that already has `heads`.
    pub fn save_after(&mut self, heads: &[ChangeHash]) -> Vec<u8> {
        self.doc.save_after(heads)
    }

    /// Apply incremental bytes from a peer.
    ///
    /// Returns the number of operations applied.
    pub fn load_incremental(&mut self, data: &[u8]) -> Result<usize, SyncError> {
        self.doc
            .load_incremental(data)
            .map_err(|e| SyncError::SyncMerge(e.to_string()))
    }

    // ===== References =====

    /// Append a reference to a child artifact at the end of the references list.
    pub fn append_ref(&mut self, child_id: &ArtifactId, position: u64, label: Option<&str>) {
        let refs = self.references_obj();
        let len = self.doc.length(&refs);
        let map_id = self
            .doc
            .insert_object(&refs, len, ObjType::Map)
            .expect("Failed to insert reference map");
        self.doc
            .put(&map_id, keys::REF_ARTIFACT_ID, artifact_id_to_str(child_id))
            .expect("Failed to write ref artifact_id");
        self.doc
            .put(&map_id, keys::REF_POSITION, position as i64)
            .expect("Failed to write ref position");
        if let Some(lbl) = label {
            self.doc
                .put(&map_id, keys::REF_LABEL, lbl)
                .expect("Failed to write ref label");
        }
    }

    /// Remove a reference by artifact_id (removes first match).
    pub fn remove_ref(&mut self, child_id: &ArtifactId) {
        let target = artifact_id_to_str(child_id);
        let refs = self.references_obj();
        let len = self.doc.length(&refs);
        for i in 0..len {
            if let Ok(Some((_, map_id))) = self.doc.get(&refs, i) {
                if let Some(id_str) = get_string(&self.doc, &map_id, keys::REF_ARTIFACT_ID) {
                    if id_str == target {
                        self.doc.delete(&refs, i).expect("Failed to delete reference");
                        return;
                    }
                }
            }
        }
    }

    /// Read all references from the list.
    pub fn references(&self) -> Vec<ArtifactRef> {
        let refs = self.references_obj();
        let len = self.doc.length(&refs);
        let mut out = Vec::new();
        for i in 0..len {
            if let Ok(Some((_, map_id))) = self.doc.get(&refs, i) {
                let id_str = match get_string(&self.doc, &map_id, keys::REF_ARTIFACT_ID) {
                    Some(s) => s,
                    None => continue,
                };
                let position = match get_i64(&self.doc, &map_id, keys::REF_POSITION) {
                    Some(p) => p as u64,
                    None => continue,
                };
                let label = get_string(&self.doc, &map_id, keys::REF_LABEL);
                out.push(ArtifactRef {
                    artifact_id: str_to_artifact_id(&id_str),
                    position,
                    label,
                });
            }
        }
        out
    }

    // ===== Grants =====

    /// Add an access grant to the grants list.
    pub fn add_grant(&mut self, grant: &AccessGrant) {
        let grants = self.grants_obj();
        let len = self.doc.length(&grants);
        let map_id = self
            .doc
            .insert_object(&grants, len, ObjType::Map)
            .expect("Failed to insert grant map");
        self.doc
            .put(&map_id, keys::GRANT_GRANTEE, hex::encode(grant.grantee))
            .expect("Failed to write grant grantee");
        self.doc
            .put(
                &map_id,
                keys::GRANT_MODE,
                access_mode_to_str(&grant.mode),
            )
            .expect("Failed to write grant mode");
        self.doc
            .put(&map_id, keys::GRANT_GRANTED_AT, grant.granted_at)
            .expect("Failed to write grant granted_at");
        self.doc
            .put(
                &map_id,
                keys::GRANT_GRANTED_BY,
                hex::encode(grant.granted_by),
            )
            .expect("Failed to write grant granted_by");
    }

    /// Remove a grant by grantee (removes first match).
    pub fn remove_grant(&mut self, grantee: &PlayerId) {
        let target = hex::encode(grantee);
        let grants = self.grants_obj();
        let len = self.doc.length(&grants);
        for i in 0..len {
            if let Ok(Some((_, map_id))) = self.doc.get(&grants, i) {
                if let Some(g) = get_string(&self.doc, &map_id, keys::GRANT_GRANTEE) {
                    if g == target {
                        self.doc.delete(&grants, i).expect("Failed to delete grant");
                        return;
                    }
                }
            }
        }
    }

    /// Read all grants from the list.
    pub fn grants(&self) -> Vec<AccessGrant> {
        let grants = self.grants_obj();
        let len = self.doc.length(&grants);
        let mut out = Vec::new();
        for i in 0..len {
            if let Ok(Some((_, map_id))) = self.doc.get(&grants, i) {
                let grantee_hex = match get_string(&self.doc, &map_id, keys::GRANT_GRANTEE) {
                    Some(s) => s,
                    None => continue,
                };
                let mode_str = match get_string(&self.doc, &map_id, keys::GRANT_MODE) {
                    Some(s) => s,
                    None => continue,
                };
                let granted_at = match get_i64(&self.doc, &map_id, keys::GRANT_GRANTED_AT) {
                    Some(t) => t,
                    None => continue,
                };
                let granted_by_hex = match get_string(&self.doc, &map_id, keys::GRANT_GRANTED_BY) {
                    Some(s) => s,
                    None => continue,
                };

                let grantee_bytes = hex::decode(&grantee_hex).expect("invalid grantee hex");
                let mut grantee = [0u8; 32];
                grantee.copy_from_slice(&grantee_bytes);

                let granted_by_bytes =
                    hex::decode(&granted_by_hex).expect("invalid granted_by hex");
                let mut granted_by = [0u8; 32];
                granted_by.copy_from_slice(&granted_by_bytes);

                out.push(AccessGrant {
                    grantee,
                    mode: str_to_access_mode(&mode_str),
                    granted_at,
                    granted_by,
                });
            }
        }
        out
    }

    // ===== Metadata =====

    /// Set a metadata key to raw bytes.
    pub fn set_metadata(&mut self, key: &str, value: &[u8]) {
        let meta = self.metadata_obj();
        self.doc
            .put(&meta, key, ScalarValue::Bytes(value.to_vec()))
            .expect("Failed to set metadata");
    }

    /// Get a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<Vec<u8>> {
        let meta = self.metadata_obj();
        match self.doc.get(&meta, key) {
            Ok(Some((Value::Scalar(cow), _))) => match cow.as_ref() {
                ScalarValue::Bytes(b) => Some(b.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Read all metadata as a sorted map.
    pub fn metadata(&self) -> BTreeMap<String, Vec<u8>> {
        let meta = self.metadata_obj();
        let mut out = BTreeMap::new();
        for key in self.doc.keys(&meta) {
            if let Some(val) = self.get_metadata(&key) {
                out.insert(key, val);
            }
        }
        out
    }

    // ===== Status / steward (scalar LWW registers) =====

    /// Read the artifact lifecycle status.
    pub fn status(&self) -> ArtifactStatus {
        get_string(&self.doc, &ROOT, keys::STATUS)
            .map(|s| str_to_status(&s))
            .unwrap_or(ArtifactStatus::Active)
    }

    /// Update the artifact lifecycle status.
    pub fn set_status(&mut self, status: &ArtifactStatus) {
        self.doc
            .put(ROOT, keys::STATUS, status_to_str(status))
            .expect("Failed to set status");
    }

    /// Read the current steward's PlayerId.
    pub fn steward(&self) -> PlayerId {
        let hex_str = get_string(&self.doc, &ROOT, keys::STEWARD)
            .expect("steward field missing");
        let bytes = hex::decode(&hex_str).expect("invalid steward hex");
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        arr
    }

    /// Update the steward (e.g., after a transfer).
    pub fn set_steward(&mut self, steward: &PlayerId) {
        self.doc
            .put(ROOT, keys::STEWARD, hex::encode(steward))
            .expect("Failed to set steward");
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    fn make_player(seed: u8) -> PlayerId {
        [seed; 32]
    }

    fn make_blob_id(seed: u8) -> ArtifactId {
        ArtifactId::Blob([seed; 32])
    }

    fn make_doc_id(seed: u8) -> ArtifactId {
        ArtifactId::Doc([seed; 32])
    }

    // ===== 1. Schema initialization on new() =====

    #[test]
    fn test_schema_initialization() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let doc = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Vault, 1000);

        // Status starts active
        assert!(matches!(doc.status(), ArtifactStatus::Active));

        // Steward round-trips
        assert_eq!(doc.steward(), steward);

        // Lists start empty
        assert!(doc.references().is_empty());
        assert!(doc.grants().is_empty());
        assert!(doc.metadata().is_empty());
    }

    // ===== 2. Append and read references =====

    #[test]
    fn test_append_and_read_references() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let mut doc = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Story, 1000);

        let child_a = make_blob_id(10);
        let child_b = make_doc_id(11);

        doc.append_ref(&child_a, 0, Some("intro"));
        doc.append_ref(&child_b, 1, None);

        let refs = doc.references();
        assert_eq!(refs.len(), 2);

        assert_eq!(refs[0].artifact_id, child_a);
        assert_eq!(refs[0].position, 0);
        assert_eq!(refs[0].label, Some("intro".to_string()));

        assert_eq!(refs[1].artifact_id, child_b);
        assert_eq!(refs[1].position, 1);
        assert_eq!(refs[1].label, None);
    }

    // ===== 3. Concurrent ref append: fork, append on both, merge via save_after/load_incremental =====

    #[test]
    fn test_concurrent_ref_append_both_present_after_merge() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let mut doc_a = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Gallery, 1000);

        // Fork to get a shared base
        let mut doc_b = doc_a.fork().unwrap();

        let heads_a = doc_a.get_heads();
        let heads_b = doc_b.get_heads();

        // Each side appends a different ref (simulating concurrent writes)
        let child_a = make_blob_id(10);
        let child_b = make_blob_id(20);

        doc_a.append_ref(&child_a, 0, Some("from-a"));
        doc_b.append_ref(&child_b, 1, Some("from-b"));

        // Exchange deltas
        let delta_a = doc_a.save_after(&heads_a);
        let delta_b = doc_b.save_after(&heads_b);

        doc_a.load_incremental(&delta_b).unwrap();
        doc_b.load_incremental(&delta_a).unwrap();

        // Both docs should converge and have both refs
        let refs_a = doc_a.references();
        let refs_b = doc_b.references();

        assert_eq!(refs_a.len(), 2, "doc_a should have 2 refs");
        assert_eq!(refs_b.len(), 2, "doc_b should have 2 refs");

        // Verify convergence (same heads)
        assert_eq!(doc_a.get_heads(), doc_b.get_heads());

        let ids_a: Vec<ArtifactId> = refs_a.iter().map(|r| r.artifact_id).collect();
        assert!(ids_a.contains(&child_a));
        assert!(ids_a.contains(&child_b));
    }

    // ===== 4. Add/remove grants with each AccessMode =====

    #[test]
    fn test_add_remove_grants_all_modes() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let mut doc = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Document, 1000);

        let grantee_a = make_player(10);
        let grantee_b = make_player(11);
        let grantee_c = make_player(12);
        let grantee_d = make_player(13);
        let grantor = make_player(99);

        doc.add_grant(&AccessGrant {
            grantee: grantee_a,
            mode: AccessMode::Revocable,
            granted_at: 100,
            granted_by: grantor,
        });
        doc.add_grant(&AccessGrant {
            grantee: grantee_b,
            mode: AccessMode::Permanent,
            granted_at: 101,
            granted_by: grantor,
        });
        doc.add_grant(&AccessGrant {
            grantee: grantee_c,
            mode: AccessMode::Timed { expires_at: 9999 },
            granted_at: 102,
            granted_by: grantor,
        });
        doc.add_grant(&AccessGrant {
            grantee: grantee_d,
            mode: AccessMode::Transfer,
            granted_at: 103,
            granted_by: grantor,
        });

        let grants = doc.grants();
        assert_eq!(grants.len(), 4);

        // Verify modes round-trip correctly
        let modes: Vec<&AccessMode> = grants.iter().map(|g| &g.mode).collect();
        assert!(modes.iter().any(|m| matches!(m, AccessMode::Revocable)));
        assert!(modes.iter().any(|m| matches!(m, AccessMode::Permanent)));
        assert!(modes
            .iter()
            .any(|m| matches!(m, AccessMode::Timed { expires_at: 9999 })));
        assert!(modes.iter().any(|m| matches!(m, AccessMode::Transfer)));

        // Remove one grant
        doc.remove_grant(&grantee_b);
        let grants_after = doc.grants();
        assert_eq!(grants_after.len(), 3);
        assert!(!grants_after.iter().any(|g| g.grantee == grantee_b));
    }

    // ===== 5. Metadata set/get, concurrent metadata merge (different keys) =====

    #[test]
    fn test_metadata_set_get() {
        let artifact_id = make_doc_id(1);
        let steward = make_player(2);
        let mut doc = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Vault, 1000);

        doc.set_metadata("mime", b"image/png");
        doc.set_metadata("thumb", b"\x89PNG\r\n");

        assert_eq!(doc.get_metadata("mime"), Some(b"image/png".to_vec()));
        assert_eq!(doc.get_metadata("thumb"), Some(b"\x89PNG\r\n".to_vec()));
        assert_eq!(doc.get_metadata("missing"), None);

        let all = doc.metadata();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("mime"));
        assert!(all.contains_key("thumb"));
    }

    #[test]
    fn test_concurrent_metadata_merge_different_keys() {
        let artifact_id = make_doc_id(1);
        let steward = make_player(2);
        let mut doc_a = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Vault, 1000);
        let mut doc_b = doc_a.fork().unwrap();

        let heads_a = doc_a.get_heads();
        let heads_b = doc_b.get_heads();

        // Each side writes a different metadata key
        doc_a.set_metadata("key-a", b"value-a");
        doc_b.set_metadata("key-b", b"value-b");

        let delta_a = doc_a.save_after(&heads_a);
        let delta_b = doc_b.save_after(&heads_b);

        doc_a.load_incremental(&delta_b).unwrap();
        doc_b.load_incremental(&delta_a).unwrap();

        // Both keys should be present after merge
        assert_eq!(doc_a.get_metadata("key-a"), Some(b"value-a".to_vec()));
        assert_eq!(doc_a.get_metadata("key-b"), Some(b"value-b".to_vec()));
        assert_eq!(doc_b.get_metadata("key-a"), Some(b"value-a".to_vec()));
        assert_eq!(doc_b.get_metadata("key-b"), Some(b"value-b".to_vec()));

        assert_eq!(doc_a.get_heads(), doc_b.get_heads());
    }

    // ===== 6. Status roundtrip (each variant) =====

    #[test]
    fn test_status_roundtrip_all_variants() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let mut doc = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Request, 1000);

        // Active (default)
        assert!(matches!(doc.status(), ArtifactStatus::Active));

        // Recalled
        doc.set_status(&ArtifactStatus::Recalled { recalled_at: 5000 });
        match doc.status() {
            ArtifactStatus::Recalled { recalled_at } => assert_eq!(recalled_at, 5000),
            other => panic!("Expected Recalled, got {other:?}"),
        }

        // Transferred
        let new_steward = make_player(99);
        doc.set_status(&ArtifactStatus::Transferred {
            to: new_steward,
            transferred_at: 6000,
        });
        match doc.status() {
            ArtifactStatus::Transferred { to, transferred_at } => {
                assert_eq!(to, new_steward);
                assert_eq!(transferred_at, 6000);
            }
            other => panic!("Expected Transferred, got {other:?}"),
        }

        // Back to active
        doc.set_status(&ArtifactStatus::Active);
        assert!(matches!(doc.status(), ArtifactStatus::Active));
    }

    // ===== 7. Save/load roundtrip with refs + grants + metadata =====

    #[test]
    fn test_save_load_roundtrip() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let mut doc = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Collection, 1000);

        let child = make_doc_id(5);
        doc.append_ref(&child, 0, Some("chapter-1"));

        let grantee = make_player(7);
        doc.add_grant(&AccessGrant {
            grantee,
            mode: AccessMode::Permanent,
            granted_at: 500,
            granted_by: steward,
        });

        doc.set_metadata("description", b"A collection of things");
        doc.set_status(&ArtifactStatus::Recalled { recalled_at: 2000 });

        let bytes = doc.save();
        let loaded = ArtifactDocument::load(&bytes).unwrap();

        // Verify refs
        let refs = loaded.references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].artifact_id, child);
        assert_eq!(refs[0].position, 0);
        assert_eq!(refs[0].label, Some("chapter-1".to_string()));

        // Verify grants
        let grants = loaded.grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].grantee, grantee);
        assert!(matches!(grants[0].mode, AccessMode::Permanent));

        // Verify metadata
        assert_eq!(
            loaded.get_metadata("description"),
            Some(b"A collection of things".to_vec())
        );

        // Verify status
        match loaded.status() {
            ArtifactStatus::Recalled { recalled_at } => assert_eq!(recalled_at, 2000),
            other => panic!("Expected Recalled, got {other:?}"),
        }
    }

    // ===== 8. save_after / load_incremental delta sync =====

    #[test]
    fn test_save_after_load_incremental_delta_sync() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let mut doc_sender =
            ArtifactDocument::new(&artifact_id, &steward, &TreeType::Inbox, 1000);

        // Receiver starts from the same base
        let mut doc_receiver = doc_sender.fork().unwrap();
        let receiver_heads = doc_receiver.get_heads();

        // Sender makes changes
        doc_sender.append_ref(&make_blob_id(10), 0, None);
        doc_sender.set_metadata("key", b"val");

        // Save only the delta since receiver's heads
        let delta = doc_sender.save_after(&receiver_heads);
        assert!(!delta.is_empty(), "delta should be non-empty");

        // Apply delta to receiver
        let ops_applied = doc_receiver.load_incremental(&delta).unwrap();
        assert!(ops_applied > 0, "should have applied some operations");

        // Receiver should now have the same state
        assert_eq!(doc_receiver.references().len(), 1);
        assert_eq!(
            doc_receiver.get_metadata("key"),
            Some(b"val".to_vec())
        );
    }

    // ===== 9. load_incremental idempotency (apply same bytes twice) =====

    #[test]
    fn test_load_incremental_idempotency() {
        let artifact_id = make_blob_id(1);
        let steward = make_player(2);
        let mut doc_a = ArtifactDocument::new(&artifact_id, &steward, &TreeType::Quest, 1000);
        let mut doc_b = doc_a.fork().unwrap();

        let heads_b = doc_b.get_heads();

        doc_a.append_ref(&make_blob_id(5), 0, Some("first"));
        let delta = doc_a.save_after(&heads_b);

        // Apply the same delta twice — should be idempotent
        doc_b.load_incremental(&delta).unwrap();
        doc_b.load_incremental(&delta).unwrap();

        // Should still have exactly 1 ref (not duplicated)
        let refs = doc_b.references();
        assert_eq!(refs.len(), 1, "idempotent: should not duplicate refs");
        assert_eq!(refs[0].label, Some("first".to_string()));
    }
}
