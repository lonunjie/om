# Evidence and External Reporting

## 1. Default operatorless model

The OM protocol does not require reports to pass through the OM Project.

A recipient can:

- block a sender;
- revoke/rotate relationship capabilities;
- export selected evidence;
- send that evidence to an authority, employer, school, parent/guardian, community administrator, lawyer or other chosen recipient.

## 2. EvidencePackage

A logical `EvidencePackage` may include only user-selected data:

- selected canonical events;
- selected attachments or attachment hashes;
- conversation/account/device assertions needed to interpret the evidence;
- local receipt/timestamp context;
- cryptographic evidence/proofs available under the conversation's authentication model;
- reporter-authored statement/context;
- package creation time and format version.

It must not automatically dump unrelated conversation history, contacts, backup keys or private delivery capabilities.

## 3. Authenticity vs deniability

Do not accidentally promise non-repudiation if the underlying messaging protocol intentionally preserves deniability properties.

The report design must clearly document what can be proven:

- that a package was generated from local state;
- that included protocol objects verify under their expected keys/state;
- what remains forgeable/deniable by design;
- what local timestamps/metadata are merely reporter assertions.

Selected direction: ordinary messages remain participant-authenticated/deniable according to the secure session or MLS authentication model and do not receive an additional durable AccountID signature. Durable control/discovery objects remain independently verifiable. EvidencePackage documents exactly which included objects are independently verifiable and which content/timestamps are reporter-held evidence rather than protocol non-repudiation.

## 4. Store/provider-specific reporting

A client distributor or managed provider may add a `ReportTarget` integration required by its store policy or local law.

This remains outside canonical message delivery. A report is an explicit user disclosure event.

## 5. Privacy invariant

> **Normal operation gives no central project ambient conversation plaintext merely to enable possible future reporting.**
