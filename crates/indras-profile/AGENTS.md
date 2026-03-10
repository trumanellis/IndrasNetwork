# indras-profile

Lightweight types crate for IndrasNetwork member profiles. Intentionally dependency-free
(only `serde`) so it can be imported by any crate without pulling in the full artifact stack.

## Purpose

Defines the canonical `Profile` shape and its visibility system. A profile describes a member
to the outside world — identity, activity, social links, and content — with per-field access
control baked in via `Visible<T>`.

## Key Types

### `Profile`
Rich member profile with four logical sections:

- **Identity** — display name, bio, avatar URL, location, website
- **Activity** — join timestamp, last-seen, gifts given/received counts, active intentions count
- **Social** — linked accounts (e.g. GitHub, Fediverse handles)
- **Content** — pinned intention summaries, recent gift highlights

Every field is wrapped in `Visible<T>`, so each piece of data carries its own access level.

### `Visible<T>`
Generic wrapper pairing a value with a `Visibility` setting:

```rust
pub struct Visible<T> {
    pub value: T,
    pub visibility: Visibility,
}
```

Enables per-field privacy rather than a single profile-wide setting.

### `Visibility`
Per-field access level:

- `Public` — visible to anyone
- `Connections` — visible to members who have an accepted gift exchange with this member
- `Private` — visible only to the member themselves (owner)

### `ViewLevel`
The resolved access level of the viewer requesting a profile:

- `Public` — unauthenticated or unrecognized requester
- `Connection` — requester is a confirmed connection of the profile owner
- `Owner` — requester is the profile owner

`ViewLevel` is compared against `Visibility` when rendering to decide which fields to expose.
Grant-to-`ViewLevel` resolution is **not** done here — see `indras-homepage`.

### `IntentionSummary`
Lightweight summary of an intention for embedding in a profile (title, status, created_at).
Avoids depending on the full intention type from `indras-artifacts`.

### `PROFILE_ARTIFACT_NAME`
Well-known constant `"_profile"`. All tooling that locates a member's profile artifact uses
this name as the lookup key within the home realm's `ArtifactIndex`.

### `profile_artifact_id(member_key)`
Derives a stable, deterministic artifact ID from a member's public key:

```
"indras:profile:" + hex(member_key_bytes[..17])
```

The 17-byte prefix keeps IDs short while remaining collision-resistant for practical network
sizes. IDs are stable across restarts because they depend only on the key, not on state.

## Architecture

```
indras-profile  (types only, serde dep)
    ↑
    ├── indras-homepage   (renders profile, resolves grant → ViewLevel, serves HTTP)
    ├── indras-gift-cycle (creates profile artifact, updates live stats)
    └── indras-node       (optionally mounts homepage server)
```

**Storage model:** Each member's home realm contains exactly one profile artifact
(1-to-1 relationship). It is stored under `PROFILE_ARTIFACT_NAME` in the realm's
`ArtifactIndex`. The artifact's grant list is the source of truth for Connection-level
access — any member holding a grant gets `ViewLevel::Connection` when fetching the profile.

**Why grant resolution lives in `indras-homepage`:** Resolving a grant list requires reading
from `indras-artifacts`, which would add a heavy dependency to this types crate and break the
lightweight contract. `indras-homepage` already depends on artifacts and is the only consumer
that actually needs to serve rendered profiles.

## Conventions

- All fields use `Visible<T>` — no bare fields on `Profile`
- `profile_artifact_id` is the single canonical way to derive a profile's artifact ID
- Do not add business logic here; this crate is a pure data-shape library
