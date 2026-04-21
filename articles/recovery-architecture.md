# Frictionless recovery — architecture overview

This doc stitches together what Plans A, B, and C of the
`frictionless-recovery` plan implemented. It's written for
engineers picking up vault-integration follow-ups, not end users.

## The three layers

1. **Human handshake (Plan A).** DM-realm CRDT docs carry
   invitations, acceptances, recovery requests, and share
   releases. All copy is plain-language; the user never sees hex.
2. **Logical-account identity (Plan B).** An `AccountRoot`
   (Dilithium3 keypair) signs `DeviceCertificate`s. Published in
   a home-realm `DeviceRoster` doc. The root `sk` is envelope-
   sealed (ChaCha20-Poly1305) under a 32-byte wrapping key W; W
   is Shamir-split across stewards and otherwise discarded.
3. **Personal-data durability (Plan C).** Files are encrypted
   with a per-file key K; K is envelope-sealed under W; the
   encrypted bytes are Reed-Solomon erasure-coded across Backup
   Peers. Any K-of-N shards reconstruct the ciphertext; any
   steward quorum reassembles W; W unwraps every per-file key.

In short: one secret W holds the whole thing together. Stewards
guard W. Losing a device is a re-pull + re-derive, not a data
loss.

## CRDT doc keys, at a glance

| Key                                       | Writer       | Realm        | Purpose                                             |
|-------------------------------------------|--------------|--------------|-----------------------------------------------------|
| `_steward_invite:{sender_uid}`            | sender       | DM           | "Will you be my backup friend?"                      |
| `_steward_accept:{sender_uid}`            | steward      | DM           | Yes/no + steward's fresh KEM ek                      |
| `_steward_share:{sender_uid}`             | sender       | DM           | Sender's K-of-N Shamir share, sealed to steward's ek |
| `_recovery_request:{new_device_uid}`      | new device   | DM           | "Please help me recover"                             |
| `_share_release:{new_device_uid}`         | steward      | DM           | Steward's share, re-sealed to the new device's ek    |
| `_device_roster`                          | any trusted  | home         | Root `vk` + list of signed device certs             |
| `_account_root_envelope`                  | sender       | home         | Root `sk` sealed under W                             |
| `_backup_role:{sender_uid}`               | sender       | DM           | "You're one of my Backup Peers" role assignment      |
| `_file_shard:{file_id_hex}:{shard_index}` | sender       | DM           | One Reed-Solomon shard + per-file-key envelope       |

Every doc merges via last-writer-wins on `*_at_millis`, except the
`DeviceRoster` which upserts per device (so revocations compose
cleanly with additions).

## Lifecycle — end to end

### Account creation (Plan B)

1. User picks a display name.
2. Device generates a fresh PQ identity, builds `IndrasNetwork`.
3. `bootstrap_account_root` fires: generate `AccountRoot`, sign
   initial `DeviceCertificate`, publish `DeviceRoster` into the
   home realm, stash `sk` in `account_root.pending`.
4. User opens the Backup-plan overlay and invites friends.

### Steward enrollment (Plan A)

1. User taps "Ask" on a peer; the device publishes
   `_steward_invite:{my_uid}` into the peer's DM realm.
2. Peer's inbox surfaces the request; on Accept, the peer writes
   `_steward_accept:{my_uid}` with their fresh KEM ek.
3. The Backup-plan overlay watches the accepted fingerprint and,
   once ≥ K peers have agreed, calls `finalize_steward_split`.

### Root split (Plan B)

1. `finalize_steward_split` loads the pending root from the
   cache, generates a random wrapping key W, seals the root into
   `_account_root_envelope`, and publishes that doc.
2. It Shamir-splits W into K-of-N shares, encrypts each share to
   the corresponding steward's accepted KEM ek, and writes
   `_steward_share:{my_uid}` per peer.
3. On quorum delivery, `account_root.pending` is deleted — W now
   lives only as steward shares.

### File publication (Plan C primitive)

1. For each file write, draw a per-file key K and a fresh nonce.
2. Encrypt file bytes with K (ChaCha20-Poly1305).
3. Reed-Solomon encode the ciphertext into `data_threshold +
   parity` shards.
