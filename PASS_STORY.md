# Pass Story Authentication — Implementation Roadmap

## 1. Overview

Pass story authentication replaces traditional passwords and seed phrases with a **23-slot autobiographical narrative** based on the hero's journey. The user fills in a past-tense template with real memories from their own life. The 23 slot values are concatenated and fed through Argon2id to derive cryptographic keys.

### The compositional security model

The 23 slots are **non-decomposable**: the KDF takes the entire concatenated story as a single input and produces a single output. There is no oracle for individual slots. An attacker who gets 22 slots right and 1 wrong gets the same result as one who gets all 23 wrong — *nothing*.

This means the attack cost is the **product** of per-slot possibilities, not the sum:

| Per-slot entropy | Decomposable (sum) | Non-decomposable (product) |
|------------------|---------------------|----------------------------|
| 10 bits (1,024 choices) | 23 × 1,024 = 23,552 | 1,024^23 = **2^230** |
| 12 bits (4,096 choices) | 23 × 4,096 = 94,208 | 4,096^23 = **2^276** |

Both 2^230 and 2^276 exceed NIST Post-Quantum Level 1 after Grover's halving (2^115 and 2^138 quantum bits respectively).

### How it fits into SyncEngine

Pass story auth extends the existing identity system. It does **not** replace ML-KEM or ML-DSA — it provides a human-memorable way to derive (or encrypt) those keys. The story-derived key encrypts the existing PQ key material at rest, following the same pattern as `EncryptedKeystore` but with higher KDF parameters and a richer input space.

---

## 2. Architecture Diagram

```
User's autobiographical story (23 slots)
          │
          ▼
┌─────────────────────┐
│   Normalization      │  Unicode NFC, lowercase, whitespace collapse
│   (pass_story.rs)    │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│  Canonical Encoding  │  slot_1 \x00 slot_2 \x00 ... \x00 slot_23
│   (pass_story.rs)    │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│  Entropy Estimation  │  Slot-aware frequency model → reject if < 256 bits
│   (entropy.rs)       │
└─────────┬───────────┘
          │ (passes gate)
          ▼
┌─────────────────────┐
│  Argon2id KDF        │  256MB memory, 4 iterations, parallelism 4
│   (pass_story.rs)    │  salt = user_id ∥ registration_timestamp
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│  HKDF-SHA512         │  4 subkeys: identity, encryption, signing, recovery
│   (pass_story.rs)    │
└─────────┬───────────┘
          │
          ▼
┌─────────────────────────────────────────────────────┐
│              Existing SyncEngine Primitives           │
│                                                       │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │
│  │  ML-KEM-768  │  │  ML-DSA-65   │  │ ChaCha20-  │ │
│  │  (pq_kem.rs) │  │(pq_identity) │  │ Poly1305   │ │
│  └──────────────┘  └──────────────┘  └────────────┘ │
│                                                       │
│  Story-derived key encrypts PQ keys at rest           │
│  (same pattern as EncryptedKeystore)                  │
└───────────────────────────────────────────────────────┘
```

### Integration with existing crates

| Crate | Role | Reused primitives |
|-------|------|-------------------|
| `indras-crypto` | New modules live here | `SecureBytes`, `PQIdentity`, `PQKemKeyPair`, `CryptoError`, BLAKE3 `hash_content()`, ChaCha20-Poly1305 encrypt/decrypt |
| `indras-node` | Extended keystore | `EncryptedKeystore`, `Keystore`, Argon2id (already a dependency) |
| `indras-network` | Builder API + auth flows | `NetworkBuilder`, `NetworkConfig`, `IndraError` |

---

## 3. Phase 1 — Core Crypto Pipeline

### New module: `crates/indras-crypto/src/pass_story.rs`

**New dependencies** (add to `crates/indras-crypto/Cargo.toml`):

```toml
argon2 = "0.5"
hkdf = "0.12"
sha2 = "0.10"
unicode-normalization = "0.1"
```

