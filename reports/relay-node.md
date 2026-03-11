# Indras Relay Node — User's Guide

The relay is a blind store-and-forward server for the Indras P2P mesh. It holds encrypted events on behalf of peers who may be offline, without ever seeing their content. Each relay is owned by one player (personal server mode) or open to a community.

## Quick Start

```bash
# Run with defaults (creates relay-data/ in current directory)
cargo run -p indras-relay

# Run with a config file
cargo run -p indras-relay -- --config relay.toml

# Override data directory
cargo run -p indras-relay -- --data-dir /var/lib/indras-relay
```

On first run the relay generates an iroh identity key (`secret.key`) and an empty `events.redb` database in the data directory. The node ID is logged at startup — peers need this to connect.

## CLI Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--config <PATH>` | `-c` | `relay.toml` | TOML config file (missing file uses all defaults) |
| `--data-dir <PATH>` | `-d` | from config | Overrides `data_dir` in config |
| `--admin-bind <ADDR>` | | from config | Overrides `admin_bind` in config |

CLI flags take precedence over the config file.

## Deployment Modes

### Personal Server

Set `owner_player_id` to your 64-character hex PlayerId. The relay then assigns three storage tiers based on who connects:

| Who | Tier | Default Quota | Default TTL |
|-----|------|---------------|-------------|
| You (owner) | Self_ | 1 GB | 365 days |
| Your contacts | Connections | 500 MB | 90 days |
| Everyone else | Public | 50 MB | 7 days |

### Community Server

Set `community_mode = true` and omit `owner_player_id`. All authenticated peers get Public tier unless they appear in the contacts list (which grants Connections tier).

## Configuration Reference

Create a `relay.toml` file. All fields are optional — defaults are shown below.

```toml
# Where to store the database, identity key, and contacts
data_dir = "./relay-data"

# Display name (shown in /health and logs)
display_name = "indras-relay"

# Admin HTTP API
admin_bind = "127.0.0.1:9090"
admin_token = "change-me"          # CHANGE THIS for production

# Personal server mode (omit for community mode)
# owner_player_id = "abcd1234...64 hex chars..."
# community_mode = false

[quota]
default_max_bytes_per_peer = 104857600      # 100 MB
default_max_interfaces_per_peer = 50
global_max_bytes = 10737418240              # 10 GB

[storage]
default_event_ttl_days = 90
max_event_ttl_days = 365
cleanup_interval_secs = 3600                # 1 hour

[tiers]
self_max_bytes = 1073741824                 # 1 GB
self_ttl_days = 365
self_max_interfaces = 100

connections_max_bytes = 524288000           # 500 MB
connections_ttl_days = 90
connections_max_interfaces = 200

public_max_bytes = 52428800                 # 50 MB
public_ttl_days = 7
public_max_interfaces = 50
```

## Authentication

Peers authenticate by presenting a signed credential that links their profile-layer PlayerId to their transport-layer iroh identity. The relay validates:

1. **Credential parsing** — postcard-encoded `CredentialV1` + 64-byte Ed25519 signature
2. **Identity match** — the credential's `player_id` matches the claimed identity
3. **Transport match** — the credential's `transport_pubkey` matches the connecting peer's iroh key
4. **Expiry** — `expires_at_millis` is in the future
5. **Signature** — Ed25519 verification using `player_id` as the public key

After authentication, the relay assigns tiers based on the peer's relationship to the owner (see Deployment Modes above). Tier access is cumulative: Self_ includes Connections + Public access.

## Contacts Management

The contacts list determines who gets Connections tier. It can be managed two ways:

**Via protocol (recommended):** The relay owner sends a `RelayContactsSync` message containing player IDs extracted from their profile artifact's grant list. The relay accepts this only from Self_-tier peers, replaces its contact list, and persists to `contacts.json`.

**Manually:** Edit `contacts.json` in the data directory. Format is a JSON array of 64-character hex strings:

```json
[
  "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234",
  "ef56789aef56789aef56789aef56789aef56789aef56789aef56789aef56789a"
]
```

The relay loads `contacts.json` on startup. Changes via the protocol are persisted immediately.

## Three Staging Areas

The relay's central feature is its three-tier storage model. Every piece of data lives in exactly one staging area, each with its own quota, retention policy, and access rules. The tiers map directly to the profile visibility model used by `indras-homepage`:

