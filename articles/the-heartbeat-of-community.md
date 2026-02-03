# The Heartbeat of Community

### *How subjective value, trust chains, and proof of life turn tokens into letters of introduction*

---

<iframe src="images/10-transfer-flow.html" width="100%" height="500" style="border:none; border-radius:8px;" title="Trust-weighted bounty view — same quest valued differently by different observers"></iframe>

A token travels from Nairobi to Buenos Aires through three pairs of hands. Cypress earned it by translating a community guide into Swahili -- forty minutes of careful work, witnessed by the people who asked for the help. He blessed it forward to Solene, who carried it across an ocean of network hops to a mutual aid circle in Montevideo. Solene passed it to Theron, who offered it as thanks for a meal shared at a neighborhood asado.

What is it worth?

If you ask Theron, it carries the warmth of the dinner and the trust he places in Solene, who has never steered him wrong. If you ask Solene, it holds the weight of Cypress's reputation -- a person she's collaborated with for two years across three time zones. If you ask a stranger with no connection to any of them, it is worth precisely nothing. Not because the token is broken. Because value, in this system, is not a number printed on a coin. It is the trust you place in the hands that carried it.

Every economy on Earth assumes that a dollar is a dollar regardless of who holds it. We built an economy where that assumption is abandoned on purpose. A token's worth depends on who gave it, who carried it, and how much the person evaluating it trusts each link in the chain. The same token, the same history, valued differently by every observer.

This is not a flaw. This is the design. ✦

---

## 🛡️ The Vulnerability

Before we describe the full system, we need to be honest about the problem it exists to solve.

Indra's Network has a bounty system called **Tokens of Gratitude**. The original design works like this: someone posts a quest -- a request for help, a task that needs doing, a call for contribution. Another person fulfills the quest and receives attention from the community -- people witness the work, spend time evaluating it, and express gratitude. That attention gets distilled into tokens. The tokens are blessed by the people who witnessed the work, and they can be passed along, spent, or offered as thanks elsewhere in the network.

It's a beautiful idea. It's also vulnerable.

Here is the attack. It requires no technical sophistication, no exploits, no zero-days. Just patience and a willingness to lie.

**Step 1: Create the farm.** An attacker spins up fifty accounts. In the current system, there is nothing stopping this. Each account is a valid participant with a valid identity.

**Step 2: Generate synthetic attention.** The fifty fake accounts interact with each other. They post quests. They fulfill quests. They spend time -- or claim to spend time -- evaluating each other's contributions. The system registers attention events: account A watched account B's contribution for three minutes. Account C blessed account D's work. All of it looks, from the protocol's perspective, like genuine community activity.

**Step 3: Concentrate the tokens.** The fake accounts bless a single confederate -- a real-looking account controlled by the attacker. Tokens of Gratitude flow inward from fifty sources, each one minted from fabricated attention. The confederate now holds a stack of tokens that appear legitimate.

**Step 4: Spend into the real economy.** The confederate enters genuine communities and offers these tokens as payment, as thanks, as social currency. They look real. They have blessing histories. They came from accounts that generated attention and fulfilled quests.

The attack works because the original system treats all attention as equal. If any account can generate attention, and attention mints tokens, then anyone who controls enough accounts controls the money supply. This is the sybil farm problem, and it is fatal to any token economy that doesn't address it.

> The vulnerability isn't in the cryptography or the transport layer. It's in the assumption that attention is objective. It isn't. Attention from a stranger and attention from a trusted friend are fundamentally different things.

---

## 👁️ Subjective Valuation

The fix is not to build a better sybil detector. It is to abandon the premise that tokens have objective value.

In the redesigned system, a Token of Gratitude does not carry an intrinsic worth. It carries a *history* -- who created it, who blessed it, who held it, how long they paid attention. The worth of that history is evaluated independently by every observer, using their own trust relationships.

The formula is simple:

`perceived_value = attention_duration × max(sentiment_toward(blesser), 0.0)`