Note: `argon2` is already a dependency of `indras-node`. Adding it to `indras-crypto` keeps the KDF pipeline self-contained.

### Functions

```rust
use crate::error::{CryptoError, CryptoResult};
use crate::SecureBytes;

/// Number of slots in a pass story.
pub const STORY_SLOT_COUNT: usize = 23;

/// Minimum total entropy in bits for story acceptance.
pub const MIN_ENTROPY_BITS: f64 = 256.0;

/// Argon2id parameters (higher than EncryptedKeystore's OWASP defaults).
pub const ARGON2_MEMORY_KIB: u32 = 262_144;   // 256 MB
pub const ARGON2_ITERATIONS: u32 = 4;
pub const ARGON2_PARALLELISM: u32 = 4;
pub const ARGON2_OUTPUT_LEN: usize = 64;       // 512-bit master key

/// Normalize a single slot value.
///
/// - Unicode NFC normalization
/// - Lowercase
/// - Whitespace collapsed to single spaces
/// - Leading/trailing whitespace stripped
pub fn normalize_slot(raw: &str) -> String;

/// Concatenate 23 normalized slots with \x00 delimiter.
/// Returns error if slot count != 23 or any slot contains \x00.
pub fn canonical_encode(slots: &[String; STORY_SLOT_COUNT]) -> CryptoResult<Vec<u8>>;

/// Derive a 512-bit master key from the canonical story encoding.
///
/// Salt should be `user_id || registration_timestamp`.
/// Returns SecureBytes (zeroized on drop).
pub fn derive_master_key(canonical: &[u8], salt: &[u8]) -> CryptoResult<SecureBytes>;

/// Expand master key into 4 purpose-specific subkeys via HKDF-SHA512.
///
/// Returns (identity_key, encryption_key, signing_key, recovery_key),
/// each 32 bytes.
pub struct StorySubkeys {
    pub identity:   SecureBytes,  // 32 bytes
    pub encryption: SecureBytes,  // 32 bytes
    pub signing:    SecureBytes,  // 32 bytes
    pub recovery:   SecureBytes,  // 32 bytes
}

pub fn expand_subkeys(master_key: &SecureBytes) -> CryptoResult<StorySubkeys>;

/// Full pipeline: normalize → encode → derive → expand.
pub fn derive_keys_from_story(
    raw_slots: &[&str; STORY_SLOT_COUNT],
    salt: &[u8],
) -> CryptoResult<StorySubkeys>;

/// Generate a verification token (BLAKE3 hash of the derived master key).
/// Stored server-side to verify authentication without storing the story.
pub fn story_verification_token(master_key: &SecureBytes) -> [u8; 32];
```

### Integration approach

The story-derived `encryption` subkey encrypts the existing PQ key material at rest, extending the `EncryptedKeystore` pattern:

1. On **account creation**: generate random PQ keys (ML-KEM-768, ML-DSA-65) as today
2. Derive story subkeys from the user's pass story
3. Encrypt PQ private keys with the story-derived encryption key (ChaCha20-Poly1305)
4. Store encrypted keys + salt + verification token
5. On **authentication**: re-derive story subkeys → decrypt PQ keys → proceed as normal

This means the PQ keys themselves remain random (maximum entropy), while the story provides the human-memorable encryption layer.

---

## 4. Phase 2 — Story Template Engine

### New module: `crates/indras-crypto/src/story_template.rs`

### Types

