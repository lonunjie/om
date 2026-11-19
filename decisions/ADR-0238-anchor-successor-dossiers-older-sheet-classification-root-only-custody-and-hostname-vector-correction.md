# ADR-0238 — Anchor successor dossiers, older-sheet classification, root-only no-store custody and the hostname vector correction

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-19). The design is accepted as the specification. Code may not ship or be enabled until vectors `RK2A-01` to `RK2A-06` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registries named below are updated in the same change, and a specialist signs off. Clarifies ADR-0225 section 1 and ADR-0230 sections 5, 6 and the `RK2-01` negative (additive pointers there); it does not change any floor, and a sheet-only device still never adopts a head.
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** Additive **private RecoveryRecord v1 plaintext** field (new recovery-ACS-evidence key 5), security-tightening of the sheet-only verifier's decision tree, and UI/fallback clarification. No frozen W0 state, signature or Home operation changes meaning. An implementation that does not know the new field must refuse a gated record rather than misclassify its recovery suffix.
- **Answers:** the ADR-0230 questions 1–3.
- **Supersedes/clarifies:** ADR-0230 §5–6 as to what authenticated evidence makes a post-checkpoint owner-recovery link recognizable, and ADR-0225 §1's phrase “complete verified post-checkpoint chain”: verification of the *cryptographic link predicates* is distinct from acceptance of verifier-local elapsed-time credit. **Preserves:** ADR-0202's rule that peer-provided time never counts; ADR-0225's anchor, forward-only verification and separate terminal `saved_sheet_older_than_recovery` outcome; ADR-0205's root-only fallback; Maximum Privacy independent relationship-key custody. This amendment to accepted ADRs is explicit. No weakening of an audited verifier floor is proposed.

## Design sources read and not read

Read: [`AGENTS.md`](../AGENTS.md); ADR-0230–0232, ADR-0216, ADR-0225–0227; ADR-0199/0202/0203/0205 relevant sections; [`spec/cddl/recovery-record.cddl`](../spec/cddl/recovery-record.cddl), [`account.cddl`](../spec/cddl/account.cddl), [`spec/protocol/recovery-sheet.md`](../spec/protocol/recovery-sheet.md); control verifier `verify_recovery_successor`, `verify_extension_partial` and dossier shape; relevant [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) §§10.2/10.3/10.6/10.7; recovery-extension Python/Rust fixtures. Not read in full: all remaining architecture/constitution texts, the provider runtime, every UI screen and every recovery witness path. This is a design ruling, not observed end-to-end validation.

## Ruling 1. The record carries exact **per-recovery-link dossiers** inside the outer AEAD

`RecoveryRecord.acs_evidence` keeps keys 0–4 unchanged and gains optional key **5**, an array in chain order of exact recovery-link dossiers for the contiguous anchor/tail/head union. Each entry binds a **child ACS hash** to:

1. the canonical **parent-committed** `RecoveryPolicy` body,
2. the canonical **child-committed next** `RecoveryPolicy` body, and
3. the **exact signed RecoveryRequest bytes** (including its recovery-role signature).

The fully signed recovery child itself is the **exact signed state already present in the anchor segment, tail or head**; repeat bytes or a second unsigned child in the dossier cannot replace it. The signed request's target `DeviceCredential` body must be available in the existing `credentials` union, and its digest and binding are verified against the requested target and successor; the credential union includes all bodies needed for all carried signed states and **each recovery request's signer/target verification**, subject to the existing 320-credential/700,000-byte bounds. If the established verifier requires a `RecoverySuccessorBundle`, reconstruct it from the *exact* signed child, exact request, next policy and target credential, not from a sender-written hash or a free-text claim. The dossier is authenticated as bytes only by the existing RecoveryRecord AEAD: the **parent policy commitment, request signature, child signature and ordinary account verifier**, not the Home, establish the public-key facts.

