# Consolidate Realms and Artifacts into Unified P2P Sync

## Setup: Create Worktree

```bash
# Add .worktrees to .gitignore and commit
echo ".worktrees/" >> .gitignore
git add .gitignore && git commit -m "Add .worktrees to gitignore"

# Create isolated worktree
git worktree add .worktrees/unify-sync -b unify/realm-artifact-consolidation

# Copy plan
cp /Users/truman/.claude/plans/atomic-herding-honey.md .worktrees/unify-sync/PLAN.md
```

All implementation work happens in `.worktrees/unify-sync/`.

---

## Context

The codebase has two parallel systems for syncing content between peers:

1. **Realms** (`indras-network`) — collaborative spaces with full P2P sync (Automerge CRDT + store-and-forward + gossip). Members join via invite codes.
2. **Artifacts** (`indras-artifacts`) — content with access grants (audience list) but **no independent P2P sync**. Artifact metadata piggybacks on the HomeRealm's CRDT document.

The redundancy: both systems define "who gets this content" (realm members vs artifact audience), both have deterministic DM IDs (`dm_realm_id` vs `dm_story_id`), and both model sequential content between participants (realm messages vs Story artifacts). But only realms have sync infrastructure.

**Goal:** Artifacts should auto-sync P2P to their audience, just like realms sync to their members. A realm IS a syncing artifact.

---

## Redundancy Map

| Concept | Realm Implementation | Artifact Implementation |
|---------|---------------------|------------------------|
| "Who gets this" | `NInterface.members: HashSet<I>` | `artifact.audience(now)` from `Vec<AccessGrant>` |
| DM identity | `dm_realm_id(a,b)` — `"realm-peers-v1:"` prefix | `dm_story_id(a,b)` — `"dm-v1:"` prefix |
| Sequential content | `Realm.send()` → EventStore events | `Story.send_message()` → Leaf + Tree refs |
| P2P sync | NInterface + SyncTask (5s loop) | None — passive CRDT doc in HomeRealm |
| Content sharing | `Realm.share_artifact()` broadcasts Custom event | `HomeRealm.upload()` + ArtifactIndex entry |

---

## Post-Consolidation: What Survives

**Artifact system wins for content modeling.** It has richer access control (4 grant modes vs binary membership), holonic composition, attention/heat, stewardship. The artifact type system (Leaf/Tree with subtypes) already covers everything realms hold.

**Realm sync infra wins for delivery.** NInterface + EventStore + SyncTask is the proven P2P backbone. It just needs to serve artifacts, not only realms.

**After consolidation:**
- A **Realm** = a `TreeArtifact` (type Story or Realm) with audience > 1, auto-syncing via NInterface
- An **Artifact** = content (Leaf or Tree) with an owner, grants, and automatic P2P sync when audience > 1
- **ArtifactIndex** = remains as a personal catalog in HomeRealm, but no longer the sole delivery mechanism
- **Invite codes** = a mechanism to add an `AccessGrant` to a Tree artifact (instead of directly adding NInterface membership)

---

## Phase 1: Artifact Sync Registry

**Goal:** Tree artifacts with audience > 1 automatically get their own NInterface and sync channel.

### 1.1 Add `artifact_interface_id()` derivation

**File:** `crates/indras-artifacts/src/artifact.rs`

```rust
pub fn artifact_interface_id(artifact_id: &ArtifactId) -> InterfaceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"artifact-sync-v1:");
    hasher.update(artifact_id.bytes());
    InterfaceId::new(*hasher.finalize().as_bytes())
}
```

Deterministic mapping: same artifact always gets the same interface ID. Both peers compute it independently.

### 1.2 Create `ArtifactSyncRegistry`

**New file:** `crates/indras-network/src/artifact_sync.rs`

A registry that maps artifact IDs to NInterfaces. When audience changes:
- **Audience grows to 2+:** Create NInterface, register with node's sync task, join gossip topic
- **Audience shrinks to 1:** Tear down NInterface, leave gossip topic
- **Grant added/removed:** Reconcile NInterface membership with current audience