```rust
/// A single stage of the hero's journey.
#[derive(Debug, Clone)]
pub struct StoryStage {
    pub name: &'static str,          // e.g., "The Ordinary World"
    pub description: &'static str,   // e.g., "where you came from"
    pub template: &'static str,      // e.g., "I grew up in `_____`, where I was a `_____`."
    pub slot_count: usize,           // Number of blanks in this stage (1-3)
}

/// The complete hero's journey template (11 stages, 23 slots).
pub struct StoryTemplate {
    pub stages: [StoryStage; 11],
}

impl StoryTemplate {
    /// Returns the default autobiographical template.
    pub fn default() -> Self;

    /// Total number of slots across all stages.
    pub fn total_slots(&self) -> usize;  // Always 23

    /// Validate that a set of slot values matches the template shape.
    pub fn validate_shape(&self, slots: &[Vec<String>]) -> CryptoResult<()>;
}

/// A completed pass story — template + user's slot values.
pub struct PassStory {
    pub template: StoryTemplate,
    pub slots: [String; 23],     // Normalized slot values
}

impl PassStory {
    /// Create from raw user input. Normalizes all slots.
    pub fn from_raw(raw_slots: &[&str; 23]) -> CryptoResult<Self>;

    /// Render the full narrative for display.
    /// Each template sentence with blanks filled in.
    pub fn render(&self) -> String;

    /// Get the canonical encoding for KDF input.
    pub fn canonical(&self) -> CryptoResult<Vec<u8>>;
}
```

### The 11 stages (past-tense autobiographical)

| # | Stage | Template | Slots |
|---|-------|----------|-------|
| 1 | The Ordinary World | "I grew up in `_____`, where I was a `_____`." | 2 |
| 2 | The Call | "Everything changed when `_____` brought me `_____`." | 2 |
| 3 | Refusal of the Call | "I almost didn't go because of my `_____` and my `_____`." | 2 |
| 4 | Crossing the Threshold | "I left through the `_____` and arrived in `_____`." | 2 |
| 5 | The Mentor | "A `_____` showed me the `_____` I couldn't see." | 2 |
| 6 | Tests and Allies | "I learned to make `_____` from `_____` and `_____`." | 3 |
| 7 | The Ordeal | "The hardest part was when my `_____` broke against `_____`." | 2 |
| 8 | The Reward | "From that silence I found a `_____` that sang of `_____`." | 2 |
| 9 | The Road Back | "I carried the `_____` through the `_____` and home." | 2 |
| 10 | Resurrection | "Where I had been a `_____`, I became a `_____`." | 2 |
| 11 | Return with the Elixir | "Now I carry `_____`." | 1 |
| | **Total** | | **23** |

Templates are first person, past tense. The user recalls their own life — the template provides scaffolding, not fiction.

---

## 5. Phase 3 — Entropy Estimation

### New module: `crates/indras-crypto/src/entropy.rs`

### 3-tier frequency model

```rust
/// Estimate the entropy of a single slot value at a given position.
pub fn slot_entropy(word: &str, position: usize) -> f64;

/// Estimate total story entropy across all 23 slots.
/// Returns (total_bits, per_slot_bits).
pub fn story_entropy(slots: &[String; 23]) -> (f64, [f64; 23]);

/// Check if a story meets the minimum entropy threshold.
/// Returns Ok(()) or Err with the indices of weak slots.
pub fn entropy_gate(slots: &[String; 23]) -> CryptoResult<()>;
```

**Three sources combined per slot:**

1. **Base word frequency.** How common is this word in English? Sourced from COCA/Google Books Ngrams. "Shadow" is common (~5 bits). "Cassiterite" is rare (~17 bits).

2. **Positional bias.** How likely is this word *in this specific slot*? "Fear" in the Refusal slot is far more probable than "fear" in the Reward slot. Initialized from autobiographical response data, updated with differential privacy.

3. **Semantic clustering.** Conditional probabilities across slots. If the Ordinary World is "silence," the Call is more likely to involve "music." The model penalizes predictable sequences.

**Formula per slot:**

```
H(slot_i) = -log₂(P(word_i | position_i))
```

**Total:**

```
H(story) = Σ H(slot_i) for i in 1..23
```

**Threshold:** 256 bits total. This is a safety net — most users telling their real story will exceed it without trying.

