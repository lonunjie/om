# Privacy, Governance, and Authority

## Problem statement

Strong privacy protects ordinary users and can also be used by malicious users. Centralized surveillance can help a platform investigate some abuse but creates a universal capability whose safety depends on whoever controls it now and later.

OM avoids solving this by choosing either extreme.

The design target is:

> **Private communication + enforceable local/provider/community governance + no universal controller.**

## Terminology

“Big Daddy” is useful conversationally, but architecture documents should use precise terms:

- **central authority** — actor with system-wide decisive power;
- **platform operator** — operator of an app/service layer;
- **service provider** — relay/storage/media/directory provider;
- **trusted third party** — actor whose honesty/correctness is required for a security property;
- **governance authority** — actor/process permitted to make policy/protocol decisions.

## Core authority rule

> **Authority must be scoped, replaceable, and non-composable.**

### Scoped

A relay can reject traffic at that relay. It cannot revoke an AccountID globally.

A group administrator can remove a member from that group. They cannot ban the person from OM.

A directory can refuse an alias. It cannot rewrite a user's cryptographic identity.

A media provider can refuse TURN/SFU allocation. It cannot decrypt media or add participants.

### Replaceable

A user can switch relay, blob store, directory, media provider, or push strategy without creating a new identity.

### Non-composable

Using a vendor for relay + blob storage + TURN must not accidentally give that vendor enough durable identifiers or keys to reconstruct the full social graph/content. Components use distinct identifiers and minimum metadata.

## Authority map

| Actor | May control | Must not control |
| --- | --- | --- |
| User/account | own devices, contacts, recovery policy, provider choices | other users' identity |
| Group admins | membership/policy inside that group | network-wide identity or messages elsewhere |
| Directory | its aliases and discovery policy | AccountID root, private content |
| Relay | traffic/quotas/abuse policy on that relay | plaintext, global bans, root identity |
| Storage provider | object/quota lifecycle | plaintext or root identity |
| Media provider | allocations/routing/bandwidth | media plaintext, membership authority |
| Transparency witnesses | detect inconsistent directory/key views | rewrite identities |
| OM maintainers | official client, specs, conformance releases | private network-wide override |

## Governance without universal surveillance

Abuse prevention is layered:

- unknown senders are constrained before acceptance;
- recipients can block and report;
- groups can enforce membership rules;
- providers can enforce local resource/abuse rules;
- reports can disclose selected evidence intentionally;
- public/discovery spaces can be moderated more strongly than private relationships;
- endpoint-targeted lawful investigation can exist without a universal message decryption mechanism.

## Protocol governance

OM itself can become a central authority if the original project controls the only client, update key, profile registry, or compatibility tests. Therefore:

- protocol specifications are public;
- conformance tests/test vectors are public;
- independent clients are allowed;
- protocol changes are versioned;
- CryptoProfiles have lifecycle states and transparent deprecation rules;
- official build/release signing should move toward threshold/multi-party operational control where practical;
- reproducible builds/release transparency are goals;
- forks should remain able to interoperate while they share compatible protocol versions.

## Design review question

For every new feature ask:

> **Where did authority go?**

Then identify what that actor can observe, deny, modify, impersonate, or correlate, and whether changing that actor is survivable.

## Discovery does not recreate the central authority

OM explicitly rejects a mandatory protocol-global username registry. Directories control only their own namespace/search service. A local Resolver can use multiple DirectoryProviders, domain delegation, QR/invite or LAN rendezvous. Once a relationship is accepted, discovery infrastructure is no longer required for ordinary communication.

Optional oblivious lookup further divides metadata authority: a DiscoveryRelay can know the client's network address without knowing the alias query, while the DirectoryResolver can know the alias query without learning the client address. This follows the same scoped/non-composable authority principle used elsewhere in OM.

## 2026-09-22 governance refinement — no indispensable operator

Add an explicit survivability property:

> **No OM Project/Foundation/company may be indispensable to already-established private communication.**

A future foundation can steward standards and reference code. A future company can sell convenience. Neither may gain mandatory identity, relay, directory, recovery, update-for-the-whole-network, or policy authority.

The reference implementation may be socially privileged; it is never technically privileged.
## 2026-09-22 project-level anti-capture authority

Canonical project governance does not become protocol authority. SeedLife may curate a canonical release line, but public protocol rights/conformance artifacts remain independently implementable and forkable. See [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) and [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](PROJECT_STEWARDSHIP_AND_CONTINUITY.md).

