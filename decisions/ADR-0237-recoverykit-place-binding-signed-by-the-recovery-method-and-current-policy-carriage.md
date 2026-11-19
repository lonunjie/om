# ADR-0237 — Root-signed RecoveryKit place binding and current-policy carriage in kit delivery

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-19). The design is accepted as the specification. Code may not ship or be enabled until vectors `KITS2-01` to `KITS2-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registries named below are updated in the same change, and a specialist signs off. Amends ADR-0232 (additive pointer there). The recovery-method signing use needs its own cryptographic review as part of the sign-off.
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** Gated, additive non-W0 private kit field and **version-2** in-session kind-19 payload. The method-key signature authenticates a *storage rendezvous*, not an ACS transition. Existing kind-19/kind-20 version-1 offline fixtures are retained as historical test inputs, **not** accepted for a kitless recipient. If version 1 was emitted to production, negotiate a new in-session kind/profile instead of changing its meaning.
- **Answers:** the ADR-0232 questions 1 and 2; missing independent `(scope, locator)` evidence and absence of current RecoveryPolicy body.
- **Supersedes:** ADR-0232 §5's unimplementable comparison to an already known locator on a kitless target, and its version-1 kit-send/ACK CDDL. **Preserves:** ADR-0199's no-Home-secret rule, ADR-0216 promotion authority and gates, ADR-0227 durable debt and controller eligibility, ADR-0225 high-water floors, and frozen W0 RecoveryPolicy and ACS shapes. No audited W0 field is added.

## Design sources read and not read

Read: [`AGENTS.md`](../AGENTS.md); ADR-0230–0232, ADR-0216, ADR-0225–0227; ADR-0199/0202/0205's pertinent recovery/kit sections; [`spec/cddl/recovery-record.cddl`](../spec/cddl/recovery-record.cddl), [`spec/cddl/account.cddl`](../spec/cddl/account.cddl), [`spec/cddl/common.cddl`](../spec/cddl/common.cddl); `crates/om-core/src/recovery.rs`, `crypto/sig.rs`, controller-change/kit receipt paths; recovery-extension Python and Rust fixture sources. Not read in full: all identity/provider/crypto design-gate texts, every sibling-session implementation and all platform vault adapters. Integration and crypto review remain prerequisites.

## Ruling

### 1. New RecoveryRoot-signed *place certificate*, minted at key issuance

At the moment a newly generated RecoveryRoot is in volatile memory, the issuing device already knows its Home, RecoveryStore `recovery_scope_id`, its derived `read_cap`, and `locator = DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)`. It also knows the candidate RecoveryPolicy body, epoch and committed recovery method public key. Before destroying the RecoveryRoot, it signs one **RecoveryKitPlaceBinding-v1** body under that epoch's existing RecoveryRoot-derived composite signing key (`OM-RECOVERY-AUTH-KEY-SEED-v1`), which the policy commits to. Use the new `OM-RECOVERY-KIT-PLACE-BIND-SIG-v1` DS-SIG label, profile 1, role `recovery` (2), signer **AccountID** (32 bytes). Store the resulting single-signature envelope **inside protected RecoveryKit key 12**, alongside the caps and the epoch. Never place the signed binding in the RecoveryRecord, ACS, owner sheet, Home API or an unencrypted message.

The binding signs canonical OM-CBOR `{version=1, AccountID, epoch, RecoveryPolicy digest-value, scope, locator, Home host, optional port}`. It is a **signed statement of the intended storage place** made while the recovery authenticator existed; it does not claim the Home is online or honest, and never authorizes a Home write, recovery request, W0 vote, or a policy change. Bind the policy digest and AccountID so a proof from a prior recovery method or a different account cannot be transplanted. Keep the proof unchanged on retransmission. At Home changes, no controller may invent a proof after the root has been erased; an approved root-visible replacement or an explicitly verified old-place RecoveryRedirect path is necessary. A new scope/locator without an independently rooted binding is **not** a legitimate same-epoch kit relocation.

**Why this is independent:** a kitless recipient's locally verified signed ACS head commits the RecoveryPolicy digest. The kit-send supplies its exact policy body (below), whose unique `recovery_key` method supplies the public verifier. An authenticated sibling sender can lie about a place but cannot forge this root-authenticated place binding just by knowing a RecoveryKit. A digest of sender-supplied fields alone has no such property. The recipient still cannot infer that a signed place is reachable, and a compromised RecoveryRoot or method signing key can attest an attacker-controlled place: that is the explicit residual.

**Mandatory issuance constraint:** the new place certificate must be minted while the root is in memory *before* `ConfirmRecoveryKeyStored`; only on successful ACS adoption does the certified kit become active. If there is no eligible store/scope or an eligible recovery method key, do not mint or pretend to validate this proof. Existing kit lineages whose RecoveryRoot was discarded **cannot be backfilled** by a software update; refuse repeatable kit sync to an uncertified kitless target and guide the person to the existing **Make a new safety code** path. Do not ask the user to resupply the old root into a sibling message. This is an availability cost, not permission to weaken admission.

### 2. Carry the exact current RecoveryPolicy in a new encrypted send form

Retain in-session `RECOVERY_KIT` kind **19** and `RECOVERY_KIT_ACK` kind **20**, but specify application format **version 2** for both. Kind 19 version 2 adds mandatory field **10 = canonical `bstr .cbor RecoveryPolicy`**. No new Home endpoint, ACS field, object-type or mailbox format is needed. The `kit_digest` still uses the frozen `OM-RECOVERY-KIT-TRANSFER-HASH-v1` over `OM-CBOR [AccountID, epoch, policy_commitment, exact_serialized_RecoveryKit]`; key 12's certificate is part of those exact kit bytes. The ACK uses the existing `OM-RECOVERY-KIT-ACK-BIND-v1` tuple and exact v2 `kit_digest`. Both kinds are sent within the existing authenticated sibling AEAD container. Reject version-1 sends for newly kitless controllers: they cannot meet this ruling's admission preconditions.

Do **not** treat possession of a verified head hash alone as proof of a policy body's contents. The receiver must canonical-decode field 10, compute `DS-HASH(OM-RECOVERY-POLICY-HASH-v1, policy_body)` and compare its **digest value** to the `recovery_policy_commitment` in its **own locally verified and unforked head**; compare the epoch, the claimed field-7 digest, and the kit epoch. If the promotion reply was missed, first obtain/reconcile the head and credentials through the existing signed sibling ACS update. The new field 10 supplies its current policy body even if the lost promotion reply had been its only earlier carrier. If earlier recovery links in the recipient's history require evidence or verifier time, use the ordinary independent ACS/recovery path; the sender's new policy body does not waive those checks.

For admission, independently verify the place certificate's signature with the uniquely identified committed `recovery_key` method; its AccountID, epoch, policy hash and `(host, port, scope, locator)` must equal the candidate kit and current policy. Recompute `locator` from **the kit's read capability** via the frozen label, and compare to the certificate. Check the Home's HTTPS ProviderCapabilities advertises the certified scope and feature before using the place. Check the target and source are both eligible controllers in the current local verified head, the authenticated sibling identities match the message's IDs, the anchor (when present) is in the locally verified chain, and the exact transfer digest matches. No locally verified head, missing method, missing/corrupt proof, stale epoch, changed locator, fork, or excluded sender → **no kit storage, no ACK**.

Only after a durable, integrity-protected DeviceVault write may a version-2 ACK be emitted. Repeat of the same transfer must be idempotent across restart; wrong digest/policy/target/version ACK never ends sender debt. Durable debts survive the 30-day warning and are bounded by backpressure, never eviction. A previously authenticated kit with a different place requires *new* independent place evidence or no change; sender-authenticated transport is insufficient. If the existing sibling codec cannot atomically persist the vault and ACK state, retain the existing enablement gate.

## Exact CDDL delta (private RecoveryKit and in-session application)

```cddl
; Add ONLY to private type-38 RecoveryKit, preserving keys 0..10, 11 (anchor), 23:
  ? 12: bstr .cbor signed-RecoveryKitPlaceBinding,

