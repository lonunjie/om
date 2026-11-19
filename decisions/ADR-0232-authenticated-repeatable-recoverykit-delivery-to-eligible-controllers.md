# ADR-0232 — Authenticated, repeatable RecoveryKit delivery to eligible controllers

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-19). The design is accepted as the specification. Code may not ship or be enabled until vectors `KITS-01` to `KITS-04` exist from two independent codecs, the CDDL and registries named below are updated in the same change, and a specialist signs off.
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** `additive_noncritical_extension` (ADR-0165): two new in-session application kinds and two labels inside an already authenticated sibling session; no Home endpoint.
- **Answers:** the wire form of ADR-0227 section 3 (OM4-14: repeatable kit delivery to a main device).
- **Amends:** nothing. **Does not amend:** ADR-0199 section 5, ADR-0205, ADR-0216 (its enablement conditions stand), ADR-0225, ADR-0226, ADR-0227, [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md).
- **Scope:** `core_kernel` (ADR-0169): already-authorized sibling sessions and DeviceVault.
- **Controlling sources:** ADR-0199 section 5, ADR-0205, ADR-0216, ADR-0225, ADR-0226, ADR-0227 section 3.

## Design sources read, and not read

Read in full for this import: the six proposals; ADR-0204, ADR-0225, ADR-0227, ADR-0228, ADR-0229 and the passages of ADR-0199, ADR-0203 and ADR-0226 cited here; [`call.cddl`](../spec/cddl/call.cddl), [`controller-change.cddl`](../spec/cddl/controller-change.cddl), [`lan.cddl`](../spec/cddl/lan.cddl), the recovery-record CDDL, [`recovery-sheet.md`](../spec/protocol/recovery-sheet.md), the object-type, domain-separation and numeric-field registries and Home API v1 section 8.2. Not read in full: ADR-0216, the constitution documents, and the domain documents the design gate lists for these paths. The proposals were checked against the sources above only; if an unread text fixes a point differently, that text wins and this ADR is corrected by a follow-up.

## Decision

1. **Kit is a secret, not an ACS vote.** Use the existing type-38 `RecoveryKit` as the only kit plaintext; include its **optional key 11 anchor** from ADR-0230 (reserved, gated). A kit carries record read/write key, inbox capability and (where present) independent relationship snapshot key; it grants no ACS, quorum, device-link or recovery-signing authority. Neither the Home nor an ordinary noncontroller sibling can receive a kit. Do not put the kit in an ACS, sender-visible envelope, unsealed note or group message.
2. **New same-account in-session kinds.** In the authenticated sibling session's **existing encrypted/authenticated application container**, allocate **in-session kind 19 = `RECOVERY_KIT`** and **in-session kind 20 = `RECOVERY_KIT_ACK`**, disjoint in-session application kinds using the CDDL below. They are in-session kinds in the sense of ADR-0199 section 5 and ADR-0227 section 3 item 2, numbered in the same sequence as the Core's existing in-session kinds (1 to 18 are in use at this snapshot), and are **not** object types: the object-type registry ([`om-object-types.json`](../spec/registries/om-object-types.json), assigned through 44) is not touched. The reviewer proposed 45 and 46; see "Import notes". A sender can retry *bit-identically* or rewrap authenticated plaintext under a fresh ratchet envelope; the same `transfer_id` and kit digest remain stable for the epoch.
3. **Digest and context.** Define `kit_digest = DS-HASH(OM-RECOVERY-KIT-TRANSFER-HASH-v1, canonical OM-CBOR [account_id, recovery_epoch, policy_commitment, serialized_RecoveryKit])` (digest **value**, 32 bytes). Add and register this label and `OM-RECOVERY-KIT-ACK-BIND-v1` for the authenticated ACK context (canonical CBOR `[label, account_id, source_DeviceID, target_DeviceID, epoch, kit_digest, transfer_id]`). Both signatures/authentication come from the already authenticated session; *do not* add a home-verifiable detached signature or reuse an ACS signing label. A receiver derives its sender/recipient DeviceIDs from authenticated session state and requires the CDDL IDs to agree.
4. **Durable debt before first send.** Persist `(target_DeviceID, source_DeviceID, epoch, policy_commitment, kit_digest, transfer_id, created_at, retry_state)` atomically with the new kit/eligible-controller state before the first send; retry on each ordinary sibling sync tick after restart until a valid ACK. A per-target/epoch newer kit supersedes the older debt; a target that is **verified excluded** ends its debt (and cannot obtain later writes). After 30 days show “main device needs setup” and **retain debt**. On disk cap overflow **backpressure** new distribution rather than evict live debts. Never report all controllers synchronized while any owed debt remains.
5. **Receiver admission and ACK order.** Confirm the sender is currently authenticated and both endpoints are **eligible controllers in the receiver's verified unforked head**, recovery epoch and policy commitment match that head, account/scope/locator match the receiving vault's lineage, anchors match the committed verified chain, and the kit digest matches exact canonical content. Reject older epochs without ACK, reject newer unverified epochs without ACK, and reject a kit from an excluded controller. Commit the kit to protected DeviceVault with durable integrity, then and **only then** send ACK bound to the sender, recipient, transfer, digest, epoch and policy. A restart after storing but before ACK should issue the same authenticated ACK on retry. ACK of the wrong target/epoch/digest never clears debt.
6. **Enablement is separate.** Do not enable until ADR-0216's durable fork detection, second-controller request delivery after first inbox drain, excluded-controller old-kit write guard and shared-vector verification pass. Missing kit means no recovery-record writes on that controller; no ACS rollback. A pending 30-day debt is an operational warning, not delegated authority to weaken quorum or silently replace a key.

