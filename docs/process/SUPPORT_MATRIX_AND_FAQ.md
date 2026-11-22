# Support matrix and FAQ (draft)

**Status: draft for operator review, 2026-11-22.** This is user-facing writing, not a design document. It states what OM does
today, what is built but switched off, what is not built, and what has never been tried on a real device. It makes no security
claim: no human cryptographer has audited OM ([`SECURITY.md`](../../SECURITY.md)). The sources for every row are the reports linked
in [`2026-11-22-evidence-index.md`](../reports/2026-11-22-evidence-index.md) and the checkpoint in [`SNAPSHOT.md`](../../SNAPSHOT.md).
Where this page and a normative design document disagree, the design document wins; report the difference.

## 1. How to read the labels

| Label | Meaning |
|---|---|
| **In the open beta** | In the build served at <https://om.lonunjie.org> (version 0.0.12 when this was written). |
| **Built, not released** | The code exists in the repository, but the public build does not offer it. It is switched off until a release declares it. |
| **Not built** | No working code. Do not expect it. |
| **Not tried on real devices** | Automated tests ran in desktop browser engines. Nobody has run it on the phones or systems named. |

## 2. Features

| Feature | State | Notes |
|---|---|---|
| One-to-one chats, end-to-end encrypted by design | In the open beta | Each person sets up their own delivery service; there is no built-in default. |
| Invite by link | In the open beta | |
| Linking a second device | In the open beta | |
| Chat-history backup file (password-sealed) | In the open beta | Restores history only, as read-only conversations; it does not restore contacts or keys. |
| Attachments, voice notes, location cards | In the open beta | |
| Search in your own chats | In the open beta | Runs on your device. |
| App lock (password or device prompt) | In the open beta | |
| Rooms (group chats, up to 32 people) | Built, not released | Off in the public build. Larger-Room behaviour and several user-interface details are still being checked. |
| Replies, reactions, edit, delete, polls, scheduled send | Built, not released | Scheduled send is on your device only: the device must be on and OM must be able to run. |
| Alerts when OM is closed (push) | Built, not released | Needs a delivery service update and a release. Never tried on real iPhones or Android phones. |
| One-to-one voice and video calls | Built, not released | Off by a build setting. Never tried on real phones. No production relay for call traffic exists yet. |
| Stickers, custom reactions, sticker packs | Built, not released | Off by a build setting. Pack files only; there is no catalog or store. Never tried on real phones. |
| Recovery of your account without your old device | Built, not released | Several steps exist (key sheet, waiting period, approval by a controller). It has not been run end to end by a person outside the project. |
| Group calls | Not built | Waiting on a specialist review of how call keys reach people who are not in a one-to-one relationship. |
| Typing indicators | Not built | |
| Finding people by phone number or username | Not built, by design | A central directory is something OM refuses to depend on. |
| Link previews | Not built | OM does not fetch pages for you; links the app judges unsafe are not made clickable. |
| Local network mode, bridges, plugins, communities and publications, the portable archive | Not built | These parts of the design are not yet authorized for engineering. |
| Native iPhone, Android or desktop apps | Not built | The web app is the reference client and installs from the browser. |

## 3. Platforms

The beta is a web app you install from the browser. The table lists what the project has actually tested.

| Platform | State |
|---|---|
| Chrome on a Mac | Probe and automated tests passed. Not checked with the shipped client by a person on every feature. |
| Safari on a Mac | Same. |
| Firefox on a Mac | Same. |
| Safari on an iPhone | A test page ran on one phone in 2026-10-01. The shipped client has not been checked on a real iPhone. Installing to the Home Screen is required for background features. |
| Chrome on an Android phone | A test page ran on one phone in 2026-10-01. The shipped client has not been checked on a real Android phone. |
| Chrome or Edge on Windows | Not tested. Treat it as unsupported. |
| Linux browsers | Not tested. |
| Other browsers | Not tested. |

Known platform facts, from the device probe: passkeys that unlock the vault are synced by the operating system on every tested
platform, because none offered a device-bound credential that can do the job; OM falls back to a password or a software-held key.
Notifications on Android keep their text in the system notification history, which an app cannot clear; OM's default there is
notifications with no message content ([`PLATFORM_CAPABILITY_MATRIX.md`](../architecture/client/PLATFORM_CAPABILITY_MATRIX.md)).

## 4. What has not been proven

- **Real-device behaviour** of push, calls and stickers: no run exists. The manual matrix for calls is all "Pending"
  ([`2026-11-19-calls-v007-evidence.md`](../reports/2026-11-19-calls-v007-evidence.md)).
- **First setup by a person** who is not a project agent, with a new Cloudflare account and a GitHub account: not done.
- **Independent review:** every review so far was done by AI models. No human cryptographer has audited OM, and an independent
  implementation of several parts is still required by the project's own gates.
- **Resilience to a provider being blocked:** not claimed. A second delivery-service implementation with no cloud-vendor
  dependency is not built, so OM does not claim censorship resistance.
- **Calls across engines:** the automated call test runs in Chromium only.

## 5. FAQ

**What is OM?** An open-source messenger with end-to-end encryption by design, built so that no single company, server or project,
including its authors, controls your identity or conversations. It is stewarded by SeedLife under the lonunjie organization.

**Is it safe to use for sensitive conversations?** Not yet. It is a beta, it has not been audited by a human cryptographer, it makes no
security claims, and data may be lost. Use it for things you can afford to lose and watch the [`SECURITY.md`](../../SECURITY.md) page.

**Do I need an account with a company?** To set up your own delivery service you need a Cloudflare account, and the deploy button
copies a template into a GitHub account. That is the current reference setup; the design allows other providers, but only this one is
built. The design gives the project no access to your messages, and the project does not run your delivery service for you.

**What is a delivery service ("Home")?** A small online mailbox that holds encrypted messages until the other person's device collects
them. Each person owns theirs. It can observe things like when messages arrive and their sizes; the privacy notice lists them
([`BETA_PRIVACY_NOTICE.md`](BETA_PRIVACY_NOTICE.md)).

**Why is setup not one tap?** Because OM refuses to ship one shared server. Setup is about a minute for most people, but it has not been
run by someone outside the project yet, so expect rough edges and tell us.

**Will I get a notification when OM is closed?** Not in the current public build. Alerts are built but not released and have not been
tried on real phones. While OM is open on the screen, delivery is event-driven; when it is idle or hidden the app checks less often.

**Can I make calls?** Not yet in the public build. One-to-one calls are built and switched off. Group calls are not built.

**Can I use stickers and custom reactions?** Not yet in the public build. They are built and switched off.

**Can I have group chats?** Not in the public build yet. Rooms are built and switched off.

**What if I lose my phone?** If you linked a second device, use it. The backup file restores chat history only. Full recovery without any
device is built but not released and has not been run by an outside person. Keep your key sheet if one has been offered to you.

**Does OM work in the background on iPhone?** Not yet. Alerts need the app installed to the Home Screen and a future release.

**Can I find friends by phone number?** No. You share an invite link or code. That is deliberate.

**How do I report a bug or a security problem?** Bugs and ideas go to the project's GitHub issue forms and discussions (Settings > Help and
feedback in the app). Security problems follow [`SECURITY.md`](../../SECURITY.md).

**Is this a product I can rely on every day?** Not yet. The gaps between today and a daily-driver messenger are tracked in the project's
reports; the short list is in section 2 and section 4 above.
