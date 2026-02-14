use std::collections::HashMap;
use std::time::SystemTime;

use fuser::{FileAttr, FileType};
use indras_artifacts::{ArtifactId, LeafType, TreeType};

// Reserved inode numbers for top-level structure.
pub const ROOT_INO: u64 = 1;
pub const VAULT_INO: u64 = 2;
pub const PEERS_INO: u64 = 3;
pub const REALMS_INO: u64 = 4;
pub const DOT_INDRA_INO: u64 = 5;

// Reserved inode numbers for virtual files in .indra/.
pub const ATTENTION_LOG_INO: u64 = 6;
pub const HEAT_JSON_INO: u64 = 7;
pub const PEERS_JSON_INO: u64 = 8;
pub const PLAYER_JSON_INO: u64 = 9;

pub const FIRST_DYNAMIC_INO: u64 = 10;

#[derive(Debug, Clone)]
pub enum VirtualFileType {
    AttentionLog,
    HeatJson,
    PeersJson,
    PlayerJson,
}

#[derive(Debug, Clone)]
pub enum InodeKind {
    /// Maps to a LeafArtifact.
    File { leaf_type: LeafType },
    /// Maps to a TreeArtifact (or a virtual directory).
    Directory { tree_type: Option<TreeType> },
    /// Virtual file generated on read (.indra/ contents).
    Virtual { vtype: VirtualFileType },
}

#[derive(Debug, Clone)]
pub struct InodeEntry {
    pub artifact_id: Option<ArtifactId>,
    pub parent_inode: u64,
    pub name: String,
    pub kind: InodeKind,
    pub attr: FileAttr,
}

/// Maps inode numbers to artifact metadata and vice versa.
pub struct InodeTable {
    entries: HashMap<u64, InodeEntry>,
    artifact_to_inode: HashMap<ArtifactId, u64>,
    next_inode: u64,
    lookup_counts: HashMap<u64, u64>,
}

impl InodeTable {
    pub fn new(uid: u32, gid: u32) -> Self {
        let mut table = Self {
            entries: HashMap::new(),
            artifact_to_inode: HashMap::new(),
            next_inode: FIRST_DYNAMIC_INO,
            lookup_counts: HashMap::new(),
        };

        let now = SystemTime::now();

        // Root: /indra/
        table.insert_reserved(
            ROOT_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: ROOT_INO,
                name: String::new(),
                kind: InodeKind::Directory { tree_type: None },
                attr: dir_attr(ROOT_INO, uid, gid, now, 0o755),
            },
        );

        // /indra/vault/
        table.insert_reserved(
            VAULT_INO,
            InodeEntry {
                artifact_id: None, // Set when vault is loaded
                parent_inode: ROOT_INO,
                name: "vault".into(),
                kind: InodeKind::Directory {
                    tree_type: Some(TreeType::Vault),
                },
                attr: dir_attr(VAULT_INO, uid, gid, now, 0o755),
            },
        );

