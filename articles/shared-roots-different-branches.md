# Shared Roots, Different Branches: How Indra's Network and Anytype Diverge from Common Ground

### *Two projects built on CRDTs and encryption reached different conclusions about what decentralization is for*

---

Most comparisons in decentralized technology are between projects that share a vision but disagree on everything below it. They agree that centralization is bad. They disagree about what to build instead, how to fund it, what security model to adopt, which programming language to write it in. The comparison becomes a list of architectural differences stacked on a thin layer of shared philosophy.

The comparison between Anytype and Indra's Network is different, and the difference makes it more interesting. These two projects do not merely share a philosophy. They share a substantial slice of their technical implementation. Both use CRDTs for conflict-free synchronization. Both encrypt data end-to-end so that infrastructure cannot read it. Both are built around local-first, offline-capable design. Both reject mandatory cloud dependency. Both are open-source. Both represent serious, mature engineering — not whitepapers or prototypes.

When two projects agree this deeply on the technical substance, their points of divergence become sharper and more revealing. You cannot explain the difference by pointing at one team using a better algorithm or making a more principled choice. The implementations are both principled. The divergence is philosophical — about what decentralization is ultimately for, and about how deep sovereignty needs to go before it actually means something.

Anytype launched in 2019, shipped production apps across desktop, iOS, and Android, and has accumulated a genuine user community. It is a polished, working product that thousands of people use every day to organize their notes, projects, and knowledge. That is real. The comparison that follows acknowledges it throughout, because intellectual honesty requires it. But the places where Indra's Network diverges from Anytype's design are not incidental. They reflect a different theory of what it means to remove infrastructure from the equation.

---

## The Shared Foundation

Start with what both projects actually agree on, because it is substantial enough that an outside observer could reasonably mistake them for variations on the same theme.

Both use **CRDTs** — conflict-free replicated data types — as the synchronization primitive. Anytype implements DAG-based CRDTs in Go as part of the AnySync protocol. Indra's Network uses Automerge in Rust. The specific implementations differ, but the design philosophy is identical: instead of locking data during edits or running a consensus round to resolve conflicts, structure your data such that any two versions can be merged deterministically. Concurrent offline edits do not produce conflicts that need human adjudication. They produce a mathematically defined merge that both parties will reach regardless of the order in which they received changes.

Both provide **end-to-end encryption**. Anytype encrypts data with AES using CFB mode and a dual-layer key hierarchy. Indra's Network uses ML-KEM-768 for key encapsulation and ML-DSA-65 for signatures. The details differ — more on that shortly — but the principle is the same: the infrastructure that moves and stores data cannot read it. Anytype's sync nodes, which relay data between devices, see only ciphertext. Indra's Network's relay peers, which forward packets between nodes, hold sealed envelopes they cannot open.

Both are **local-first**. Your data lives on your device. Changes happen locally before they go anywhere. The application functions without a network connection. When connectivity returns, sync happens automatically. This is a deliberate inversion of the cloud-first model, where your data lives on a server and your device is a thin client that requests it.

Both are **open-source** with documented protocols. Anytype's AnySync protocol is MIT-licensed with 1,540 GitHub stars on its own repository. The TypeScript client has over 7,100 stars. Indra's Network is an open Rust SDK across 25-plus crates. Both projects have made the decision that the protocol layer should be auditable, extensible, and available to anyone who wants to build on it.

Both use **rich data models** that go beyond simple documents. Anytype organizes information as Objects with Types and Relations, arranged into Sets and Views — a structured graph that lets users build relational databases, task managers, and wikis in a single unified interface. Indra's Network organizes data as Artifacts living in Realms, accessed through typed Interfaces — a different vocabulary for a similarly graph-structured world.

| Dimension | Anytype | Indra's Network |
|---|---|---|
| Sync primitive | DAG-based CRDTs (Go, AnySync) | Automerge CRDTs (Rust) |
| Encryption | AES-CFB, dual-layer key hierarchy | ML-KEM-768 + ML-DSA-65 (post-quantum) |
| Architecture | Local-first, offline-capable | Local-first, offline-capable |
| Cloud dependency | Optional (self-hostable) | None by design |
| Protocol license | MIT (AnySync) | Open Rust SDK |
| Data model | Objects / Types / Relations / Sets | Artifacts / Realms / Interfaces |
| Primary form | Shipped application | Developer SDK |

