use std::collections::HashMap;
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};

use fuser::{
    FileAttr, FileType, Filesystem, KernelConfig, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite,
    Request as FuseRequest,
};
use indras_artifacts::{
    Artifact, ArtifactId, ArtifactStore, AttentionStore, LeafType, PayloadStore, Vault,
};
use tracing::{debug, error, warn};

use crate::attention;
use crate::inode::{
    file_attr, InodeEntry, InodeKind, InodeTable, VirtualFileType, ROOT_INO, VAULT_INO,
};
use crate::mapping::{leaf_to_fileattr, leaf_type_from_extension, tree_to_fileattr, tree_type_from_name};
use crate::permissions::{dir_perm_for_steward, file_perm_for_steward, OWNER_FILE_PERM};
use crate::virtual_files::{
    generate_attention_log, generate_heat_json, generate_peers_json, generate_player_json,
};
use crate::write_buffer::WriteBuffer;

/// TTL for FUSE reply caching.
const TTL: Duration = Duration::from_secs(1);

/// An open file handle.
#[derive(Debug)]
pub struct OpenFile {
    /// The inode this handle refers to.
    pub inode: u64,
    /// The artifact backing this file, if any.
    pub artifact_id: Option<ArtifactId>,
    /// Parent inode (for composing on flush).
    pub parent_inode: u64,
    /// Open flags (O_RDONLY, O_WRONLY, O_RDWR).
    pub flags: i32,
    /// Write buffer accumulating changes before flush.
    pub write_buffer: Option<WriteBuffer>,
}

/// The IndraFS FUSE filesystem, backed by an artifact Vault.
pub struct IndraFS<A: ArtifactStore, P: PayloadStore, T: AttentionStore> {
    pub vault: Vault<A, P, T>,
    pub inodes: InodeTable,
    pub open_files: HashMap<u64, OpenFile>,
    pub next_fh: u64,
    pub uid: u32,
    pub gid: u32,
}

impl<A: ArtifactStore, P: PayloadStore, T: AttentionStore> IndraFS<A, P, T> {
    pub fn new(vault: Vault<A, P, T>, uid: u32, gid: u32) -> Self {
        let mut inodes = InodeTable::new(uid, gid);

        // Link the vault root artifact to the VAULT_INO inode.
        let vault_root_id = vault.root.id.clone();
        inodes.update_artifact_id(VAULT_INO, vault_root_id);

        Self {
            vault,
            inodes,
            open_files: HashMap::new(),
            next_fh: 1,
            uid,
            gid,
        }
    }

    /// Allocate the next file handle.
    fn alloc_fh(&mut self) -> u64 {
        let fh = self.next_fh;
        self.next_fh += 1;
        fh
    }

    /// Get the current timestamp in milliseconds.
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    /// Generate virtual file content by type.
    fn generate_virtual_content(&self, vtype: &VirtualFileType) -> Vec<u8> {
        match vtype {
            VirtualFileType::AttentionLog => generate_attention_log(&self.vault),
            VirtualFileType::HeatJson => {
                let now = self.now_ms();
                let paths = self.collect_artifact_paths();
                generate_heat_json(&self.vault, &paths, now)
            }
            VirtualFileType::PeersJson => generate_peers_json(&self.vault),
            VirtualFileType::PlayerJson => generate_player_json(self.vault.player()),
        }
    }

    /// Collect (path, ArtifactId) pairs from the inode table for heat computation.
    fn collect_artifact_paths(&self) -> Vec<(String, ArtifactId)> {
        let mut paths = Vec::new();
        self.collect_paths_recursive(VAULT_INO, "vault".to_string(), &mut paths);
        paths
    }

    fn collect_paths_recursive(
        &self,
        parent_ino: u64,
        prefix: String,
        paths: &mut Vec<(String, ArtifactId)>,
    ) {
        for (_, entry) in self.inodes.children_of(parent_ino) {
            let child_path = format!("{}/{}", prefix, entry.name);
            if let Some(ref aid) = entry.artifact_id {
                paths.push((child_path.clone(), aid.clone()));
            }
            if matches!(entry.kind, InodeKind::Directory { .. }) {
                // Only recurse into known artifact directories to avoid infinite loops
                if entry.artifact_id.is_some() {
                    self.collect_paths_recursive(entry.attr.ino, child_path, paths);
                }
            }
        }
    }