Two variables. The first is verifiable: how long did the blesser actually spend attending to the work? The second is subjective: how much does the *evaluator* trust the person who blessed the token?

Only positive sentiment counts. If the evaluator's sentiment toward the blesser is negative or zero -- if they distrust the blesser, or simply don't know them -- the contribution to value is zero. Not negative. Not penalized. Just invisible. The token doesn't become worthless in some global sense. It simply has no weight *in that observer's eyes*.

This changes everything about the sybil farm attack.

Zephyr receives a token. She looks at its history. It was blessed by Nova -- someone Zephyr has worked with for a year, someone she rates at +1. Nova spent twenty minutes evaluating the contribution. Zephyr's perceived value: `20 × 1.0 = 20`. That token means something to her.

The same token crosses paths with Ember. Ember doesn't know Nova. Her sentiment toward Nova is 0 -- the default for strangers. Ember's perceived value: `20 × max(0.0, 0.0) = 0`. The token is invisible to Ember. Not rejected, not flagged -- simply without weight.

Now consider the sybil farm. Fifty fake accounts bless a confederate's tokens. Those blessings carry attention durations and account identities. But when a real person evaluates those tokens, they run the formula against their own sentiment graph. Sentiment toward accounts they've never interacted with? Zero. Every blessing from the farm multiplies against zero. The tokens are ghosts.

> A sybil farm can mint a million tokens. Every single one of them is worth zero to anyone who doesn't already trust the farm. The attack doesn't fail because it's detected. It fails because it's *irrelevant*.

The same token, carried by the same chain of hands, evaluated by different observers through different trust relationships, produces different values. There is no canonical price. There is no exchange rate. There is only the question: *do you trust the people who touched this?*

---

## 🔗 The Steward Chain

Tokens don't just carry a single blessing. They accumulate a **chain of custody** -- a steward chain that records every hand the token has passed through.

When Cypress earns a token for his translation work, the token's chain begins with him. When he passes it to Solene, her identity is appended. When Solene passes it to Theron, his identity is appended. The chain grows: `Cypress → Solene → Theron`. Every release is a signed operation. The history is immutable.

This is not a ledger in the blockchain sense -- there is no global consensus, no mining, no proof of work. It's a provenance record, like the stamps in a passport or the chain of title on a house. Each entry says: *this person held this token and chose to pass it on.*

Trust decays with distance. When you evaluate a token, you don't just look at the most recent steward. You evaluate every link in the chain, and trust attenuates with each hop:

`trust_weight = sentiment_toward(steward) × 0.7^hops_since`

The person who last handed you the token carries full weight. The person before them is discounted to 70%. Two hops back, 49%. Three hops, 34%. By the time you're looking at someone five steps removed, their contribution to the token's perceived value has faded to 17% -- still nonzero if you trust them, but appropriately softened by distance.

This makes the token a **letter of introduction**, not a coin. A coin doesn't care who held it before you. A letter of introduction is *defined* by who carried it. When Ember receives a token from Sage, and Sage received it from Nova, Ember is reading a chain of trust: Nova vouched for this value. Sage vouched for Nova's judgment by carrying it forward. Each handoff is a reputation stake.

And here's the accountability mechanism: if you pass along garbage -- a token blessed by nobody trustworthy, backed by no real attention -- your name is in the chain. The next person who evaluates it sees your stewardship. If they trust you and the token turns out to be hollow, your reputation absorbs the cost. Not through an explicit punishment mechanism, but through the natural consequence of being associated with bad signal. Pass along garbage, and the people who trusted you learn to trust you a little less.

Walk through the math. Ember receives a token from Sage. Sage got it from Nova. Ember's sentiment toward Sage is 0.9 (high trust). Ember's sentiment toward Nova is 0.6 (moderate trust -- she's met Nova but doesn't know her well).

- Sage's contribution: `0.9 × 0.7^0 = 0.9`
- Nova's contribution: `0.6 × 0.7^1 = 0.42`

