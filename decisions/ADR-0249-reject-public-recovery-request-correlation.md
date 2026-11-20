# ADR-0249 — Metadata judgement: reject public request-to-successor digest linkage

- **Status:** Accepted (2026-11-20). It rejects the public request-digest extension proposed by ADR-0243 (ADR-0243 is Rejected, superseded by this ADR) and makes the sheet-only `needs_evidence` fallback permanent. It adds no wire shape, field or label and changes no audited text; vectors `RK2F-01` to `RK2F-04` are verification-only.
- **Class:** Privacy/security holding ruling; no new wire shape, field, label or authority.
- **Answers:** RK2B-03 and ADR-0243 request-to-child metadata judgement.
- **Priority:** Critical.

## Metadata judgement — REJECT AS PROPOSED

The proposed `om.critical.acs.recovery_request_ref.v1` places the **unblinded `OM-RECOVERY-REQUEST-HASH-v1` digest value** in the publicly discloseable signed ACS child's critical extension map. Recovery fan-out already exposes an AccountID and recovery attempt to the receiving Home. Head gossip exposes ACS bodies to other observers. The new extension permits *equality linkage of a specific recovery request observed in the Home/veto path with a published signed child*, across surfaces that are deliberately separated today. The nonce makes distinct requests distinguishable; it does not hide this exact linkage. This is a deliberate new stable correlator and is **not required for ordinary W0 ACS body/signer verification**. On C-14, C-20, C-26, C-29 and the Metadata Threat Model, the metadata cost is unnecessary for the narrow purpose of sheet-only status classification. **Do not accept the public digest construction on delegation.** This is a specialist review recommendation, not a claim that an operator has already amended ADR-0243.

The request-to-child binding problem is real: a root holder can sign request A and request B with identical account/parent/target/policy but different nonces; the current W0 recovery child signs neither nonce. A private RecoveryRecord key-5 dossier, even with a child hash and AEAD authentication, is writable by a recovery-root/kit holder and cannot prove which request originally produced that immutable child. Replacing A with B can change the relevant veto digest. Neither “same-fields” matching nor a retrospective root-signed receipt fixes this.

**Current mandatory result:** when a sheet-only reader has only its sheet, RecoveryRecord and supplied chain, and encounters a recovery-signed link with no independent contemporaneous request-to-child binding, return **`needs_evidence`**, restricted, with no head adoption, no elapsed-time credit and no `saved_sheet_older_than_recovery` positive. This is the **permanent outcome for that evidence class under current W0**, not a temporary software version fallback. It does not invalidate legitimate historical P pins, existing controllers' own locally timed verification, or ordinary W0 checks supported by their originally authenticated context. A separately authenticated surviving-controller ceremony may produce a new recovery acceptance under its *own* rules; it cannot retroactively prove the old request nonce from a root-authenticated dossier.

A future minimal alternative is conceivable only if it proves exact original request identity **without leaking a public equality correlator** and is independently verifiable by every consumer whose acceptance semantics it changes (for example, genuinely contemporaneous independent controller evidence). An arbitrary new private MAC, salt, root-key signature or AEAD field **is not yet such a proven alternative**. Do not register a “private fix” by extrapolation from the dossier. Submit a separate explicit privacy/crypto evolution if one is developed.

## CDDL, labels and vectors

```cddl
; No additions. Keep W0 AccountControlState ? 23: extensions as published.
; Do NOT register om.critical.acs.recovery_request_ref.v1.
; Keep ADR-0238 recovery-record-link-dossier key 3 as *candidate evidence*,
; not as exact original-request proof to a sheet-only reader.
```

No new domain labels, Home APIs, registry IDs, or W0 credential authority. ADR-0243 remains proposed and **must be explicitly marked not accepted**; amend ADR-0242/0238's gated positive test assertions by additive pointers rather than silently editing audited W0 semantics.

- `RK2F-01`: valid root-signed request A and B differ only in nonce, same pre-extension recovery child, dossiers alternately present A/B; neither yields positive sheet-only recovery classification, both `needs_evidence` absent independent original linkage.
- `RK2F-02`: valid known veto for request A cannot be erased by substituting B; no borrowed timer or clean-veto claim is permitted.
- `RK2F-03`: fabricated recovery-role entry in an unverified suffix or AEAD-authenticated root dossier never produces `saved_sheet_older_than_recovery`; invalid cryptographic signatures still produce `integrity_failure` when demonstrably invalid.
- `RK2F-04`: trace observer matrix: Home sees request digest/AccountID and head observer sees child; demonstrate proposed public extension permits correlation and no-extension path carries no mandated identical digest. Do not claim general unlinkability against timing or compromised participants.

## Authority and gate

The Home receives no new authority. Rejecting this public critical extension retains present metadata separations but imposes an explicit availability/classification limitation. New recovery-child exact binding is **not solved** by this ruling. If the audit owner insists on new positive sheet-only classification, it is a **separate incompatible protocol evolution**, requiring acceptable privacy proof, explicit audited acceptance/compatibility handling, independent hybrid-signature vectors and formal veto/authority traces. No product enablement or release approval.