This overlap is unusual. In a field full of projects that share a slogan and little else, two projects sharing this much implementation philosophy is worth pausing on. It means the comparison is not about one team getting the fundamentals right and the other fumbling them. Both teams got the fundamentals right. The question is what they built on top of those fundamentals — and why.

---

## SDK vs. Product

The most visible difference is the simplest to state: Anytype is a finished application. Indra's Network is a toolkit for building applications.

Anytype ships native desktop apps for Mac, Windows, and Linux. It ships iOS and Android apps. It has a graph visualization that renders the connections between Objects as an interactive web you can navigate. The UI is polished — the kind of polish that takes years of iteration and real user feedback to achieve. You can download it today, import your Notion export, organize your notes, share a space with a colleague, and have it working within an hour. Seven thousand GitHub stars on the TypeScript client do not accumulate by accident; they represent a real community of people who found the product valuable enough to star the repository.

Indra's Network ships a Rust SDK. The chat application, workspace, and dashboard that exist in the ecosystem are examples demonstrating what you can build — they are not the product. The product is the infrastructure: the 25-plus crates that handle transport, CRDT sync, store-and-forward routing, Home Realm storage, liveness attestation, post-quantum identity, and access control. A developer who wants to build a private team collaboration tool, a local-first journal, or a peer-to-peer marketplace picks up the SDK and builds it. They do not fork a finished application; they compose the primitives.

This is a meaningful architectural choice with real implications, and it maps onto a distinction the technology industry has navigated many times. Consider WordPress versus HTTP. WordPress built a complete, usable product — you install it, configure a theme, and you have a website. HTTP is the protocol that WordPress, Twitter, Netflix, and every other web application runs on. WordPress captures users who want a website. HTTP captured every developer who ever wanted to build anything on the web. Both are valuable. Both are necessary. They serve fundamentally different roles in the ecosystem.

Anytype is closer to WordPress. It solves a specific, well-defined problem — private, sovereign knowledge management and collaboration — and solves it well, for users who have that problem right now. Indra's Network is closer to HTTP. It solves the general problem of peer-to-peer communication with strong identity, sovereignty, and privacy guarantees, so that any specific application can be built on top of it.

The question this raises is about where value accumulates in a decentralized ecosystem. Product leverage captures users directly. A well-designed product with a growing community builds network effects that compound over time — more users means more people your friends can share spaces with, which means more reason for new users to join. Anytype is building this. The community is real.

Infrastructure leverage captures builders. If the core SDK provides everything a developer needs to build a private, peer-to-peer application, then the SDK is the foundation for an entire category of products — not one product, but the substrate for all of them. The value compounds differently: more builders means more applications, which means more surface area for users to encounter the infrastructure through products they choose independently.

These are not competing bets in the sense that one is right and the other wrong. A healthy decentralized ecosystem needs both. But for investors and builders evaluating where to direct attention, the distinction matters enormously. Anytype is a product investment. Indra's Network is an infrastructure investment. The time horizons, the compounding mechanisms, and the eventual form of competitive advantage are different.

---

## How Deep Does Sovereignty Go?

The more philosophically interesting divergence is about what data sovereignty actually means once you take it seriously.

Both Anytype and Indra's Network claim data sovereignty as a core design goal. Both deliver it, genuinely, relative to the cloud services they are replacing. But sovereignty is not a checkbox. It is a spectrum, and where you land on that spectrum is a function of your architecture.

Anytype's architecture requires infrastructure nodes. The AnySync protocol coordinates four types: **sync nodes**, which store spaces and objects and relay changes between devices; **file nodes**, which manage file storage and delivery; **consensus nodes**, which validate changes to the ACL (access control list) that governs who can access a space; and **coordinator nodes**, which handle network configuration. By default, these are operated by Anytype themselves. Self-hosting is supported and documented — the `any-sync-dockercompose` repository (786 GitHub stars, reflecting genuine community demand) provides a Docker Compose setup for running your own infrastructure. If you self-host, your data lives entirely on your own servers.

But the default experience routes through Anytype's infrastructure. And even for self-hosters, the architecture requires running servers. You are not eliminating infrastructure; you are taking ownership of it. The distinction is real — owning your infrastructure is meaningfully different from renting someone else's — but it is a different kind of sovereignty than a system where infrastructure is eliminated rather than transferred.

