# OM Project Stewardship and Continuity

- **Status:** Canonical project-governance architecture
- **Checkpoint:** 2026-09-23 08:21 PHT
- **Current intended forge home:** `github.com/lonunjie/OM` under the SeedLife/lonunjie organization

## 1. Stewardship model

SeedLife/lonunjie is the initial project umbrella and repository steward. It is **not protocol identity authority** and is not encoded into OM accounts, relationships, wire objects, provider contracts, or cryptographic trust.

The GitHub organization technically owns repositories hosted under its namespace. That repository ownership is an operational permission model, not ownership of the protocol commons. Open licenses, mirrors, public conformance artifacts and independent implementations make repository-host capture survivable.

## 2. Incubation state: founder custody is intentionally 1-of-1

At this checkpoint the project accepts a single founder steward for repository and release/governance decisions.

This is an explicit temporary operational risk, not a claim of distributed governance.

OM will not grant sensitive authority to people merely to satisfy a numeric redundancy target. Trustworthy co-stewards are added when real people with demonstrated judgment and commitment emerge.

## 3. Succession rails must exist now

Although roles may be unpopulated, the project architecture must make later delegation routine rather than a redesign.

Prepared roles:

```text
SeedLife organization owners
OM repository administrators
OM maintainers
OM security response
OM protocol/spec editors
OM release managers
OM release-root custodians
OM infrastructure/domain custodians
OM app-distribution custodians
```

The machine governance contract records which roles are currently active and the minimum maturity stage required before they become multi-person.

## 4. Maturity stages

### S0 — Founder incubation (current)

- one trusted founder steward is acceptable;
- repository may live under `lonunjie/OM`;
- release-root custody may be 1-of-1 if releases are experimental/pre-production;
- no false claim of organizational redundancy;
- all critical assets and handoff procedures are inventoried;
- protocol/source licenses already permit independent continuation.

### S1 — Trusted maintainer circle

Trigger: trustworthy maintainers exist and have demonstrated sustained technical/security judgment.

Possible progression:

- scoped GitHub teams first;
- at least one additional repository administrator where justified;
- security/reporting role separation;
- signed maintainer roster;
- documented review thresholds for protocol/security changes.

Organization-owner authority is granted only when genuinely warranted.

### S2 — Public-production resilience

Trigger: OM has meaningful independent users and/or official public production distributions.

Requirements before claiming resilient project custody:

- multiple organization owners;
- multi-party release-root threshold;
- independent recovery of domain/DNS/registry/store credentials;
- multiple security contacts;
- reproducible/mirrored release path;
- documented emergency rotation/compromise procedure;
- tested creator-disappearance drill.

### S3 — Institutional longevity

Trigger: community/project scale makes individual stewardship insufficient.

Potential mechanisms include a formal nonprofit/foundation, elected/appointed technical council, independent security committee, or another structure consistent with the anti-capture constitution. The exact institution is deliberately not frozen in S0.

## 5. Asset custody map

Continuity planning MUST cover at least:

- GitHub organization and repository administration;
- source mirrors;
- canonical website/documentation domain;
- registrar and DNS;
- TUF/update/release-root keys;
- artifact/package/container registries;
- code-signing/notarization identities;
- Apple/Google distribution accounts where used;
- APNs/FCM credentials where used;
- security contact/mailboxes;
- trademark/domain ownership where applicable;
- funding/billing accounts for any project-operated services;
- encrypted incident/recovery documentation.

None of these assets may become part of AccountID or end-user protocol authority.

## 6. Adding people later

The repository should make expansion mechanical:

1. add the person to the relevant scoped team;
2. grant only the needed repository/operational role;
3. update the machine stewardship registry;
4. if release authority changes, perform the documented key-rotation/threshold ceremony;
5. record the change in an ADR/change log when it affects project-wide trust;
6. run repository/continuity validation.

Adding a maintainer MUST NOT require protocol migration for users.

## 7. Removal or compromise

A steward's project credentials can be rotated/revoked without changing OM user identities. Project release-key compromise affects trust in future official project releases, not ownership of users' AccountIDs.

The project continuity plan must distinguish:

```text
project stewardship compromise
!=
user account compromise
!=
provider compromise
```

## 8. Creator-disappearance objective

At S0, creator disappearance may halt the canonical project's ability to publish a new official release until someone forks/assumes stewardship. It MUST NOT make already established independent OM communication cryptographically depend on the missing creator.

By S2, the canonical project itself should also be able to continue through the documented multi-person custody arrangements.

## 9. 2026-09-23 08:21 0M longevity refinement

`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md` is canonical for the 20-year A-015 pass.

Project succession and protocol succession are deliberately different. A successor steward does **not** inherit user Account/Room/Service authority and does not gain the right to redefine already-published registry IDs. Existing protocol generations remain independently mirrorable/interpretable through Protocol Continuity Bundles.

Before S2/public-production resilience is claimed, the project must run a creator-disappearance/forge-domain/provider/release-root continuity drill using independent mirrors and demonstrate a credible successor release path without changing end-user AccountIDs merely because project infrastructure vanished.
