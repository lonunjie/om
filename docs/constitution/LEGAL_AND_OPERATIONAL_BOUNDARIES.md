# Legal and Operational Boundaries

> Architecture/planning document, not legal advice. Legal classification depends on jurisdiction, facts, distribution method and services actually operated.

## 1. Purpose

OM should minimize centralized custody without pretending that decentralization erases law.

The key legal/operational question is always:

> **What does this particular actor actually operate, control, possess, publish or distribute?**

## 2. Role matrix

| Actor / component | Typical custody | Role boundary |
|---|---|---|
| Protocol/source project | public source/spec/tests | software/spec publisher/stewardship |
| Personal user deployment | user's relay/discovery/provider account | user-operated endpoint |
| Organization directory | alias/index metadata for its namespace | directory operator |
| Managed relay/storage/media provider | whatever its service handles | service operator |
| PWA distribution | web client/release/update origin | client distributor |
| Native store distribution | signed app, update/store/push boundary | app distributor |
| External evidence recipient | user-selected exported evidence | authority/organization chosen by reporter |

No actor should gain unrelated authority merely because it provides one of these functions.

## 3. Abuse reporting in the operatorless core

The protocol does not require a central OM AbuseDesk.

Core user actions:

```text
Block locally
Revoke relationship capability
Export selected evidence
Share evidence with a user-chosen authority/organization
```

`EvidencePackage` is generated locally from user-selected material and cryptographic context. The OM Project need not receive it.

A particular app-store distribution or managed/community service may add its own report handling if required by its policies/law. That is distribution/provider-local behavior, not universal protocol semantics.

## 4. Philippine legal review remains necessary

Philippine RA 11930 defines participative network platform providers broadly enough to mention instant messaging and imposes duties on internet intermediaries for data in their control/possession and specified child-safety processes.

RA 10175 also contains preservation/disclosure duties for qualifying service providers regarding data in their possession/control.

The operatorless design materially narrows centralized possession, but does **not** prove that a developer, app distributor, deployment operator or provider is outside those statutes.

Before broadly promoting a Philippine public release, obtain jurisdiction-specific advice on at least:

- whether publishing the reference PWA/source alone makes the publisher a covered platform/provider;
- whether a user's personal deployment is treated differently from a managed service;
- whether a native store distribution changes classification;
- NTC/VAS treatment of a user-owned OTT messenger/calling implementation;
- what preservation/reporting obligations attach to each concrete operator role.

## 5. App-store boundary

Current Apple and Google policies impose UGC/reporting/blocking/moderation expectations on qualifying store-distributed apps.

Therefore:

- blocking is a core protocol/client capability;
- local evidence export is core;
- a store distributor may need an additional distributor-operated report/review process;
- the existence of that process must not introduce a universal decryption key or make it a mandatory OM network service.

## 6. Public vs private product surfaces

Private finite-recipient messaging should remain architecturally distinct from future public feeds/channels/discovery surfaces.

Public broadcast/social functionality creates a substantially different moderation and regulatory surface. If added, treat it as a separate capability/product layer rather than quietly expanding private chat semantics.

## 7. Claims to avoid

Do not market OM as:

- “untraceable”;
- “government-proof”;
- “impossible to subpoena”;
- “no laws apply”;
- “perfect anonymity.”

Preferred description:

> **OM is a general-purpose open messaging protocol and client family with end-to-end encryption and user-controlled, replaceable infrastructure.**

## 8. Design objective

The objective is not legal evasion. It is architectural honesty:

- don't collect data merely so a central project can possess it;
- don't centralize operational authority unnecessarily;
- comply with valid obligations attached to the role actually operated;
- keep protocol security independent of any one operator's legal jurisdiction.