```
┌─────────────────────────────────────────────────────┐
│                    RELAY NODE                        │
│                                                      │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────┐  │
│  │   Self_      │  │ Connections  │  │  Public    │  │
│  │             │  │              │  │            │  │
│  │  Owner's    │  │  Contact     │  │  Network   │  │
│  │  backups,   │  │  realm sync, │  │  announce- │  │
│  │  pinned     │  │  encrypted   │  │  ments,    │  │
│  │  data,      │  │  S&F, data   │  │  discovery │  │
│  │  cross-     │  │  custody     │  │  broadcast │  │
│  │  device     │  │              │  │            │  │
│  │  sync       │  │              │  │            │  │
│  │             │  │              │  │            │  │
│  │  1 GB       │  │  500 MB      │  │  50 MB     │  │
│  │  365 days   │  │  90 days     │  │  7 days    │  │
│  │  100 ifaces │  │  200 ifaces  │  │  50 ifaces │  │
│  └─────────────┘  └──────────────┘  └────────────┘  │
│                                                      │
│  ViewLevel:Owner   ViewLevel:Connection  ViewLevel:  │
│  (player_id ==     (in profile artifact  Public      │
│   owner_id)         grant list)          (everyone)  │
└─────────────────────────────────────────────────────┘
```

### How tiers are assigned

When a peer authenticates, the relay checks their `player_id` against:

1. **Self_** — `player_id == owner_player_id` in config. This is you, the relay owner.
2. **Connections** — `player_id` appears in the contacts list (loaded from `contacts.json` or synced via `RelayContactsSync`). These are peers you've granted access to in your profile artifact.
3. **Public** — everyone else with a valid credential.

Tier access is cumulative: a Self_ peer can store into all three tiers. A Connections peer can store into Connections and Public. A Public peer can only store into Public.

### What goes where

Each tier serves a distinct purpose:

**Self_ (owner backup)**
- Cross-device sync — your phone pushes data here, your laptop retrieves it
- Backup pinning — long-term storage of your own artifacts
- Content type: `Backup` via `RelayStore`

**Connections (contact relay)**
- Realm sync — encrypted interface events for your mutual contacts
- Store-and-forward — gossip events captured while a contact is offline
- Data custody — a contact can transfer data to your relay for safekeeping
- Content types: `Event` (automatic via gossip), `CustodyTransfer` via `RelayStore`

**Public (network broadcast)**
- Discovery announcements — presence info for the wider network
- Public broadcasts — content intended for anyone
- Content type: `Broadcast` via `RelayStore`

### Storage isolation

Each tier has physically separate database tables (`self_events`, `conn_events`, `pub_events`). This means:

- **Independent quotas** — filling up Public storage doesn't affect Self_ or Connections capacity
- **Independent TTLs** — Public data expires in 7 days while Self_ data persists for a year
- **Independent cleanup** — the background cleanup task runs each tier's TTL independently
- **Per-tier usage tracking** — the admin API (`/stats`) reports `self_bytes`, `connections_bytes`, and `public_bytes` separately

### Explicit storage via RelayStore

Beyond automatic gossip capture (which always goes to Connections), peers can explicitly push data into a specific tier using `RelayStore`:

```
RelayStore {
    tier: StorageTier,        // which staging area
    interface_id: InterfaceId,
    data: Vec<u8>,            // opaque encrypted blob
    metadata: StoreMetadata {
        content_type: Event | Backup | CustodyTransfer | Broadcast,
        pin: bool,            // request pinning (Self_ tier only)
        ttl_override_days: Option<u64>,  // override default TTL
    },
}
```

The relay checks: (1) the peer is authenticated, (2) the peer's tier grants access to the target tier, (3) the tier quota isn't exceeded. Violations are rejected with a descriptive reason in `RelayStoreAck`.

**Pin and TTL overrides are honored:**
- `pin: true` (Self_ tier only) marks the event as pinned — pinned events survive cleanup and are never auto-deleted by the background TTL task
- `ttl_override_days` overrides the tier's default TTL for that specific event — useful for extending retention of important data beyond the tier default

Both are stored in dedicated redb tables (`pinned_events`, `ttl_overrides`) and checked during each cleanup cycle.

### Connecting to the profile grant system

The contacts list that drives Connections tier access is derived from the owner's profile artifact grants. The flow:

1. Owner grants access to peers via `AccessGrant` in their profile artifact
2. Client extracts active grantee IDs using `extract_contact_ids(grants, now)` — this filters expired `Timed` grants and deduplicates
3. Client sends `RelayContactsSync { contacts }` to their relay
4. Relay validates the sender has Self_ tier, replaces its contacts DashMap, and persists to `contacts.json`
5. New connections from those player IDs now receive Connections tier

This makes the profile artifact the single source of truth for relay access — grant someone profile visibility and they automatically get relay storage access.

## How Store-and-Forward Works

1. A peer authenticates and registers interface IDs with the relay
2. The relay subscribes to iroh-gossip topics for those interfaces
3. When any peer publishes an `InterfaceEvent` to gossip, the relay captures and stores it
4. When the registered peer reconnects, they send `RelayRetrieve` to fetch missed events (optionally specifying which tier to retrieve from — defaults to Connections)
5. The relay delivers stored events and the peer is caught up

