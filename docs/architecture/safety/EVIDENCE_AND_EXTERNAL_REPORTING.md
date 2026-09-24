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

Selected direction: ordinary messages do not receive an additional durable AccountID signature. What the underlying protocol already proves differs by conversation type (ADR-0128 rule 3):

- **1:1 sessions** give participant-deniable authentication. 1:1 content in an EvidencePackage is reporter-held
  evidence, not protocol non-repudiation.
- **MLS groups** give **transferable sender authentication**. RFC 9420 signs every application message with the
  sender leaf's signature key, and the leaf credential chains through DeviceIdentity and AccountControlState to the
  AccountID. An EvidencePackage may include MLS FramedContent and signatures, which a third party can verify
  independently. The same property means a seized device yields provable transcripts of other members' group
  messages (threat T-060). Deniable groups are not a v1 property; they would need a research ADR.
- **HistoryBundle** content proves only that the bundle author shared it ([`GROUP_CHAT_ARCHITECTURE.md`](../groups/GROUP_CHAT_ARCHITECTURE.md)).

Durable control/discovery objects remain independently verifiable. EvidencePackage documents exactly which included objects are independently verifiable and which content/timestamps are reporter-held evidence rather than protocol non-repudiation.

## 4. Store/provider-specific reporting

A client distributor or managed provider may add a `ReportTarget` integration required by its store policy or local law.

This remains outside canonical message delivery. A report is an explicit user disclosure event.

## 5. Privacy invariant

> **Normal operation gives no central project ambient conversation plaintext merely to enable possible future reporting.**

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0123](../../../decisions/ADR-0123-moderation-evidence-access-and-appeal-hold.md): an EvidencePackage is encrypted to the moderator set in force at the report's sending epoch. A demotion removes access only to later evidence. Appeal retention is capped at `hold_max`, at most 180 days after the original retention expiry.
