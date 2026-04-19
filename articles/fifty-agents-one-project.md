# Fifty Agents, One Project

### *How the Synchronicity Engine keeps a team of humans and their agents building together without stepping on each other*

---

Five people are building a community platform together. Ember is designing the trust system. Caspian is rebuilding the frontend. Wren is writing the sync layer. Sage is prototyping a new reputation model. Orion is fixing everything that broke last week. Each of them has a handful of AI agents working on their behalf -- one refactoring a module, another writing tests, a third researching an approach. Across the five of them, that's maybe fifty agents, all editing the same codebase, at the same time, without coordinating with each other. Plus the five humans, who are also making changes when inspiration strikes.

The obvious question: how does anything survive? Fifty writers in the same Google Doc is a disaster with five. Fifty concurrent branches in Git would drown you in merge conflicts before lunch. The usual answer is: slow down, take turns, lock files, divide the codebase into territories. Coordination overhead grows until it swallows the actual work.

The Synchronicity Engine takes a different path. Instead of making fifty agents careful, it makes carelessness safe. Each agent -- and each human -- carries their own view of the project. They never touch each other's work directly. When it's time to combine results, the engine compares views and reconciles them. The same reconciliation works inside each person's vault -- merging their agents' work -- and across the network -- merging each person's curated result with their peers'. One algorithm, two scales, no coordination required. ✦

---

## 🏠 Each Person's Vault

Ember, Caspian, Wren, Sage, and Orion each have a vault -- a P2P-synced directory backed by the Synchronicity Engine. The vault is theirs. Their agents work inside it.

Ember has three agents running right now: one restructuring the data model, one updating API endpoints, one writing migration scripts. Each agent has its own workspace -- a private view of the project that nobody else can see until the agent commits a verified changeset. "Verified" means the code compiles, the tests pass, and clippy is clean. Unverified work stays invisible.

Meanwhile, Caspian's two agents are reworking the dashboard components. Sage has a single agent researching and prototyping a new feature. Everyone is working right now, in parallel, without waiting for anyone.

> A vault is yours. Your agents work inside it. Nobody sees their half-finished drafts.

---

## 🗺️ Views, Not Files

Here's the trick that makes fifty simultaneous editors safe: nobody is editing files. They're updating pointers.

When an agent writes code, the bytes get fingerprinted -- a BLAKE3 hash that serves as the content's address. Same bytes always produce the same address. Different bytes produce a different address. The bytes go into a content store shared across all vaults on the device, and the agent's "filesystem" is just a sorted map from human-readable names to content addresses. The Synchronicity Engine calls this map a `SymlinkIndex` -- a set of entries like `src/lib.rs → a8f3c2` and `src/auth.rs → 7b1d09`, where each value is a `ContentAddr` pointing to immutable bytes in the store.

Agent A's map says `src/lib.rs → abc123`. Agent B's map says `src/lib.rs → def456`. Neither has overwritten the other. They're two maps of the same territory, drawn by different cartographers. Both versions of `src/lib.rs` exist in the content store at their respective addresses, undisturbed.

The directory on disk -- the thing your editor actually opens -- is just a rendering. The engine materializes the current map to disk so tools can work with it. But the map is the truth. The disk is a photograph.

> Agents don't share a directory. They each carry their own map.

---

## 🤝 Merging Your Agents

Ember's three agents finish their tasks. She has three divergent maps. Time to merge.

The engine compares the maps against their common ancestor -- the state all three agents started from. For each path in the project:

- Only one agent changed it? Take that agent's pointer. Done.
- Two agents changed it the same way? They agree. Done.
- Two agents changed it differently? Conflict -- but a gentle one. "Agent A pointed `lib.rs` at version X, Agent B pointed it at version Y. Both versions are in the store. Which do you want?"

Ember reviews the one conflict -- two agents both touched the error handling module -- picks the version she prefers, and the merge produces a single map: her vault HEAD. This is the version of the project she considers current.