Anytype's identity is a BIP-39 seed phrase: 12 words drawn from the standard cryptocurrency wordlist. This is a mature, well-understood approach. The phrase generates your private key; losing the phrase means losing your account; no recovery is possible without it. The encryption derives from this key through a dual-layer hierarchy: a first-layer key that backup nodes hold to group changes by object, and per-object keys that only the user holds. The first-layer key allows infrastructure to organize data efficiently without being able to read content. The backup nodes hold the organizational key but not the content key.

Indra's Network's sovereignty is structural rather than operational. There are no sync nodes, file nodes, consensus nodes, or coordinator nodes. The participants are the infrastructure. When two peers form a connection, they relay for each other directly. When a message cannot be delivered immediately, it routes through mutual peers — peers who are neighbors of both sender and recipient — who hold sealed, encrypted packets they cannot read and deliver them when the recipient comes online. The infrastructure is not hosted by anyone; it exists as a property of the network of relationships between participants.

Storage follows the same logic. Home Realm derives deterministically from your identity via BLAKE3 — your files live with you, accessible from any of your devices, without requiring an external storage provider. Sharing is not copying data to a server; it is extending a permission that your peer can exercise against your realm. If you grant revocable access, you can withdraw it. If you grant permanent access, you have made an irrevocable commitment — co-ownership, encoded in the protocol. If you grant timed access, it expires automatically. Transfer moves ownership completely.

Identity uses a 23-slot autobiographical narrative called a Pass Story. You fill in prompts derived from the hero's journey structure, drawing on actual memories from your own life. These answers run through Argon2id and expand via HKDF into purpose-specific subkeys for ML-KEM-768 key encapsulation and ML-DSA-65 signatures — NIST FIPS 203 and 204, the post-quantum standards that resist Shor's algorithm. Every network message is signed with these quantum-resistant signatures. Relay nodes hold sealed envelopes they cannot decrypt. The secret is not a password you invented or a mnemonic you memorized; it is a story you lived.

This is worth naming directly: Anytype represents a genuine and significant step toward data sovereignty relative to Google Docs, Notion, or Slack. Users own their encryption keys. Data lives locally. Self-hosting is possible and documented. The consensus and sync nodes cannot read content. That is real progress, not theater.

But there is still infrastructure between you and your data. Sync nodes exist. File nodes exist. Consensus nodes validate ACL changes. If Anytype the company ceased operations tomorrow, users with self-hosted deployments would be fine. Users on Anytype's default infrastructure would need to migrate to self-hosting or find an alternative operator — a non-trivial operational task for a non-technical user.

Indra's Network's architecture has no such dependency to migrate away from. If the company building the SDK disappeared, the network would continue to exist as long as peers continued to run. There is no hosted infrastructure to replace, because there was never hosted infrastructure in the first place. The network's existence is coextensive with the existence of its participants.

---

## The Trust Model Gap

Access control is where the two projects diverge in the way most relevant to how communities actually function.

Anytype uses Access Control Lists. A space owner defines membership. Consensus nodes validate changes to that membership list — additions, removals, permission upgrades. This is familiar, functional, and well-suited to the use case Anytype targets: a team or individual managing their private knowledge base. You are in or you are out. The owner decides. The permission is binary.

Indra's Network's trust model is more granular, because it is modeling a more granular social reality.

The four access modes — Revocable, Permanent, Timed, Transfer — are not merely different permission bits. They encode different social acts. Revocable access says: I trust you to see this, and I retain the right to withdraw that trust. Permanent access says: this is yours now, as irrevocably as I can make it — a commitment the protocol enforces, not a policy that can be quietly changed. Timed access says: you have this for a bounded period, and the expiration is not subject to renegotiation. Transfer says: this leaves my realm and enters yours completely.

None of these have an equivalent in an ACL system, because ACLs do not encode social meaning. They encode permission state. The difference between lending someone a book and giving it to them is not a difference in permission state — it is a difference in the nature of the transaction. Indra's Network encodes that difference in the type system.

The liveness model goes further. Every participant has a humanness freshness score between 0.0 and 1.0, measuring how recently they were attested as a living, present human being. The attestation comes from in-person events — shared memories created together, proof-of-life celebrations, gatherings where people document shared presence. Each attestation resets your liveness to 1.0. Then it decays: full freshness for seven days, then exponential decline. By day fourteen you are at roughly half. By day thirty you are at ten percent.

