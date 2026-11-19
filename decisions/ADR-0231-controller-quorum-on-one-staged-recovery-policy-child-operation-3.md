# ADR-0231 — Operation 3: controller quorum on one staged recovery-policy child

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-19). The design is accepted as the specification. Code may not ship or be enabled until vectors `CC3-01` to `CC3-04` exist from two independent codecs, the CDDL and registries named below are updated in the same change, and a specialist signs off. This extends the account's authority path, so the specialist sign-off is a hard condition.
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** `additive_noncritical_extension` (ADR-0165): one added operation digit and one added request shape in a private ceremony. Operations 0 and 1 stay byte-identical.
- **Answers:** the wire form of ADR-0227 section 2 (OM4-13: votes for a staged replacement child).
- **Amends:** nothing. **Does not amend:** ADR-0192, ADR-0205, ADR-0216 (operations 0 and 1 and its enablement conditions stand), ADR-0227, [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md), the W0 account verifier.
- **Scope:** `core_kernel` (ADR-0169).
- **Controlling sources:** ADR-0192, ADR-0205, ADR-0216, ADR-0227 sections 1 and 2; [`controller-change.cddl`](../spec/cddl/controller-change.cddl), [`account.cddl`](../spec/cddl/account.cddl).

## Design sources read, and not read

Read in full for this import: the six proposals; ADR-0204, ADR-0225, ADR-0227, ADR-0228, ADR-0229 and the passages of ADR-0199, ADR-0203 and ADR-0226 cited here; [`call.cddl`](../spec/cddl/call.cddl), [`controller-change.cddl`](../spec/cddl/controller-change.cddl), [`lan.cddl`](../spec/cddl/lan.cddl), the recovery-record CDDL, [`recovery-sheet.md`](../spec/protocol/recovery-sheet.md), the object-type, domain-separation and numeric-field registries and Home API v1 section 8.2. Not read in full: ADR-0216, the constitution documents, and the domain documents the design gate lists for these paths. The proposals were checked against the sources above only; if an unread text fixes a point differently, that text wins and this ADR is corrected by a follow-up.

## Decision

