# Plan: Project concept inside Realm

## Goal

Introduce a **Project** layer that lives inside a Realm and groups agents.
Each Project is:

- Its own peer-broadcastable entity with its own `RealmId` (so existing braid / realm sync machinery carries it).
- A **manifest-materialized view** of the blob store: a JSON manifest maps `path → blob-hash`, which the engine materializes as a real directory tree under the parent Realm.
- The parent directory of that Project's agent working trees. Each agent lives in a subfolder inside the Project and uses the same manifest materialization for its sandbox root.
- Sandboxed **cooperatively** — agent path-prefix checks in `agent_hooks.rs`, not OS isolation.

Hierarchical braid sync:

```
agent working tree  ─►  Project braid  ─►  Realm / peer broadcast
```

Agent edits land into the Project braid; the Project braid is what peers exchange.

## Design decisions (locked in)

| Question | Decision |
|----------|----------|
| Project identity | Own `RealmId` — reuse realm/braid plumbing |
| Blob materialization | Manifest (JSON: path → blob-hash), materialize on open |
| Hierarchy semantics | Organizational grouping — agents grouped by Project; no new braid tier |
| Agent sandbox | Cooperative path-prefix checks in `agent_hooks.rs` |

## Model

```
Realm (RealmId = R)
 └── Projects (each has own RealmId = P₁, P₂, …)
      ├── project-manifest.json   (path → blob-hash, managed by engine)
      ├── <materialized files>    (from manifest; working copies)
      └── agents/
           ├── <agent-a>/         (sandbox root; also manifest-materialized subset)
           └── <agent-b>/
```

- `Project` is typed as a realm with `RealmCategory::Project { parent: R }` (new variant in `crates/synchronicity-engine/src/state.rs:88`).
- Parent Realm tracks child Project IDs in its own state, so "open realm" can surface its projects.
- Agents stay in `agent_hooks.rs` machinery — no new hook protocol; they just get a different working-tree root (a subfolder under the Project folder).

## Phases

### Phase 1 — Data model + manifest store *(no UI)*
1. New `ProjectManifest` struct + (de)serialization (`crates/indras-sync-engine/src/project/manifest.rs`):
   - `entries: BTreeMap<RelativePath, BlobHash>`
   - Versioned; stored as a blob itself so it rides existing blob sync.
2. New `Project` type in `indras-network` paralleling `Realm`: own `RealmId`, `parent_realm: RealmId`, manifest head ref.
3. `RealmCategory::Project { parent }` in `state.rs`.
4. Materializer: `fn materialize(manifest, dest_dir, blob_store) -> Result<()>`
   - First pass: copy blob bytes into the destination path.
   - Optimization later: hardlink when blob store and dest are on same filesystem.
5. Reverse pass: `fn snapshot(dest_dir) -> ProjectManifest` — hash files, insert into blob store, build new manifest.

Exit criteria: unit tests round-trip `materialize → edit file → snapshot → materialize` on a tmpdir.

### Phase 2 — Wiring into Realm / vault
1. `vault_manager.rs:209` (`vault_path`) extended with `project_path(realm_id, project_id)`.
2. On Project creation: allocate new `RealmId`, write empty manifest blob, register under parent realm.
3. On Project open: materialize manifest into `<realm_root>/projects/<project_id>/`.
4. On `/sync`: before staging, run `snapshot` on every open Project folder, diff against last manifest, update manifest blob if changed.
5. Reuse `land_agent_snapshot_on_first` semantics, but targeting the Project's braid instead of the Realm's inner braid.

Exit criteria: two peers, one Project, round-trip a file edit by landing a snapshot and re-materializing on the other peer.

### Phase 3 — Agent grouping
1. Agent creation UI takes a `project_id` (required) instead of realm_id-only.
2. Agent working-tree root = `<project_folder>/agents/<agent_slug>/`.
3. `agent_hooks.rs` `PreToolUse` hook rejects file operations whose resolved path escapes the agent's working-tree root (cooperative sandbox).
4. When the Project snapshots, agent subfolders are included — their edits ride up into the Project manifest naturally.

Exit criteria: agent in Project P₁ cannot write into Project P₂'s folder; edits by agent-a appear in snapshot of Project P₁ on peer B after sync.

### Phase 4 — UI
1. Agent Roster (recently re-styled) becomes scoped to a Project rather than a Realm.
2. Realm column shows Projects as children; expanding a Project reveals its agents.
3. "Create Project" inline affordance inside the Realm view (frictionless — no modal).

Exit criteria: manual test — create Realm → create Project → create Agent → agent works in the Project folder; peer sees it after sync.

## Out of scope (explicitly)

- **No new braid tier.** Hierarchical propagation = cooperative snapshot at Project boundary, then normal Realm/braid sync. If we later want per-agent braids, that's a separate plan.
- **No OS sandbox** (chroot, bubblewrap, sandbox-exec). Path-prefix check is the contract; agents that cheat break the contract, same as any cooperative system.
- **No symlink-on-disk materialization.** Real files, because symlink targets would differ per peer and across platforms.
- **No backward compatibility shims.** Greenfield — existing single-agent-per-realm flows migrate to "default Project per Realm" or are replaced.

## Open questions to revisit before Phase 4

1. Default Project per Realm on creation, or always explicit? (Frictionless default suggests "auto-create `main` Project".)
2. Does the Project manifest store file modes / executable bits? (Probably yes; cheap to include.)
3. Large files — threshold for hardlink vs. copy on materialize? (Defer; measure first.)
4. Agent rename / move across Projects — allowed? If so, how do we rewrite paths in hook configs?

## Files likely to change

- `crates/indras-network/src/realm.rs` — add `Project` sibling type, or extend `Realm` to carry `ProjectKind`.
- `crates/indras-sync-engine/src/project/` — new module: `manifest.rs`, `materialize.rs`, `snapshot.rs`.
- `crates/synchronicity-engine/src/state.rs` — `RealmCategory::Project { parent }`.
- `crates/synchronicity-engine/src/vault_manager.rs` — `project_path`, Project open/close lifecycle.
- `crates/synchronicity-engine/src/agent_hooks.rs` — cooperative path-prefix sandbox check.
- `crates/synchronicity-engine/src/components/realm_column.rs` + `agent_lane.rs` — UI for Projects inside Realm.
- `crates/synchronicity-engine/src/components/create_realm.rs` — `CreateProjectOverlay` (or inline variant).