A writer that has the verified local chain and the existing in-session `RecoveryLinkEvidence` retains policy/request/next-policy **bytes** for each post-anchor recovery link and includes the dossier in every record, **without** including any stored verifier time. Dossiers are sorted by each linked signed child's position in the carried chain, unique by child hash; one entry per post-anchor recovery link, no extras, no substituted request, and no gaps. Signed parent/child, policies and credentials must all fit the original record envelope, 65-state and 1,048,000-byte plaintext limits. At capacity, do **not** truncate the chain or dossiers to look complete: mark `needs_new_safety_code` and retain the existing limited, verifiable record with an explicit incompleteness status. This field is *not* an independent source of recovery approval.

## Exact CDDL delta (`recovery-record.cddl` only)

```cddl
; Existing recovery-acs-evidence keys 0..3 and ADR-0230 optional key 4 stand.
recovery-acs-evidence = {
  0: bstr .cbor signed-AccountControlState,       ; head
  1: [0*8 bstr .cbor signed-AccountControlState], ; tail
  2: [0*320 bstr .cbor DeviceCredential],        ; exact needed credential union
  3: bstr .cbor RecoveryPolicy,                  ; head policy
  ? 4: [0*56 bstr .cbor signed-AccountControlState], ; anchor segment
  ? 5: [0*64 recovery-record-link-dossier],     ; exact one per recovery-signed link in union
}
recovery-record-link-dossier = {
  0: bstr .size 32,                   ; digest value of OM-ACS-HASH-v1(child body)
  1: bstr .cbor RecoveryPolicy,       ; parent's committed policy body
  2: bstr .cbor RecoveryPolicy,       ; child's committed next policy body
  3: bstr .cbor signed-RecoveryRequest, ; complete exact request, not request digest
}
```

**Semantic checks:** `child_hash` must name exactly one included signed state; the child must link to the immediately previous state in the anchored walk, and its AccountID/generation/credential references must agree. The signed request must verify with the *parent* committed recovery-method public key and bind the parent hash/epoch, target DeviceID, credential and request nonce. Both policies must hash to their respective parent/child commitments; no unreferenced dossier is allowed. If there is no recovery-role signature in the anchored union, key 5 is absent or an empty array; a nonempty entry for an ordinary transition is an integrity failure. A body with unknown required elements is refused and prompts app upgrade, not a silent omission.

No new AEAD/KDF/domain-separation label: the existing `OM-RECOVERY-RECORD-AEAD-AD-v1`, `OM-ACS-HASH-v1`, `OM-RECOVERY-POLICY-HASH-v1` and W0 signature labels already authenticate all required bytes. **Key 5 is inside** the existing padded AEAD, never sent as a new Home field.

## Ruling 2. Exact sheet-only forward verifier and outcomes

Start from the independently custodied v2 sheet's canonical checkpoint `(AccountID, generation, state_hash)`. First find an **exact signed checkpoint state**, then establish a contiguous chain through the carried head. The checkpoint authenticates the starting hash only; decode and signature-verify the checkpoint as far as the existing pinned-checkpoint rule requires. Never start at an arbitrary self-signed genesis in a root-authenticated record as a substitute.

At each transition *before any recovery link*, validate parent→child hash, generation, AccountID, complete required credentials, W0 controller signatures and current committed policy using the ordinary verifier. If the chain/checkpoint/needed credential or dossier is unavailable but there is no proved invalidity, return **`needs_evidence`**, remain restricted, and allow the existing independent contact/in-session evidence routes. A fabricated recovery-role entry in an unverified suffix proves **nothing**; it cannot change the outcome to `saved_sheet_older_than_recovery`. A *cryptographically invalid* signature/policy/request or a conflicting checkpoint is an **integrity/conflict failure**, not evidence of an older legitimate recovery.

