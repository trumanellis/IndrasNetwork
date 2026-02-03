# Your Story Is Your Key

### *How a hero's journey becomes unbreakable authentication*

---

What if your password was a story you told yourself?

Not a string of characters you forget and reset. Not a biometric you can't change if it's compromised. Not a seed phrase -- twenty-four random words you stash in a fireproof safe and pray you never need.

What if the thing that proved you are you was the same thing that *makes* you you: a story about where you came from, what you faced, and what you became?

We think authentication can be personal, meaningful, and quantum-resistant -- all at the same time. We call it a **pass story**. And it follows the oldest narrative structure humans know.

---

## The Problem with Secrets

Every authentication system asks the same question: *prove you are who you claim to be.* The answers have gotten progressively worse.

**Passwords** are short, reused, forgotten, phished, and breached. The average person has over a hundred accounts and maybe four passwords shared across all of them. Password managers help, but they move the problem -- now the master password is the single point of failure, and the manager itself becomes a target.

**Biometrics** can't be rotated. Your fingerprint is not a secret -- you leave copies on every surface you touch. Your face is public. If a biometric is compromised, you can't generate a new one. You only get the fingers you were born with.

**Seed phrases** are cryptographically excellent. Twelve to twenty-four words drawn from a standardized list, encoding enough entropy to secure a private key. BIP-39 solved the problem of representing a 256-bit secret in a human-readable form. But it didn't solve the problem of *remembering* it. Nobody memorizes twenty-four random words. They write them on metal plates. They split them into shares stored in safe deposit boxes. The security is real, but it's cold, mechanical, and completely disconnected from the person it's supposed to represent.

> The best secret is one you never forget, never write down, and never repeat the same way twice -- except when it matters.

---

## What Memory Actually Wants

Cognitive science has known for decades that human memory is not a filing cabinet. It's a storyteller.

We forget random facts almost immediately. The "forgetting curve," first measured by Hermann Ebbinghaus in 1885, shows that we lose roughly 70% of arbitrary information within twenty-four hours. But we remember *narratives* -- especially ones with emotional weight, vivid imagery, and personal significance -- for years. Often for life.

This isn't a flaw. It's architecture. The hippocampus encodes memories more durably when they're embedded in spatial, temporal, and emotional context. A word in a list is data. A word in a story is an experience. The difference in retention is not incremental -- it's orders of magnitude.

**The method of loci**, used by memory champions, exploits this by placing items to remember inside an imagined journey through a familiar space. You don't memorize a list of fifty items. You *walk through your house* and encounter each item in a room. The narrative structure of the journey is the scaffolding that holds the data.

A pass story takes this principle and makes it the foundation of authentication. The journey isn't arbitrary. It's *yours*.

---

## The Hero's Journey as Cryptographic Template

In 1949, Joseph Campbell described a pattern he found in myths across every human culture. He called it the monomyth, or the hero's journey: a protagonist leaves the ordinary world, crosses into the unknown, faces trials, undergoes transformation, and returns changed.

This isn't just a literary observation. It's a cognitive fingerprint. Humans think in hero's journeys. We structure our own life stories this way. When you describe a hard year to a friend, you unconsciously follow the arc: the way things were, the disruption, the struggle, the breakthrough, the new normal.

A pass story uses this universal structure as its **template**. The template is public -- everyone knows the hero's journey. The security lies in the words *you* choose at each stage.

Here is how the template maps to a life you've already lived:

---

### The Template

Each stage of the journey asks you to remember. The template sentences are stored in the clear. Only your chosen words are secret.

#### Act I: Departure

**The Ordinary World** — where you came from.

> *"I grew up in `_____`, where I was a `_____`."*

You choose two words. Maybe you grew up in "static" — the sound of your father's radio. Maybe you were a "collector" — of stamps, of grudges, of stray cats. The words should be true for you, in whatever way truth works. Literal truth and poetic truth both count.

**The Call** — the moment everything shifted.

> *"Everything changed when `_____` brought me `_____`."*

**Refusal of the Call** — what almost held you back.

> *"I almost didn't go because of my `_____` and my `_____`."*

#### Act II: Initiation

**Crossing the Threshold** — leaving the old world behind.

> *"I left through the `_____` and arrived in `_____`."*

**The Mentor** — the one who showed you what you couldn't see.

> *"A `_____` showed me the `_____` I couldn't see."*

**Tests and Allies** — what you learned and who taught you.

> *"I learned to make `_____` from `_____` and `_____`."*

**The Ordeal** — the hardest part.

> *"The hardest part was when my `_____` broke against `_____`."*