## In-session CDDL (kinds reserved by this ADR; not emitted before the gate opens)

```cddl
; Opaque encrypted session application payload, never a Home request.
; In-session kinds 19 and 20 (reserved by this ADR), not object types.
recovery-kit-sync = recovery-kit-send / recovery-kit-ack
recovery-kit-send = {
  0: 19,                                          ; in-session kind RECOVERY_KIT
  1: 1,                                           ; format version
  2: bstr .size 16,                               ; transfer_id CSPRNG
  3: bstr .size 32,                               ; account_id
  4: bstr .size 16,                               ; source DeviceID
  5: bstr .size 16,                               ; target DeviceID
  6: uint,                                        ; recovery epoch
  7: bstr .size 32,                               ; committed RecoveryPolicy digest
  8: bstr .size 32,                               ; kit_digest (DS-HASH label above)
  9: bstr .cbor RecoveryKit,                      ; canonical type 38 bytes
}
recovery-kit-ack = {
  0: 20,                                          ; in-session kind RECOVERY_KIT_ACK
  1: 1,
  2: bstr .size 16,                               ; same transfer_id
  3: bstr .size 32,                               ; account_id
  4: bstr .size 16,                               ; sender/controller DeviceID of ACK
  5: bstr .size 16,                               ; recipient/controller DeviceID of ACK
  6: uint,                                        ; epoch
  7: bstr .size 32,                               ; policy_commitment
  8: bstr .size 32,                               ; exact kit_digest
}
```

The existing sibling session carries the payload in its established ratchet, with sender/recipient membership as authenticated data. There is no standalone `RECOVERY_KIT` HTTP/relay object and **no new provider capability**. If the current sibling application codec cannot guarantee authenticated source/target identity or atomically persist an ACK after vault write, **this is an audited session-carriage change**, and the specialist must approve the new channel envelope before a feature flag may be set. Do not emit these bytes before the sibling-session dispatch approval.

## Metadata analysis

The Home sees only the ordinary encrypted same-account deposits, their timing, size buckets and target queue correlation; it must learn neither the existence of a RecoveryKit field nor epoch/digest/role through a new public endpoint. A same-account sibling already sees the other controller's online/sync timing. The *receiving controller* learns recovery locus, epoch and read/write caps by design. A malicious or removed controller holding an **earlier** kit retains old-lineage access; key replacement and writer exclusion guard are essential, and this carriage cannot revoke already copied secrets. Padded messages are preferable to avoid trivially distinguishing a kit sync from ordinary sync by length; the format must remain within the existing in-session envelope cap. `kit_digest` is **not** an unkeyed identifier advertised to the Home; it stays inside session AEAD.

## Required golden and failure vectors

- `KITS-01`: fixed account `11*32`, source `22*16`, target `33*16`, epoch 7, policy `44*32`, transfer `55*16`, a canonical type-38 kit with fixed caps/anchor. Both Python and Rust emit identical send CBOR, exact DS-HASH digest, ACK CBOR and ACK-binding bytes (publish the full hex fixture once kinds 19 and 20 are fixed in the vectors).
- `KITS-02`: mismatched epoch, parent policy digest, target ID, account, anchor, malformed canonical CBOR, extra unknown key, removed sender or fork -> no vault write and no ACK.
- `KITS-03`: lost send, duplicate send, ACK lost, restart before/after vault commit, wrong ACK, identical authenticated replay and newer-epoch supersession -> exactly one vault value; debt ends only with authenticated matching ACK or verified ineligibility.
- `KITS-04`: dropped promotion reply with newer head followed by repeatable kit, older kit rejected; 30-day warning with debt retained; max debt capacity returns explicit backpressure; disabled until four ADR-0216 guards verified.

**Authority:** durable synchronization of a secret to an already-authorized controller, never an enlargement of the controller set. No release claim follows from the presence of this ADR alone.

## Gate

Before any code ships or is enabled: the two labels are registered in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) and the in-session kinds are recorded where Core registers them, together with vectors `KITS-01` to `KITS-04` from two independent codecs; the four ADR-0216 enablement conditions hold; and a specialist approves the sibling-session carriage if the current codec cannot guarantee authenticated source and target identity or an atomic ACK after the vault write.

## Import notes (collision check and edits, 2026-11-19)

- **Value change: 45 and 46 become 19 and 20.** The reviewer proposed object types 45 and 46. Neither number is allocated anywhere (the object-type registry ends at 44; the Core's in-session kinds end at 18), so there is no literal collision. They were not used because the carrier is an in-session kind, not an object type: ADR-0199 section 5 says the builder takes the next free number where in-session kinds are registered, the existing in-session veto kind (12) has no object-type row, and an object-type entry would add a registry row for something no Home or relay ever carries. The next free in-session numbers are 19 and 20 (the in-session kind 10 is unassigned in Core and left alone). Event kinds (1 to 18, [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json)) are a separate namespace.
- **Labels checked.** `OM-RECOVERY-KIT-TRANSFER-HASH-v1` and `OM-RECOVERY-KIT-ACK-BIND-v1` are absent from [`om-domain-separation.json`](../spec/registries/om-domain-separation.json): no collision. They are reserved by this ADR text and not registered until the gate opens. `RecoveryKit` key 11 (anchor) is ADR-0230's reserved field.
- **Relation to ADR-0227.** Section 3 of ADR-0227 decides the debt, receiver checks and lifecycle; this ADR gives the bytes. Where they differ, ADR-0227 wins.
- **Edit.** Provisional-number sentence removed; numbers and gate text rewritten.
