# OM Project Philosophy and Anti-Capture Constitution

- **Status:** Canonical project constitution
- **Checkpoint:** 2026-09-23 07:56 PHT
- **Scope:** Protocol, architecture, source, governance, providers, official distributions, future businesses, and project succession

## 1. Purpose

OM exists as a **gift to humanity**: durable private communication that people can continue to use, implement, operate, repair, and carry forward without needing permission from one company, founder, government, cloud, app store, repository host, or future project steward.

OM is not anti-business. Businesses may build clients, operate providers, sell support, host infrastructure, integrate OM, and compete on service. **Commercial participation is welcome; protocol privilege is not.**

The project therefore distinguishes stewardship from ownership of the commons:

> **SeedLife/lonunjie may steward the canonical project, but neither SeedLife, the founder, nor any successor owns the right to make private communication depend on them.**

## 2. Constitutional principles

### P-01 — Users are principals; providers are replaceable tools

Identity, history meaning, relationships, and protocol authority do not belong to infrastructure providers. Relays, directories, storage, push, media, discovery, app stores, and managed services exercise only narrow capabilities.

### P-02 — No indispensable institution

Established independent users must remain able to communicate if the original author, SeedLife, GitHub, the project website, a business, a cloud provider, or an official client disappears.

Where a feature cannot meet this property, the dependency must be explicit, optional, replaceable, and unable to redefine canonical identity/history.

### P-03 — The protocol is implementable without permission

A conforming implementation must be possible from public normative artifacts, without a proprietary SDK, secret server, private registry, trademark license, commercial contract, or authorization from SeedLife.

### P-04 — No hidden protocol privilege

Official clients and convenience providers receive no secret extension, privileged endpoint, private key hierarchy, unpublished compatibility rule, or mandatory service unavailable to independent implementations.

### P-05 — Freedom survives governance capture

Governance is useful but cannot be the final security boundary. Public releases are under irrevocable open licenses. Normative protocol artifacts, test vectors, schemas, and conformance material are mirrorable. If the canonical steward becomes hostile or captured, the community retains the legal and technical ability to fork and continue.

### P-06 — Open does not mean weak

Interoperability never justifies lowering security or privacy floors. Extensions that affect authentication, confidentiality, identity, authority, downgrade behavior, or wire compatibility are explicit, versioned, testable, and fail safely when critical semantics are unknown.

### P-07 — No proprietary mandatory extension

Canonical OM MUST NOT require an extension whose implementation needs a royalty-bearing patent license, closed SDK, undisclosed specification, private approval, or vendor-only service. Optional bridges may depend on external proprietary systems, but they are not OM protocol authority.

### P-08 — Privacy is structural

OM minimizes the information any one party can learn. It prefers pairwise/opaque capabilities, local-first state, separate roles, explicit disclosure, and replaceable providers over centralized observation followed by policy promises.

### P-09 — Complexity belongs under the product

Normal users should experience a messenger, not a distributed-systems control panel. Strong defaults and automatic safe migration are preferred. Advanced controls exist without making ordinary users understand relays, ciphers, stores, or governance.

### P-10 — Measure repeatedly; cut once

Architecture is hardened before product engineering. Ambiguity at security, authority, wire, recovery, or migration boundaries is treated as unfinished architecture, not something to improvise in implementation.

Implementation begins only after the architecture-freeze contract says it may begin.

### P-11 — Prose states values; executable contracts prevent drift

This constitution is intentionally human-readable. Operational obligations are represented in machine-readable requirements, decisions, open questions, schemas, architecture maps, and repository validation code.

A prose claim that is not reflected in the machine contract where enforceable is incomplete.

### P-12 — Evidence may change design; convenience may not silently change principles

Architecture can be revised when standards, formal models, interoperability evidence, security review, platform reality, or measured implementation constraints contradict it. Such changes require an ADR and updates to the threat model/machine contract.

"It is easier to implement" is not sufficient reason to introduce central authority, hidden privilege, lock-in, or weaker security.

### P-13 — 0M: zero total control

The `O` in OM may also be read as `0`: **0M**. Zero means no one has total control of OM.

