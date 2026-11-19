# ADR-0242 — Non-genesis sheet pins and complete ordinary recovery-policy history

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-19). The design is accepted as the specification. Code may not ship or be enabled until vectors `RK2B-01` to `RK2B-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registries named below are updated in the same change, and a specialist signs off. Clarifies ADR-0238 and ADR-0225 section 1 (additive pointer in ADR-0238). The recovery-link part of the walk depends on the request binding of ADR-0243, which is only proposed: until ADR-0243 is accepted, a recovery-signed link is never classified positively (outcome `needs_evidence`).
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** Additive, fail-closed private RecoveryRecord evidence field and pinned-verifier clarification, building on ADR-0225/0230/0238. No W0 signature/authority change.
- **Answers:** what a pinned non-genesis sheet checkpoint establishes, and how the policy history between checkpoint and head is carried (`RK2A-01`, `RK2A-04`).

## Ruling: pinned checkpoint is an axiom, not a genesis in disguise

A verifiably owner-custodied v2 sheet independently pins `(AccountID, generation, OM-ACS-HASH-v1 digest)` from the issuing device's **then-verified** head. A reader must locate the exact canonical signed checkpoint envelope and check the unsigned body's AccountID, generation and canonical digest, its well-formed signature-entry structure and the applicable critical-extension decoding; it also checks the checkpoint's credential references are structurally valid. The pin **authenticates the checkpoint body as a starting authority state**. It does not assert that the new device cryptographically reverified every W0 vote on that checkpoint.

For a **non-genesis** checkpoint whose own signer was rotated *by that checkpoint*, verifying the checkpoint's W0 signatures against the checkpoint's *new* key does not apply: W0 used its immediate predecessor's committed key. The owner sheet intentionally does **not** carry the predecessor authority state. The minimum mandated at the sheet pin is therefore **canonical body + exact independent pin + structural envelope validation**, not a genesis proof or W0 cryptographic re-verification of the pinned state. If independently anchored and fully verified predecessor evidence is actually available, the verifier MAY additionally verify those historical signatures against their true predecessor references; an invalid available proof is an integrity failure. However, absence of pre-checkpoint history **does not** turn a valid pin into `needs_evidence`: ADR-0225 explicitly says pre-checkpoint evidence is not required. A record-authenticated or attacker-supplied checkpoint that lacks an independent sheet pin remains advisory, not an account anchor.

From this trusted starting state, for every **subsequent** `P → C` link validate contiguous previous hash and generation, account identity, W0 controller signatures using `P`'s committed credentials, and the ordinary parent constitutional authorization. A recovery-signed link additionally needs the ADR-0238 key-5 dossier and the ADR-0243 request-binding rule, then terminates `saved_sheet_older_than_recovery` only after all independent non-time predicates are met. Do not continue to or adopt the suffix on sheet-only evidence. A missing subsequent credential or policy body is `needs_evidence`; a demonstrably mismatched one is `integrity_failure`. A fabricated recovery-role signature in an unverified suffix is never an older-sheet classification.

## Complete ordinary policy history (not inferred from commitments)

W0's `verify_successor` proves *authorization of a recovery-policy commitment change* from parent-controller signatures; it does **not** supply, canonical-decode or validate the historical RecoveryPolicy bodies. Commitment-only checking is sufficient for W0's **pure account-state authorization verdict**, but is **not** sufficient to call the richer RecoveryRecord/sheet walk a verified historical-policy walk. `acs_evidence[3]` only gives the final head's policy. A chain may change the commitment any number of times before that head.

Add optional private `acs_evidence` key **6**, the exact **policy-run** carriage for the pinned contiguous window. The first entry supplies the checkpoint's committed policy body (if its commitment is present); one more entry supplies the new policy body at **each** later signed state's change to a different commitment. Do not emit entries for unchanged commitments or an absent commitment. Entries are ordered by first appearance in the anchored chain, unique by state hash, with no extra entries. Canonical-decode each body, hash with the existing `OM-RECOVERY-POLICY-HASH-v1` and compare to the state that first commits to it; associate it with subsequent unchanged states until replaced. Also compare the final active body to key 3 and cross-check every ADR-0238 key-5 dossier's parent and next policies against the policies of its respective parent and child states. A policy commitment removed entirely is represented by the signed state's absence of commitment, not a fabricated policy body. All bodies must be supplied under the existing AEAD; AEAD authentication is *not* their independent authority, the W0 commitment is.

## Exact CDDL delta (`spec/cddl/recovery-record.cddl`)

```cddl
; ADR-0238 recovery-acs-evidence keys 0..5 are retained.
recovery-acs-evidence = {
  0: bstr .cbor signed-AccountControlState,
  1: [0*8 bstr .cbor signed-AccountControlState],
  2: [0*320 bstr .cbor DeviceCredential],
  3: bstr .cbor RecoveryPolicy,
  ? 4: [0*56 bstr .cbor signed-AccountControlState],
  ? 5: [0*64 recovery-record-link-dossier],
  ? 6: [0*65 recovery-record-policy-run], ; checkpoint + each changed commitment
}
recovery-record-policy-run = {
  0: bstr .size 32,                   ; ACS state hash where this policy run begins
  1: bstr .cbor RecoveryPolicy,       ; exact body committed by that ACS
}
```

For the new sheet-only classification profile, key 6 is semantically **required whenever the checkpoint/window commits to a policy**. Its `?` preserves decoding of stored legacy plaintexts but does **not** make missing required bodies eligible for positive sheet-only classification. An old app that does not understand key 6 refuses rather than guesses; if any such record version has already been emitted and cannot safely gain this nested field, assign a new versioned plaintext/associated-data lineage instead of reinterpreting it. State/record limits remain **65 signed states total, 56 segment, 8 tail, 1 head, 320 credential bodies/700,000 bytes, 1,048,000 plaintext bytes**. If policy bodies plus dossiers overflow, **no truncation of proofs**, status `needs_new_safety_code`, no false “On”. Missing exact bodies on read: `needs_evidence`, not adopted head.

## Vectors and metadata

- `RK2B-01`: checkpoint at generation >1 signed with predecessor key `H_old`, while checkpoint commits `H_new`. Owner-pinned checkpoint passes without predecessor history; next ordinary signed child verifies against `H_new`. Same hash with altered checkpoint bytes fails. Absence of independent pin leaves restricted.
- `RK2B-02`: checkpoint policy `P0`, ordinary W0 changes `P0 → P1 → P2 → P3` over three separated links, then head policy `P3`. Include all four key-6 entries, check each digest, every transition signer, and the final key-3 body; missing middle `P2` gives `needs_evidence`, invalid/misordered/extra entry gives `integrity_failure`.
- `RK2B-03`: a recovery link after those ordinary changes: key-5 parent/next policies must match key-6 active runs. Missing child credential or dossier fails closed. Use ADR-0243's request-binding positive and nonce-substitution negative.
- `RK2B-04`: 56+8+1 state maximum, 65 policy runs, 320 distinct credential `(DeviceID, digest)` identities, plaintext 1,048,000/overflow, restarts with floors; no writer may omit policy history to pretend the old sheet reaches the head.

No new domain label or Home-visible field. Policy history resides inside the **existing padded RecoveryRecord AEAD**. Larger proof data may move a record into a larger existing bucket and reveal a bucket/timing change to the Home. A RecoveryRoot/kit holder can read more historical policy detail, increasing exposure in a compromise, but gets no additional W0 authority; no verifier-local time is carried or logged. Limit historical retention and use the existing size ceilings; no hidden network lookup for predecessor policies.

## Gate and audited-text check

**Audited W0 and ADR-0225's no-pre-checkpoint-evidence rule do not change.** ADR-0238's ambiguous checkpoint signature phrase and optional key-5 verifier description **must be explicitly amended/qualified** before this scheme is enabled. The private CDDL and numeric-field registry must gain key 6 and its bounds, with independent policy-history vectors and a specialist review. This ADR does not authorize restore/adoption or a release.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision.** `recovery-acs-evidence` keys are 0 to 3 (existing), 4 (ADR-0230, anchor segment) and 5 (ADR-0238, link dossiers); key **6** is free in [`recovery-record.cddl`](../spec/cddl/recovery-record.cddl), [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json), the logical schema and every ADR, and is kept. The `[0*65]` bound equals the existing 65-state chain bound; the 56/8/1, 320-credential and 1,048,000-byte limits are unchanged. No new label. Vector ids `RK2B-01` to `RK2B-04` appear nowhere in the repository. The frozen CDDL and registries were not edited (reserved by this ADR, to be changed with the gate).
- **Status and floors.** The pin-as-axiom rule restates ADR-0225's existing statement that pre-checkpoint evidence is not required, and every later link is still fully verified, so no floor is weakened and no audited text is amended; it is therefore imported as accepted by delegation, enablement gated. The pinned checkpoint stays advisory without an independent sheet pin.
- **Dependency on ADR-0243.** Section "Ruling" requires, for a recovery-signed link, the ADR-0243 request-binding rule. ADR-0243 is proposed and needs specialist and operator acceptance, so meanwhile this ADR is used only for ordinary (non-recovery) links and for policy-run carriage. A recovery-signed link in the walk yields no positive older-sheet classification; the outcome stays `needs_evidence`.
- **Edits.** The finding-severity line and one phrase calling an earlier verifier reading wrong were reworded neutrally; the status, date and decided-by lines were rewritten; this section was added. No ruling text was changed.