    /// Flush an open file's dirty write buffer to the vault.
    /// Returns Ok(()) on success or the error code on failure.
    fn flush_write_buffer(&mut self, fh: u64) -> Result<(), i32> {
        // Extract what we need from the open file without holding a borrow.
        let (inode, parent_inode, data, artifact_id) = {
            let of = self.open_files.get(&fh).ok_or(libc::EBADF)?;
            let wb = match of.write_buffer.as_ref() {
                Some(wb) if wb.is_dirty() => wb,
                _ => return Ok(()),
            };
            (
                of.inode,
                of.parent_inode,
                wb.data().to_vec(),
                of.artifact_id.clone(),
            )
        };

        let now = self.now_ms();

        // Determine the leaf type from the inode's name.
        let leaf_type = self
            .inodes
            .get(inode)
            .map(|e| leaf_type_from_extension(&e.name))
            .unwrap_or(LeafType::File);

        // Store payload and place leaf.
        let leaf = self
            .vault
            .place_leaf(&data, leaf_type, now)
            .map_err(|e| {
                error!("place_leaf failed: {e}");
                libc::EIO
            })?;

        let new_artifact_id = leaf.id.clone();

        // Get parent's artifact_id for composing.
        let parent_artifact_id = self
            .inodes
            .get(parent_inode)
            .and_then(|e| e.artifact_id.clone())
            .ok_or_else(|| {
                error!("parent inode {parent_inode} has no artifact_id");
                libc::EIO
            })?;

        // If there was a previous artifact, remove the old ref from parent.
        if let Some(ref old_id) = artifact_id {
            let _ = self.vault.remove_ref(&parent_artifact_id, old_id);
        }

        // Compute next position for the compose.
        let next_pos = self
            .vault
            .get_artifact(&parent_artifact_id)
            .ok()
            .flatten()
            .and_then(|a| a.as_tree().cloned())
            .map(|t| t.references.iter().map(|r| r.position).max().unwrap_or(0) + 1)
            .unwrap_or(0);

        // Get the label from the inode.
        let label = self.inodes.get(inode).map(|e| e.name.clone());

        // Compose new leaf into parent tree.
        self.vault
            .compose(
                &parent_artifact_id,
                new_artifact_id.clone(),
                next_pos,
                label,
            )
            .map_err(|e| {
                error!("compose failed: {e}");
                libc::EIO
            })?;

        // Update the inode's artifact_id and size.
        self.inodes.update_artifact_id(inode, new_artifact_id.clone());
        if let Some(entry) = self.inodes.get_mut(inode) {
            entry.attr.size = data.len() as u64;
            entry.attr.blocks = (data.len() as u64 + 511) / 512;
            entry.attr.mtime = SystemTime::now();
        }

        // Update the open file.
        if let Some(of) = self.open_files.get_mut(&fh) {
            of.artifact_id = Some(new_artifact_id);
            if let Some(ref mut wb) = of.write_buffer {
                wb.mark_clean();
            }
        }

        Ok(())
    }

    /// Look up a child artifact by label in a tree artifact's references.
    fn lookup_artifact_child(
        &mut self,
        parent_ino: u64,
        parent_artifact_id: &ArtifactId,
        name: &str,
    ) -> Option<(u64, FileAttr)> {
        let tree = self
            .vault
            .get_artifact(parent_artifact_id)
            .ok()?
            .and_then(|a| a.as_tree().cloned())?;

        let aref = tree
            .references
            .iter()
            .find(|r| r.label.as_deref() == Some(name))?;

        let child_artifact_id = aref.artifact_id.clone();

        // Check if we already have an inode for this artifact.
        if let Some(existing_ino) = self.inodes.get_by_artifact(&child_artifact_id) {
            self.inodes.inc_lookup(existing_ino);
            let attr = self.inodes.get(existing_ino)?.attr;
            return Some((existing_ino, attr));
        }

        // Fetch the child artifact to determine its type.
        let child_artifact = self.vault.get_artifact(&child_artifact_id).ok()??;
        let player = *self.vault.player();

        match child_artifact {
            Artifact::Leaf(ref leaf) => {
                let perm = file_perm_for_steward(&leaf.steward, &player);
                let attr = leaf_to_fileattr(0, leaf, self.uid, self.gid, perm);
                let ino = self.inodes.allocate(InodeEntry {
                    artifact_id: Some(child_artifact_id),
                    parent_inode: parent_ino,
                    name: name.to_string(),
                    kind: InodeKind::File {
                        leaf_type: leaf.artifact_type.clone(),
                    },
                    attr,
                });
                let final_attr = self.inodes.get(ino)?.attr;
                Some((ino, final_attr))
            }
            Artifact::Tree(ref tree) => {
                let perm = dir_perm_for_steward(&tree.steward, &player);
                let attr = tree_to_fileattr(0, tree, self.uid, self.gid, perm);
                let ino = self.inodes.allocate(InodeEntry {
                    artifact_id: Some(child_artifact_id),
                    parent_inode: parent_ino,
                    name: name.to_string(),
                    kind: InodeKind::Directory {
                        tree_type: Some(tree.artifact_type.clone()),
                    },
                    attr,
                });
                let final_attr = self.inodes.get(ino)?.attr;
                Some((ino, final_attr))
            }
        }
    }