The token carries weight from both stewards, decayed by distance. If the chain included someone Ember doesn't know at all (sentiment 0), that link contributes nothing. The chain is only as strong as the trust relationships the evaluator has with each steward.

> A Token of Gratitude is not a coin. It is a letter of introduction that says: these people, in this order, thought this was worth carrying. Whether you agree depends on whether you trust them.

---

## 🤖 Human and AI Accounts

The network has two kinds of accounts, and the distinction matters for the token economy.

**Human accounts** can do everything: generate attention, bless tokens, create quests, fulfill quests, and participate in all aspects of the network. Attention -- the scarce resource that backs the entire token economy -- is a fundamentally human act. It takes time. It takes judgment. It takes a person actually looking at someone else's work and deciding it has value.

**AI agent accounts** are explicitly marked as non-human. They can fulfill quests -- an AI that translates a document, generates a summary, writes code, or performs analysis is doing real work and can receive tokens for it. They can hold tokens. They can pass tokens along. But they cannot generate attention, and they cannot bless tokens. The thing that mints new value into the economy -- the act of a person witnessing another person's contribution -- is reserved for humans.

This is not a technical enforcement. In a decentralized system with no central authority, you cannot build a protocol-level gate that reliably distinguishes human from machine. Any such gate becomes an arms race, and the gate always loses eventually. Instead, the distinction is enforced the same way everything else in this system is enforced: **through subjective evaluation**.

When you evaluate a token, you can see whether the attention that backed it came from accounts attested as human or accounts marked as AI. You can see whether the blessings came from humans. You decide what weight to give each. If a community decides that AI-generated attention is worthless, tokens backed by AI attention will carry zero value in that community -- not because the protocol blocks them, but because every member's subjective evaluation discounts them.

This keeps the architecture honest. We don't pretend we can solve the human-verification problem at the protocol level. We push the judgment to the edges, where it belongs -- into the hands of the people who are actually deciding whether to trust.

> Attention is the scarce human resource that backs the token economy. Machines can work. Only people can witness.

---

## 💓 Proof of Humanness

If subjective evaluation of humanness is the enforcement mechanism, the network needs a way for people to make claims about their humanness that others can evaluate. This is the attestation layer.

But it is not what you might expect from the phrase "proof of humanness."

Most identity verification systems treat humanness as a binary: you prove you're human once, you get a credential, you carry it forever. A CAPTCHA. A biometric scan. A government ID check. One gate, one moment, one stamp.

The problem with a one-time credential is that it becomes a commodity. If being "verified human" is a permanent status, the incentive to steal or forge that status is enormous. A sybil farmer who gets past the gate once has a verified human account forever. The credential decays in meaning the moment it's issued, because the issuer has no ongoing relationship with the holder.

Indra's Network treats humanness as a **heartbeat**, not a credential. Freshness matters. An attestation from yesterday is worth more than one from last month. An attestation from three months ago is worth almost nothing.

The decay function is exponential, with a 7-day grace period:

`freshness = e^(-0.1 × max(days_since_attestation - 7, 0))`

For the first seven days after an attestation, freshness is 1.0 -- full strength. On day 8, it begins to decay. By day 14, freshness has dropped to about 0.50. By day 21, it's around 0.25. By day 37, it's under 0.05 -- effectively zero.

Think of it as a pulse. Each attestation is a heartbeat. Miss too many beats and the system assumes you've flatlined -- not that you're dead, but that your claim to active humanness has gone stale. You don't get banned. You don't lose your account. Your tokens simply carry less weight when evaluated by others, because the freshness multiplier in their assessment trends toward zero.

This creates a continuous signal rather than a binary gate. The question is never "is this person human?" but "how recently has this person demonstrated that they're an active, embodied human being?" The answer is always a number between 0 and 1, always decaying, always in need of renewal.

> Humanness is not a credential you earn once. It is a pulse that must keep beating.

---

