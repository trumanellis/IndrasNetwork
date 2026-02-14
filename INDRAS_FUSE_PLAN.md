# indras-fuse: P2P Artifact Vault as a FUSE Filesystem

## Vision

Turn Indra's Network into a **P2P filesystem platform**. Any application — AppFlowy, Obsidian, VS Code, `vim` — reads and writes to a mounted directory. Under the hood, every file operation maps to artifact operations: content-addressed storage, CRDT sync, encrypted sharing, access control, and attention tracking.

```
/indra/
├── vault/                          # Player's root vault (TreeArtifact)
│   ├── notes/                      # Collection (TreeArtifact)
│   │   ├── project-ideas.md        # LeafArtifact (content-addressed blob)
│   │   └── meeting-2026-02-14.md   # LeafArtifact
│   ├── stories/                    # Stories container
│   │   └── quest-log/              # Story (TreeArtifact, ordered)
│   │       ├── 001-kickoff.md
│   │       └── 002-progress.md
│   ├── gallery/                    # Gallery (TreeArtifact)
│   │   ├── photo-001.jpg           # LeafArtifact (Image)
│   │   └── photo-002.png           # LeafArtifact (Image)
│   └── inbox/                      # Inbox (TreeArtifact)
│       └── connection-request.json # LeafArtifact (Attestation)
├── peers/                          # Read-only view of peered vaults
│   ├── zephyr/                     # Peer's shared artifacts
│   │   └── shared-doc.md           # Artifacts they've granted us access to
│   └── nova/
│       └── collab-notes.md
├── realms/                         # Realm views (filtered by shared access)
│   └── project-aurora/
│       ├── spec.md
│       └── design.png
└── .indra/                         # Metadata (hidden)
    ├── attention.log               # Attention events (read-only)
    ├── heat.json                   # Current heat values (read-only)
    └── peers.json                  # Peer registry (read-only)
```

**The killer feature**: Every `open()` call fires an `AttentionSwitchEvent`. Attention tracking happens invisibly — AppFlowy doesn't know it's being tracked, but Indra's Network knows exactly which artifacts are hot.

---

## Architecture

### Layer Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    Applications                          │
│  AppFlowy  │  Obsidian  │  VS Code  │  vim  │  Any app  │
└──────┬──────────┬───────────┬─────────┬───────┬─────────┘
       │          │           │         │       │
       ▼          ▼           ▼         ▼       ▼
┌─────────────────────────────────────────────────────────┐
│                 /indra/ mount point                       │
│              (standard POSIX filesystem)                  │
└──────────────────────────┬──────────────────────────────┘
                           │ FUSE protocol