On encountering a purported recovery-signed successor, require a matching dossier from key 5 (or equivalent independently revalidated carrier) and verify **all non-time predicates** using the parent's committed policy, its recovery signing key, the signed request, next policy, exact signed child and target credential. The verifier must establish that this is a well-formed candidate **owner-recovery** transition that would pass every applicable check other than its verifier-local activation interval and absent-veto freshness evidence. For a *sheet-only* device that lacks **its own historical timed verification evidence**, stop **at this first verified-as-to-cryptography recovery link**, return terminal **`saved_sheet_older_than_recovery`**, and stay restricted. The remainder of the suffix **has not been adopted or certified valid**; the status describes the sheet's evidentiary age, not proof the prior takeover completed lawfully. Do not manufacture historical elapsed seconds from a dossier, policy delay, Home timestamp or another controller's local observation; do not continue past this point then adopt by a different automatic fallback. A separately authenticated surviving-controller approval follows ADR-0202/0203's own explicit recovery exchange, not a bypass of this sheet-only outcome.

This corrects the literal “complete verified chain” wording in ADR-0230 §6 and ADR-0225 §1: it means the **contiguous, verified prefix through a fully checked non-time recovery candidate**, not that a device with no clock evidence somehow verified and accepted the whole descendant chain. If the specialist instead requires proof that such a transition actually completed, the demanded distinction is **unimplementable from a sheet and RecoveryRecord alone**; the outcome must remain `needs_evidence` until an independent verifier's authenticated *decision* is obtained, and the accepted ADR text must be changed accordingly. There is no cryptographic proof of elapsed time in the current dossier format.

For a root-only/legacy sheet with **no independently custodied checkpoint**, retain ADR-0202's prior rule: an AEAD-authenticated record alone is advisory and cannot establish the original account. Do not claim a verified old-recovery crossing or a fully restored identity from that record. A pinned live device or independently authenticated chain/ceremony evidence is still required. Under no condition does the new field authorize ACS adoption.

## Ruling 3. Maximum Privacy when no eligible RecoveryStore exists

Preserve ADR-0205 §2's **root-only fallback** for a person with no eligible Home/scope. At safety-code issuance, the person receives the existing root-only form and its appropriate existing write-down confirmation. Do **not** display, generate or claim a usable `version=2, kind=0x03` **Relationship code (contacts only)**: the required *matching Home, scope and checkpoint* cannot be checked, so minting such a code would falsely promise recoverability. Do not invent a dummy scope, Home, checkpoint or second QR, and do not silently cache the second key in the root-only sheet.

UI state is explicit and separate: **“Recovery key saved. Contact reconnection backup is unavailable on this Home.”** Do not show a fully “On” RecoveryRecord/relationship-backup status. A root-only key may still start the existing identity recovery path, but it cannot by itself provide an independent checkpoint/complete record or guarantee relationships. If later a suitable RecoveryStore becomes available, the **existing safety-code replacement flow**, not an automatic upgrade of the old sheet, issues a **matched pair of v2 root and relationship sheets**, an anchored kit and the key-8 record only after independent custody confirmation and a verified parent-policy transition. Until then contacts must be reconnected through the already authorized paths.

## Ruling 4. RK2-01 hostname correction

The existing v1 syntax explicitly **allows** ASCII punycode labels. Correct the `RK2-01` negative's label from “malformed/punycode host” to **“host violating the existing ASCII hostname syntax”**. `xn--.example` is **invalid** (the `xn--` label ends with `-`); `xn--bcher-kva.example` is **valid syntax** and must remain accepted when all other fields and checksums are correct. No additional IDNA normalization, network validation or punycode prohibition is authorized. The QR kind and checksum still decide independently of host syntax. This is an editorial correction to the vector description; its accepted/rejected test behavior does not change.

## Test vectors and negative cases (two independent codecs)