## 🏛️ Temples of Refuge & the Bioregional Delegation Tree

Who attests? In a decentralized system, there's no DMV, no passport office, no central authority that stamps your humanness card. Attestation has to emerge from the network itself.

Indra's Network uses a fractal delegation structure rooted in geography -- specifically, in the bioregional hierarchy developed by OneEarth. The structure follows the actual ecological organization of the planet:

| Level | Count | Example |
|-------|-------|---------|
| **Temples of Refuge** (root) | 1 | The global root of the delegation tree |
| **Realm Temples** | 14 | Neotropical Temple, Palearctic Temple |
| **Subrealm Temples** | 52 | Central America Temple, Western Africa Temple |
| **Bioregion Temples** | 185 | Greater Antilles Temple, Borneo Lowlands Temple |
| **Ecoregion Temples** | 844 | Cuban Moist Forests Temple, Sumatran Peat Forests Temple |
| **Individual attesters** | unbounded | People on the land |

Each level is the same operation: a **signed delegation**. The root Temples of Refuge delegates authority to 14 Realm Temples. Each Realm Temple delegates to its Subrealm Temples. Each Subrealm delegates to its Bioregion Temples. Each Bioregion delegates to its Ecoregion Temples. Each Ecoregion Temple delegates to individual attesters -- real people, rooted in a specific place, who can vouch for the humanness of people in their community.

The delegation is a signed message: *I, the Central America Temple, authorize the Greater Antilles Temple to attest on my behalf.* And then: *I, the Greater Antilles Temple, authorize Kai to attest on my behalf.* And then: *I, Kai, attest that Zephyr is an active human being I interacted with on this date.*

When someone evaluates Zephyr's humanness attestation, they see the full chain: `Temples of Refuge → Neotropical → Central America → Greater Antilles → Kai → Zephyr`. Each link is a signed delegation. The evaluator assesses each link through their own sentiment graph.

And here is the critical point: **trust in the chain is subjective**. One observer might have high confidence in the Neotropical Temple and the people who run it. Another might distrust that entire branch and place more weight on attestations from the Palearctic delegation tree. A third might ignore the institutional layers entirely and evaluate only based on their direct sentiment toward Kai, the individual attester.

The hierarchy provides structure. Subjectivity provides resilience. No single compromised node -- not even a compromised Realm Temple -- can force the rest of the network to accept its attestations. Each observer evaluates each link on its own merits, through their own trust relationships. The delegation tree is a *suggestion* of trust, not a *mandate*.

> The bioregional tree is a scaffold for trust, not a chain of command. Every link is evaluated by every observer through their own eyes.

---

## 🎉 Proof of Life Celebrations

If humanness requires a heartbeat, where does each beat come from?

The most elegant answer: from the act of being alive together.

In Indra's Network, **Memories** are shared artifacts created within a realm -- photos, notes, recordings, documents, anything saved to a space where multiple people are present. When you share a Memory into a realm, you are creating a shared artifact that requires the participation of embodied human beings. You were there. Other people were there. Something happened worth remembering.

**Creating a shared Memory in any realm automatically refreshes the humanness attestation for all participants.** The protocol records the event: these people, in this place (physical or virtual), created something together on this date. Each participant's freshness counter resets to 1.0.

The genius of this design is its friction profile. Traditional proof-of-humanness systems add friction: solve this CAPTCHA, scan your iris, submit your documents. They interrupt what you're doing to prove you're allowed to do it. In Indra's Network, the attestation *is* the activity. You don't pause your life to prove you're human. You live your life, and the proof emerges as a byproduct.

A dinner with friends. Someone takes a photo, shares it to the group's realm. Everyone's humanness refreshes.

A community meeting. Notes are saved to the shared space. Attestation renewed for all participants.

A walk with a neighbor. You pass a blooming jacaranda tree and one of you snaps a picture, drops it in your shared realm. Heartbeat recorded.