The SyncTask already iterates all registered interfaces — artifact interfaces join the same pool. No changes to the sync loop.

### 1.3 Wire grant changes to the registry

**File:** `crates/indras-network/src/home_realm.rs`

After `grant_access()`, `revoke_access()`, `recall()`, or `transfer()`:
- Compute current audience for the affected artifact
- If audience >= 2 and no interface exists → create one
- If audience < 2 and interface exists → tear it down
- Otherwise → reconcile membership

### 1.4 Existing reusable code

- `NInterface::new()` and `NInterface::add_member()` / `NInterface::remove_member()` in `crates/indras-sync/src/n_interface.rs`
- `IndrasNode::create_interface()` in `crates/indras-node/src/lib.rs` — already handles gossip topic creation
- `SyncTask::run()` in `crates/indras-node/src/sync_task.rs` — already syncs all registered interfaces

### Verification

- Write a test: create a Tree artifact, grant access to a second peer, verify an NInterface is created with the correct deterministic ID
- Verify the SyncTask picks up the new interface and attempts sync
- Verify revoking the grant tears down the interface

---

## Phase 2: Unify DM Realm and DM Story

**Goal:** A DM conversation is a single concept with one ID scheme.

### 2.1 Adopt `dm_story_id` as canonical

**File:** `crates/indras-network/src/direct_connect.rs` (or wherever `dm_realm_id` lives in `network.rs`)

Replace `dm_realm_id(a, b)` with a call to `dm_story_id(a, b)` from `indras-artifacts`. Both sort peer IDs lexicographically and derive a deterministic `[u8; 32]`.

### 2.2 `connect()` creates a Story artifact

**File:** `crates/indras-network/src/network.rs` line ~671

Instead of creating a raw interface:
1. Create a `TreeArtifact` with `TreeType::Story` and the deterministic DM ID
2. Grant both peers `Permanent` access
3. The `ArtifactSyncRegistry` from Phase 1 auto-creates the NInterface
4. Return a `Realm` wrapper pointing to this artifact's interface

### 2.3 Clean up

Remove duplicate `dm_realm_id()` function. The `dm_story_id()` in `crates/indras-artifacts/src/artifact.rs` becomes the single source.

### Verification

- `connect(peer_id)` from both sides produces the same artifact ID and interface ID
- Messages sent via the DM realm sync to the other peer
- Existing DM functionality (send, receive, history) works through the Story path

---

## Phase 3: Realm as Syncing Tree Artifact

**Goal:** `create_realm()` creates a Tree artifact. The `Realm` struct becomes a thin wrapper.

### 3.1 `create_realm()` creates a Tree artifact

**File:** `crates/indras-network/src/network.rs` line ~531

1. Create a `TreeArtifact` (type `Story` — realms and stories are the same thing)
2. Grant the creator `Permanent` access
3. `ArtifactSyncRegistry` creates the NInterface
4. Generate an invite code that, when redeemed, adds a `Permanent` AccessGrant

### 3.2 `join()` adds an AccessGrant

**File:** `crates/indras-network/src/network.rs` line ~569

Instead of calling `IndrasNode::join_interface()` directly, the invite code's payload specifies the artifact ID. Joining:
1. Adds the joiner as a grantee on the Tree artifact
2. `ArtifactSyncRegistry` detects the audience change and adds the joiner to the NInterface

### 3.3 Realm API remaps to artifact operations

**File:** `crates/indras-network/src/realm.rs`

| Realm method | New implementation |
|---|---|
| `send(content)` | Create `LeafArtifact(Message)`, append to Tree via compose |
| `messages()` | Read Tree's references, resolve each Leaf |
| `member_list()` | `artifact.audience(now)` from grants |
| `share_artifact(path)` | `vault.place_leaf()` + compose into realm's Tree |
| `document(name)` | Child Tree of type Document (already CRDT-backed) |
| `invite_code()` | Encodes artifact ID + bootstrap peers + PQ keys |

