//! Extension trait adding vault sync methods to Realm.

use crate::vault_document::VaultFileDocument;
use crate::vault_file::{ConflictRecord, VaultFile};
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::Realm;

/// Vault file sync extension trait for Realm.
#[allow(async_fn_in_trait)]
pub trait RealmVault {
    /// Get the vault file index document.
    async fn vault_index(&self) -> Result<Document<VaultFileDocument>>;
    /// Register or update a file in the vault index.
    async fn upsert_file(
        &self,
        path: &str,
        hash: [u8; 32],
        size: u64,
        author: MemberId,
    ) -> Result<()>;
    /// Mark a file as deleted (tombstone).
    async fn delete_file(&self, path: &str, author: MemberId) -> Result<()>;
    /// List all active (non-deleted) files.
    async fn list_files(&self) -> Result<Vec<VaultFile>>;
    /// List unresolved conflicts.
    async fn list_conflicts(&self) -> Result<Vec<ConflictRecord>>;
    /// Resolve a conflict.
    async fn resolve_conflict(&self, path: &str, loser_hash: [u8; 32]) -> Result<()>;
}

impl RealmVault for Realm {
    async fn vault_index(&self) -> Result<Document<VaultFileDocument>> {
        self.document::<VaultFileDocument>("vault-index").await
    }

    async fn upsert_file(
        &self,
        path: &str,
        hash: [u8; 32],
        size: u64,
        author: MemberId,
    ) -> Result<()> {
        let file = VaultFile::new(path, hash, size, author);
        let doc = self.vault_index().await?;
        doc.update(|d| {
            d.upsert(file);
        })
        .await
    }

    async fn delete_file(&self, path: &str, author: MemberId) -> Result<()> {
        let doc = self.vault_index().await?;
        doc.update(|d| {
            d.remove(path, author);
        })
        .await
    }

    async fn list_files(&self) -> Result<Vec<VaultFile>> {
        let doc = self.vault_index().await?;
        let files = doc.read().await.active_files().into_iter().cloned().collect();
        Ok(files)
    }

    async fn list_conflicts(&self) -> Result<Vec<ConflictRecord>> {
        let doc = self.vault_index().await?;
        let conflicts = doc
            .read()
            .await
            .unresolved_conflicts()
            .into_iter()
            .cloned()
            .collect();
        Ok(conflicts)
    }

    async fn resolve_conflict(&self, path: &str, loser_hash: [u8; 32]) -> Result<()> {
        let doc = self.vault_index().await?;
        doc.update(|d| {
            d.resolve_conflict(path, &loser_hash);
        })
        .await
    }
}
