# Messenger Baseline and Ecosystem Architecture Queue

- **Status:** Architecture design queue; intentionally unresolved before engineering
- **Checkpoint:** 2026-09-23 05:45 PHT

OM must be both a rigorous secure protocol and a messenger ordinary people actually want to use. The questions in `machine/architecture_design_queue.jsonl` therefore block the engineering transition until their protocol-impacting boundaries are decided.

## Current provisional direction

### Stickers and GIF-like animation: yes

Stickers are now ordinary messenger language, not a niche add-on. OM should support portable external packs, easy install-from-message/share, emoji/keyword association, recents/favorites and static/animated/video artwork. Telegram's strongest lesson is not one codec; it is the **open creator loop**: anyone can create, pack, share and install artwork, with emoji acting as a fast discovery surface.

Generic “GIF” UX should also exist, but OM should model it semantically as looping animation/media so clients can use efficient safe codecs rather than making the legacy GIF codec a protocol requirement. Search is an optional `MediaSearchProvider`, not a mandatory central Giphy/Tenor dependency.

### External packs: yes

A pack should be a signed/integrity-addressed portable manifest plus bounded resources, installable from a direct OM link/QR/file/provider. The OM Project does not need to host a global sticker store. Independent creators, communities and catalogs can host packs.

A pack signature proves provenance/integrity, not that SeedLife verified ownership. Manifests should carry optional author/license/attribution/source fields and make copying/forking explicit where permitted.

### Paid packs: keep commerce outside Core

OM should not issue a token, process payments, custody funds or become merchant-of-record simply to let artists sell artwork. The architecture queue will evaluate a generic optional entitlement proof/provider interface so a creator or marketplace can unlock a pack after an external transaction.

A decentralized payment rail is **not** a baseline messenger feature. Operating one can add payment, fraud, refund, tax, sanctions, app-store and security obligations far beyond messaging, depending on the role and jurisdiction. If users later want a particular payment system, it should arrive through a separately operated plugin/provider with explicit trust and jurisdiction boundaries.

### Bots and LLMs: first-class but explicit

OpenMessenger means independent clients and agents are welcome. LLMs should use the same explicit service-principal model as bots: scoped capabilities, visible installation, minimum context, revocation, and a clear distinction between local/private inference and a remote service receiving plaintext.

### Groups, communities and broadcasts

Do not copy one product wholesale:

- **Discord:** learn from roles, scoped channel permissions, apps/commands and persistent forum channels.
- **Telegram:** learn from separate broadcast channels vs interactive groups, scalable topics, expressive media, and its 2026 Communities model that links distinct groups/channels/bots without collapsing them into one chat.
- **Matrix:** learn from Spaces as containers of independent rooms and globally namespaced extensible event concepts, while avoiding a mandatory homeserver in OM.
- **Zulip:** learn from persistent topic organization and follow/mute semantics for high-volume conversation.
- **WhatsApp Communities:** learn from a simple umbrella + announcements mental model for non-technical users.

The likely OM shape is a composable hierarchy:

```text
Identity / Organization
        |
   Space / Community        optional organizational container
     /     |      \
  Room   Room    Room        independent conversation/security objects
   |       |       |
 chat   broadcast  forum/topic-capable
```

But membership inheritance, role scoping, MLS relationships, public publication and topic semantics remain queued because those choices affect the wire/control model.

### Broadcast channels: likely yes, but separate semantics

A private encrypted group and a public publication feed are not the same trust/legal surface. OM should be able to support one-to-many publication without forcing all users into a social network or requiring SeedLife to operate discovery/moderation. Public discovery and hosting are optional provider roles.

## Why these remain in the architecture queue

The above directions are strong, but details can affect:

- canonical event types;
- group/control authority;
- MLS membership and scaling;
- bot plaintext access;
- public-content legal/operator boundaries;
- extension criticality;
- pack/parser attack surface;
- app-store commerce rules;
- interoperability expectations.

OM will decide those before product engineering starts rather than retrofit them after the event model hardens.
