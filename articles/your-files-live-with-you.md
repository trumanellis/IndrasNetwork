# Your Files Live With You

### *How a peer-to-peer filesystem turns sharing into a spectrum of trust*

---

<iframe src="images/08-trust-radius.html" width="100%" height="500" style="border:none; border-radius:8px;" title="Trust radius diagram — an interactive visualization showing concentric rings of access around a central identity, from revocable at the edge to transfer at the core"></iframe>

We trust differently with different people. You'd lend your favorite book to one friend and never to another. You'd show a coworker your rough draft but wouldn't email it to them. You'd hand your house keys to your sister for a week and want them back. You'd give your old car to your kid and mean it forever.

Every one of these is a different act of sharing. Every one carries a different weight of trust. And every digital tool you've ever used collapses all of them into a single gesture: *share*.

You press the button and it's gone. The file is copied. The link is forwarded. The attachment lives in their inbox until the heat death of their storage quota. You can't lend a file. You can't show someone something without giving it away. You can't hand someone a key and ask for it back on Friday. The nuance of human trust -- the spectrum from "glance at this" to "this is yours forever" -- is erased the moment you go digital.

We built a filesystem that brings it back. ✦

---

## 🗂️ The Problem: Sharing Without Nuance

Think about how sharing works in the tools you use today. Google Drive, Dropbox, a shared folder on your company's server. You put a file somewhere, you set some permissions, and you hope for the best.

But the moment you share, control fractures. The file is copied to their device. The link gets forwarded to someone you've never met. The attachment sits in three different inboxes, each one a copy you can't touch. You shared it with your team of five and now seventeen people have it. You sent a draft to a client and they still have the version from six months ago that you'd rather they didn't.

"Share" in these systems means "duplicate and distribute." It's a photocopy machine, not a handshake. The person who receives your file doesn't receive *your trust* -- they receive a copy that exists independently of you, your intentions, and your relationship with them.

Compare this to how trust actually works in physical life. You hand a friend a photograph. They look at it, hand it back. That's a viewing. You give a colleague your notes. They photocopy them, keep the copy, and share it with their team. That's co-ownership. You lend your neighbor a tool. They use it for a week and return it. That's bounded trust. You give your old guitar to your daughter. It's hers now. That's a transfer.

Each of these carries different weight. Each one says something different about the relationship. And none of them is a "share link."

> Sharing in every tool you use today is collapsed into a binary: you either share or you don't. The gradient of trust -- the thing that makes human relationships work -- vanishes.

---

## 🏠 Your Files Live With You

In Indra's Network, every user has a **Home Realm** -- a personal space that belongs to them and nobody else. It's derived deterministically from their identity: a BLAKE3 hash of `home-realm-v1:` concatenated with their member ID. The same identity always produces the same Home Realm. It doesn't matter which device you're on -- your phone, your laptop, your tablet -- they all access the same space, because the space is a function of who you are.

Nobody else can enter your Home Realm. It isn't a folder on someone else's server that you've been given permission to use. It's *yours* the way your thoughts are yours. It exists because you exist.

Inside your Home Realm lives the **ArtifactIndex** -- a single document that tracks everything you own. It's a CRDT, which means it synchronizes automatically across all your devices without conflicts, even if you're offline on some of them. This is your source of truth.

Every file you add to this system gets:

- A **BLAKE3 content hash** as its identity -- the file's unique fingerprint, computed from its contents. The same file always produces the same hash. Two identical files are recognized as the same artifact, automatically.
- A **HomeArtifactEntry** with its name, size, MIME type, lifecycle status, and a list of access grants.
- Storage as a **blob** in your node's storage layer. One file, one blob, one metadata record.

> "One blob, one metadata record." Artifacts are never duplicated across realms. Sharing doesn't copy the file -- it grants access to the original.

This is the key idea, and it inverts everything you know about digital sharing: **sharing is an act you perform, not a location you put things.** Your files don't move to a shared folder. They stay in your Home Realm. You extend a hand outward -- a grant, a permission, a gesture of trust -- and someone else can see what you chose to show them. The file lives with you. Always.

---

## 💛 A Spectrum of Trust

This is the heart of the system: four access modes, each expressing a different depth of trust.

| Mode | What it means | View | Download | Reshare | Can revoke | Expires | Trust level |
|------|--------------|------|----------|---------|------------|---------|-------------|
| **Revocable** | "I trust you to see this" | Yes | No | No | Yes | No | Provisional |
| **Permanent** | "I trust you with this completely" | Yes | Yes | Yes | No | No | Deep |
| **Timed** | "I trust you with this for now" | Yes | No | No | Yes | Yes | Bounded |
| **Transfer** | "This belongs to you now" | -- | -- | -- | -- | -- | Complete |