**Note:** Real-time message delivery still uses EventStore events for low-latency. The Leaf artifacts are the persistence layer. EventStore events are ephemeral delivery; artifacts are durable state.

### 3.4 Invite code format update

**File:** `crates/indras-network/src/invite.rs`

Change `InviteKey` to carry the artifact ID (from which the interface ID is derived) instead of a raw interface ID. The prefix stays `indra:realm:` for UX continuity.

### Verification

- `create_realm("name")` → produces a Tree artifact with the creator as sole audience member
- `join(invite)` → adds joiner to artifact grants, NInterface membership updates
- `send()` / `messages()` work end-to-end between two peers
- Store-and-forward: peer goes offline, comes back, receives missed messages

---

## Phase 4: Clean Up Redundant Code

### 4.1 Remove duplicate `Artifact` struct

The `Artifact` in `crates/indras-network/src/artifact.rs` (lines 27-50) duplicates `indras_artifacts::Artifact` with different fields (`sharer: Member`, `shared_at`, `is_encrypted`). Merge these into the canonical `indras_artifacts` types or drop them.

### 4.2 Remove duplicate `guess_mime_type()`

Exists in both `realm.rs` (line 1104) and `home_realm.rs` (line 555). Extract to a shared utility.

### 4.3 Simplify ArtifactIndex

With artifacts auto-syncing to their audience, `ArtifactIndex` no longer needs to be the sole delivery mechanism. It remains as a personal catalog (what do I own?) but "realm views" (`accessible_by_all`) become unnecessary — the realm IS the artifact, and its children are the shared content.

### 4.4 Consolidate HomeRealm

`HomeRealm` simplifies to: personal Vault + personal settings documents. Artifact sharing moves to the artifact sync system. Methods like `share_artifact_with_mode()` on Realm become redundant — granting access on any artifact automatically syncs it.

---

## Key Design Decisions

### Messages: Events vs Artifacts?

**Hybrid approach.** Real-time delivery uses EventStore events (low latency, ephemeral). Persistence uses Leaf artifacts in the Tree (durable, content-addressed). The SyncTask delivers both: events for real-time, Automerge doc for catch-up. This avoids the overhead of creating a Leaf artifact for every chat message while still getting the artifact model's benefits for persistence.

### Interface-per-artifact scalability

Only Tree artifacts with audience > 1 get interfaces. Leaf artifacts inherit their parent Tree's interface. A user with 10 active conversations and 5 group realms = 15 interfaces, well within the SyncTask's capacity (it already handles per-interface loops with backoff).

### Encryption

Derive interface keys from the artifact ID + a shared secret (e.g., the invite key). For DM artifacts, use the existing PQ key exchange. For group artifacts, the invite key IS the symmetric key material, distributed via the invite code.

---

## Files Modified (by phase)

### Phase 1
- `crates/indras-artifacts/src/artifact.rs` — add `artifact_interface_id()`
- **New:** `crates/indras-network/src/artifact_sync.rs` — `ArtifactSyncRegistry`
- `crates/indras-network/src/home_realm.rs` — wire grants to registry
- `crates/indras-network/src/lib.rs` — export new module

### Phase 2
- `crates/indras-network/src/network.rs` — `connect()` rewrite
- `crates/indras-network/src/direct_connect.rs` — unify DM ID scheme

### Phase 3
- `crates/indras-network/src/network.rs` — `create_realm()` and `join()` rewrite
- `crates/indras-network/src/realm.rs` — reimplement methods over artifact ops
- `crates/indras-network/src/invite.rs` — carry artifact ID in invite

### Phase 4
- `crates/indras-network/src/artifact.rs` — delete or merge
- `crates/indras-network/src/artifact_index.rs` — simplify
- `crates/indras-network/src/home_realm.rs` — simplify
- Various — remove duplicate utilities