    /// List children of an artifact directory from the vault, allocating inodes as needed.
    fn populate_artifact_children(&mut self, parent_ino: u64, parent_artifact_id: &ArtifactId) {
        let tree = match self.vault.get_artifact(parent_artifact_id) {
            Ok(Some(artifact)) => match artifact.as_tree() {
                Some(t) => t.clone(),
                None => return,
            },
            _ => return,
        };

        let player = *self.vault.player();

        for aref in &tree.references {
            let label = match aref.label.as_deref() {
                Some(l) => l.to_string(),
                None => format!("{}", aref.artifact_id),
            };

            // Skip if already allocated.
            if self.inodes.find_child(parent_ino, &label).is_some() {
                continue;
            }

            let child = match self.vault.get_artifact(&aref.artifact_id) {
                Ok(Some(a)) => a,
                _ => continue,
            };

            match child {
                Artifact::Leaf(ref leaf) => {
                    let perm = file_perm_for_steward(&leaf.steward, &player);
                    let attr = leaf_to_fileattr(0, leaf, self.uid, self.gid, perm);
                    self.inodes.allocate(InodeEntry {
                        artifact_id: Some(aref.artifact_id.clone()),
                        parent_inode: parent_ino,
                        name: label,
                        kind: InodeKind::File {
                            leaf_type: leaf.artifact_type.clone(),
                        },
                        attr,
                    });
                }
                Artifact::Tree(ref tree_art) => {
                    let perm = dir_perm_for_steward(&tree_art.steward, &player);
                    let attr = tree_to_fileattr(0, tree_art, self.uid, self.gid, perm);
                    self.inodes.allocate(InodeEntry {
                        artifact_id: Some(aref.artifact_id.clone()),
                        parent_inode: parent_ino,
                        name: label,
                        kind: InodeKind::Directory {
                            tree_type: Some(tree_art.artifact_type.clone()),
                        },
                        attr,
                    });
                }
            }
        }
    }
}

impl<A: ArtifactStore, P: PayloadStore, T: AttentionStore> Filesystem for IndraFS<A, P, T> {
    fn init(
        &mut self,
        _req: &FuseRequest<'_>,
        _config: &mut KernelConfig,
    ) -> Result<(), libc::c_int> {
        tracing::info!("IndraFS mounted");
        Ok(())
    }

    fn lookup(&mut self, _req: &FuseRequest<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        debug!("lookup: parent={parent} name={name_str:?}");

        // Check if this child is already in the inode table.
        if let Some((ino, entry)) = self.inodes.find_child(parent, name_str) {
            let attr = entry.attr;
            self.inodes.inc_lookup(ino);
            reply.entry(&TTL, &attr, 0);
            return;
        }

        // For artifact-backed directories, look up the child in the vault.
        if let Some(parent_aid) = self.inodes.get(parent).and_then(|e| e.artifact_id.clone()) {
            if let Some((_ino, attr)) = self.lookup_artifact_child(parent, &parent_aid, name_str) {
                reply.entry(&TTL, &attr, 0);
                return;
            }
        }

        reply.error(libc::ENOENT);
    }

    fn forget(&mut self, _req: &FuseRequest<'_>, ino: u64, nlookup: u64) {
        debug!("forget: ino={ino} nlookup={nlookup}");
        self.inodes.forget(ino, nlookup);
    }