These aren't just permission levels. They're *social acts*. Each one says something different about how you relate to the person receiving access. Let's make this concrete.

---

Ember is a designer working on a brand refresh for a community project. She has mood boards, brand guidelines, draft concepts, and final assets -- all stored in her Home Realm.

**Soren** is new to the team. He joined last week. Ember likes him, but she doesn't know him well yet. She shares the mood board with Soren using **Revocable** access. He can look at it. He can study the colors, the textures, the references she's collected. But he can't download it, can't reshare it, and if things don't work out -- if Soren turns out to not be a good fit, if the project direction changes, if Ember simply changes her mind -- she can take the access back. No confrontation. No awkward email asking him to delete something. She adjusts the grant, and his window into the mood board closes.

This is the first step of trust. Showing without giving.

**Caspian** has been Ember's collaborator for years. They've built three projects together. She trusts his judgment, his discretion, and his taste. Ember shares the brand guidelines with Caspian using **Permanent** access. He can download them. He can save his own copy. He can reshare them with the sub-team he's managing. And here's the part that matters: Ember can never revoke this access. Not because the system is broken -- because the system is working. Permanent means permanent. It's a commitment. The irrevocability *is* the gesture. It says: I trust you with this completely, and I won't take that back.

This is co-ownership. It means something precisely because it can't be undone.

**Wren** is a consultant brought in for a week to review the brand direction. She's good at what she does, but she's temporary. Ember shares a draft with Wren using **Timed** access, set to expire Friday. Wren can review the work, leave her feedback, reference the designs in her evaluation -- and when Friday arrives, the access dissolves on its own. No awkward "please delete that when you're done" conversation. No wondering whether the contractor still has your files six months later. The boundary is built into the act of sharing.

This is bounded trust. Trust with a horizon.

Months later, the project evolves. Ember is moving on to other work. **Juniper** is taking over as project lead. Ember **transfers** the final assets to Juniper. This is the ultimate act: ownership itself moves. Juniper becomes the owner. A new HomeArtifactEntry appears in Juniper's index, with full provenance -- recording that Ember was the original creator, that the transfer happened on this date, via this mechanism. Ember automatically receives Revocable access back to what she made. She can still see her work. But Juniper controls it now.

And Caspian? Caspian keeps his Permanent access. Co-ownership survives ownership changes. Because that's what co-ownership means.

> Revocable is the default mode. When you share without specifying, the system assumes the most cautious trust level. You have to deliberately choose to trust more deeply. The architecture makes generosity intentional.

---

## 🔄 What Revocation Actually Means

<iframe src="images/09-realm-view-window.html" width="100%" height="500" style="border:none; border-radius:8px;" title="Realm view window — an interactive visualization showing how shared realms filter artifacts based on active grants, with revocable and permanent grants highlighted differently"></iframe>

Two operations. Two very different meanings.

**Revoke** removes a single person's access to a single artifact. Ember can revoke Soren's access to the mood board at any time. His grant disappears. The next time his device checks, the window is closed.

But try to revoke Caspian's Permanent grant, and the system refuses. Not silently -- it returns a `CannotRevoke` error. This isn't a bug or a limitation. It's the architecture enforcing social meaning. You made a commitment when you granted Permanent access. The code holds you to it. This is enforced at the data structure level, not by policy, not by terms of service, not by a moderator's discretion. The Rust type system won't let you construct a revocation of a Permanent grant. The commitment is structural.

**Recall** is the nuclear option. It removes *all* Revocable and Timed grants on an artifact at once. Every casual viewer, every temporary consultant, every person who had provisional access -- gone. The artifact's status changes to Recalled.

But Permanent grants survive recall. Caspian still has access. Co-owners keep theirs. Because real co-ownership can't be taken back by a unilateral decision.

> Recall is like pulling something off a shared shelf. Casual viewers lose access. Co-owners keep theirs. Because co-ownership that can be revoked isn't co-ownership at all -- it's a lease dressed up in nicer language.

The revocation system uses defense in depth. When access is revoked, three things happen: the grant is removed from the ArtifactIndex, the per-artifact encryption key is deleted (rendering any cached ciphertext undecryptable), and a signed RevocationEntry is broadcast to all connected peers. Online peers delete their local copy immediately. Offline peers delete on next sync. A tombstone remains in the audit trail -- proof that the artifact existed, that it was recalled, and when. The content is gone. The record remains.

---

## 🎁 Giving Things Away

