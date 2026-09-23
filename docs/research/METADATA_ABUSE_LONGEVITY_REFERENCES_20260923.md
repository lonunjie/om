# Metadata, Abuse, and Longevity References — 2026-09-23

This file records external patterns used as evidence during the 08:21 PHT architecture-hardening pass. They are inputs, not authorities over OM design.

## Metadata/privacy

### MLS — RFC 9420

RFC 9420 explicitly notes that observable MLS fields/lengths and delivery-service choices can reveal group ID, epoch activity, message frequency and potentially membership. This supports OM's choice to avoid equating E2EE with metadata privacy and to wrap inner group/session traffic in provider-opaque OM transport envelopes.

- https://www.rfc-editor.org/rfc/rfc9420.html

### Oblivious HTTP — RFC 9458

OHTTP separates a relay that sees the client's network connection from a gateway that sees the plaintext request, under non-collusion and traffic-analysis assumptions. OM uses this as precedent for split private discovery rather than inventing a project-specific privacy proxy trust model.

- https://www.rfc-editor.org/rfc/rfc9458.html

### Privacy Pass — RFC 9576

Privacy Pass describes unlinkable authorization/admission tokens and explicitly discusses deployment/non-collusion assumptions. This informs IntroGate's optional privacy-preserving admission proof interface; OM does not mandate a universal issuer/attester.

- https://www.rfc-editor.org/rfc/rfc9576.html

### Signal sealed sender and private groups

Signal demonstrates two useful architectural goals: reducing sender/routing metadata exposed to the service, and protecting group membership/attributes from server plaintext while still supporting server-assisted consistency/access control. OM does not copy Signal's centralized service assumptions, but adopts the principle that providers need not receive canonical social-graph identifiers merely to route encrypted data.

- https://signal.org/blog/sealed-sender/
- https://signal.org/blog/signal-private-group-system/
- https://signal.org/blog/pdfs/signal_private_group_system.pdf

### SimpleX

SimpleX demonstrates strong metadata minimization using pairwise/unidirectional queue identities and no provider-visible user profile identifier. OM retains stable provider-independent AccountID for continuity/authority, but follows the compatible lesson that stable identity does not need to become a relay address and that pairwise/provider-scoped queue capabilities reduce social-graph visibility.

- https://simplex.chat/messaging/
- https://simplex.chat/docs/simplex.html

## Decentralized moderation/safety

### AT Protocol labelers

AT Protocol separates user controls, network/provider takedowns and signed labels produced by selectable moderation services. Users/apps may choose labelers. This is useful precedent for OM `SafetyAdvisory`: signed third-party advice whose enforcement remains local/scoped rather than universal protocol truth.

- https://atproto.com/guides/moderation
- https://atproto.com/guides/labels
- https://atproto.com/specs/label
- https://atproto.com/guides/creating-a-labeler

### Matrix / Mjolnir policy lists

Matrix communities can share/watch moderation lists and apply them through community-controlled tooling. The useful idea is collaborative defense across communities; the caution is that a moderation bot with broad administrator authority can become a de facto owner of many rooms. OM therefore keeps shared advisories separate from Room authority and requires explicit scoped capability delegation.

- https://matrix.org/docs/communities/moderation/

## Long-term continuity

The 08:21 longevity pass primarily derives from OM's existing supply-chain, stewardship, protocol-extensibility, crypto-agility, provider-portability and 0M architecture rather than a single external system. Existing relevant reference families include:

- The Update Framework: https://theupdateframework.io/
- in-toto: https://in-toto.io/
- SLSA: https://slsa.dev/
- RFC 8949 deterministic/canonical CBOR considerations: https://www.rfc-editor.org/rfc/rfc8949.html

The architectural conclusion is intentionally narrower than “decentralize everything”: published protocol artifacts must be independently mirrorable/interpretable; runtime protocol authority must not require a live canonical project server; and incompatible future governance/protocol lineages must fork explicitly rather than rewriting old semantics.