signed-RecoveryKitPlaceBinding = {
  1: bstr .cbor RecoveryKitPlaceBinding,
  2: [1*1 signature],                    ; exactly one recovery-role AccountID signer
}
RecoveryKitPlaceBinding = {
  0: 1,                                 ; place-binding format version
  1: bstr .size 32,                   ; AccountID
  2: uint,                             ; recovery epoch
  3: bstr .size 32,                   ; OM-RECOVERY-POLICY-HASH-v1 digest value
  4: bstr .size 32,                   ; RecoveryStore recovery_scope_id
  5: bstr .size 32,                   ; OM-RECOVERY-RECORD-LOCATOR-v1 digest value
  6: tstr .size (1..253),             ; lowercase ASCII host, recovery-sheet syntax
  ? 7: 1..65535,                      ; Home port, absent = 443
}

; In-session kind 19, new application version 2; *not* a Home object.
recovery-kit-send-v2 = {
  0: 19, 1: 2,
  2: bstr .size 16,                   ; transfer_id, stable for retry
  3: bstr .size 32,                   ; AccountID
  4: bstr .size 16,                   ; source controller DeviceID
  5: bstr .size 16,                   ; target controller DeviceID
  6: uint,                             ; epoch
  7: bstr .size 32,                   ; RecoveryPolicy commitment digest value
  8: bstr .size 32,                   ; existing kit_digest
  9: bstr .cbor RecoveryKit,          ; includes key 12 proof, mandatory for v2
  10: bstr .cbor RecoveryPolicy,       ; exact current body, mandatory
}
recovery-kit-ack-v2 = {
  0: 20, 1: 2,
  2: bstr .size 16, 3: bstr .size 32,
  4: bstr .size 16, 5: bstr .size 16,
  6: uint, 7: bstr .size 32, 8: bstr .size 32,
}
```

**CDDL note:** `? 12` is optional for backwards local storage, not optional for admission of version-2 kit delivery; the semantic rule requires it. Old kind-19 version-1 fixtures are never silently reinterpreted as version 2. A full Home move/redirect-chain proof would require a later versioned ruling; this ADR does not claim that an opaque redirected locator is independently bound simply because a sender says so.

## Domain separation

Register `OM-RECOVERY-KIT-PLACE-BIND-SIG-v1`, signature construction `DS-SIG(label, crypto_profile=1, role=2, signer=AccountID, body=canonical RecoveryKitPlaceBinding bytes)`. No changes to `OM-RECOVERY-AUTH-KEY-SEED-v1`, `OM-RECOVERY-RECORD-LOCATOR-v1`, `OM-RECOVERY-POLICY-HASH-v1`, `OM-RECOVERY-KIT-TRANSFER-HASH-v1`, `OM-RECOVERY-KIT-ACK-BIND-v1`, or any W0 signing label. Do not reuse an ACS/recovery-request signature as a location proof or confuse a signature on an opaque digest with signature of a RecoveryPolicy.

## Required independent vectors and state-machine tests

- `KITS2-01` (**positive bytes**): fixed `RecoveryRoot=40..5f`, `AccountID=11*32`, scope=`60*32`, epoch `7`, Home `home.example` port 443, target/source/transfer from existing `KITS-01`. From that root derive the epoch's composite method key and the scope's **actual** read/write/record caps using the frozen HKDF (not arbitrary `61*32` from old `KITS-01`), compute locator and policy commitment. With fixed ML-DSA-65 signing randomness, publish exact canonical binding bytes, DS-SIG preimage, root signature, key-12 kit, complete kind-19 version-2 message, kit digest, version-2 ACK and ACK binding. Independent Python and Rust encoders must produce identical bytes; recipient verifies with no preexisting kit and a separately pinned head.
- `KITS2-02` (**admission**): valid sibling session but sender fabricates a different scope/locator/host, uses old-epoch signer, a different AccountID, wrong root proof, wrong policy body, wrong commitment, missing field 10 or 12, or valid but unrelated place certificate. Each refuses both vault write and ACK. Test a receipt with no verified head and one with a verified head but unknown policy body: only the latter succeeds after field 10 verifies.
- `KITS2-03` (**missed promotion**): promotion reply lost, later signed ACS sibling update received, valid policy and certificate delivered, then durable kit write and authenticated ACK. Retry, ACK loss and restart preserve exact recipient state and debt. An old-kit holder receives neither newer signer proof nor a new controller vote by virtue of an old kit.
- `KITS2-04` (**migration/limits**): kitless controller, no certificate because root was erased; refuse and show new-code path. Changed Home scope without re-issuance; refuse. Bounded debt and 30-day warning. Subject-to-quorum exclusion ends debt; unverified membership does not. W0 old policy, malformed hybrid signature, noncanonical CBOR and malformed hostname negatives.

Existing `KITS-01` in [`verification/vectors/recovery-extension-v1.json`](../verification/vectors/recovery-extension-v1.json) is a **format-only v1 fixture**: its arbitrary caps and absence of both signed place binding and policy body mean it is **not an admission-positive fixture**. Never advertise it as such.

## Metadata and authority

The signed place proof and policy stay inside the same-account end-to-end-encrypted sibling session and DeviceVault. The Home sees the existing queue, traffic/size buckets and times, not the AccountID, policy digest, `locator` or certificate carried inside the kit. Both controllers necessarily learn the recovery storage endpoint and caps. Exposure of the *certificate itself* permits public correlation of AccountID with Home and locator: it must be secret-handled like the kit even though its signature is public-verifiable. The proof adds an independent holder-of-recovery-method-key assertion for storage routing only. A compromised or stale controller with an old kit can still read the older lineage; neither this signature nor an ACK revokes a copied secret. The Root is still never sent to a controller or retained after issuance.

## Gate

No kind-19 version-2 emission until this private CDDL, label, registry and real cross-codec signatures are independently tested and approved, the root-visible issuance hook is implemented and the ADR-0216/0227 gates are met. An uncertified old lineage remains `needs_new_safety_code` for repeatable delivery, not silently authenticated. No release claim follows.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision.** `RecoveryKit` (type 38) keys in [`recovery-record.cddl`](../spec/cddl/recovery-record.cddl) and [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json) are 0 to 10 and 23, and ADR-0230 reserves 11 (anchor); key **12** is free and is kept. In-session kind 19 (`RECOVERY_KIT`) is reserved by ADR-0232 with keys 0 to 9; key **10** (current RecoveryPolicy) is free and is kept, and the application version `2` is new for that kind. The label `OM-RECOVERY-KIT-PLACE-BIND-SIG-v1` does not exist in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) or any ADR; it is reserved here and registered when the gate opens. Vector ids `KITS2-01` to `KITS2-04` are unused. No existing number changes.
- **Relation to ADR-0232.** This ADR replaces ADR-0232 section 5's comparison of a kitless target to an already-known locator, and the version-1 kit-send and ACK CDDL for a kitless recipient. ADR-0232's durable debt, ACK order, digest and gate stand. Kinds 19 and 20 stay reserved and unemitted.
- **Cost admitted.** A kit lineage whose RecoveryRoot was already discarded cannot be given a place binding and so cannot be delivered to a kitless controller; the text sends the person to "Make a new safety code". That is an availability cost, not a weakening of admission.
- **Edits.** Review-numbering references in the header were reworded to neutral wording; the status, decided-by and amends lines were rewritten; this section was added. No ruling text was changed.