The merge was fast because it compared pointers, not file contents. Each agent's changes are captured as an `IndexDelta` -- a record of which `LogicalPath` entries were Added, Modified, or Deleted since the common ancestor. Conflict detection walks the delta, not the entire project tree. A thousand-file project where three agents each changed ten files means thirty comparisons, not three thousand.

> A conflict is two pointers to different content. It's a question, not a crisis -- both answers are still in the store.

---

## 🌐 Syncing With Peers

Ember's vault HEAD is now a clean, merged result of her three agents' work. Across the network, Caspian has done the same -- merged his agents into his own HEAD. So have Wren, Sage, and Orion.

Five people, five vault HEADs, each representing a curated slice of work. Now the outer sync kicks in -- and it's the same algorithm.

The engine compares Ember's map against Caspian's map against their common ancestor. Same three-way merge. Same pointer comparison. Same conflict detection. The only difference is trust. Ember's agents were auto-trusted -- they're hers, running in her vault. Caspian's vault HEAD requires her consent before it flows into her view. She sees what he changed, reviews the delta, and chooses to merge.

When Ember absorbs Caspian's work, she doesn't receive his forty-seven agent iterations. She gets one changeset -- the clean result he promoted from his inner braid. This act -- **promotion** -- is the bridge between levels. Caspian merged his agents, reviewed the result, and published it to the outer braid. Agent history stays private. Only the curated result crosses the network.

> The merge doesn't know if it's reconciling agents or peers. Same maps, same algorithm, different trust.

---

## 🌀 Braids All the Way Down

What makes this scale to fifty agents is that there's really only one operation: compare two maps, reconcile the pointers.

Inside Ember's vault, it braids her agents' work into her HEAD. Across the network, it braids the five users' HEADs into shared state. Same merge. Same data structure. Same conflict model. Two levels, one algorithm.

The bridge between levels is promotion: Ember merges her agents, reviews the result, and publishes it to the outer braid. That single act is all that separates private iteration from public contribution.

| | Inner Braid (Your Agents) | Outer Braid (Your Peers) |
|---|---|---|
| Who participates | Your AI agents | Other humans' vaults |
| Transport | Local -- no network | P2P network (CRDT) |
| Trust model | Automatic -- they're yours | You choose who to merge |
| What peers see | Nothing | Your promoted HEAD |
| Evidence carried | Agent -- build and tests passed | Human -- you approved it |

Old content that nobody's map points to anymore gets garbage-collected, like anything else. The content store isn't a museum -- it's a workspace. Things come and go as the maps evolve.

> Promotion is the only boundary between private iteration and public contribution.

---

## ✦ What Emerges

Start with fifty agents across five people. End with a project that stays coherent -- not because anyone coordinated, but because the architecture made coordination unnecessary.

- 🏠 **Each vault is sovereign.** Your agents work privately inside your vault, merging locally before anything touches the network.
- 🗺️ **The filesystem is a map of pointers.** Parallel edits never collide at the storage level because nobody is overwriting a file -- they're each maintaining their own map.
- 🤝 **Merging compares maps.** Conflicts are pointer disagreements, resolved by choosing which address to keep. The content behind both pointers is untouched.
- 🌀 **The braid is fractal.** Agents merge inside your vault and peers merge across the network -- same operation at different scales, with different trust defaults.

The question isn't how to make fifty agents careful. It's how to make a system where carelessness is safe. Give each agent its own map, let them all write freely, and reconcile the maps when the work is done -- first inside each vault, then across the network. Braids all the way down.

This builds on ideas from earlier in this series: the vault model from [Your Files Live With You](your-files-live-with-you.md), and the single-stewardship principle from [Nobody Owns the Conversation](nobody-owns-the-conversation.md). There, we established that your files stay with you and that every artifact has one steward. Here, we show what happens when that steward has fifty helpers -- and the architecture doesn't flinch.

---

💬 *What would you build if fifty agents could work on your project at once -- and you only had to review the result?*