#### Act III: Return

**The Reward** — what you found on the other side.

> *"From that silence I found a `_____` that sang of `_____`."*

**The Road Back** — carrying what you gained.

> *"I carried the `_____` through the `_____` and home."*

**Resurrection** — the transformation.

> *"Where I had been a `_____`, I became a `_____`."*

**Return with the Elixir** — what you carry now.

> *"Now I carry `_____`."*

---

That's **23 word slots** across 11 stages. Each slot accepts free-form input. The user fills them with real memories — places, people, objects, feelings — in whatever language of truth they prefer. The template is a scaffold for autobiography, not fiction.

When they need to authenticate, they retell the story of their own life.

---

## Twenty-Three Passwords, One Story

Here is the property that makes a pass story fundamentally different from any single password, no matter how long.

A combination lock with four dials, each having digits 0–9, appears to have 10,000 combinations. But it is **decomposable**: an attacker can solve each dial independently. Listen for the click. The first dial has 10 possibilities. Then the second. Then the third. The total attack cost is 10 + 10 + 10 + 10 = 40 attempts, not 10,000.

A pass story is **non-decomposable**.

The 23 slot values are concatenated and fed into Argon2id as a single input. The KDF produces a single output — the derived key. There is no intermediate signal. No dial clicks. An attacker who guesses 22 slots correctly and gets the 23rd wrong receives the same output as one who guesses all 23 wrong: *nothing*. The system provides no oracle for individual slots.

This means the attack cost is not the *sum* of per-slot possibilities — it is the *product*.

### The composition arithmetic

Suppose each slot carries only 10 bits of entropy — roughly 1,000 equally likely choices per slot. That's conservative; many real-world autobiographical details are far less predictable.

- **Decomposable** (each slot verified independently): 23 × 1,024 = **23,552 guesses**
- **Non-decomposable** (all slots verified together): 1,024^23 = **2^230 guesses**

The difference is not a rounding error. It is the difference between a lock a teenager picks in an afternoon and a lock that outlasts the sun.

At 12 bits per slot (4,096 choices each):

- Decomposable: 23 × 4,096 = **94,208 guesses**
- Non-decomposable: 4,096^23 = **2^276 guesses**