    fn getattr(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        _fh: Option<u64>,
        reply: ReplyAttr,
    ) {
        debug!("getattr: ino={ino}");
        match self.inodes.get(ino) {
            Some(entry) => reply.attr(&TTL, &entry.attr),
            None => reply.error(libc::ENOENT),
        }
    }

    fn setattr(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        debug!("setattr: ino={ino} size={size:?}");

        // Handle truncation.
        if let Some(new_size) = size {
            // If there's an open file handle with a write buffer, truncate it.
            if let Some(fh_val) = fh {
                if let Some(of) = self.open_files.get_mut(&fh_val) {
                    if let Some(ref mut wb) = of.write_buffer {
                        wb.truncate(new_size as usize);
                    }
                }
            }

            if let Some(entry) = self.inodes.get_mut(ino) {
                entry.attr.size = new_size;
                entry.attr.blocks = (new_size + 511) / 512;
                entry.attr.mtime = SystemTime::now();
            }
        }

        match self.inodes.get(ino) {
            Some(entry) => reply.attr(&TTL, &entry.attr),
            None => reply.error(libc::ENOENT),
        }
    }

    fn readdir(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        debug!("readdir: ino={ino} offset={offset}");

        let parent_ino = self
            .inodes
            .get(ino)
            .map(|e| e.parent_inode)
            .unwrap_or(ROOT_INO);

        // For artifact-backed directories, populate children from vault first.
        if let Some(aid) = self.inodes.get(ino).and_then(|e| e.artifact_id.clone()) {
            self.populate_artifact_children(ino, &aid);
        }

        let mut entries: Vec<(u64, FileType, String)> = Vec::new();

        // Always include . and ..
        entries.push((ino, FileType::Directory, ".".to_string()));
        entries.push((parent_ino, FileType::Directory, "..".to_string()));

        // Gather children from the inode table.
        let children = self.inodes.children_of(ino);
        for (child_ino, child_entry) in children {
            let ft = child_entry.attr.kind;
            entries.push((child_ino, ft, child_entry.name.clone()));
        }

        // Apply offset-based pagination.
        for (i, (entry_ino, kind, name)) in entries.iter().enumerate().skip(offset as usize) {
            // reply.add returns true when the buffer is full.
            if reply.add(*entry_ino, (i + 1) as i64, *kind, name) {
                break;
            }
        }

        reply.ok();
    }

    fn opendir(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        _flags: i32,
        reply: ReplyOpen,
    ) {
        debug!("opendir: ino={ino}");
        match self.inodes.get(ino) {
            Some(entry) if matches!(entry.kind, InodeKind::Directory { .. }) => {
                let fh = self.alloc_fh();
                reply.opened(fh, 0);
            }
            Some(_) => reply.error(libc::ENOTDIR),
            None => reply.error(libc::ENOENT),
        }
    }