**UX framing:** When the gate fires, the message is "This doesn't sound like a story only you would tell" — not "too weak." The fix is to make the generic parts more personal, not harder to guess.

### New module: `crates/indras-crypto/src/word_frequencies.rs`

Embedded word frequency data (~50K words). Compile-time included via `include_str!` or `phf` perfect hash map.

```rust
/// Get the base frequency rank of a word (lower = more common).
pub fn word_rank(word: &str) -> Option<u32>;

/// Get the base entropy estimate for a word (in bits).
pub fn base_entropy(word: &str) -> f64;

/// Get positional bias for a word in a given slot.
pub fn positional_entropy(word: &str, slot_position: usize) -> f64;
```

**Size estimate:** ~500KB–1MB for 50K words with frequency data. Acceptable for desktop/server, may need trimming for IoT targets.

---

## 6. Phase 4 — SyncEngine Integration

### Extend `EncryptedKeystore`

**File:** `crates/indras-node/src/keystore.rs`

Add a `StoryKeystore` variant that wraps `EncryptedKeystore` with pass-story-specific behavior:

```rust
pub struct StoryKeystore {
    inner: EncryptedKeystore,
    verification_token: Option<[u8; 32]>,
    rehearsal_state: Option<RehearsalState>,
}

impl StoryKeystore {
    /// Create a new story-backed keystore.
    pub fn new(data_dir: &Path) -> Self;

    /// Initialize with a pass story. Generates PQ keys, encrypts them
    /// with the story-derived encryption key, stores verification token.
    pub fn initialize(&mut self, story: &PassStory, salt: &[u8]) -> NodeResult<()>;

    /// Authenticate with a pass story. Derives keys, verifies token,
    /// decrypts PQ keys.
    pub fn authenticate(&mut self, story: &PassStory, salt: &[u8]) -> NodeResult<()>;

    /// Check if a story matches without fully unlocking.
    pub fn verify_story(&self, story: &PassStory, salt: &[u8]) -> NodeResult<bool>;

    /// Rotate to a new story. Re-encrypts all keys with new story-derived key.
    pub fn rotate_story(
        &mut self,
        old_story: &PassStory,
        new_story: &PassStory,
        salt: &[u8],
    ) -> NodeResult<()>;
}
```