Both figures — 2^230 and 2^276 — exceed the 2^256 threshold for quantum resistance (after Grover's halving, they yield 2^115 and 2^138 bits of quantum security, both above NIST Level 1).

### Why autobiographical input helps

The hero's journey template is public. An attacker who knows the template knows the *shape* of every user's story. But autobiographical details — the specific place you grew up, the specific fear that almost held you back, the specific object your mentor showed you — are drawn from the full complexity of a lived life.

Fictional choices tend to cluster around narrative archetypes: "darkness" for the ordinary world, "light" for the reward, "sword" for the ordeal. Real memories don't cluster the same way. The sound of your father's radio, the name of a street, the color of a door — these carry natural entropy that fictional choices often lack.

The entropy gate (described below) still enforces a floor. But most users telling their real story will clear it without trying.

### The key insight

A single password must be strong. Twenty-three passwords composed together just need to be *yours*.

---

## The Entropy Gate

The composition property does the heavy lifting. But composition only works if the slots aren't all filled with the same word. A degenerate story — "darkness" in every slot — collapses 2^230 possibilities back to one.

The entropy gate is a **safety net** for this edge case. Most users will never know it's there.

### How entropy estimation works

When a user submits their story, the system evaluates each word against a **slot-aware frequency model**. This model knows, for each template position, how often each word has been chosen (or would likely be chosen) by a population of users.

The entropy of each slot is estimated as:

```
H(slot_i) = -log₂(P(word_i | template_position_i))
```

Where `P(word_i | template_position_i)` is the estimated probability of that word appearing in that slot. A word like "fear" in the Refusal slot might have a probability of 0.03 (many people will pick it), contributing about 5 bits. A word like "cassiterite" in the same slot might have a probability of 0.00001, contributing about 17 bits.

The total story entropy is the sum across all slots:

```
H(story) = Σ H(slot_i) for i in 1..23
```

### The minimum threshold

For quantum resistance at NIST Level 1, we need approximately 256 bits of classical entropy (which Grover's algorithm reduces to 128 bits of quantum security).

The system **rejects** any story below this threshold — but the rejection is a signal about *meaning*, not strength:

> *"This doesn't sound like a story only you would tell. Your journey needs more of *you* at these moments."*

The system highlights the weak slots — the words that too many others would also choose — and asks the user to reconsider just those. The strong slots are left alone. You don't rewrite your whole story. You make the generic parts more yours.

### When the gate fires

Most users telling their real story will pass this threshold without thinking about it. Real memories are naturally diverse — nobody else grew up in exactly your house, on exactly your street, with exactly your fears.

The gate fires in two cases:

1. **Fully generic stories.** A user who writes "darkness," "light," "sword," "fire" at every opportunity — narrative archetypes with no personal detail.
2. **Minimal effort.** A user who types the same word in every slot, or leaves slots effectively blank.

In both cases, the rejection message is accurate: this doesn't sound like *their* story. The fix isn't to make it harder to guess — it's to make it more theirs.

### The frequency model

The slot-aware frequency model combines three sources:

1. **Base word frequency.** How common is this word in English generally? Drawn from corpora like Google Books Ngrams or COCA. "Shadow" is common. "Pyrrhic" is not.

2. **Positional bias.** How likely is this word *in this specific template slot*? "Fear" in a Refusal slot is far more probable than "fear" in a Reward slot. This is estimated initially from autobiographical response corpora and updated (with differential privacy) as real users create stories.

3. **Semantic clustering.** Words aren't independent. If your Ordinary World is "silence," your Call is more likely to involve "music" or "sound." The model accounts for conditional probabilities across slots, penalizing predictable *sequences* as well as predictable individual words.

The combination means that a story can use some common words — every real life has a few familiar beats — as long as enough slots carry sufficient surprise to meet the total entropy threshold.

### What the user experiences

The creation flow looks like this:

1. The template is presented as a story being remembered, one stage at a time.
2. At each stage, the user types their word(s) freely.
3. After the full story is written, the system shows the complete narrative back to the user and highlights any "generic" moments — places where their story sounds like everyone's story.
4. The user revises the generic slots, making them more theirs. Maybe they change "fear" to "vertigo." Maybe "light" becomes "fluorescence."
5. Once the threshold is met, the system shows the story one final time. The user reads it through. This single narrative reading is worth more for memory consolidation than a dozen repetitions of a word list.
6. The story is confirmed.

No lists to choose from. No multiple-choice constraints. Just a life you remember, with a gentle nudge toward specificity where it matters.

---

## From Story to Key: The Derivation Pipeline

A pass story is a human-legible secret. Cryptographic systems need bytes. The pipeline from one to the other must be deterministic, one-way, and resistant to both classical and quantum attack.

### Step 1: Normalization

The raw input is normalized to eliminate ambiguity:

- Unicode NFC normalization
- Lowercase
- Whitespace collapsed to single spaces
- Leading and trailing whitespace stripped per slot

The user is informed of these rules: *"Your story ignores capitalization and extra spaces. Only your words matter."*

### Step 2: Canonical encoding

The 23 normalized slot values are concatenated with a fixed delimiter that cannot appear in any slot (e.g., `\x00`):

```
canonical = slot_1 || \x00 || slot_2 || \x00 || ... || slot_23
```

### Step 3: Key derivation

The canonical string is fed into a memory-hard key derivation function. **Argon2id** is the current best practice -- it resists GPU attacks, ASIC attacks, and side-channel attacks:

```
salt = user_id || registration_timestamp
key = Argon2id(canonical, salt, memory=256MB, iterations=4, parallelism=4)
```

The high memory parameter makes brute-force attacks expensive even for attackers with specialized hardware. The salt prevents precomputation across users.

### Step 4: Key expansion

The derived key is expanded via HKDF into purpose-specific subkeys:

```
identity_key   = HKDF-SHA512(key, info="identity")
encryption_key = HKDF-SHA512(key, info="encryption")
signing_key    = HKDF-SHA512(key, info="signing")
recovery_key   = HKDF-SHA512(key, info="recovery")
```

Each subkey feeds into the appropriate cryptographic primitive within SyncEngine. The identity key proves who you are. The encryption key protects your data. The signing key authenticates your actions. The recovery key enables account restoration.

### Step 5: Post-quantum primitives

The subkeys are used with post-quantum algorithms:

- **ML-KEM** (formerly CRYSTALS-Kyber) for key encapsulation
- **ML-DSA** (formerly CRYSTALS-Dilithium) for digital signatures
- **BLAKE3** for hashing where applicable

These algorithms are resistant to Shor's algorithm. Combined with the entropy gate ensuring sufficient classical entropy, the full pipeline achieves quantum resistance: Grover's algorithm halves the effective entropy, but 256+ classical bits still yields 128+ quantum bits, meeting NIST Post-Quantum Level 1.

---

## Quantum Security Analysis

The pass story's quantum resistance rests on three independent pillars.

**Pillar 1: Compositional entropy.**

The 23 slots compose multiplicatively, not additively. Even at a conservative 10 bits per slot, the search space is 2^230 — yielding 2^115 bits of quantum security after Grover's halving. At 12 bits per slot, it's 2^276 classical / 2^138 quantum. The entropy gate enforces a minimum of 256 classical bits, but the compositional structure means most users exceed it substantially.

Autobiographical inputs typically carry higher per-slot entropy than fictional choices. Real memories are drawn from the full complexity of a life; fictional choices tend to cluster around narrative archetypes. The gate rarely fires for users telling their real story.

**Pillar 2: Post-quantum cryptographic primitives.**

Even infinite entropy in the passphrase cannot save a system that uses RSA or ECC for its actual cryptographic operations. Shor's algorithm breaks those regardless of key size. The SyncEngine pipeline uses lattice-based cryptography (ML-KEM, ML-DSA) which has no known quantum speedup. The pass story derives the keys; the lattice math protects them.

**Pillar 3: Memory-hard key derivation.**

Argon2id's memory requirement makes quantum brute force even harder. A quantum computer running Grover's algorithm would need to evaluate Argon2id at each step, and each evaluation requires 256MB of memory. Quantum computers do not have cheap memory. The memory-hardness acts as a quantum tax on top of the entropy halving.

The three pillars are independent. Any one of them could be weakened without compromising the others. Together, they provide defense in depth.

---

## Threat Model

No system is secure against all threats. Here is what a pass story defends against and what it doesn't.

### Attacks the pass story resists

**Brute force.** The 23-slot composition creates a search space of at least 2^230, behind a memory-hard KDF requiring 256MB per evaluation, using post-quantum primitives. Non-decomposability means no shortcut exists — an attacker cannot verify individual slots and must guess the entire story at once. The cost of brute-forcing this exceeds the energy output of the sun over its remaining lifetime.

**Phishing.** The pass story is never transmitted as a whole. Authentication happens locally: the user retells their story on their own device, the device derives the key, and only the derived key (or a proof derived from it) touches the network. There is nothing to intercept.

**Credential stuffing.** Every user's story is unique, and non-decomposability is the primary defense: there is no "common password" equivalent to try across accounts, because even a partially-correct story produces a completely wrong key. The slot-aware frequency model provides additional protection by preventing the most generic word choices from being accepted.

**Shoulder surfing.** An observer would need to read and memorize 23 words in context -- far harder than watching someone type an 8-character password. The story can also be entered one stage at a time, with each stage clearing the screen.

**Server breach.** The server never stores the story. It stores only the Argon2id hash. Reversing Argon2id to recover the original story is computationally infeasible.

### Attacks the pass story does not resist

**Rubber hose cryptanalysis.** If someone can compel you to reveal your story, no authentication scheme helps. This is a social and legal problem, not a cryptographic one.

**Malware on the device.** If an attacker has a keylogger on the device where you enter your story, they capture the words as you type them. Device security is a prerequisite, not a feature of the authentication layer.

**Story drift.** This is the pass story's unique vulnerability. Over months or years, a user might subtly alter their story -- swapping "shattered" for "broke," or "astrolabe" for "compass." The normalization layer handles capitalization and spacing, but it cannot handle synonym substitution. Mitigation strategies are discussed below.

---

## Mitigating Story Drift

Story drift is the one risk that is unique to this system. Traditional passwords either match or don't. A story, being narrative, is subject to the human tendency to remember the *gist* of things rather than the *words*.

Three mechanisms address this.

### 1. Rehearsal protocol

During the first week after story creation, the system prompts the user to retell their story three times at increasing intervals (day 1, day 3, day 7). This follows the **spaced repetition** schedule known to optimize long-term retention. After the first week, the prompts become monthly.

Each rehearsal is a normal authentication attempt. If the user gets a word wrong, the system tells them *which stage* failed (but not what the correct word was) and lets them try again. This immediate feedback corrects drift before it solidifies.

### 2. Story confirmation display

Every successful authentication ends with the full story displayed on screen for a few seconds. The user passively reads their own story every time they log in. This incidental re-exposure reinforces the exact wording without requiring active effort.

### 3. Partial recovery

If a user gets most of their story right but fails on one or two slots, and can verify their identity through a secondary channel (trusted steward confirmation, recovery key held by a friend, etc.), the system can reveal the forgotten slot(s) and require a new rehearsal cycle.

This is not a backdoor. The secondary channel does not grant access -- it grants *a hint*. The user must still retell the complete correct story to derive their key.

---

## The Experience of Authentication

Here is what it feels like to log in with a pass story.

Ember opens SyncEngine on her phone. The screen shows the first stage of her journey:

> *"I grew up in `_____`, where I was a `_____`."*

She types: `amaranth`. Then: `horologist`.

She smiles. She did grow up surrounded by amaranth — her grandmother's garden was full of it. And she was a horologist in the way that matters: she took apart every clock in the house before she turned ten.

The screen transitions to the next stage:

> *"Everything changed when `_____` brought me `_____`."*

She types her words. And the next. And the next. Each stage brings back a real memory — a place, a person, an object, a feeling. The story she's telling is the story of her life, organized by the oldest narrative structure humans know.

It takes about ninety seconds. Longer than a password. But she doesn't need a password manager, doesn't need to find a hardware token, doesn't need to check her email for a magic link, doesn't need to hold her face up to the camera. She just remembers who she is.

When she finishes, her full story appears on screen for a moment — a small autobiography she wrote, about a journey she actually lived. She reads a word and remembers the smell of her grandmother's garden. Then the story fades, the key is derived, and she's in.

She has just performed 256-bit quantum-resistant authentication by remembering her own life.

---

## Implementation Considerations

### Word list for entropy estimation

The frequency model requires a large reference corpus. We recommend initializing from:

- **English word frequency:** COCA (Corpus of Contemporary American English), 60,000+ words with frequency ranks
- **Narrative word frequency:** A subset weighted toward creative/literary usage
- **Positional priors:** Bootstrap from a small study of template responses, then update with differential privacy as real data accumulates

The model must be **public**. Security through obscurity in the frequency model would be a vulnerability -- an attacker who obtains it gains an advantage over one who doesn't. If the model is public, all attackers are on equal footing, and the entropy estimate is honest.

### Internationalization

The template sentences can be translated. The word slots are language-agnostic -- the system normalizes Unicode and the frequency model can be extended per language. A user who writes in Mandarin, Arabic, or Swahili gets the same entropy guarantees as one who writes in English, provided the frequency model covers that language.

### Accessibility

For users who cannot type fluently, the system can offer voice input with speech-to-text normalization. The story metaphor works especially well in oral cultures, where narrative memory is the oldest and most natural form of knowledge preservation.

### Storage requirements

Per user, the system stores:

- Argon2id hash of the canonical story (128 bytes)
- Salt (32 bytes)
- Entropy estimate at creation time (4 bytes)
- Rehearsal schedule metadata (64 bytes)

Total: under 256 bytes per user. The story itself is never stored anywhere.

---

## Why This Matters Beyond Security

Most authentication systems treat identity as a problem to be solved — a gate to get through, a barrier between you and what you want to do. They are adversarial by design: the system assumes you might be lying and demands proof.

A pass story treats identity as something you *remember*. The act of authentication is not a challenge-response protocol. It's a recitation — a small ritual in which you recall where you came from, what you faced, and what you became. You don't imagine a character. You remember yourself.

In the [first article](indras-network-every-node-a-mirror.md), we described Indra's Network as a system where every node is a mirror reflecting every other. In the [second](your-network-has-an-immune-system.md), we described a network that protects itself the way a body does — through local sentiment, not central authority. In the [third](your-files-live-with-you.md), we described files that live with their owners, not on distant servers.

This article describes the door. And the door asks the only question that matters:

*Tell me your story.*

---

## Summary

| Property | Pass story | Traditional password | Seed phrase |
|----------|-----------|---------------------|-------------|
| Memorability | High (narrative memory) | Low (arbitrary strings) | Very low (random words) |
| Structure | 23 independent passwords, one story | Single string | 12-24 random words |
| Decomposability | None (all-or-nothing KDF) | N/A (single input) | N/A (single input) |
| Entropy floor | 256 bits (enforced) | ~30 bits (typical) | 128-256 bits (fixed) |
| Quantum resistance | Yes (3 pillars) | No (insufficient entropy) | Partial (entropy only) |
| Phishing resistance | High (local derivation) | Low (transmitted) | Medium (rarely entered) |
| Rotation | Full (write a new story) | Full | Costly (new wallet) |
| Personal meaning | Core design goal (autobiographical) | None | None |
| Authentication time | ~90 seconds | ~5 seconds | Rarely used for auth |
| Recovery | Partial (steward hints) | Email/SMS reset | Metal plate in a safe |

---

*In Indra's Net, every jewel reflects every other. But first, each jewel must know its own light. Your story is that light. Tell it, and the network knows you.*
