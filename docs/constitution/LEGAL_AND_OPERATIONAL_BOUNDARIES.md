# Legal and Operational Boundaries

> Architecture/planning document, not legal advice. Legal obligations depend on jurisdiction, facts, distribution method, content surface and the service a particular actor actually operates.

- **Status:** Canonical jurisdiction-neutral risk-allocation architecture
- **Checkpoint:** 2026-09-23 05:45 PHT

## 1. Objective

OM should minimize the OM Project's avoidable legal and operational exposure **without pretending that an open license, decentralization or cryptography can erase mandatory law**.

The design rule is:

> **Liability and compliance should attach as closely as practical to the actor that actually chooses to operate, publish, host, distribute, transact, or control the relevant service.**

The protocol/source project should not become a universal intermediary merely because it publishes interoperable software.

## 2. Role matrix

| Actor / component | Typical custody/control | Expected responsibility boundary |
|---|---|---|
| OM protocol/source project | public source/specs/tests/releases | software/spec publisher and steward |
| Individual self-host | their chosen relay/discovery/storage/client deployment | user-operated endpoint under their applicable law |
| Organization/community | its own membership, namespace, rooms, policies and optional providers | operator/admin for the space it chooses to run |
| Directory/catalog provider | aliases/index/pack metadata it elects to publish | directory/catalog operator |
| Managed relay/storage/media provider | service traffic/data within its role | service operator |
| Bot/LLM/bridge operator | data/context intentionally delivered to that principal | third-party service operator |
| PWA/native distributor | client build/update/store/push surfaces it controls | software distributor; store obligations if applicable |
| Marketplace/payment/entitlement provider | catalog, transaction and entitlement state it chooses to operate | commerce/payment operator |
| External evidence recipient | user-selected exported evidence | recipient/authority chosen by reporter |

Providing one role never grants authority over another.

## 3. Project-exposure minimization rules

The canonical OM project SHOULD, unless a later ADR establishes a compelling reason otherwise:

- operate no mandatory user-account service;
- custody no mandatory conversation plaintext or decryption key;
- operate no mandatory relay, blob store, directory, bot cloud, community host or payment processor;
- avoid a project-global public-content/sticker/bot marketplace when direct or third-party discovery is sufficient;
- avoid collecting identity/contact/telemetry merely for product analytics;
- keep diagnostics local/redacted/user-exported by default;
- publish software/protocol releases separately from optional hosted services;
- make optional hosted services separately identifiable operators with their own terms/privacy/compliance obligations;
- design export/migration so users and organizations can leave an operator;
- avoid marketing claims that promise legal immunity, perfect anonymity or impossibility of lawful process.

## 4. Open-source license protection

OM's standard licenses are deliberately established, widely used licenses with warranty disclaimers and limitations of liability **to the extent applicable law permits**. `LICENSING_AND_CONTRIBUTION_POLICY.md` defines the licensing stack.

The project SHOULD NOT add a custom restrictive EULA merely to shift every conceivable risk to users; that could conflict with open-source goals, create uncertain enforceability and become a capture mechanism. A distributor/provider may adopt role-specific terms for the service it actually operates, subject to its law and the applicable open-source license.

The project provides no default commercial warranty, SLA, indemnity, legal advice, content-rights warranty, payment guarantee or compliance certification unless a separate actor explicitly agrees to one in writing on its own behalf.

## 5. User-generated content and public surfaces

Private messaging, unlisted groups, public broadcast publication, searchable directories and marketplaces have materially different operational/legal surfaces.

Therefore:

- private communication remains the protocol baseline;
- public discovery is always a distinct optional provider/capability;
- broadcast/publication can be represented without requiring an OM-operated global feed;
- content packs can be shared directly or through independent catalogs;
- a catalog operator decides what it lists and handles obligations created by that choice;
- OM signatures establish provenance/integrity, not ownership of copyright or project endorsement;
- the reference client should expose reporting/blocking/export tools without requiring a universal OM moderation desk.

## 6. App-store distribution

An official store distribution may inherit reporting, moderation, commerce, age, privacy or other platform obligations. Those are **distribution-local** responsibilities.

Core protocol/client capabilities should make compliance possible without adding:

- a universal decryption key;
- mandatory central content review;
- a global OM user-account service; or
- a network service that independent distributions must use.

## 7. Payments and digital goods

OM Core does not process payments, custody value, issue a token, perform exchange, or act as merchant-of-record merely because a future extension can represent a paid entitlement.

Any future commerce design must preserve a hard boundary between:

```text
OM message/content-pack interoperability
             !=
payment / marketplace / merchant / settlement operation
```

Payment, tax, consumer-protection, sanctions, AML/KYC, app-store billing and refund obligations belong to the actor/service that elects to transact, subject to its jurisdiction. A future OM extension may carry an opaque entitlement proof without making the OM Project the seller or payment rail.

## 8. Jurisdiction-neutral release review

No country is a protocol home jurisdiction. Before a project-controlled distribution or hosted service is broadly offered, review the legal obligations of **that concrete role in the jurisdictions where that actor actually operates/offers it**.

The review is role-based, not protocol-global. It does not block people elsewhere from independently implementing or operating the openly licensed protocol.

## 9. Claims to avoid

Do not market OM as “untraceable,” “government-proof,” “impossible to subpoena,” “law-free,” or “perfectly anonymous.”

Preferred description:

> **OM is a general-purpose open messaging protocol and client family with end-to-end encryption, interoperable clients, and user-controlled replaceable infrastructure.**

## 10. Design objective

The objective is not legal evasion. It is minimizing unnecessary project custody and correctly scoping responsibility:

- publish a commons rather than become everyone's operator;
- collect less, therefore possess less;
- let optional providers shoulder the obligations of services they voluntarily operate;
- let users/organizations control the spaces they voluntarily operate;
- preserve protocol security regardless of any one operator's jurisdiction.