4. Seal K under W (ChaCha20-Poly1305 with a fresh nonce).
5. Publish one `_file_shard:*` per Backup Peer carrying shard
   bytes + the per-file-key envelope.

### Recovery on a new device (Plan A + B + C)

1. New device creates its own PQ identity. Out-of-band, the user
   asks each steward to add this new identity as a DM peer.
2. New device publishes `_recovery_request:{new_device_uid}` in
   each steward's DM realm.
3. Steward UIs surface "Alex is asking to recover" with a plain-
   language prompt to verify out-of-band (call, video, in person).
4. On approval, each steward decrypts their share, re-wraps it to
   the new device's KEM ek, and publishes `_share_release:*`.
5. `poll_recovery_releases` collects K releases; the new device
   decrypts each share, Shamir-reassembles W.
6. **Identity**: fetch `_account_root_envelope`, unseal with W to
   recover root `sk`, sign a fresh `DeviceCertificate`, upsert
   into `_device_roster`, drop `sk`.
7. **Data**: for each file, fetch all surviving `_file_shard:*`
   docs, unwrap K with W, Reed-Solomon decode the ciphertext,
   ChaCha20-decrypt the file bytes, write to the vault.
8. **Shared realms**: peers observing the new device cert signed
   by the trusted root re-admit it automatically (pending the
   network-layer wiring tracked in C.4–C.6).

## Security posture

| Secret                 | Where it lives                                              | Exposure window                                  |
|------------------------|-------------------------------------------------------------|--------------------------------------------------|
| `AccountRoot.sk`       | `account_root.pending` (disk) + in memory                   | From account creation until first steward quorum |
| Wrapping key W         | Stewards' Shamir shares                                     | Only in memory on sign / recover operations     |
| Per-file key K         | Envelope ciphertext in every `_file_shard:*`                | Only in memory during encrypt / decrypt          |
| Steward Shamir share   | Each steward's `steward_holdings.json` + DM-realm doc       | At rest on steward devices                       |
| File bytes             | Vault filesystem (plaintext) + Backup-Peer shards (AEAD)    | Plaintext only on sender + recovered-devices     |

Compromise scenarios — each mitigated by the layer above:

- Stolen Backup Peer shards → opaque ciphertext; need K peers **and** W.
- Stolen K steward shares → reconstruct W; still need the
  envelope ciphertext, which lives only in the account's home
  realm (peers who never co-resided with the account can't see it).
- Stolen root `sk` pre-split → account compromised until the split
  completes. This is the security window `account_root.pending`
  opens; Plan D+ tightens it with hardware-key-wrapped storage.

## Deferred / follow-up

- Multi-peer DM-realm E2E tests — need a `DirectConnect` harness
  in `indras-network/tests`.
- Vault save hook + recovery re-pull UI — touches code paths
  Plan/CLAUDE.md reserves for agent1.
- Shared-realm admission wiring via `peer_verification` — one-
  function change at the network layer, outside this plan.
- Inline migration for Phase-1 accounts — see
  `docs/migrations/account-root.md`.
- Threshold signatures (FROST for Dilithium) to eliminate the
  reassembled-root exposure window entirely.

## File references

- `crates/indras-crypto/src/account_root.rs`
- `crates/indras-crypto/src/device_cert.rs`
- `crates/indras-crypto/src/erasure.rs`
- `crates/indras-sync-engine/src/steward_enrollment.rs`
- `crates/indras-sync-engine/src/recovery_protocol.rs`
- `crates/indras-sync-engine/src/share_delivery.rs`
- `crates/indras-sync-engine/src/device_roster.rs`
- `crates/indras-sync-engine/src/account_root_envelope.rs`
- `crates/indras-sync-engine/src/account_root_cache.rs`
- `crates/indras-sync-engine/src/backup_peers.rs`
- `crates/indras-sync-engine/src/file_shard.rs`
- `crates/indras-sync-engine/src/peer_verification.rs`
- `crates/synchronicity-engine/src/recovery_bridge.rs`
- `crates/synchronicity-engine/src/components/recovery_setup.rs`
- `crates/synchronicity-engine/src/components/recovery_use.rs`
- `crates/synchronicity-engine/src/components/steward_inbox.rs`