        // /indra/peers/
        table.insert_reserved(
            PEERS_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: ROOT_INO,
                name: "peers".into(),
                kind: InodeKind::Directory { tree_type: None },
                attr: dir_attr(PEERS_INO, uid, gid, now, 0o555),
            },
        );

        // /indra/realms/
        table.insert_reserved(
            REALMS_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: ROOT_INO,
                name: "realms".into(),
                kind: InodeKind::Directory { tree_type: None },
                attr: dir_attr(REALMS_INO, uid, gid, now, 0o555),
            },
        );

        // /indra/.indra/
        table.insert_reserved(
            DOT_INDRA_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: ROOT_INO,
                name: ".indra".into(),
                kind: InodeKind::Directory { tree_type: None },
                attr: dir_attr(DOT_INDRA_INO, uid, gid, now, 0o555),
            },
        );

        // Virtual files in .indra/
        table.insert_reserved(
            ATTENTION_LOG_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: DOT_INDRA_INO,
                name: "attention.log".into(),
                kind: InodeKind::Virtual {
                    vtype: VirtualFileType::AttentionLog,
                },
                attr: file_attr(ATTENTION_LOG_INO, 0, uid, gid, now, 0o444),
            },
        );

        table.insert_reserved(
            HEAT_JSON_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: DOT_INDRA_INO,
                name: "heat.json".into(),
                kind: InodeKind::Virtual {
                    vtype: VirtualFileType::HeatJson,
                },
                attr: file_attr(HEAT_JSON_INO, 0, uid, gid, now, 0o444),
            },
        );

        table.insert_reserved(
            PEERS_JSON_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: DOT_INDRA_INO,
                name: "peers.json".into(),
                kind: InodeKind::Virtual {
                    vtype: VirtualFileType::PeersJson,
                },
                attr: file_attr(PEERS_JSON_INO, 0, uid, gid, now, 0o444),
            },
        );

        table.insert_reserved(
            PLAYER_JSON_INO,
            InodeEntry {
                artifact_id: None,
                parent_inode: DOT_INDRA_INO,
                name: "player.json".into(),
                kind: InodeKind::Virtual {
                    vtype: VirtualFileType::PlayerJson,
                },
                attr: file_attr(PLAYER_JSON_INO, 0, uid, gid, now, 0o444),
            },
        );

        table
    }

    fn insert_reserved(&mut self, ino: u64, entry: InodeEntry) {
        self.entries.insert(ino, entry);
        self.lookup_counts.insert(ino, u64::MAX); // Never evict reserved inodes
    }

    pub fn get(&self, ino: u64) -> Option<&InodeEntry> {
        self.entries.get(&ino)
    }

    pub fn get_mut(&mut self, ino: u64) -> Option<&mut InodeEntry> {
        self.entries.get_mut(&ino)
    }

    pub fn get_by_artifact(&self, id: &ArtifactId) -> Option<u64> {
        self.artifact_to_inode.get(id).copied()
    }

    /// Allocate a new inode for the given entry. Returns the assigned inode number.
    /// Sets `entry.attr.ino` to the correct value before inserting.
    pub fn allocate(&mut self, mut entry: InodeEntry) -> u64 {
        // Check if this artifact already has an inode
        if let Some(ref aid) = entry.artifact_id {
            if let Some(existing) = self.artifact_to_inode.get(aid) {
                let ino = *existing;
                self.inc_lookup(ino);
                return ino;
            }
        }

        let ino = self.next_inode;
        self.next_inode += 1;

        entry.attr.ino = ino;

        if let Some(ref aid) = entry.artifact_id {
            self.artifact_to_inode.insert(aid.clone(), ino);
        }

        self.entries.insert(ino, entry);
        self.lookup_counts.insert(ino, 1);
        ino
    }

    pub fn inc_lookup(&mut self, ino: u64) {
        if let Some(count) = self.lookup_counts.get_mut(&ino) {
            if *count < u64::MAX {
                *count += 1;
            }
        }
    }

    /// Decrement lookup count. Evict dynamic inodes when count reaches zero.
    pub fn forget(&mut self, ino: u64, nlookup: u64) {
        if let Some(count) = self.lookup_counts.get_mut(&ino) {
            if *count == u64::MAX {
                return; // Reserved inode, never evict
            }
            *count = count.saturating_sub(nlookup);
            if *count == 0 {
                if let Some(entry) = self.entries.remove(&ino) {
                    if let Some(ref aid) = entry.artifact_id {
                        self.artifact_to_inode.remove(aid);
                    }
                }
                self.lookup_counts.remove(&ino);
            }
        }
    }

    /// Find all children of a given parent inode.
    pub fn children_of(&self, parent_ino: u64) -> Vec<(u64, &InodeEntry)> {
        self.entries
            .iter()
            .filter(|(ino, e)| e.parent_inode == parent_ino && **ino != parent_ino)
            .map(|(ino, e)| (*ino, e))
            .collect()
    }

    /// Find a specific child by name under a parent.
    pub fn find_child(&self, parent_ino: u64, name: &str) -> Option<(u64, &InodeEntry)> {
        self.entries
            .iter()
            .find(|(ino, e)| e.parent_inode == parent_ino && **ino != parent_ino && e.name == name)
            .map(|(ino, e)| (*ino, e))
    }

    /// Remove an inode entry entirely.
    pub fn remove(&mut self, ino: u64) -> Option<InodeEntry> {
        if let Some(entry) = self.entries.remove(&ino) {
            if let Some(ref aid) = entry.artifact_id {
                self.artifact_to_inode.remove(aid);
            }
            self.lookup_counts.remove(&ino);
            Some(entry)
        } else {
            None
        }
    }

    /// Update the ArtifactId for an existing inode (after content-addressed flush).
    pub fn update_artifact_id(&mut self, ino: u64, new_id: ArtifactId) {
        if let Some(entry) = self.entries.get_mut(&ino) {
            if let Some(ref old_id) = entry.artifact_id {
                self.artifact_to_inode.remove(old_id);
            }
            self.artifact_to_inode.insert(new_id.clone(), ino);
            entry.artifact_id = Some(new_id);
        }
    }
}