<iframe src="images/10-transfer-flow.html" width="100%" height="500" style="border:none; border-radius:8px;" title="Transfer flow diagram — an interactive visualization showing how ownership moves from one home realm to another, with provenance chain and inherited grants"></iframe>

Transfer is the deepest expression of trust in the system. It says: this thing I made, this thing I own, I'm giving it to you. Not sharing. Not lending. *Giving.*

When Ember transfers the final brand assets to Juniper, here's what happens under the surface:

1. Ember's original HomeArtifactEntry changes status to **Transferred**, recording who received it and when. The entry remains in Ember's index as a historical record -- she can see that she once owned this, and where it went.

2. A new HomeArtifactEntry appears in Juniper's ArtifactIndex. It's Active. Juniper is the owner now.

3. Ember automatically receives **Revocable** access back to the artifact. She can still see what she made. But Juniper could revoke even that, if she chose to. The creator becomes a viewer at the new owner's discretion.

4. All **Permanent** grants are inherited. Caspian had Permanent access from Ember. He still has Permanent access under Juniper. Co-ownership travels with the artifact, because co-ownership is a relationship with the *work*, not just with whoever happens to hold the title.

5. The new entry carries full **provenance**: an ArtifactProvenance record listing the original owner, who it was received from, when, and how (in this case, via Transfer). If Juniper later transfers the assets to someone else, the chain extends. Every received artifact carries its history. You can always trace the lineage back to the creator.

This provenance system means artifacts have memory. They know where they came from. In a world where digital files are infinitely copyable and endlessly anonymous, this is quietly radical: a file that remembers who made it and every hand it passed through.

Think about what Ember experiences. She poured weeks into these brand assets. She's leaving the project, but the work continues. Transfer lets her say: *this is yours now, take care of it* -- and walk away knowing the provenance records her contribution permanently. She didn't just upload a file to a shared drive and hope someone notices the metadata. She performed an act of giving, and the system remembers.

---

## 🪟 Shared Spaces as Shared Views

So far we've talked about individual sharing -- one person granting access to another. But groups work together in shared spaces. In Indra's Network, these are **realms**: collaborative spaces where multiple people interact.

Here's the crucial insight: shared realms don't store artifacts. They don't contain files. They don't have a "shared folder" where things accumulate. Instead, a realm *queries* each member's Home Realm and shows the intersection -- only artifacts where **every** member has an active grant.

The function is called `accessible_by_all`. It takes the list of realm members and the current time, and returns only the artifacts where every single member has a non-expired grant. If Ember shared a file with Caspian and Soren but not Wren, and all four are in the same realm, that file doesn't appear in the realm view. Everyone has to have access for the realm to reflect it.

A realm is a **window**, not a container. A view into what everyone agreed to share. The artifacts stay in their owners' Home Realms. The realm just shows what's visible to all.

Two sharing patterns make this flexible:

1. **Broadcast**: `share_artifact_with_mode` -- grants the same access mode to every member of the realm. Ember can share a reference image with the whole team at Revocable. Everyone sees it. Everyone can lose it if she recalls.

2. **Granular**: `share_artifact_granular` -- different modes for different people. Ember can share the same artifact into the realm but give Caspian Permanent access and everyone else Revocable. The realm shows the artifact to everyone (because everyone has *some* grant), but the depth of trust varies per person.

This means the same shared space can contain different trust relationships simultaneously. Caspian can download the brand guidelines and reshare them. Soren can view but not save. Wren's access will expire on Friday. They're all looking at the same realm, seeing the same artifact -- but their relationship to it is different. The view adapts to the trust topology within it.

No shared folder in the history of computing has ever worked this way. Shared folders are containers. Realms are lenses.

---

## 🛟 When You Lose Everything

<iframe src="images/11-recovery-network.html" width="100%" height="500" style="border:none; border-radius:8px;" title="Recovery network diagram — an interactive visualization showing how peers with permanent and transfer access can help recover artifacts after device loss, forming a distributed backup through trust relationships"></iframe>

Your phone falls in a river. Your laptop's drive dies. A fire takes your home office. You've lost your device, and with it, your Home Realm.

In a centralized system, this is either fine (because the cloud has your data) or catastrophic (because you had no backup). There's no middle ground. Your safety depends entirely on whether you were paying a company to store copies.

In Indra's Network, your safety depends on something more interesting: the people you trusted.

The sharing model doubles as a distributed backup system. Here's how recovery works:

1. You set up a new device and regenerate your identity. Your Home Realm ID is deterministic -- same identity, same realm -- so your new device knows *where* your data should be, even though it's empty.