The relay never decrypts events — it stores opaque encrypted blobs and nonces. It has no interface keys.

Peers can also explicitly push data via `RelayStore` messages, targeting a specific tier for backups, custody transfers, or broadcast announcements.

## Protocol Messages

All communication happens over QUIC (`indras/1` ALPN) with framed postcard-serialized messages.

| Message | Direction | Auth | Purpose |
|---------|-----------|------|---------|
| `RelayAuth` | peer -> relay | No | Present credential |
| `RelayAuthAck` | relay -> peer | -- | Tier grants + quota info |
| `RelayRegister` | peer -> relay | Yes | Register interfaces for S&F |
| `RelayRegisterAck` | relay -> peer | -- | Accepted/rejected interfaces |
| `RelayUnregister` | peer -> relay | No | Remove registrations |
| `RelayRetrieve` | peer -> relay | Yes | Fetch stored events (tier-aware) |
| `RelayDelivery` | relay -> peer | -- | Batch of stored events |
| `RelayStore` | peer -> relay | Yes | Push data to a specific tier |
| `RelayStoreAck` | relay -> peer | -- | Accept/reject with reason |
| `RelayContactsSync` | peer -> relay | Self_ | Replace contacts list |
| `RelayContactsSyncAck` | relay -> peer | -- | Confirmation + count |
| `Ping` / `Pong` | both | No | Keepalive |

## Admin API

HTTP API at `admin_bind` (default `127.0.0.1:9090`). All endpoints except `/health` require `Authorization: Bearer <admin_token>`.

### GET /health (no auth)

```bash
curl http://localhost:9090/health
```

```json
{
  "status": "ok",
  "uptime_secs": 3600,
  "display_name": "indras-relay"
}
```

### GET /stats

```bash
curl -H "Authorization: Bearer change-me" http://localhost:9090/stats
```

```json
{
  "peer_count": 5,
  "interface_count": 12,
  "total_events": 1847,
  "total_storage_bytes": 2457600,
  "self_bytes": 1024000,
  "connections_bytes": 1228800,
  "public_bytes": 204800
}
```

### GET /peers

```bash
curl -H "Authorization: Bearer change-me" http://localhost:9090/peers
```

Returns an array of registered peers with their interface count, registration time, last seen time, and granted tiers.

### GET /interfaces

```bash
curl -H "Authorization: Bearer change-me" http://localhost:9090/interfaces
```

Returns an array of registered interfaces with event count and storage bytes.

### GET /contacts

```bash
curl -H "Authorization: Bearer change-me" http://localhost:9090/contacts
```

```json
{
  "contacts": [
    "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234"
  ],
  "count": 1
}
```

### PUT /contacts

```bash
curl -X PUT -H "Authorization: Bearer change-me" \
  -H "Content-Type: application/json" \
  -d '{"contacts":["abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234"]}' \
  http://localhost:9090/contacts
```

Replaces the entire contacts list. Returns the updated list and count.

## Data Directory Layout

```
relay-data/
  secret.key            # 32-byte iroh Ed25519 identity (generated once)
  events.redb           # redb database (8 tables: events + usage per tier, plus pinned_events + ttl_overrides)
  registrations.json    # Peer/interface registrations (survives restarts)
  contacts.json         # Owner's contacts list (survives restarts)
```

## Storage and Cleanup

Events are stored in separate redb tables per tier (`self_events`, `conn_events`, `pub_events`). Each tier has its own TTL from the `[tiers]` config.

A background cleanup task runs every `cleanup_interval_secs` (default 1 hour) and removes events older than each tier's TTL. Cleanup runs independently per tier so Self_ data (365 days) lives much longer than Public data (7 days).

Per-interface storage usage is tracked atomically alongside each insert and available through the admin API. Pinned events (set via `RelayStore` with `pin: true`) are never cleaned up regardless of age. Events with TTL overrides use their custom TTL instead of the tier default during cleanup.

## Operational Notes

- **Identity key:** Deleting `secret.key` forces a new identity on next startup. All peers must reconnect to the new node ID.
- **Graceful shutdown:** Ctrl-C triggers graceful shutdown of the admin server and cleanup task.
- **Quota resets:** In-memory quota counters reset on restart (storage usage is recalculated from the database, but per-peer registration counts start fresh).
- **Gossip persistence:** On restart, the relay re-subscribes to all gossip topics from `registrations.json` before accepting new connections, minimizing the window for missed events.
- **Logging:** Controlled by `RUST_LOG` env var. Default: `info` for `indras_relay`. Set `RUST_LOG=indras_relay=debug` for verbose output.
