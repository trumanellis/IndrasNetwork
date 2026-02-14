use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{FileAttr, FileType};
use indras_artifacts::{LeafArtifact, LeafType, TreeArtifact, TreeType};

/// Infer LeafType from file extension.
pub fn leaf_type_from_extension(name: &str) -> LeafType {
    match name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" => LeafType::Image,
        "token" => LeafType::Token,
        "attestation" => LeafType::Attestation,
        _ => LeafType::File,
    }
}

/// Infer TreeType from directory name.
pub fn tree_type_from_name(name: &str) -> TreeType {
    match name.to_lowercase().as_str() {
        "stories" => TreeType::Story,
        "gallery" => TreeType::Gallery,
        "inbox" => TreeType::Inbox,
        "documents" => TreeType::Document,
        _ => TreeType::Collection,
    }
}

/// Convert a millisecond timestamp to SystemTime.
pub fn timestamp_to_system_time(ts: i64) -> SystemTime {
    if ts >= 0 {
        UNIX_EPOCH + Duration::from_millis(ts as u64)
    } else {
        UNIX_EPOCH
    }
}

/// Build FileAttr for a LeafArtifact.
pub fn leaf_to_fileattr(
    ino: u64,
    leaf: &LeafArtifact,
    uid: u32,
    gid: u32,
    perm: u16,
) -> FileAttr {
    let time = timestamp_to_system_time(leaf.created_at);
    FileAttr {
        ino,
        size: leaf.size,
        blocks: (leaf.size + 511) / 512,
        atime: time,
        mtime: time,
        ctime: time,
        crtime: time,
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

/// Build FileAttr for a TreeArtifact.
pub fn tree_to_fileattr(
    ino: u64,
    tree: &TreeArtifact,
    uid: u32,
    gid: u32,
    perm: u16,
) -> FileAttr {
    let time = timestamp_to_system_time(tree.created_at);
    FileAttr {
        ino,
        size: 0,
        blocks: 0,
        atime: time,
        mtime: time,
        ctime: time,
        crtime: time,
        kind: FileType::Directory,
        perm,
        nlink: 2 + tree.references.len() as u32,
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
    fn test_leaf_type_from_extension() {
        assert_eq!(leaf_type_from_extension("photo.jpg"), LeafType::Image);
        assert_eq!(leaf_type_from_extension("photo.PNG"), LeafType::Image);
        assert_eq!(leaf_type_from_extension("doc.md"), LeafType::File);
        assert_eq!(leaf_type_from_extension("gift.token"), LeafType::Token);
        assert_eq!(
            leaf_type_from_extension("proof.attestation"),
            LeafType::Attestation
        );
        assert_eq!(leaf_type_from_extension("noext"), LeafType::File);
    }

    #[test]
    fn test_tree_type_from_name() {
        assert_eq!(tree_type_from_name("stories"), TreeType::Story);
        assert_eq!(tree_type_from_name("Stories"), TreeType::Story);
        assert_eq!(tree_type_from_name("gallery"), TreeType::Gallery);
        assert_eq!(tree_type_from_name("inbox"), TreeType::Inbox);
        assert_eq!(tree_type_from_name("documents"), TreeType::Document);
        assert_eq!(tree_type_from_name("notes"), TreeType::Collection);
        assert_eq!(tree_type_from_name("random"), TreeType::Collection);
    }

    #[test]
    fn test_timestamp_to_system_time() {
        let st = timestamp_to_system_time(1000);
        assert_eq!(st, UNIX_EPOCH + Duration::from_millis(1000));
        assert_eq!(timestamp_to_system_time(-1), UNIX_EPOCH);
    }
}
