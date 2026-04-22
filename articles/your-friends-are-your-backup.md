# Your friends are your backup

Most apps protect your account with a password. If you forget it,
you're locked out forever — or you give some company permission
to reset things for you, which means they could lock you out too.

The Synchronicity Engine takes a different route. Your account is
protected by a small group of friends you trust. If you ever lose
your device, any few of them can help you back in. No company is
involved. No password to forget. No story to memorize.

## How it works, in three glances

### Setting up

Open **Backup plan** from the status bar. Your direct-message
friends show up as a list. Tap the ones you trust. You pick how
many of them need to agree before you can recover — three out of
five is a common choice.

Each friend gets a notification asking if they'll be one of your
backup friends. They see a plain-language description of what
that means. Tap Accept. Done.

The app handles the rest in the background — your recovery key
gets split into sealed pieces, one per friend, in the same way
ancient scribes might have torn a letter into pieces and given
each courier just one. No single friend can do anything with
their piece alone. Only together — enough of them, working
together — can they help.

### If you lose your device

Get a new device. Make a fresh account. Message your backup
friends through any channel — text, a call, in person — and ask
them to add your new device as a contact.

Once they've added you, open **Use backup** on your new device.
You'll see your friends listed. Tap the ones you want to ask for
help. Tap **Ask for help**.

Each friend now gets a notification: "Someone on a new device
claims to be you and is asking for help recovering." Before they
approve, they verify it's really you — a phone call, a video, a
meeting in person. They use whatever channel they'd normally use
to be sure.

When enough of them have approved, your new device is
automatically accepted as yours. Your profile syncs back. Your
files come back from the same friends — they were holding sealed
pieces of those too, and they all come home.

### Who holds what

There are two kinds of backup your friends can hold for you:

1. **Recovery pieces.** A small group (usually three to five
   people) holds sealed parts of your account's recovery key.
   Together, any of them can bring you back.
2. **File pieces.** A wider group holds encrypted chunks of your
   personal files — the actual photos, documents, notes. They
   can't read the contents. They can't reassemble them on their
   own. But if your device dies, enough of their chunks together
   rebuild every file.

You can ask the same friends to do both, or split the job between
different people.

## What your friends actually see

The app is careful to explain, in plain language, what each role
means:

> **Alex wants you to be one of their backup friends.**
>
> If Alex ever loses their phone, you'll help them get back in.
> Before you approve, you'll verify it's really Alex through
> another channel — call them, video chat, see them in person.
> You just tap Approve once you're sure.

No cryptography vocabulary. No keys, no shares, no codes to copy.
Your friends don't have to be technical. They just have to be
people you trust to take a moment and make sure it's you.

## What this isn't

**Not a password manager.** If you want to unlock something on a
whim, a password is faster. Backup friends are for the bigger
question: "what if I lose everything?"

**Not a single point of failure.** No one person can impersonate
you. No one person can lock you out. Your friends are a
distributed net, not a gatekeeper.

**Not a company.** No support ticket. No verification queue. No
servers holding the keys. Your friends' phones are the vault, and
you verify each other the same way humans always have — by
recognizing each other.

## Why do this

Because the alternative is renting your account from a company
that can change the terms at any time, or trusting a single piece
of paper you might lose. This is neither. This is trust as a
shape — three or four people who know you, and whose phones
together know how to unlock your life.

It's the same shape that's always worked: your neighborhood, your
group of friends, the circle of people who would vouch for you.
The software just makes the shape legible to the network.

## Get started

Open the Synchronicity Engine. Add a few direct-message contacts.
Go to **Backup plan**. Pick them. You're done.

If you ever need it — and we hope you never do — they're there.

---

Engineers: the full technical write-up is in
[`articles/recovery-architecture.md`](./recovery-architecture.md).