A co-working session. Kai and Soren are building something together, saving artifacts to their project realm as they go. Every shared save is a mutual attestation: we are here, we are human, we are making something.

The protocol doesn't care *what* the Memory is. It cares that multiple humans were involved in creating it. The content is irrelevant to the attestation -- what matters is the act of shared creation. The system incentivizes exactly the thing it's trying to measure: people being alive together.

> The token economy doesn't just tolerate community gathering. It incentivizes it. Every celebration, every shared meal, every collaborative session is a proof of life that keeps the economic engine running.

---

## ⚔️ Attack Analysis

Now let's walk the attacks against the full system -- the original sybil farm, plus every variation we could think of.

### Sybil farm → tokens are invisible

The fifty fake accounts generate attention, bless tokens, and funnel them to a confederate. The confederate walks into a real community and offers the tokens.

Every member of the real community evaluates those tokens through their own sentiment graph. Sentiment toward the fifty fake blessers? Zero -- nobody knows them. The perceived value formula: `attention_duration × max(0.0, 0.0) = 0`. Every token from the farm is worth exactly nothing to every member of the community.

The attack doesn't fail because it's detected. It fails because it's irrelevant. The sybil farmer spent resources creating accounts and generating activity that produces tokens nobody values. It's the economic equivalent of counterfeiting a currency that only works with people who trust you -- and nobody trusts you.

### Token laundering → steward chain accountability

A more sophisticated attacker tries to launder sybil-minted tokens through legitimate intermediaries. They earn real trust with a real person -- Lyra, say -- and then pass her a batch of sybil-backed tokens, hoping she'll carry them further into the network.

Lyra receives the tokens and sees the steward chain. The chain shows blessings from accounts she doesn't recognize. Even if she trusts the person handing them to her, the chain's deeper links carry zero weight in her evaluation. She can see the provenance, and the provenance is thin.

If Lyra passes them anyway, her name enters the chain. When Soren evaluates those tokens downstream, he sees Lyra's name attached to tokens with thin provenance. His trust in Lyra absorbs some of the cost -- he might value them slightly because he trusts her judgment -- but the 0.7^hops decay means the unknown blessers deep in the chain contribute almost nothing. And if it turns out the tokens are consistently hollow, Soren adjusts his sentiment toward Lyra. She staked her reputation by forwarding them, and reputation is a finite resource.

### Griefing via negative reputation → only positive sentiment counts

An attacker tries to destroy someone's token economy by rating them at -1 with many accounts, hoping to poison their ability to bless tokens.

This doesn't work because the valuation formula uses `max(sentiment, 0.0)`. Negative sentiment doesn't produce negative token value. It produces zero. The attacker's negative ratings make the target's blessings invisible *to the attacker's trust cluster* -- but they don't reduce the target's value in the eyes of people who actually trust them. Zephyr's +1 sentiment toward Nova means Nova's blessings carry full weight for Zephyr, regardless of what a thousand hostile strangers think.

There is no aggregated reputation score to attack. Each observer computes their own view. Griefing is structurally ineffective.

### AI flooding → humanness freshness = 0

An attacker deploys a fleet of AI agents to fulfill quests and accumulate tokens, then uses those tokens as if they were human-generated economic value.

AI accounts cannot generate attention and cannot bless tokens. They can earn tokens by doing work, but the tokens they hold still carry blessings from the humans who originally minted them. The AI is a steward, not a source.

The deeper problem for the attacker is humanness freshness. AI accounts have no attestation heartbeat. Their freshness is permanently zero unless they can somehow participate in shared Memory creation -- and shared Memories require the mutual presence of attested humans. An AI that hasn't been part of any recent human gathering has a freshness of 0, and evaluators who weight humanness freshness will discount everything the AI touches.

Can an AI fake participation in a shared Memory? Only if a real human includes it. And that human's attestation is what's actually being refreshed -- the AI's presence is incidental. The scarce resource remains human attention and human presence.

### Compromised local Temple → the immune system works on institutions too