**Argon2id parameters for StoryKeystore** (higher than EncryptedKeystore's OWASP defaults):

| Parameter | EncryptedKeystore (current) | StoryKeystore (new) |
|-----------|---------------------------|---------------------|
| Memory | 19,456 KiB (~19 MB) | 262,144 KiB (256 MB) |
| Iterations | 2 | 4 |
| Parallelism | 1 | 4 |
| Output | 32 bytes | 64 bytes |

The higher parameters are justified because pass story auth happens infrequently (~90 seconds of user time already) and the input space, while large, benefits from maximum KDF hardness.

### Add `pass_story()` to `NetworkBuilder`

**File:** `crates/indras-network/src/config.rs`

```rust
impl NetworkBuilder {
    // Existing methods...

    /// Configure the network to use pass story authentication.
    /// This sets up StoryKeystore instead of EncryptedKeystore.
    pub fn pass_story(mut self, story: PassStory) -> Self;
}
```

### Account creation flow

1. User opens SyncEngine for the first time
2. Template presented one stage at a time (past-tense, autobiographical)
3. User fills 23 slots with real memories
4. Entropy gate evaluates the story
5. If below threshold: highlight generic slots, user revises
6. If above threshold: show full rendered narrative for confirmation
7. User confirms
8. System generates random PQ keys (ML-KEM-768 + ML-DSA-65) and iroh Ed25519
9. Story-derived encryption key encrypts PQ private keys at rest
10. Verification token + salt + encrypted keys stored to disk

### Authentication flow

1. Template presented one stage at a time
2. User retells their story (fills 23 slots)
3. Story normalized → canonical encoded → Argon2id → HKDF
4. Verification token checked against stored token
5. If match: decrypt PQ keys, proceed with normal SyncEngine operation

### Key rotation

1. Authenticate with old story (unlocks PQ keys)
2. User writes new story (same template, new words)
3. New story passes entropy gate
4. Re-encrypt PQ keys with new story-derived key
5. Store new verification token + salt
6. Begin new rehearsal cycle

---

## 7. Phase 5 — Drift Mitigation & Recovery

### New module: `crates/indras-network/src/rehearsal.rs`

**Spaced repetition schedule:**

| Day | Action |
|-----|--------|
| 1 | First rehearsal prompt |
| 3 | Second rehearsal prompt |
| 7 | Third rehearsal prompt |
| 30+ | Monthly rehearsal prompts |

```rust
pub struct RehearsalState {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_rehearsal: Option<chrono::DateTime<chrono::Utc>>,
    pub rehearsal_count: u32,
    pub next_rehearsal: chrono::DateTime<chrono::Utc>,
    pub consecutive_successes: u32,
}

impl RehearsalState {
    pub fn new() -> Self;
    pub fn record_success(&mut self);
    pub fn record_failure(&mut self);
    pub fn is_due(&self) -> bool;
    pub fn next_due(&self) -> chrono::DateTime<chrono::Utc>;
}
```

### Story confirmation display

Every successful authentication ends with the full rendered story displayed briefly. The user passively re-reads their own story on every login, reinforcing exact wording.

### Partial recovery via steward channel + Shamir's Secret Sharing

**New module:** `crates/indras-network/src/story_auth.rs`

If a user fails on 1-2 slots but gets the rest right, and can verify identity through a secondary channel:

1. Trusted stewards hold Shamir shares of the recovery subkey
2. Threshold (e.g., 3-of-5) stewards reconstruct the recovery key
3. Recovery key decrypts a hint: *which stage* failed (not the answer)
4. User retries with the hint
5. On success: mandatory new rehearsal cycle begins

```rust
pub struct StoryAuth {
    keystore: StoryKeystore,
    rehearsal: RehearsalState,
}

impl StoryAuth {
    /// Full account creation flow.
    pub async fn create_account(
        data_dir: &Path,
        story: &PassStory,
        user_id: &[u8],
        timestamp: u64,
    ) -> NodeResult<Self>;

    /// Full authentication flow.
    pub async fn authenticate(
        &mut self,
        story: &PassStory,
    ) -> NodeResult<AuthResult>;

    /// Partial recovery: identify which stages failed.
    pub fn recovery_hint(
        &self,
        attempted_story: &PassStory,
        recovery_key: &SecureBytes,
    ) -> NodeResult<Vec<usize>>;  // Indices of failed stages

    /// Rotate to new story.
    pub async fn rotate(
        &mut self,
        old_story: &PassStory,
        new_story: &PassStory,
    ) -> NodeResult<()>;
}

pub enum AuthResult {
    Success,
    Failed,
    RehearsalDue,  // Authenticated, but remind user to rehearse
}
```

---

## 8. File Map

### New files

| File | Purpose |
|------|---------|
| `crates/indras-crypto/src/pass_story.rs` | KDF pipeline: normalize, encode, derive, expand |
| `crates/indras-crypto/src/story_template.rs` | Template engine: 11 stages, 23 slots, rendering |
| `crates/indras-crypto/src/entropy.rs` | Entropy estimation: 3-tier frequency model, gate |
| `crates/indras-crypto/src/word_frequencies.rs` | Embedded word frequency data (~50K words) |
| `crates/indras-network/src/rehearsal.rs` | Spaced repetition schedule and state |
| `crates/indras-network/src/story_auth.rs` | High-level auth flow, recovery, rotation |

### Modified files

| File | Changes |
|------|---------|
| `crates/indras-crypto/Cargo.toml` | Add `argon2 = "0.5"`, `hkdf = "0.12"`, `sha2 = "0.10"`, `unicode-normalization = "0.1"` |
| `crates/indras-crypto/src/lib.rs` | Add `pub mod pass_story;` `pub mod story_template;` `pub mod entropy;` `pub mod word_frequencies;` |
| `crates/indras-crypto/src/error.rs` | Add `StoryError` variants: `SlotCountMismatch`, `NullByteInSlot`, `EntropyBelowThreshold { total: f64, required: f64, weak_slots: Vec<usize> }`, `InvalidStory(String)` |
| `crates/indras-node/src/keystore.rs` | Add `StoryKeystore` struct wrapping `EncryptedKeystore` |
| `crates/indras-node/src/error.rs` | Add `StoryAuth(String)` variant to `NodeError` |
| `crates/indras-network/src/config.rs` | Add `pass_story()` method to `NetworkBuilder`, add `story: Option<PassStory>` to `NetworkConfig` |
| `crates/indras-network/src/error.rs` | Add `StoryAuth { reason: String }` variant to `IndraError` |

---

## 9. Testing Strategy

### Unit tests

| Test | Module | Verifies |
|------|--------|----------|
| Normalization idempotency | `pass_story` | `normalize(normalize(x)) == normalize(x)` for all inputs |
| Case insensitivity | `pass_story` | `normalize("AMARANTH") == normalize("amaranth")` |
| Whitespace collapse | `pass_story` | `normalize("  hello   world  ") == "hello world"` |
| Canonical encoding determinism | `pass_story` | Same slots always produce same bytes |
| Null byte rejection | `pass_story` | Slot containing `\x00` returns error |
| Slot count validation | `pass_story` | 22 or 24 slots return error |
| KDF output length | `pass_story` | Master key is exactly 64 bytes |
| Subkey derivation | `pass_story` | 4 subkeys, each 32 bytes, all distinct |
| Verification token | `pass_story` | Same story → same token; different story → different token |
| Template slot count | `story_template` | Total slots across all stages == 23 |
| Template rendering | `story_template` | Rendered story contains all slot values |
| Entropy: common words | `entropy` | "darkness", "light", "sword" score < 6 bits each |
| Entropy: rare words | `entropy` | "cassiterite", "pyrrhic", "amaranth" score > 12 bits each |
| Entropy gate: generic story | `entropy` | 23 common words → rejected |
| Entropy gate: diverse story | `entropy` | 23 moderate-entropy words → accepted |

### Integration tests

| Test | Modules | Verifies |
|------|---------|----------|
| Create → authenticate round-trip | `story_auth` | Full pipeline: create account, then authenticate with same story |
| Wrong story rejection | `story_auth` | Different story → authentication fails |
| Normalization equivalence | `pass_story` + `story_auth` | "AMARANTH" and "amaranth" authenticate identically |
| Entropy gate enforcement | `entropy` + `story_auth` | Account creation rejected for generic stories |
| Key rotation | `story_auth` | Old story stops working, new story works |
| Verification token stability | `pass_story` | Token doesn't change across process restarts |
| Rehearsal scheduling | `rehearsal` | Correct intervals: day 1, 3, 7, then monthly |
| Partial recovery | `story_auth` | Recovery hint identifies correct failed stages |

### Entropy model validation

| Scenario | Expected |
|----------|----------|
| 23 slots of "the" | Rejected (< 256 bits total) |
| 23 slots of distinct rare words | Accepted (>> 256 bits) |
| Mix of 10 common + 13 rare words | Accepted (rare words compensate) |
| All slots identical ("darkness" × 23) | Rejected (near-zero effective entropy) |
| Semantically clustered ("light"/"dark"/"shadow"/"flame"...) | Reduced entropy via clustering penalty |

---

## 10. Open Questions

### Deterministic key gen vs. encrypted-at-rest

**Current plan:** Story-derived key encrypts random PQ keys at rest (encrypted-at-rest model).

**Alternative:** Derive PQ keys deterministically from the story (deterministic model). This enables keyless portability — log in from any device with just your story — but ties PQ key quality to story entropy and makes key rotation impossible without changing PQ keys (breaking existing encrypted data/signatures).

**Recommendation:** Encrypted-at-rest for Phase 1. Deterministic derivation as optional Phase 6 for portability-critical use cases.

### Argon2id 256MB on mobile/embedded

256MB is reasonable on desktop and server. On mobile, it's borderline — most modern phones have 4-8GB RAM, but background memory pressure may cause the OS to kill the process.

**Options:**
- Reduce to 64MB on mobile with compensating iteration increase
- Profile on target devices before shipping
- Make parameters configurable per `Preset` (IoT preset uses lower memory)

### Word frequency data size

~500KB–1MB for 50K words. Acceptable for desktop. For IoT targets (where `Preset::IoT` is used), consider:
- Smaller vocabulary (10K words, ~100KB)
- Server-side entropy estimation with local cache
- Lazy loading

### Internationalization

English-only for Phase 1. The architecture supports per-language frequency models — each model is a separate data file loaded based on locale. Priority languages for Phase 2: Spanish, Mandarin, Arabic, Hindi.

Template sentences need professional translation (not just word-for-word — the poetic register must be preserved).

### Iroh Ed25519 key

Currently the iroh transport key is Ed25519 (generated randomly). Options:
- **Also derive from story:** Simpler mental model (one story → all keys), but ties transport identity to story rotation
- **Keep random, encrypt at rest:** Consistent with PQ key treatment. Story rotation doesn't affect transport identity
- **Recommendation:** Keep random, encrypt at rest (same as PQ keys)

### Steward threshold for Shamir

Suggested: **3-of-5** stewards required for recovery key reconstruction.

Considerations:
- Too low (2-of-3): Collusion risk
- Too high (5-of-7): Recovery becomes impractical
- Stewards should be from different social circles
- User chooses their own stewards during account creation

### Story confirmation display as shoulder-surfing risk

The confirmation display shows the full story on screen for a few seconds after authentication. This creates a shoulder-surfing window.

**Mitigations:**
- Display duration is brief (3-5 seconds)
- User can disable confirmation display in settings
- Display can be partial (show first word of each slot, not full text)
- On mobile: require explicit tap to reveal, auto-hide on background

---

## Appendix: Existing Primitives Reference

These types and functions from the current codebase will be reused or extended:

| Primitive | Location | Used for |
|-----------|----------|----------|
| `SecureBytes` | `indras-crypto/src/lib.rs` | Zeroizing wrapper for all key material |
| `PQIdentity` (ML-DSA-65) | `indras-crypto/src/pq_identity.rs` | Signing keys (4,000 byte SK, 1,952 byte PK) |
| `PQKemKeyPair` (ML-KEM-768) | `indras-crypto/src/pq_kem.rs` | Key encapsulation (2,400 byte DK, 1,184 byte EK) |
| `CryptoError` | `indras-crypto/src/error.rs` | Extended with StoryError variants |
| `EncryptedKeystore` | `indras-node/src/keystore.rs` | Pattern for StoryKeystore (Argon2id + ChaCha20-Poly1305) |
| `hash_content()` (BLAKE3) | `indras-crypto/src/artifact_encryption.rs` | Verification token generation |
| `InterfaceKey` (ChaCha20-Poly1305) | `indras-crypto/src/interface_key.rs` | Encrypt/decrypt pattern reused for key-at-rest |
| `NetworkBuilder` | `indras-network/src/config.rs` | Extended with `pass_story()` method |
| `IndraError` | `indras-network/src/error.rs` | Extended with StoryAuth variant |
| `NodeError` | `indras-node/src/error.rs` | Extended with StoryAuth variant |