1. **One immutable proposed child.** `ReplaceRecoveryKey` creates a durable pending tuple `(account_id, parent_hash, parent_generation, parent_policy_epoch, parent_controller_set_digest, new_policy_bytes, candidate_acs_body, child_hash, staged_secret_ref, expires_at)` **before** any invitation. `candidate_acs_body` is canonical W0 `AccountControlState` with exactly the recovery policy transition and no unapproved ancillary membership edits. The `child_hash` is `OM-ACS-HASH-v1` over that exact canonical body. The candidate may be **partially signed**; it does not become an adopted ACS until the full parent-required quorum and ordinary verifier accept it. One staged secret can name only one candidate. The parent and votes are rechecked at **stage, invite, vote, receive and final confirmation**; any intervening verified head, open incoming recovery request, changed policy epoch or unresolved fork aborts with no adoption.
2. **Explicit operation 3, never 2.** Reuse the existing private link `om:controller-change/3/<origin-device-id-in-lowercase-hex>/<ADR-0203-offer-token>`, 300-second lifetime, ≤60-second clock skew, existing request kind 1 / reply kind 2, ceremony KDF, XChaCha20-Poly1305 envelope, code comparison and exact request/reply binding. Extend `controller-binding` operation enum from `0/1` to `0/1/3` **only**; its field order and `om.controller_change.v1` purpose string remain unchanged. All existing operation-0/1 vectors must remain byte-identical. Every decoder dispatches on the **verified operation in the transcript**, not the similar-shaped CDDL request body alone.
3. **Candidate carriage.** Operation-3 request transports a canonical child **body** (not a signed full successor), the exact parent signed state and parent-committed policy/credential evidence in the existing bounded ceremony envelope. Redundant digest fields must match independently recomputed hashes; a parser rejects non-canonical CBOR, conflicting duplicate fields, missing credential, non-parent child, wrong policy delta, invalid ephemeral request signature or oversize. Origin supplies its own independent vote over the *same body* using the frozen `OM-ACS` signature construction; the remote controller adds exactly its own controller-role signature, never signs a free-form digest or a textual prompt. The reply contains the **same candidate body** and its signature-entry (or an enveloped partial signed ACS containing the original vote plus responder vote) and is bound by the existing signed `controller-reply-binding`. No new Home capability, registry object type or recovery root is sent.
4. **What the other controller sees before voting.** The verified remote device must show **“Replace the safety code?”**, the owning account's locally trusted identity, old parent generation/hash short fingerprint, the policy epoch before/after, a warning that old saved codes may still work on stale devices/contacts, and the *fact that this changes recovery authority*; it must never show or receive the new written RecoveryRoot, RelationshipRecoveryKey, record/read/write caps or full RecoveryKit. On both screens display the existing ceremony comparison code computed over exact parent, candidate hash, selected DeviceID and operation `3`; require humans to compare it **before** either vote/confirmation. Unchanged recipient or same human taps alone do not bypass that step.
5. **Vote acceptance.** Enforce distinct currently eligible controller DeviceIDs and parent-policy quorum. Reconstruct the final canonical `signed-AccountControlState` as the existing W0 `signed-AccountControlState` map with signatures sorted/canonical per W0; validate signatures under the **parent** committed credentials and call ordinary `authorize`/fork/recovery-policy checks. Reject vote replay into a different parent, child, epoch, account, target or operation. Single-controller accounts use their normal one-vote path; accounts requiring multiple votes fail staging with “needs your other main device” when op3 is unavailable. On cancelled/expired/mismatched ceremony retain the old adopted state and old code.
6. **Commit order.** Revalidate head and request state again when user confirms written custody; adopt child, only then report new code “On” after anchored record acknowledgement and, separately, enqueue old-record deletion. The old sheet cannot be promised globally invalid (ADR-0227 §4).

## Additive CDDL (reserved by this ADR; not yet in `spec/cddl/controller-change.cddl`)

```cddl
; Import AccountControlState and RecoveryPolicy from account.cddl.
; Existing promotion/removal branches, proof sizes, envelope and purposes unchanged.
controller-request = promotion-request / removal-request / replacement-request
controller-binding-op = 0 / 1 / 3
replacement-request = [1, device-proof, bstr .cbor replacement-proposal, null]
replacement-proposal = {
  0: 1,                                                ; proposal version
  1: bstr .cbor signed-AccountControlState,              ; fully verified parent
  2: bstr .cbor AccountControlState,                     ; proposed child BODY, no quorum yet
  3: bstr .cbor RecoveryPolicy,                          ; policy in proposed child
  4: bstr .size 32,                                     ; OM-ACS-HASH-v1 of parent
  5: bstr .size 32,                                     ; OM-ACS-HASH-v1 of child BODY
  6: uint,                                              ; parent policy epoch
  7: uint,                                              ; candidate policy epoch
  8: [1*16 bstr .size 32],                              ; parent-controller credential hash set
}
; Reuse controller-reply: [bstr .cbor controller-reply-body, device-proof].
; For op 3, existing signed-control-state in reply body contains the proposed
; child with the two votes; the kit-transfer slot is exactly null.
```

The proposal field 8 MUST be sorted by parent-controller DeviceID, encoded alongside identities under the verified parent, and equals that parent set. No new public domain-separation label is needed for the **existing** binding/KDF, but **the existing `om.controller_change.v1` comparison binding must include operation value `3`**. If the existing comparison code implementation cannot prove it commits to the request's entire proposal bytes, introduce/register `OM-CONTROLLER-RECOVERY-PROPOSAL-HASH-v1` as `DS-HASH(label, OM-CBOR [parent_hash, candidate_body, policy_epoch])` and include its 32-byte digest in the **new op-3-only** proof, with updated explicit test fixtures. Do not silently reuse a digest that the code does not bind.