What if an Ecoregion Temple goes rogue? Say the Cuban Moist Forests Temple starts attesting bots as humans, issuing fraudulent humanness attestations to sybil accounts.

Every observer evaluates every link in the delegation chain through their own sentiment graph. If people in the network develop negative sentiment toward the Cuban Moist Forests Temple -- because attestations originating from that branch keep being associated with suspicious accounts and hollow tokens -- the entire branch loses weight in their evaluations. The immune system described in [Part 2](your-network-has-an-immune-system.md) works on institutions the same way it works on individuals. Sentiment is local. Response is emergent. A compromised Temple doesn't poison the tree -- it loses trust among the observers who notice, and that loss propagates through the same trust channels that carry everything else.

The bioregional hierarchy is a convenience, not a single point of failure. Even if an entire Subrealm Temple is compromised, observers who distrust that branch simply weight it at zero and rely on attestation chains they do trust. The system degrades gracefully because trust was never centralized in the first place.

> Every attack fails the same way: by colliding with subjectivity. You can't forge trust. You can't manufacture sentiment. You can't counterfeit a relationship.

---

## 🌱 What Emerges

Step back from the mechanisms. Forget the exponential decay functions and the steward chains and the bioregional delegation trees for a moment. Look at what this architecture produces as a human experience.

Hard data, soft interpretation. The facts are verifiable: this token was blessed by this person, who spent this many minutes, and the token passed through these hands on these dates. The attestation chain is cryptographically signed, the steward chain is immutable, the attention durations are recorded. All of this is objective, auditable, concrete.

But the *meaning* of those facts is evaluated subjectively, by every observer, through the lens of their own relationships. There is no central authority that declares a token's worth. There is no exchange rate. There is no bank. There is only the accumulated trust between people who know each other, applied independently to the same set of verifiable claims.

This produces an economy that is:

- 🌐 **Sybil-resistant without identity verification** -- fake accounts can mint tokens that nobody values
- 🔗 **Accountable without punishment** -- the steward chain makes reputation a natural consequence of behavior
- 💓 **Human-centered without gatekeeping** -- humanness is a heartbeat, not a credential
- 🏛️ **Structured without hierarchy** -- the bioregional tree provides scaffolding that every observer evaluates independently
- 🎉 **Self-renewing through celebration** -- the act of gathering refreshes the economic engine

What does it mean when value comes from relationship rather than scarcity? When the worth of a token is not what's printed on it but who carried it to you and whether you trust them? When the most effective way to increase your economic influence is not to accumulate but to deepen your relationships with the people around you?

In the [first article](indras-network-every-node-a-mirror.md), we described a transport layer with no center -- messages bouncing between peers like light between jewels. In the [second](your-network-has-an-immune-system.md), an immune system with no moderator -- communities protecting themselves through local sentiment and emergent response. In the [third](your-files-live-with-you.md), a filesystem with no cloud -- files that live with their owners, shared through acts of trust rather than acts of copying.

Now, an economy with no bank. Value that lives in the space between people, not in the tokens themselves. An economy where the source of meaning is not abstract scarcity but concrete human connection -- the act of showing up, paying attention, and being alive together.

The pattern holds: **the participants are the infrastructure.**

The tokens are letters of introduction. The steward chains are webs of accountability. The attestations are heartbeats. And the whole system runs on the one resource that no machine can manufacture and no sybil farm can fake: the trust that accumulates, slowly and irreversibly, between people who show up for each other.

---

*What would an economy look like if its currency was trust?* 💬

**This is Part 4** of a series on Indra's Network. [Part 1: Every Node a Mirror](indras-network-every-node-a-mirror.md) covers the transport layer. [Part 2: Your Network Has an Immune System](your-network-has-an-immune-system.md) covers the social defense layer. [Part 3: Your Files Live With You](your-files-live-with-you.md) covers the shared filesystem.

**Subscribe** for future articles on governance, collective decision-making, and the structures that emerge when communities own their own economies.