This does not require every component to be peer-to-peer or forbid centralized services. It requires that centralized components remain scoped and replaceable, and that no founder, steward, provider, catalog, dominant client, extension, cloud, app store, government or business can become the universal permission boundary for continued OM interoperability.

OM is decentralized primarily in **ultimate authority and survivability**: it is for and by everyone, with local operators retaining local authority but no actor receiving protocol-wide ownership.

## 3. Anti-capture mechanisms

OM uses multiple independent protections because no single license or governance rule is enough:

1. **open normative protocol artifacts** that anyone can implement;
2. **public conformance vectors/suites** that define behavior independently of the reference implementation;
3. **replaceable infrastructure roles** with no vendor encoded as protocol authority;
4. **copyleft on shared core code** so distributed modifications to OM's core remain available;
5. **network copyleft on OM reference provider/server software** so hosted improvements to those implementations cannot simply disappear behind a service boundary;
6. **no broad copyright-assignment/relicensing CLA** as the ordinary contribution path;
7. **DCO/inbound-equals-outbound contribution rules** so contributors retain their copyrights while licensing contributions under the project's declared outbound terms;
8. **royalty-free protocol patent policy** before accepting normative patent-encumbered contributions;
9. **transparent versioned protocol governance** and no private compatibility decisions;
10. **forkability** as the final escape hatch if stewardship fails.

## 4. What OM deliberately does not prohibit

OM does not prohibit commercial use, paid hosting, paid support, proprietary surrounding applications, research use, government use, or competing distributions merely because of who operates them. Such field-of-use restrictions would undermine the project's claim to be open source and a common protocol.

Instead OM prevents capture by ensuring that no participant can convert its commercial success into mandatory protocol authority.

## 5. Canonical stewardship versus protocol freedom

The canonical OM project may choose:

- which changes enter its own release line;
- which versions are called supported by the canonical project;
- which builds may use canonical project trademarks under the trademark policy;
- which maintainers receive repository/release authority.

It may **not** retroactively revoke rights already granted under open licenses, make existing AccountIDs depend on the steward, or stop third parties from implementing published protocol versions under their licenses.

A fork may cease to be the canonical OM project or use a different name, but it must remain technically and legally possible.

## 6. Founder period

During incubation, a single founder steward is an accepted operational reality. OM will not manufacture artificial multi-party trust before trustworthy people actually exist.

The architecture therefore requires **succession rails, not pretend succession**:

- roles and key ceremonies are documented before they are populated;
- authority can later become multi-person without protocol migration;
- repository teams and scoped roles are designed in advance;
- release-root thresholds can move from 1-of-1 incubation custody to multi-party custody;
- domain, forge, registry, store, and security-contact custody have explicit migration procedures;
- no protocol object is bound to the founder's personal account.

The transition occurs when trustworthy maintainers exist and the project has enough real users/importance to justify distributed custody. It is not forced by an arbitrary calendar date.

## 7. Interpretation rule

When two plausible designs conflict, prefer the one that, while remaining usable and secure:

1. leaves users with more durable agency;
2. requires fewer irreplaceable authorities;
3. exposes less metadata;
4. survives provider/project failure better;
5. remains independently implementable;
6. makes capture easier to escape;
7. keeps ordinary UX simpler;
8. creates fewer irreversible protocol commitments.

This document is the highest-level project intent. It does not override cryptographic evidence, law, or physical impossibility; it tells future maintainers what OM is trying to preserve when tradeoffs arise.

## 8. The seven meanings of OM

`NAMING.md` is canonical for the name. OM intentionally means **Open-source Messenger, OpenMessenger, OwnMessenger, OurMessenger, OneMessenger and OhMessenger**, plus the secondary **0M** reading: zero total control. Together these require an open commons, independent implementations and agents, safe customization, strong group/organization capability, extensibility without fragmentation, an approachable default product, and architecture in which no one party becomes an indispensable universal controller.

## 9. Capability support does not imply project operation

OM may define interoperable capabilities for features that SeedLife/lonunjie never operates as services. Supporting a content pack, bot, community, broadcast room, directory, entitlement proof or future commerce extension does not require the OM Project to host a catalog, run the bot, moderate the community, process payment, custody funds, or become the default provider.

Prefer protocol/client capability plus replaceable operator roles over project-operated infrastructure whenever the latter is not required for security or usability.
