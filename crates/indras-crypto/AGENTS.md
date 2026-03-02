# indras-crypto

Cryptographic primitives for Indras Network. Handles symmetric encryption of interface
events, post-quantum key encapsulation and signatures for member onboarding, content hashing
for blob storage, and human-memorable mnemonic key derivation via pass-story.

## Purpose

Provide a layered crypto stack: ChaCha20-Poly1305 for high-throughput symmetric encryption,
ML-KEM-768 (FIPS 203) for quantum-resistant key transport, ML-DSA-65 (FIPS 204) for message
authentication, and Argon2id/HKDF for deterministic key derivation from mnemonic phrases.

## Module Map

| Module | Contents |
|---|---|
| `interface_key` | `InterfaceKey`, `EncryptedData`, `EncapsulatedKey`; ChaCha20-Poly1305 ops |
| `key_distribution` | `KeyDistribution`, `KeyInvite`, `FullInvite`, `InviteMetadata`; member onboarding |
| `pq_identity` | `PQIdentity`, `PQPublicIdentity`, `PQSignature`; ML-DSA-65 signing |
| `pq_kem` | `PQKemKeyPair`, `PQEncapsulationKey`, `PQCiphertext`; ML-KEM-768 KEM |
| `artifact_encryption` | `ArtifactKey`, `EncryptedArtifact`; per-blob envelope encryption |
| `pass_story` | `StorySubkeys`; Argon2id + HKDF key derivation from a story phrase |
| `story_template` | `PassStory`, `StoryTemplate`, `StoryStage`; mnemonic template engine |
| `word_frequencies` | Frequency-weighted word list used by template generation |
| `entropy` | Entropy helpers (CSPRNG wrappers) |
| `error` | `CryptoError`, `CryptoResult` |

## Key Types

- **`InterfaceKey`** — 32-byte ChaCha20-Poly1305 key scoped to one `InterfaceId`. `encrypt`
  and `decrypt` methods append/consume a random 12-byte nonce prepended to ciphertext.
- **`KeyDistribution`** — static methods `create_invite` / `accept_invite` to wrap an
  `InterfaceKey` in a ML-KEM ciphertext addressed to a recipient's encapsulation key.
- **`KeyInvite`** / **`FullInvite`** — wire types carrying an ML-KEM ciphertext + metadata;
  `FullInvite` bundles interface and member metadata alongside the encrypted key.
- **`PQIdentity`** — ML-DSA-65 signing keypair; `sign(&msg)` → `PQSignature`.
- **`PQPublicIdentity`** — verifying key only; used by peers that receive signed messages.
- **`PQKemKeyPair`** — ML-KEM-768 keypair; `encapsulation_key()` exposes the public half for
  sharing; `decapsulate(ct)` recovers the shared secret.
- **`ArtifactKey`** — random 32-byte key for encrypting a single blob; derived once per
  artifact and then itself encrypted via `InterfaceKey` for distribution.
- **`PassStory`** / **`StoryTemplate`** — mnemonic structures; a story is a fixed template
  populated with weighted-random words, producing a human-speakable passphrase.
- **`StorySubkeys`** — output of Argon2id + HKDF over a `PassStory`; yields multiple
  independent subkeys (encryption, signing, etc.).

## Key Patterns

- **Nonce handling**: `InterfaceKey::encrypt` generates a fresh nonce per call and prepends
  it; `decrypt` reads the first `NONCE_SIZE` bytes. Never reuse nonces manually.
- **Two-phase KEM invite**: sender calls `create_invite(interface_key, recipient_ek)` →
  `KeyInvite`; recipient calls `accept_invite(invite, kem_keypair)` → `InterfaceKey`. No
  shared state required between phases.
- **Zeroize on drop**: `InterfaceKey`, `StorySubkeys`, and `PQKemKeyPair` implement `Zeroize`
  so secret bytes are wiped when they go out of scope.
- **`SecureBytes`** wraps raw key material and implements `Zeroize`; use it instead of `Vec`
  for any intermediate secret values.
- **Deprecated x25519**: `PublicKey` and `StaticSecret` from `x25519-dalek` are re-exported
  with a `#[deprecated]` annotation for legacy callers. New code must use ML-KEM.

## Gotchas

- `pqcrypto-kyber` is the `pqcrypto` crate for ML-KEM (Kyber). The API uses its own
  `pqcrypto_traits` interfaces — don't confuse with the `kyber` crate or `ml-kem` crate.
- `ARTIFACT_KEY_SIZE` is a constant (32); `KEY_SIZE` and `NONCE_SIZE` are separate constants
  for the interface key — check which one you need before hardcoding a length.
- `ExportedKey` is `#[deprecated]` — it serializes the raw key bytes and was replaced by
  the ML-KEM invite system.
- Argon2id in `pass_story` is intentionally slow (memory-hard). Do not call on the async
  executor thread; use `tokio::task::spawn_blocking`.
- BLAKE3 hashing in `artifact_encryption::hash_content` is synchronous and fast; safe to
  call inline.

## Dependencies

| Crate | Use |
|---|---|
| `indras-core` | `InterfaceId`, shared types |
| `chacha20poly1305` | Symmetric AEAD |
| `pqcrypto-kyber` | ML-KEM-768 (FIPS 203) |
| `pqcrypto-dilithium` | ML-DSA-65 (FIPS 204) |
| `pqcrypto-traits` | Common PQ trait interfaces |
| `x25519-dalek` | Legacy KEM (deprecated) |
| `blake3` | Content hashing for blobs |
| `argon2` | Memory-hard KDF for pass-story |
| `hkdf` + `sha2` | Key expansion from Argon2id output |
| `zeroize` | Secure memory zeroing |
| `unicode-normalization` | NFC passphrase normalisation before KDF |
| `serde` + `postcard` | Wire serialisation of invite types |

## Testing

```bash
cargo test -p indras-crypto
cargo bench -p indras-crypto   # runs crypto_benchmarks (criterion)
```

- Tests are inline in each module under `#[cfg(test)]`.
- The criterion benchmark suite (`benches/crypto_benchmarks.rs`) covers encrypt/decrypt
  and KEM encapsulate/decapsulate throughput.
- For KEM round-trip tests: generate a `PQKemKeyPair`, call `create_invite`, then
  `accept_invite` and assert the recovered key matches the original.
