# ADR-0136 — core_kernel independent-review remediation (gpt-5.6-sol, 2026-09-26)

- **Status:** Accepted for verification; re-review pending (the executor wrote it, so ADR-0117 requires a non-Anthropic reviewer)
- **Date:** 2026-09-26
- **Decided by:** verification executor (Claude Opus 5.5) under ADR-0134, applying findings F-1..F-9 of [`2026-09-26-gpt-5-6-sol-core-kernel.md`](../verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md)
- **Amends:** ADR-0111/ADR-0127 (model only), ADR-0120 (adds K12), ADR-0132 (evidence boundary), ADR-0135 (adds §7)
- **Evidence:** see the `Resolution:` line of each finding in the review file

## Context

The independent review rejected all seven items in the core_kernel queue. Three findings are blockers. F-1: the V-002
model let a two-method recovery request escape a single controller's veto. F-4 and F-9: nothing bound an inbound
PQXDH initiator key to an authorized sender device. The other six are model, evidence or descriptor mismatches. None of
them changes OM's authority model. Each finding either makes an existing rule executable or states a rule that two
honest implementations would otherwise read differently.

## Decision

1. **Recovery method count never changes authority (F-1).** A RecoveryPolicy is one vote, whatever custodians or methods
   produced it. Any live, non-quarantined controller vetoes a recovery-alone request acting alone during the delay.
   Two-method custody (Hardened) only makes the vote harder to steal. It never makes the vote harder to stop.
2. **Quarantine freeze and its resolution (F-2).** A quarantine that leaves fewer non-quarantined controllers than the
   parent majority enters SAFETY_FREEZE (§4 rule 4). Only a device-only majority of the parent's controllers resolves
   it. Quarantined controllers count toward that majority by signing with their §5 challenge capability. Without that,
   one compromised controller could freeze a two-controller account forever. A recovery vote never counts.
3. **The lift rule is state-derived (F-3).** Lifting a quarantine needs a majority of active, non-quarantined
   controllers, with quarantined controllers still in N and no recovery vote. Nothing depends on who initiated a
   quarantine: the ACS does not record it, and no verifier may keep side state for it.
4. **Responder binds the initiator before accepting a session (F-4, F-9; session rule K12).** The initiation's `IK_A`
   must be the `key_agreement_public_key` of exactly one DeviceCredential committed by the trusted ACS head held for
   the relationship that delivered the envelope. That device must be active and not quarantined. The check runs before
   any secret-key operation, one-time-key consumption, session creation or plaintext release. On no match, several
   matches or an inactive device, the initiation fails closed. A possibly stale head is refreshed (never rolled back,
   K7) before the envelope is processed. No wire field is added. ADR-0135 §7 records the composed authentication
   argument.
5. **Stable per-device correlators are declared (F-5).** W0 metadata descriptors gain `stable_device_correlator`.
   The value is `always` for:
   - the KeyMaterialProvider (`subject_device_id`);
   - mailbox-class Homes: every 1:1 envelope starts with the sender's `EncodeEC(IK)`, and ADR-0131 rule 4 leaves the
     envelope visible at the relay layer. The review did not name this; the executor found it while fixing F-5;
   - the push provider (push token).

   Hiding the sender device key needs a sealed-sender outer envelope, a new CryptoProfile, tracked as A-054.
6. **Referral tokens use the exact RFC 9578 type 0x0002 profile (F-6, F-7).** Verification is RSASSA-PSS with
   SHA-384, MGF1-SHA-384 and a 48-byte salt (RSABSSA-SHA384-PSS-Deterministic). PSSZERO is a different profile and
   must never be accepted in its place. `token_key_id` is computed over the RFC 9578 §6.5 id-RSASSA-PSS SPKI. The
   redemption model covers verification, challenge binding, epochs and replay. Issuance unlinkability is evidenced
   separately: RFC 9474 blind/finalize known-answer tests plus a structural issuer-transcript test.
7. **The continuity bundle has an explicit normative manifest (F-8).** The manifest lives at
   [`spec/continuity/om-crypto-v1-bundle-manifest.json`](../spec/continuity/om-crypto-v1-bundle-manifest.json).
   `verify` requires every member and rejects unresolved normative references. `check-manifest` fails when the
   repository gains a normative doc that is not in the manifest.

## Consequences

- V-001 is reopened by F-9 and re-evidenced: the responder binding, 9 selftest checks and 11 initiator-binding
  vectors, which the Python checker also verifies.
- V-002/V-003/V-015/V-016/V-017 have new evidence and need re-review of the changed artifacts only (ADR-0117).
- Open for the re-reviewer: whether the issuer-transcript test satisfies F-7, and what W0 issuance unlinkability
  protects and from whom. In OM the recipient is the issuer and also picks the IntroGate, and a ReferralAttestation
  can name the referrer.
- New design-queue item A-054 (sealed-sender outer envelope); it does not block engineering.