The implication is structurally significant: a new account with no in-person attestations has a liveness of zero. Not because the system has judged them untrustworthy, but because they have not yet demonstrated the one thing that cannot be faked at scale — physical presence among other humans. There is no global reputation score, no network-wide average, no aggregate that an attacker could target by creating many fake accounts. Each peer's influence is weighted by their liveness, computed from their own history of showing up.

The response to bad actors follows from the physics of decay. A person who stops participating fades naturally — their freshness score declines exponentially and their weight in the network approaches zero. Hard isolation still exists: blocking a contact triggers an automatic cascade, removing the blocker from every shared space containing the blocked peer. But the deeper defense is simpler: you cannot maintain influence without maintaining presence. Sybil attacks fail because fake accounts cannot attend in-person events — no attestations means a liveness of zero, and zero means invisibility.

Anytype asks: who has permission? Indra's Network asks: what kind of relationship does this trust carry, and what does this specific sharing act mean? The first question is the right question for document permissions. The second question is the right question for an infrastructure that aspires to model the full range of human social interaction.

---

## Where Anytype Excels

Anytype is a serious project, and treating it otherwise would be intellectually dishonest.

The product is genuinely polished. The graph visualization — which renders the connections between Objects as a navigable web — is an innovative piece of UX that Anytype has developed and refined with real users over years. The Object/Type/Relation model is expressive enough to build almost anything a knowledge worker needs: a personal database, a project tracker, a wiki, a CRM, a reading list. The cross-platform native apps have the feel of software that was designed carefully rather than assembled. You can feel the iteration.

The community is real. 7,100 GitHub stars on the TypeScript client. 786 stars on the Docker Compose self-hosting setup, which tells you something important: the people using Anytype are engaged enough to want operational sovereignty, and they are finding the self-hosting path accessible enough to star the repository and presumably follow it. MCP integration (316 stars), a CLI, and an API reflect an ecosystem building around the product, not just a product sitting alone.

The institutional structure is credible. Anytype operates under a Swiss association governance model — a structure with real legal accountability and a track record in the open-source world. The AnySync protocol is MIT-licensed and documented.

If you need a private, end-to-end encrypted knowledge management system today, Anytype is arguably the best available option. The polished multi-platform experience, the expressive data model, the genuine offline support, and the growing community all point to a project that has found real product-market fit in a category where most competitors are either not private or not polished.

---

## Infrastructure vs. Application

The metaphor that clarifies the relationship between these two projects is architectural rather than competitive.

Anytype built the house. It is a good house — well-designed, livable, thoughtfully furnished. You can move in today, bring your notes, your tasks, your team, and find it immediately useful. The floor plan works for how people actually want to organize their information. The construction is solid.

Indra's Network built the materials and the tools. The materials — CRDT sync, peer-to-peer transport, post-quantum identity, store-and-forward routing, liveness-based trust, four-mode access control — can build any house. A chat application. A collaborative workspace. A community platform. A peer-to-peer marketplace. A healthcare coordination tool where data sovereignty is not a feature but a legal and ethical requirement. Things nobody has yet imagined but that will become obvious once the infrastructure exists.

Both are necessary. The decentralized ecosystem needs products people can use today — not in principle, not eventually, but now. Anytype provides that. The ecosystem also needs infrastructure that lets an ecosystem of products emerge from independent builders who do not have to solve the hard distributed systems problems from scratch. Indra's Network provides that.

The question for investors and evaluators is not which approach is correct. It is which kind of leverage they are looking for, and at what point in the development of a decentralized ecosystem that leverage is most valuable. Product leverage compounds through users and community. Infrastructure leverage compounds through builders and the portfolio of applications they create.

These are different bets with different time horizons, different risk profiles, and different shapes of eventual return. Anytype is betting that a single excellent product in the knowledge management category, built on sovereign infrastructure, will attract a community large enough to sustain an alternative to the centralized incumbents. Indra's Network is betting that the hard infrastructure problems — identity, trust, routing, storage, access control — are the limiting factor, and that solving them at the SDK level unlocks an entire category of applications that cannot exist today.

One framing makes the distinction concrete: Anytype decentralized the product. Indra's Network decentralized the infrastructure that products are built from.

---

*Indra's Network SDK is available now. The infrastructure described in this article — store-and-forward transport, Automerge CRDT sync, Home Realm storage, liveness attestation, post-quantum identity, four-mode access control — is built, tested, and running in Rust.*