## Metadata and abuse analysis

The Home carries the same sealed private-ceremony envelopes and already sees two DeviceID-associated queues, size buckets and traffic timing; its own Home may correlate a recovery-code replacement ceremony with the later recovery record write/delete. `operation=3`, parent/child hash, new policy, controller votes and codes stay **inside the encrypted ceremony**, not URL path query parameters sent to a Home. The private link contains the operation and origin DeviceID and is a **bearer capability**: accidental copying leaks sensitive intent and selected-device association, though never authority without device signatures and comparison. Pad/limit envelope sizes exactly as current ceremony and avoid calling Home solely to check candidate digests. Threats: malicious origin substituting child after remote sees prompt, fork replay, signature mix-and-match, a stale controller's vote, UI showing a vague message. All fail closed. Identity/control signature authority still comes only from verified parent ACS.

## Golden vectors and required tests

1. `CC3-01`: parent AccountID `11*32`, parent hash `22*32`, origin `31*16`, remote `42*16`, remote credential hash `53*32`, candidate hash `64*32`. `controller-binding` canonical CBOR must be byte-identical to existing [`verification/vectors/controller-change-v1.json`](../verification/vectors/controller-change-v1.json) **except** operation CBOR integer changes from `00` to `03` and `child_hash` reflects exact candidate; both codecs produce the same new ceremony id, req/resp/code/bind key, proof and six-digit comparison code from fixed existing ADR-0216 route context. Add a complete JSON golden, not just round-trip assertions.
2. `CC3-02`: fixed canonical parent, fixed candidate with only recovery policy epoch increment; Python reference and Rust codec agree on proposal CBOR, child hash, W0 signature entries, final signed child bytes and quorum verification. One signer alone rejected, two distinct parent controllers accepted; same signer twice rejected.
3. `CC3-03`: mutate one byte of parent, child body, policy, epoch, operation digit, credential, request hash or reply hash; reject without side effects. Wrong QR prefix, old operation-0/1 decoder, expired code or different parent must not accept.
4. `CC3-04`: repeat last reply, restart before/after a vote, pending incoming recovery, fork, changed parent, cancel and lost reply; no old-code invalidation or staged-secret export before valid final quorum.

**Gate.** Before any code ships or is enabled: the CDDL and registry rows are written together with the authoritative account-transition vectors, `CC3-01` to `CC3-04` pass in two independent codecs, the ADR-0216 enablement conditions (fork detection, second-controller request delivery, exclusion guard, shared vector) hold, and a specialist signs off the authority path. Neither this ADR nor the author of the code may authorize the recovery-authority extension on its own.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision.** Operation digit `3` is the value ADR-0227 section 2 already assigns to "recovery-policy change"; the existing enum is `0 / 1` and the existing vector [`verification/vectors/controller-change-v1.json`](../verification/vectors/controller-change-v1.json) uses operation `0`. Digit `2` is left unassigned. The private link prefix `om:controller-change/` already carries the operation digit. The proposed label `OM-CONTROLLER-RECOVERY-PROPOSAL-HASH-v1` does not exist in [`spec/registries/om-domain-separation.json`](../spec/registries/om-domain-separation.json); it is conditional (introduce it only if the existing comparison binding cannot be shown to commit to the whole proposal) and is **not** reserved by this ADR.
- **Dispatch note.** `replacement-request` has the same array shape as `removal-request` (kind 1, proof, bstr, null); a decoder therefore dispatches on the operation in the verified transcript, as item 2 above requires, never on shape.
- **Relation to ADR-0227.** Sections 1 and 2 of ADR-0227 decide the ceremony; this ADR gives the carried bytes and vectors. Where they differ, ADR-0227 wins.
- **Edit.** Provisional-number sentence removed; gate text rewritten.