┌──────────────────────────▼──────────────────────────────┐
│                    indras-fuse                            │
│                                                          │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐ │
│  │ Inode Table  │  │ Write Buffer │  │ Event Emitter  │ │
│  │ ino ↔ ArtId  │  │ (dirty pages)│  │ (attention)    │ │
│  └──────┬──────┘  └──────┬───────┘  └───────┬────────┘ │
│         │                │                   │           │
│  ┌──────▼────────────────▼───────────────────▼────────┐ │
│  │              Vault<A, P, T> Bridge                  │ │
│  │  FUSE ops → Vault methods → Store traits            │ │
│  └─────────────────────┬──────────────────────────────┘ │
└────────────────────────┼────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                  indras-artifacts                         │
│  Vault · ArtifactStore · PayloadStore · AttentionStore   │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                  indras-network                           │
│  ArtifactIndex · AccessGrants · Holonic Composition      │
│  P2P Sync · Encryption · iroh transport                  │
└─────────────────────────────────────────────────────────┘
```

### Key Design Decisions

1. **fuser crate** for FUSE implementation (works with macFUSE and FUSE-T on macOS)
2. **Mutable indirection layer** (like IPFS MFS) maps inodes to ArtifactIds
3. **Write buffering** — accumulate writes in memory, flush to blob store on `release()`/`fsync()`
4. **Attention via `open()`** — every file open fires `vault.navigate_to()`
5. **Tree artifacts = directories**, **Leaf artifacts = files**
6. **Content-addressed dedup** — identical files produce identical `ArtifactId::Blob` hashes
7. **macOS-first** via FUSE-T (no kernel extension), with FSKit as future path

---

## FUSE ↔ Artifact Mapping

### Type Mapping

| FUSE Concept | Artifact Concept | Details |
|---|---|---|
| Regular file | `LeafArtifact` | Payload in `PayloadStore`, metadata in `ArtifactStore` |
| Directory | `TreeArtifact` | `references: Vec<ArtifactRef>` are directory entries |
| Filename | `ArtifactRef.label` | The `label` field on each reference |
| File position/order | `ArtifactRef.position` | Ordering within directory listing |
| File content | `PayloadStore::get_payload()` | Raw bytes, content-addressed |
| File size | `LeafArtifact.size` | Stored in metadata |
| inode number | Internal mapping | `HashMap<u64, ArtifactId>` + reverse map |
| Symlink | `LeafArtifact` with `LeafType::Custom("symlink")` | Target path stored as payload |
| File owner | `LeafArtifact.steward` | PlayerId of steward |
| Permissions | Derived from `audience` + `AccessGrant` | See permission model below |

### LeafType ↔ MIME Type Mapping

| File Extension | LeafType | Notes |
|---|---|---|
| `.md`, `.txt`, `.json`, `.lua`, `.rs`, `.toml` | `LeafType::File` | Text content |
| `.jpg`, `.png`, `.gif`, `.webp`, `.svg` | `LeafType::Image` | Image content |
| `.mp4`, `.mov`, `.webm` | `LeafType::File` | Video (no dedicated type yet) |
| `.token` | `LeafType::Token` | Token of gratitude |
| `.attestation` | `LeafType::Attestation` | Humanness attestation |
| Other | `LeafType::File` | Default |

### TreeType ↔ Directory Semantics

| Directory Path Pattern | TreeType | Behavior |
|---|---|---|
| `/indra/vault/` | `Vault` | Root, exactly one per player |
| `/indra/vault/*/` (general dirs) | `Collection` | Unordered container |
| `/indra/vault/stories/*/` | `Story` | Ordered, append-mostly |
| `/indra/vault/gallery/` | `Gallery` | Image-focused container |
| `/indra/vault/inbox/` | `Inbox` | Incoming connection requests |
| `/indra/vault/documents/*/` | `Document` | Structured document tree |

The TreeType is inferred from:
1. Special directory names (`stories`, `gallery`, `inbox`, `documents`)
2. Extended attributes (`indra.tree_type=Story`)
3. Default: `Collection` for user-created directories

---

## Inode Management

### Inode Table Structure

```rust
pub struct InodeTable {
    /// Forward map: inode → artifact info
    entries: HashMap<u64, InodeEntry>,
    /// Reverse map: ArtifactId → inode
    artifact_to_inode: HashMap<ArtifactId, u64>,
    /// Next available inode number
    next_inode: AtomicU64,
    /// Lookup count per inode (for FUSE forget())
    lookup_counts: HashMap<u64, u64>,
}

pub struct InodeEntry {
    pub artifact_id: ArtifactId,
    pub parent_inode: u64,
    pub name: String,
    pub kind: InodeKind,
    /// Cached file attributes (refreshed on getattr)
    pub attr_cache: FileAttr,
    /// For files: dirty write buffer
    pub write_buffer: Option<Vec<u8>>,
    /// Whether this inode has uncommitted changes
    pub dirty: bool,
}

pub enum InodeKind {
    /// Maps to LeafArtifact
    File { leaf_type: LeafType, size: u64 },
    /// Maps to TreeArtifact
    Directory { tree_type: TreeType },
    /// Virtual file (attention.log, heat.json, etc.)
    Virtual { generator: VirtualFileType },
}
```

### Reserved Inodes

| Inode | Path | Purpose |
|---|---|---|
| 1 | `/indra/` | FUSE root |
| 2 | `/indra/vault/` | Player's vault root (TreeArtifact) |
| 3 | `/indra/peers/` | Virtual: peer vault views |
| 4 | `/indra/realms/` | Virtual: realm views |
| 5 | `/indra/.indra/` | Virtual: metadata directory |
| 6+ | Dynamic | Allocated on lookup/create |

### Inode Lifecycle

```
lookup(parent, "notes") → allocate inode 6 for vault's "notes" TreeArtifact
                        → InodeTable: 6 → ArtifactId::Doc([...])
                        → lookup_count[6] = 1

getattr(6)             → read TreeArtifact metadata, return FileAttr

readdir(6)             → iterate TreeArtifact.references
                        → allocate inodes 7, 8, ... for each child
                        → return directory entries

forget(6, nlookup=1)   → lookup_count[6] -= 1
                        → if 0: evict from cache (can be re-populated on next lookup)
```

---

## Read/Write Flows

### Reading a File

```
1. App calls open("/indra/vault/notes/ideas.md", O_RDONLY)
   │
2. FUSE → lookup(parent=6, name="ideas.md")
   │  → Find ArtifactRef with label="ideas.md" in parent TreeArtifact
   │  → Allocate inode, return entry
   │
3. FUSE → open(ino=7, flags=O_RDONLY)
   │  → vault.navigate_to(artifact_id)  ← ATTENTION EVENT!
   │  → Return file handle
   │
4. FUSE → read(ino=7, offset=0, size=4096)
   │  → vault.get_payload(&artifact_id)
   │  → Return bytes[offset..offset+size]
   │
5. FUSE → release(ino=7)
   │  → vault.navigate_back(parent_id)  ← ATTENTION: left artifact
```

### Writing a File (New)

```
1. App calls create("/indra/vault/notes/new-note.md")
   │
2. FUSE → create(parent=6, name="new-note.md", mode=0o644)
   │  → Allocate inode 10
   │  → Create empty write buffer
   │  → DON'T create artifact yet (no content)
   │  → Return (entry, file_handle)
   │
3. FUSE → write(ino=10, offset=0, data=b"# My Note\n...")
   │  → Append to write buffer
   │  → Mark dirty=true
   │  → Return bytes written
   │
4. FUSE → write(ino=10, offset=10, data=b"More content...")
   │  → Append to write buffer
   │  → Return bytes written
   │
5. FUSE → flush(ino=10) or release(ino=10)
   │  → payload_id = vault.store_payload(&write_buffer)
   │  → leaf = vault.place_leaf(&write_buffer, LeafType::File, now)
   │  → vault.compose(&parent_tree_id, leaf.id, next_position, Some("new-note.md"))
   │  → Clear write buffer, dirty=false
   │  → Fire attention event for creation
```

### Writing a File (Edit Existing)

```
1. App calls open("/indra/vault/notes/ideas.md", O_WRONLY)
   │
2. FUSE → open(ino=7, flags=O_WRONLY)
   │  → Load existing payload into write buffer
   │  → vault.navigate_to(artifact_id)  ← ATTENTION
   │  → Return file handle
   │
3. FUSE → write(ino=7, offset=50, data=b"edited text")
   │  → Modify write buffer at offset
   │  → Mark dirty
   │
4. FUSE → release(ino=7)
   │  → new_id = vault.store_payload(&write_buffer)  ← NEW BLAKE3 HASH
   │  → vault.place_leaf(&write_buffer, leaf_type, now)
   │  → Update parent TreeArtifact: replace old ArtifactRef with new one
   │  → Update InodeTable: inode 7 now points to new ArtifactId
   │  → Old blob eligible for GC (if no other refs)
```

### Key Insight: Content-Addressing on Flush

We don't re-hash on every `write()` call. We buffer writes in memory and only create a new content-addressed blob on `flush()`/`release()`. This means:
- Multiple small writes → one blob creation
- The inode stays stable (same inode number) even though the ArtifactId changes
- The indirection layer (InodeTable) absorbs the hash churn

---

## Attention Tracking Integration

### Automatic Events

| FUSE Operation | Attention Event | Rationale |
|---|---|---|
| `open(ino, O_RDONLY)` | `navigate_to(artifact_id)` | Reading = paying attention |
| `open(ino, O_WRONLY)` | `navigate_to(artifact_id)` | Writing = intense attention |
| `release(ino)` | `navigate_back(parent_id)` | Closing = leaving artifact |
| `readdir(ino)` | `navigate_to(dir_artifact_id)` | Browsing directory = scanning |
| `create(parent, name)` | (deferred to first write) | Creating isn't attention yet |
| `unlink(parent, name)` | No event | Deletion isn't attention |
| `rename(...)` | No event | Renaming isn't attention |

### Heat Computation via Filesystem

Because every `open()` fires attention, heat is computed naturally:

```
$ cat /indra/.indra/heat.json
{
  "vault/notes/ideas.md": { "heat": 0.87, "unique_peers": 3, "dwell_ms": 45000 },
  "vault/notes/old-draft.md": { "heat": 0.12, "unique_peers": 1, "dwell_ms": 2000 },
  "vault/gallery/photo-001.jpg": { "heat": 0.95, "unique_peers": 5, "dwell_ms": 120000 }
}
```

### Virtual Metadata Files

The `.indra/` directory exposes read-only virtual files:

| Virtual File | Content | Updated |
|---|---|---|
| `.indra/attention.log` | JSONL of recent AttentionSwitchEvents | Real-time |
| `.indra/heat.json` | Current heat values for all artifacts | Computed on read |
| `.indra/peers.json` | Peer registry with display names | On peer change |
| `.indra/player.json` | Current player ID and vault info | Static |

---

## Permission Model

### Mapping Access Control to POSIX Permissions

```
Owner (steward):     rwx  (read + write + execute/list)
Group (audience):    r-x  (read + list, no write)
Other:               ---  (no access)
```

| Artifact Role | POSIX Mapping |
|---|---|
| Steward | File owner (uid matches player) |
| Audience member | Group member |
| No access | Other |

### Access Modes → File Behavior

| AccessMode | File visible? | Writable? | Deletable? | Notes |
|---|---|---|---|---|
| `Revocable` | Yes | No | No | Read-only, can disappear if recalled |
| `Permanent` | Yes | No | No | Read-only, persists even if recalled |
| `Timed` | Yes (until expiry) | No | No | Disappears after deadline |
| `Transfer` | Yes | Yes | Yes | Full ownership, one-shot |

### Peer Vaults

Files under `/indra/peers/<name>/` are always read-only:
- We only see artifacts they've granted us access to
- Permission bits: `r--r-----` (444 for files, 555 for dirs)
- Write attempts return `EACCES`

---

## Concurrency Model

### Interior Mutability with RwLock

```rust
pub struct IndraFS {
    /// The player's vault (write-locked only on mutations)
    vault: Arc<RwLock<Vault<NetworkArtifactStore, NetworkPayloadStore, NetworkAttentionStore>>>,
    /// Inode table (frequently read, occasionally written)
    inodes: Arc<RwLock<InodeTable>>,
    /// Active file handles with write buffers
    open_files: Arc<RwLock<HashMap<u64, OpenFile>>>,
    /// Player identity
    player_id: PlayerId,
    /// Next file handle counter
    next_fh: AtomicU64,
}

pub struct OpenFile {
    pub inode: u64,
    pub artifact_id: ArtifactId,
    pub flags: i32,
    pub write_buffer: Option<Vec<u8>>,
    pub dirty: bool,
}
```

### Lock Ordering (Deadlock Prevention)

Always acquire locks in this order:
1. `inodes` (outermost)
2. `vault`
3. `open_files` (innermost)

### Read vs Write Lock Strategy

| Operation | inodes lock | vault lock | open_files lock |
|---|---|---|---|
| `lookup` | Read | Read | — |
| `getattr` | Read | — | — |
| `readdir` | Read | Read | — |
| `read` | Read | Read | Read |
| `write` | — | — | Write |
| `create` | Write | Write | Write |
| `unlink` | Write | Write | — |
| `flush/release` | Write | Write | Write |

---

## Crate Structure

```
crates/indras-fuse/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point, mount/unmount
│   ├── lib.rs               # Public API
│   ├── fs.rs                # Filesystem trait implementation
│   ├── inode.rs             # InodeTable, InodeEntry, allocation
│   ├── mapping.rs           # Artifact ↔ FUSE type conversions
│   ├── write_buffer.rs      # Write buffering and flush logic
│   ├── attention.rs         # Attention event emission on FUSE ops
│   ├── virtual_files.rs     # .indra/ virtual file generation
│   ├── permissions.rs       # Access control → POSIX permission mapping
│   ├── peers.rs             # /peers/ directory (peer vault views)
│   ├── realms.rs            # /realms/ directory (realm views)
│   └── config.rs            # Mount options, paths, settings
└── tests/
    ├── mount_test.rs         # Integration: mount, write, read, unmount
    ├── attention_test.rs     # Verify attention events fire on open()
    ├── write_flush_test.rs   # Write buffering and content-addressing
    ├── permissions_test.rs   # Access control mapping
    └── concurrent_test.rs    # Multi-threaded access
```

### Cargo.toml

```toml
[package]
name = "indras-fuse"
version = "0.1.0"
edition = "2021"

[dependencies]
indras-artifacts = { path = "../indras-artifacts" }
indras-network = { path = "../indras-network" }

# FUSE
fuser = "0.15"

# Async runtime (for network ops)
tokio = { version = "1", features = ["full"] }

# Serialization (for virtual files)
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utilities
blake3 = "1.8"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
clap = { version = "4", features = ["derive"] }
libc = "0.2"
bytes = "1"

[dev-dependencies]
tempfile = "3"
```

---

## CLI Interface

```bash
# Mount the vault
indras-fuse mount ~/indra --player <player-id>

# Mount with specific options
indras-fuse mount ~/indra \
  --player <player-id> \
  --allow-other \           # Let other users access (for AppFlowy running as different uid)
  --auto-unmount \          # Unmount when process exits
  --foreground \            # Run in foreground (for debugging)
  --log-level debug

# Unmount
indras-fuse unmount ~/indra

# Status
indras-fuse status ~/indra
# Output:
# Mounted: /Users/truman/indra
# Player: 0xabc123...
# Artifacts: 147 (23 trees, 124 leaves)
# Peers: 3 connected
# Attention events (last hour): 89
# Hot artifacts: vault/notes/ideas.md (0.92), vault/gallery/sunset.jpg (0.87)
```

---

## macOS Strategy

### Primary: FUSE-T (Recommended)

- No kernel extension required
- Uses NFSv4 local server under the hood
- Drop-in compatible with `fuser` crate
- Better performance than macFUSE on Apple Silicon
- Standard user-space installation

### Secondary: macFUSE 4.5+

- FSKit backend on macOS 15+ (no kext needed)
- Falls back to kext on older macOS (requires Recovery Mode approval)

### Future: Apple FSKit via fskit-rs

- Native Apple framework for user-space filesystems
- Requires macOS 15.4+
- Rust bindings available via `fskit-rs` crate
- Best long-term option as Apple continues deprecating kexts

### Cross-Platform Notes

- `fuser` crate works on Linux natively (libfuse)
- Windows support via WinFsp (future, lower priority)
- The artifact layer is platform-independent; only the FUSE mount is OS-specific

---

## Implementation Phases

### Phase 1: Read-Only Mount (MVP)

**Goal**: Mount the vault as a readable filesystem. AppFlowy can open it in local mode.

**Scope**:
- [ ] `main.rs`: CLI with `mount`/`unmount` commands
- [ ] `inode.rs`: InodeTable with allocation, lookup, forget
- [ ] `fs.rs`: Implement `lookup`, `getattr`, `readdir`, `open`, `read`, `release`
- [ ] `mapping.rs`: `TreeArtifact` → directory attrs, `LeafArtifact` → file attrs
- [ ] `attention.rs`: Fire `navigate_to` on `open()`, `navigate_back` on `release()`
- [ ] Mount the vault root as `/indra/vault/`

**Test**: Mount, `ls /indra/vault/`, `cat /indra/vault/notes/test.md`, verify attention log.

### Phase 2: Read-Write Support

**Goal**: Create, edit, and delete files/directories through the filesystem.

**Scope**:
- [ ] `write_buffer.rs`: In-memory write buffering per file handle
- [ ] `fs.rs`: Implement `create`, `write`, `flush`, `release` (with flush), `mkdir`, `unlink`, `rmdir`, `rename`, `setattr`
- [ ] Content-addressing on flush: buffer → `store_payload()` → `place_leaf()` → `compose()`
- [ ] Handle edit-existing: load payload into buffer on open, flush new blob on close
- [ ] Handle rename: update `ArtifactRef.label` in parent tree

**Test**: Create file via `echo "hello" > /indra/vault/notes/new.md`, verify artifact created. Edit file, verify new blob with new hash. Delete file, verify ref removed.

### Phase 3: Virtual Files & Metadata

**Goal**: Expose attention data, heat, and peer info via `.indra/` directory.

**Scope**:
- [ ] `virtual_files.rs`: Generate `attention.log`, `heat.json`, `peers.json`, `player.json` on read
- [ ] `heat.json` computed live from `vault.heat()` for all known artifacts
- [ ] `attention.log` streams recent events as JSONL

**Test**: `cat /indra/.indra/heat.json` returns valid JSON with heat values. Open a file, re-read heat.json, verify heat increased.

### Phase 4: Peer & Realm Views

**Goal**: Browse peer-shared artifacts and realm views as directories.

**Scope**:
- [ ] `peers.rs`: `/indra/peers/<name>/` shows artifacts granted to us by each peer
- [ ] `realms.rs`: `/indra/realms/<realm>/` shows artifacts where all realm members have access
- [ ] Read-only enforcement (EACCES on write)
- [ ] Access mode display via extended attributes

**Test**: Peer shares an artifact, it appears under `/indra/peers/<peer>/`. Recall it, it disappears.

### Phase 5: Advanced Features

**Goal**: Polish, performance, and advanced filesystem features.

**Scope**:
- [ ] `permissions.rs`: Full POSIX permission model from access grants
- [ ] Extended attributes (`xattr`): `indra.heat`, `indra.steward`, `indra.access_mode`, `indra.tree_type`
- [ ] File system notifications (fsevents/inotify) when P2P sync updates files
- [ ] Lazy inode allocation (don't walk entire tree on mount)
- [ ] Read-ahead caching for frequently accessed blobs
- [ ] Background sync status in `.indra/sync-status.json`
- [ ] `statfs` implementation (space used, artifact counts)

---

## Testing Strategy

### Unit Tests

```rust
// test: inode allocation is sequential and unique
// test: inode forget decrements lookup count
// test: inode reuse after forget reaches zero
// test: artifact-to-fileattr mapping preserves size, timestamps
// test: tree_type inference from directory name
// test: leaf_type inference from file extension
// test: write buffer accumulates correctly
// test: flush creates content-addressed blob
// test: edit-existing loads payload into buffer
// test: permission mapping from access grants
```

### Integration Tests

```rust
// test: mount in-memory vault, readdir returns vault contents
// test: create file, read it back, content matches
// test: create file, verify ArtifactId is BLAKE3 of content
// test: create two identical files, verify same ArtifactId (dedup)
// test: open file triggers AttentionSwitchEvent
// test: close file triggers navigate_back
// test: heat.json reflects attention after file access
// test: rename file updates ArtifactRef.label
// test: delete file removes ArtifactRef from parent
// test: mkdir creates TreeArtifact with correct TreeType
// test: rmdir removes TreeArtifact reference
// test: concurrent reads don't deadlock
// test: concurrent read + write don't deadlock
// test: peer directory is read-only (EACCES on write)
```

### Stress Tests (Lua Scenarios)

```lua
-- scenario: mount vault, create 1000 files, verify all accessible
-- scenario: concurrent readers and writers
-- scenario: create files, share with peer, verify peer sees them
-- scenario: share then recall, verify peer can no longer access
-- scenario: rapid open/close cycles, verify attention log integrity
```

---

## Open Questions

1. **TreeType inference vs explicit**: Should creating `/indra/vault/my-story/` auto-infer `Story` type, or require explicit marking (e.g., via xattr or a `.indra-type` file)?

2. **Conflict resolution**: When P2P sync delivers a conflicting edit (two people edited the same LeafArtifact), should we create a `.conflict` file (like Syncthing) or use the CRDT merge?

3. **Large files**: Should large files (>10MB) be chunked into multiple blobs, or stored as single blobs? iroh-blobs handles large transfers well, but chunking enables partial sync.

4. **Real-time sync notification**: How to notify mounted applications that a file changed due to P2P sync? macOS FSEvents? Linux inotify? Or rely on apps polling?

5. **AppFlowy integration**: Does AppFlowy's local mode work well with a FUSE mount, or does it make assumptions about the filesystem (e.g., fsync semantics, atomic rename)?

---

## References

- [fuser crate](https://github.com/cberner/fuser) — Rust FUSE implementation
- [FUSE-T](https://www.fuse-t.org/) — Kext-less FUSE for macOS via NFSv4
- [macFUSE 4.5](https://github.com/macfuse/macfuse/releases) — Traditional macOS FUSE with FSKit backend
- [fskit-rs](https://crates.io/crates/fskit-rs) — Rust bindings for Apple FSKit
- [IPFS MFS](https://docs.ipfs.tech/concepts/file-systems/) — Mutable filesystem over content-addressed store
- [ElmerFS](https://github.com/scality/elmerfs) — CRDT-based FUSE filesystem (Rust)
- [XetHub NFS approach](https://xethub.com/blog/nfs-fuse-why-we-built-nfs-server-rust) — NFS > FUSE argument
- [fuser simple.rs example](https://github.com/cberner/fuser/blob/master/examples/simple.rs) — Complete read-write FUSE example
- [iroh-blobs](https://docs.rs/iroh-blobs/latest/iroh_blobs/) — Content-addressed blob store