/// Create a FileAttr for a directory.
pub fn dir_attr(ino: u64, uid: u32, gid: u32, now: SystemTime, perm: u16) -> FileAttr {
    FileAttr {
        ino,
        size: 0,
        blocks: 0,
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: FileType::Directory,
        perm,
        nlink: 2,
        uid,
        gid,
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}

/// Create a FileAttr for a regular file.
pub fn file_attr(ino: u64, size: u64, uid: u32, gid: u32, now: SystemTime, perm: u16) -> FileAttr {
    FileAttr {
        ino,
        size,
        blocks: (size + 511) / 512,
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: FileType::RegularFile,
        perm,
        nlink: 1,
        uid,
        gid,
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reserved_inodes_exist() {
        let table = InodeTable::new(501, 20);
        assert!(table.get(ROOT_INO).is_some());
        assert!(table.get(VAULT_INO).is_some());
        assert!(table.get(PEERS_INO).is_some());
        assert!(table.get(REALMS_INO).is_some());
        assert!(table.get(DOT_INDRA_INO).is_some());
        assert!(table.get(ATTENTION_LOG_INO).is_some());
        assert!(table.get(HEAT_JSON_INO).is_some());
        assert!(table.get(PEERS_JSON_INO).is_some());
        assert!(table.get(PLAYER_JSON_INO).is_some());
    }

    #[test]
    fn test_allocate_sequential() {
        let mut table = InodeTable::new(501, 20);
        let now = SystemTime::now();

        let ino1 = table.allocate(InodeEntry {
            artifact_id: None,
            parent_inode: VAULT_INO,
            name: "test1".into(),
            kind: InodeKind::File {
                leaf_type: LeafType::File,
            },
            attr: file_attr(0, 100, 501, 20, now, 0o644),
        });
        assert_eq!(ino1, FIRST_DYNAMIC_INO);

        let ino2 = table.allocate(InodeEntry {
            artifact_id: None,
            parent_inode: VAULT_INO,
            name: "test2".into(),
            kind: InodeKind::File {
                leaf_type: LeafType::File,
            },
            attr: file_attr(0, 200, 501, 20, now, 0o644),
        });
        assert_eq!(ino2, FIRST_DYNAMIC_INO + 1);
    }

    #[test]
    fn test_allocate_dedup_artifact() {
        let mut table = InodeTable::new(501, 20);
        let now = SystemTime::now();
        let aid = indras_artifacts::ArtifactId::Blob([42u8; 32]);

        let ino1 = table.allocate(InodeEntry {
            artifact_id: Some(aid.clone()),
            parent_inode: VAULT_INO,
            name: "file.md".into(),
            kind: InodeKind::File {
                leaf_type: LeafType::File,
            },
            attr: file_attr(0, 100, 501, 20, now, 0o644),
        });

        // Same artifact ID should return the same inode
        let ino2 = table.allocate(InodeEntry {
            artifact_id: Some(aid),
            parent_inode: VAULT_INO,
            name: "file.md".into(),
            kind: InodeKind::File {
                leaf_type: LeafType::File,
            },
            attr: file_attr(0, 100, 501, 20, now, 0o644),
        });

        assert_eq!(ino1, ino2);
    }

    #[test]
    fn test_forget_evicts() {
        let mut table = InodeTable::new(501, 20);
        let now = SystemTime::now();

        let ino = table.allocate(InodeEntry {
            artifact_id: None,
            parent_inode: VAULT_INO,
            name: "temp".into(),
            kind: InodeKind::File {
                leaf_type: LeafType::File,
            },
            attr: file_attr(0, 50, 501, 20, now, 0o644),
        });

        assert!(table.get(ino).is_some());
        table.forget(ino, 1);
        assert!(table.get(ino).is_none());
    }

    #[test]
    fn test_forget_reserved_never_evicts() {
        let mut table = InodeTable::new(501, 20);
        table.forget(ROOT_INO, 1);
        assert!(table.get(ROOT_INO).is_some());
    }

    #[test]
    fn test_find_child() {
        let table = InodeTable::new(501, 20);
        let (ino, entry) = table.find_child(ROOT_INO, "vault").unwrap();
        assert_eq!(ino, VAULT_INO);
        assert_eq!(entry.name, "vault");
    }

    #[test]
    fn test_children_of_root() {
        let table = InodeTable::new(501, 20);
        let children = table.children_of(ROOT_INO);
        let names: Vec<&str> = children.iter().map(|(_, e)| e.name.as_str()).collect();
        assert!(names.contains(&"vault"));
        assert!(names.contains(&"peers"));
        assert!(names.contains(&"realms"));
        assert!(names.contains(&".indra"));
    }

    #[test]
    fn test_update_artifact_id() {
        let mut table = InodeTable::new(501, 20);
        let now = SystemTime::now();
        let old_id = indras_artifacts::ArtifactId::Blob([1u8; 32]);
        let new_id = indras_artifacts::ArtifactId::Blob([2u8; 32]);

        let ino = table.allocate(InodeEntry {
            artifact_id: Some(old_id.clone()),
            parent_inode: VAULT_INO,
            name: "file.md".into(),
            kind: InodeKind::File {
                leaf_type: LeafType::File,
            },
            attr: file_attr(0, 100, 501, 20, now, 0o644),
        });

        assert_eq!(table.get_by_artifact(&old_id), Some(ino));
        table.update_artifact_id(ino, new_id.clone());
        assert_eq!(table.get_by_artifact(&old_id), None);
        assert_eq!(table.get_by_artifact(&new_id), Some(ino));
    }
}
