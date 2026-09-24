# ADR-0123 — Moderation evidence access and appeal hold

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator (delegated 2026-09-24: "draft the design gap rulings ... check it and accept it if good")
- **Source:** V-016 gaps 4 and 5 ([`v016-room-moderation`](../verification/prototypes/v016-room-moderation/README.md)). **Amends:**
  [`EVIDENCE_AND_EXTERNAL_REPORTING.md`](../docs/architecture/safety/EVIDENCE_AND_EXTERNAL_REPORTING.md), [`DECENTRALIZED_ABUSE_MODERATION.md`](../docs/architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md).

## Decision

1. **Evidence recipients.** An EvidencePackage is encrypted to the moderator set in force at the report's sending epoch.
   A demotion removes access to evidence reported *later*. Evidence already disclosed cannot be clawed back, and the
   docs say so plainly to reporters.
2. **Appeal hold bound.** An open appeal may extend evidence retention up to the Room policy's `hold_max`. The protocol
   caps it at 180 days after the original retention expiry. When `hold_max` is reached, normal retention applies
   whatever the appeal's state, and the appeal is decided on what remains.