- `RK2A-01`: a complete, pinned checkpoint plus ordinary W0 successor, **no recovery link**, no key-5 dossier: accepted under otherwise fulfilled normal restore conditions; adding a spurious dossier fails. Repeat with 56-segment / 8-tail / 1-head bounds and exact credential union.
- `RK2A-02`: fixed exact signed RecoveryRequest, parent/next RecoveryPolicy, target DeviceCredential and signed recovery successor in key-5 dossier, linked after sheet checkpoint. Verify all cryptographic/non-time predicates, but fresh sheet-only device has no prior elapsed-time checkpoint: `saved_sheet_older_than_recovery`, no adopted head and no secret use. Same signature-role byte with missing dossier or unverified parent: `needs_evidence` (or `integrity_failure` for a proved invalid signature), **never** `saved_sheet_older_than_recovery` solely from an unverified role byte.
- `RK2A-03`: mutated signed request, wrong target, policy commitment, request nonce, child hash, credential, signature, dossier duplication, post-checkpoint gap and extra credentials. Must distinguish `needs_evidence` from a known invalid cryptographic object; never adopt or advance floor on failed AEAD/semantic checks. Inject a fabricated `recovery` entry in an *unverified suffix* and prove it cannot trigger the older-sheet class.
- `RK2A-04`: 65-state/320-credential and 1,048,000-byte limits, omitted or overflowing segment/dossier, restart with saved floors; writer shows `needs_new_safety_code`, not “On”, and no truncated dossier can yield a positive result. Test a separately verified new owner checkpoint and a post-recovery replacement.
- `RK2A-05`: Maximum Privacy with no eligible store: only root-only custody entry, no kind-03 output, no “On” status; later eligible store requires a new replacement and distinct confirmed v2 sheets. Test wrong-kind rejection before AEAD and that a root-only device cannot use an advisory record as an independent account pin.
- `RK2A-06`: valid host `xn--bcher-kva.example`; invalid `xn--.example` / leading hyphen / uppercase / trailing dot. Publish byte-exact Base32, line check, binary and QR examples, with constant-time checksum comparison. Preserve the committed `RK2-01` `home.example` positive.

## Metadata and authority

The extra dossiers are sealed under the **existing** outer RecoveryRecord AEAD, padded to the same four size buckets. The Home can infer a possible bucket increase and correlate update times; it cannot inspect recovery requests or historical policies from the encrypted record. The holder of the RecoveryRoot can already decrypt that record, so the additional historical policy/request bytes enlarge the information exposed **to a compromised root holder**; they do not disclose the independently encrypted relationship snapshot. Do not log dossier bytes or verifier timers. The sheet-only error category may disclose to the local possessor that at least one post-checkpoint owner-recovery candidate exists; avoid sending that classification to a Home. Neither an authenticated dossier nor a root-derived AEAD opens a route to account authority absent the normal verifier's own evidence.

## Gate

Amend ADR-0225 and ADR-0230 descriptions and the [`recovery-record.cddl`](../spec/cddl/recovery-record.cddl)/numeric-field registry in the same change; publish full independent two-codec canonical vectors with real signatures; exercise the restricted decision tree and root-only UX; obtain cryptographic/metadata specialist acceptance. This ADR alone does not permit feature enablement, restore acceptance or a release claim.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision.** `recovery-acs-evidence` keys: existing 0 to 3, ADR-0230 reserves 4 (anchor segment); key **5** (per-link dossiers) is free and is kept. The array bound 64 sits inside the existing 65-state and 1,048,000-byte limits. No new label (the text states the existing ones cover every byte). Vector ids `RK2A-01` to `RK2A-06` are unused; `RK2-01` is untouched except for the label correction in ruling 4.
- **Relation to accepted ADRs.** The wording "complete verified post-checkpoint chain" in ADR-0225 section 1 and ADR-0230 section 6 is read, from this ADR on, as the contiguous verified prefix through the first fully checked non-time owner-recovery link, which ends in the terminal `saved_sheet_older_than_recovery` outcome. This is a clarification of a delegation ADR, not of audited text, and it does not lower a floor: the outcome is still restricted, adopts nothing and credits no elapsed time. ADR-0202's rule that a peer-provided time never counts is preserved.
- **Ruling 3 (root-only, no store).** Restates ADR-0205 section 2's root-only fallback and forbids minting a relationship-only code with no checkable Home, scope or checkpoint; no new value.
- **Ruling 4 (hostname).** An editorial correction of the `RK2-01` negative description (a punycode label is valid; `xn--.example` is not). Accepted/rejected behavior of the vector does not change.
- **Edits.** Review-numbering references in the header were reworded to neutral wording; the status, decided-by and amends lines were rewritten; this section was added. No ruling text was changed.