2. You send an `ArtifactRecoveryRequest` to your known contacts -- the people you've shared things with, the people who've shared things with you.

3. Each peer checks their local blob store and their grant records. They know what artifacts they hold and what access mode they have.

4. Each peer responds with a `RecoveryManifest` -- a list of everything they can help restore, including the artifact name, size, type, access mode, and original owner.

5. You review the manifests and select what to recover. Your node rebuilds the ArtifactIndex entries from the recovered data and metadata.

The quality of recovery depends directly on the access mode:

- **Permanent and Transfer** holders are **fully recoverable**. They have download rights, which means they hold the actual blob data locally. Caspian has a complete copy of the brand guidelines. He can send it back whole.

- **Revocable and Timed** holders are **best-effort**. They may have viewed the artifact, but they don't necessarily have the blob cached locally. Recovery from these peers is possible but not guaranteed.

> Permanent grants serve double duty -- they're not just about sharing, they're about resilience. The more co-owners an artifact has, the more places it exists, the more recoverable it is. Trust makes your data safer.

This is the beautiful consequence of the design, and it wasn't bolted on as an afterthought. It's an emergent property of the trust model. When Ember gave Caspian Permanent access to the brand guidelines, she wasn't just trusting him with the work. She was, without thinking about it, creating a backup. If Ember loses everything, Caspian can give it back. If both Ember and Caspian lose everything, whoever Caspian reshared with can help. The trust network *is* the recovery network.

Generosity with trust creates a safety net. The people you trusted most are the ones who can help you recover. The wider your circle of deep trust, the more resilient your data becomes. In a system where hoarding is the norm -- where you protect by restricting -- Indra's Network creates an architecture where you protect by *sharing*.

---

## ✦ What Emerges

Step back from the mechanisms. Forget the BLAKE3 hashes and the CRDT synchronization and the Rust type system for a moment. Look at what this architecture produces as a human experience.

What does it mean when sharing carries weight? When granting someone Permanent access is a meaningful act -- something you think about, something that can't be undone? When the difference between "take a look" and "this is yours" is encoded in the infrastructure itself?

What does it mean when digital trust has a gradient? When you can lend without giving, show without surrendering, set a boundary that enforces itself?

What does it mean when your files genuinely belong to you? Not hosted on a server you don't control. Not stored in a cloud that could change its terms tomorrow. Living in your Home Realm, derived from your identity, accessible from any of your devices, owned by you in the most literal sense the word can carry?

What does it mean when the more trust you extend, the more resilient your data becomes?

- 🔒 A filesystem where your files are genuinely yours -- not hosted on someone else's server, not subject to someone else's terms
- 🤝 Sharing that preserves the nuance of human trust -- provisional, permanent, bounded, or complete
- 📎 No duplication -- the file exists once, sharing is a pointer plus permission
- 🛡️ Revocation that actually works -- you can take back what you showed, not just hope they deleted it
- 🌐 Recovery through trust -- co-owners are your distributed backup
- 🔗 Shared spaces as views, not containers -- realms reflect trust, they don't trap files

What emerges from this architecture isn't just a filesystem. It's a **trust fabric**. Every grant, every revocation, every transfer carries social meaning. The network doesn't just store your files -- it encodes your relationships.

In the [first article](indras-network-every-node-a-mirror.md), we described a transport layer with no center -- messages bouncing between peers like light between jewels. In the [second](your-network-has-an-immune-system.md), an immune system with no moderator -- communities protecting themselves through local sentiment and emergent response. Now, a filesystem with no cloud -- files that live with their owners, shared through acts of trust rather than acts of copying.

The pattern holds: **the participants are the infrastructure.**

---

## 🌅 What Comes Next

The shared filesystem described here is built and working. Content-addressed storage with BLAKE3. Four-mode access control -- Revocable, Permanent, Timed, Transfer. CRDT-synchronized artifact index. Realm-level views computed from trust intersections. Peer recovery through grant-based distributed backup. Provenance chains that trace every artifact's lineage. All of it implemented, tested, and running in Rust.

What gets built on top -- the economies, the governance, the creative possibilities that emerge when communities own their own storage and their own trust relationships -- is the subject of future articles.

---

*What would you share differently if sharing had weight?* 💬

**This is Part 3** of a series on Indra's Network. [Part 1: Every Node a Mirror](indras-network-every-node-a-mirror.md) covers the transport layer. [Part 2: Your Network Has an Immune System](your-network-has-an-immune-system.md) covers the social defense layer.

**Subscribe** for future articles on the economic layer -- how groups that own their own infrastructure can build their own economies.