    fn releasedir(
        &mut self,
        _req: &FuseRequest<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn open(&mut self, _req: &FuseRequest<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        debug!("open: ino={ino} flags={flags}");

        let entry = match self.inodes.get(ino) {
            Some(e) => e.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Directories should use opendir.
        if matches!(entry.kind, InodeKind::Directory { .. }) {
            reply.error(libc::EISDIR);
            return;
        }

        let is_writing = (flags & libc::O_ACCMODE) == libc::O_WRONLY
            || (flags & libc::O_ACCMODE) == libc::O_RDWR;

        // For writable opens on real artifacts, preload existing payload into write buffer.
        let write_buffer = if is_writing {
            if let Some(ref aid) = entry.artifact_id {
                match self.vault.get_payload(aid) {
                    Ok(Some(bytes)) => Some(WriteBuffer::from_existing(bytes.to_vec())),
                    _ => Some(WriteBuffer::new()),
                }
            } else {
                Some(WriteBuffer::new())
            }
        } else {
            None
        };

        let fh = self.alloc_fh();
        self.open_files.insert(
            fh,
            OpenFile {
                inode: ino,
                artifact_id: entry.artifact_id.clone(),
                parent_inode: entry.parent_inode,
                flags,
                write_buffer,
            },
        );

        // Fire attention event.
        if let Some(ref aid) = entry.artifact_id {
            let now = self.now_ms();
            attention::on_open(&mut self.vault, aid, now);
        }

        reply.opened(fh, 0);
    }

    fn read(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        debug!("read: ino={ino} fh={fh} offset={offset} size={size}");

        let offset = offset.max(0) as usize;
        let size = size as usize;

        // Check the open file for a write buffer first.
        if let Some(of) = self.open_files.get(&fh) {
            if let Some(ref wb) = of.write_buffer {
                let data = wb.read_at(offset, size);
                reply.data(data);
                return;
            }
        }

        // Check if this is a virtual file.
        if let Some(entry) = self.inodes.get(ino) {
            if let InodeKind::Virtual { ref vtype } = entry.kind {
                let content = self.generate_virtual_content(vtype);
                if offset >= content.len() {
                    reply.data(&[]);
                } else {
                    let end = (offset + size).min(content.len());
                    reply.data(&content[offset..end]);
                }
                return;
            }
        }

        // Read from vault payload.
        let artifact_id = self
            .open_files
            .get(&fh)
            .and_then(|of| of.artifact_id.clone())
            .or_else(|| {
                self.inodes
                    .get(ino)
                    .and_then(|e| e.artifact_id.clone())
            });

        match artifact_id {
            Some(ref aid) => match self.vault.get_payload(aid) {
                Ok(Some(bytes)) => {
                    if offset >= bytes.len() {
                        reply.data(&[]);
                    } else {
                        let end = (offset + size).min(bytes.len());
                        reply.data(&bytes[offset..end]);
                    }
                }
                Ok(None) => {
                    // Payload not available (lazy loaded / not yet fetched).
                    reply.data(&[]);
                }
                Err(e) => {
                    error!("get_payload failed: {e}");
                    reply.error(libc::EIO);
                }
            },
            None => {
                reply.data(&[]);
            }
        }
    }

    fn write(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        debug!("write: ino={ino} fh={fh} offset={offset} len={}", data.len());

        let of = match self.open_files.get_mut(&fh) {
            Some(of) => of,
            None => {
                reply.error(libc::EBADF);
                return;
            }
        };

        let wb = of.write_buffer.get_or_insert_with(WriteBuffer::new);
        let written = wb.write_at(offset.max(0) as usize, data);

        // Update the inode size if the buffer grew.
        let new_len = wb.len() as u64;
        if let Some(entry) = self.inodes.get_mut(ino) {
            if new_len > entry.attr.size {
                entry.attr.size = new_len;
                entry.attr.blocks = (new_len + 511) / 512;
            }
        }

        reply.written(written as u32);
    }

    fn create(
        &mut self,
        _req: &FuseRequest<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        debug!("create: parent={parent} name={name_str:?}");

        // Verify parent is a writable directory with an artifact.
        let parent_entry = match self.inodes.get(parent) {
            Some(e) => e.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if !matches!(parent_entry.kind, InodeKind::Directory { .. }) {
            reply.error(libc::ENOTDIR);
            return;
        }

        // Check for name collision.
        if self.inodes.find_child(parent, name_str).is_some() {
            reply.error(libc::EEXIST);
            return;
        }

        let now = SystemTime::now();
        let leaf_type = leaf_type_from_extension(name_str);

        // Allocate inode with no artifact yet (created on flush).
        let ino = self.inodes.allocate(InodeEntry {
            artifact_id: None,
            parent_inode: parent,
            name: name_str.to_string(),
            kind: InodeKind::File { leaf_type },
            attr: file_attr(0, 0, self.uid, self.gid, now, OWNER_FILE_PERM),
        });

        let attr = self.inodes.get(ino).unwrap().attr;

        let fh = self.alloc_fh();
        self.open_files.insert(
            fh,
            OpenFile {
                inode: ino,
                artifact_id: None,
                parent_inode: parent,
                flags,
                write_buffer: Some(WriteBuffer::new()),
            },
        );

        reply.created(&TTL, &attr, 0, fh, 0);
    }

    fn flush(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        debug!("flush: ino={ino} fh={fh}");

        match self.flush_write_buffer(fh) {
            Ok(()) => reply.ok(),
            Err(errno) => reply.error(errno),
        }
    }

    fn release(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        debug!("release: ino={ino} fh={fh}");

        // Flush if dirty.
        if let Some(of) = self.open_files.get(&fh) {
            if of
                .write_buffer
                .as_ref()
                .is_some_and(|wb| wb.is_dirty())
            {
                if let Err(errno) = self.flush_write_buffer(fh) {
                    warn!("release flush failed for ino={ino}: errno={errno}");
                }
            }
        }

        // Fire attention release event.
        if let Some(of) = self.open_files.get(&fh) {
            // Navigate back to parent on release.
            let parent_aid = self
                .inodes
                .get(of.parent_inode)
                .and_then(|e| e.artifact_id.clone());
            if let Some(ref pid) = parent_aid {
                let now = self.now_ms();
                attention::on_release(&mut self.vault, pid, now);
            }
        }

        self.open_files.remove(&fh);
        reply.ok();
    }

    fn mkdir(
        &mut self,
        _req: &FuseRequest<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        debug!("mkdir: parent={parent} name={name_str:?}");

        let parent_entry = match self.inodes.get(parent) {
            Some(e) => e.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if !matches!(parent_entry.kind, InodeKind::Directory { .. }) {
            reply.error(libc::ENOTDIR);
            return;
        }

        let parent_artifact_id = match parent_entry.artifact_id {
            Some(ref aid) => aid.clone(),
            None => {
                reply.error(libc::EACCES);
                return;
            }
        };

        // Check for name collision.
        if self.inodes.find_child(parent, name_str).is_some() {
            reply.error(libc::EEXIST);
            return;
        }

        let now = self.now_ms();
        let tree_type = tree_type_from_name(name_str);
        let player = *self.vault.player();

        // Create tree artifact in the vault.
        let tree = match self.vault.place_tree(tree_type.clone(), vec![player], now) {
            Ok(t) => t,
            Err(e) => {
                error!("place_tree failed: {e}");
                reply.error(libc::EIO);
                return;
            }
        };

        let tree_id = tree.id.clone();

        // Compute next position.
        let next_pos = self
            .vault
            .get_artifact(&parent_artifact_id)
            .ok()
            .flatten()
            .and_then(|a| a.as_tree().cloned())
            .map(|t| {
                t.references
                    .iter()
                    .map(|r| r.position)
                    .max()
                    .unwrap_or(0)
                    + 1
            })
            .unwrap_or(0);

        // Compose into parent tree.
        if let Err(e) = self.vault.compose(
            &parent_artifact_id,
            tree_id.clone(),
            next_pos,
            Some(name_str.to_string()),
        ) {
            error!("compose failed: {e}");
            reply.error(libc::EIO);
            return;
        }

        let perm = dir_perm_for_steward(&player, &player);
        let attr = tree_to_fileattr(0, &tree, self.uid, self.gid, perm);

        let ino = self.inodes.allocate(InodeEntry {
            artifact_id: Some(tree_id),
            parent_inode: parent,
            name: name_str.to_string(),
            kind: InodeKind::Directory {
                tree_type: Some(tree_type),
            },
            attr,
        });

        let final_attr = self.inodes.get(ino).unwrap().attr;
        reply.entry(&TTL, &final_attr, 0);
    }

    fn unlink(
        &mut self,
        _req: &FuseRequest<'_>,
        parent: u64,
        name: &OsStr,
        reply: ReplyEmpty,
    ) {
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        debug!("unlink: parent={parent} name={name_str:?}");

        let (child_ino, child_entry) = match self.inodes.find_child(parent, name_str) {
            Some((ino, entry)) => (ino, entry.clone()),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Cannot unlink directories.
        if matches!(child_entry.kind, InodeKind::Directory { .. }) {
            reply.error(libc::EISDIR);
            return;
        }

        // Remove ref from parent tree in vault.
        if let (Some(ref parent_aid), Some(ref child_aid)) = (
            self.inodes.get(parent).and_then(|e| e.artifact_id.clone()),
            child_entry.artifact_id.as_ref(),
        ) {
            if let Err(e) = self.vault.remove_ref(parent_aid, child_aid) {
                error!("remove_ref failed: {e}");
                reply.error(libc::EIO);
                return;
            }
        }

        self.inodes.remove(child_ino);
        reply.ok();
    }

    fn rmdir(
        &mut self,
        _req: &FuseRequest<'_>,
        parent: u64,
        name: &OsStr,
        reply: ReplyEmpty,
    ) {
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        debug!("rmdir: parent={parent} name={name_str:?}");

        let (child_ino, child_entry) = match self.inodes.find_child(parent, name_str) {
            Some((ino, entry)) => (ino, entry.clone()),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if !matches!(child_entry.kind, InodeKind::Directory { .. }) {
            reply.error(libc::ENOTDIR);
            return;
        }

        // Verify the directory tree is empty.
        if let Some(ref child_aid) = child_entry.artifact_id {
            match self.vault.get_artifact(child_aid) {
                Ok(Some(artifact)) => {
                    if let Some(tree) = artifact.as_tree() {
                        if !tree.references.is_empty() {
                            reply.error(libc::ENOTEMPTY);
                            return;
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    error!("get_artifact failed: {e}");
                    reply.error(libc::EIO);
                    return;
                }
            }
        }

        // Also check for inode-level children (newly created but not yet flushed).
        if !self.inodes.children_of(child_ino).is_empty() {
            reply.error(libc::ENOTEMPTY);
            return;
        }

        // Remove ref from parent tree.
        if let (Some(ref parent_aid), Some(ref child_aid)) = (
            self.inodes.get(parent).and_then(|e| e.artifact_id.clone()),
            child_entry.artifact_id.as_ref(),
        ) {
            if let Err(e) = self.vault.remove_ref(parent_aid, child_aid) {
                error!("remove_ref failed: {e}");
                reply.error(libc::EIO);
                return;
            }
        }

        self.inodes.remove(child_ino);
        reply.ok();
    }

    fn rename(
        &mut self,
        _req: &FuseRequest<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        let old_name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };
        let new_name = match newname.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        debug!("rename: parent={parent} name={old_name:?} newparent={newparent} newname={new_name:?}");

        let (child_ino, child_entry) = match self.inodes.find_child(parent, old_name) {
            Some((ino, entry)) => (ino, entry.clone()),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let child_artifact_id = match child_entry.artifact_id {
            Some(ref aid) => aid.clone(),
            None => {
                // File with no artifact (newly created, not flushed) - just rename the inode.
                if let Some(entry) = self.inodes.get_mut(child_ino) {
                    entry.name = new_name.to_string();
                    entry.parent_inode = newparent;
                }
                reply.ok();
                return;
            }
        };

        let old_parent_aid = match self.inodes.get(parent).and_then(|e| e.artifact_id.clone()) {
            Some(aid) => aid,
            None => {
                reply.error(libc::EIO);
                return;
            }
        };

        let new_parent_aid =
            match self
                .inodes
                .get(newparent)
                .and_then(|e| e.artifact_id.clone())
            {
                Some(aid) => aid,
                None => {
                    reply.error(libc::EIO);
                    return;
                }
            };

        // Remove from old parent.
        if let Err(e) = self.vault.remove_ref(&old_parent_aid, &child_artifact_id) {
            error!("rename remove_ref failed: {e}");
            reply.error(libc::EIO);
            return;
        }

        // Compute next position in new parent.
        let next_pos = self
            .vault
            .get_artifact(&new_parent_aid)
            .ok()
            .flatten()
            .and_then(|a| a.as_tree().cloned())
            .map(|t| {
                t.references
                    .iter()
                    .map(|r| r.position)
                    .max()
                    .unwrap_or(0)
                    + 1
            })
            .unwrap_or(0);

        // Compose into new parent with new label.
        if let Err(e) = self.vault.compose(
            &new_parent_aid,
            child_artifact_id,
            next_pos,
            Some(new_name.to_string()),
        ) {
            error!("rename compose failed: {e}");
            reply.error(libc::EIO);
            return;
        }

        // Update the inode.
        if let Some(entry) = self.inodes.get_mut(child_ino) {
            entry.name = new_name.to_string();
            entry.parent_inode = newparent;
        }

        reply.ok();
    }

    fn statfs(&mut self, _req: &FuseRequest<'_>, _ino: u64, reply: ReplyStatfs) {
        debug!("statfs");
        reply.statfs(
            1_000_000, // blocks
            500_000,   // bfree
            500_000,   // bavail
            1_000_000, // files
            500_000,   // ffree
            512,       // bsize
            255,       // namelen
            512,       // frsize
        );
    }

    fn access(
        &mut self,
        _req: &FuseRequest<'_>,
        ino: u64,
        _mask: i32,
        reply: ReplyEmpty,
    ) {
        debug!("access: ino={ino}");
        // We handle permissions via getattr; always return ok.
        if self.inodes.get(ino).is_some() {
            reply.ok();
        } else {
            reply.error(libc::ENOENT);
        }
    }
}
