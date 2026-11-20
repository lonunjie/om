# ADR-0241 — Certified RecoveryKit size, 253-character Homes and a non-ceremony delivery path

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-19). The design is accepted as the specification. Code may not ship or be enabled until vectors `KITS3-01` to `KITS3-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registries named below are updated in the same change, and a specialist signs off. Amends ADR-0237 and ADR-0216 (additive pointers there). The size and traffic metadata change needs explicit specialist review (see "Metadata" below); no audited sibling-session or transport envelope cap below 8,192 bytes was found (see Import notes), and the full mapped design gate is still owed. Qualified by the proposed ADR-0246 (additive note: the frozen type-38 `max_bytes` of 4,096 is not changed by this ADR; the preferred route is a new type 45 with a maximum of 8,192, which awaits operator and audit-owner decision; until then the positive reproduction of a 4,342-byte kit as type 38 is not to be relied on).
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** Gated private size and admission extension of ADR-0237 and ADR-0227. No change to RecoveryStore hostname validity, W0, Home API or recovery-root signing construction.
- **Answers:** how large a certified RecoveryKit may be for a valid 253-character Home hostname, and how it reaches a controller when the ceremony slot is too small (`KITS2-04`).

## Ruling

A syntactically valid 253-character lowercase ASCII hostname is **not** an ineligible RecoveryStore merely because its certified kit exceeds the implementation's historical bound. `KITS2-04` produces **4,342 canonical kit bytes** with anchor and certificate but without a snapshot key. The Core constant `MAX_KIT_BYTES = 4,096` is below that size, and ADR-0216's `kit-transfer` encrypted payload maximum of **4,112** is insufficient even before optional fields. Do not shorten the hostname, strip the root-signed place certificate, truncate the kit, omit the anchor or pretend a v1/uncertified kit is complete.

Set a **new explicit certified private RecoveryKit serialized-size maximum of 8,192 bytes**, counting the whole canonical type-38 CBOR map and fields 11 (anchor), 12 (root-signed place certificate), and 9 (optional independent snapshot key) when present. The issuer must canonical-encode and validate size **while the RecoveryRoot is still available**, before `ConfirmRecoveryKeyStored`; it must also validate the root-derived place certificate and policy commitment. The 253-character host with these defined optional fields is within this bound. Recipient and DeviceVault enforce the identical 8,192-byte maximum before decoding/decrypting or persisting. A future nonstandard extension that does not fit must obtain an explicitly versioned bounded format; it cannot enlarge the cap implicitly.

**For a valid store whose proof does not fit the supported certified-kit bound**, fail the *certified issuance/replacement attempt* before presenting a new sheet as usable or adopting a new recovery-policy child. Preserve the old code and policy where they exist, show a clear non-ready error and require a separately authorized new format or a different eligible RecoveryStore. Do not silently issue a root-only code as if the valid store were absent, degrade to an uncertified kit, or mark recovery backup “On”. If no store is eligible at all, ADR-0238's distinct explicitly labelled root-only fallback continues to apply.

**Delivery carrier:** The guaranteed certified-kit path, including to a newly promoted kitless controller, is ADR-0237's **authenticated sibling in-session kind 19, application version 2**, with mandatory exact policy field 10, kit field 9, place certificate field 12, local head check, durable vault-then-ACK and ADR-0227 delivery debt. Carry up to 8,192 raw kit bytes inside that existing session. No new Home queue, public endpoint or privileged provider role. A promotion using ADR-0216's old **4,112-byte encrypted kit-transfer** may use that slot only if it actually fits *and* the receiver has the same independent policy/certificate checks; otherwise send `null` in the ceremony, commit the properly authorized promotion and retain explicit **pending-kit debt** until the version-2 in-session transfer succeeds. The promoted target has no kit, no RecoveryRecord writing capability and no “ready” status in the interval. Never put 4,342 bytes into the old 4,112-byte slot or call its absence successful delivery. This is an explicit conditional clarification to ADR-0216's “when available” promotion reply and ADR-0227's later in-session distribution; the accepted ADRs need pointers.

## CDDL and bounds delta

```cddl
; Existing type-38 RecoveryKit fields and key-12 proof: UNCHANGED.
; New semantic bound: encoded canonical RecoveryKit (all fields) <= 8192 bytes.
; In-session kind 19 version 2, ADR-0237:
;   9: bstr .cbor RecoveryKit   ; mandatory; encoded bstr contents <= 8192 bytes
;  10: bstr .cbor RecoveryPolicy ; mandatory and checked against local verified ACS
; ADR-0216 promotion kit-transfer stays [bstr, bstr .size 1120,
;                                   bstr .size (17..4112)] with null allowed.
; Oversized kit: must use version-2 sibling send, not the promotion ciphertext slot.
```

No label changes. Retain `OM-RECOVERY-KIT-PLACE-BIND-SIG-v1`, `OM-RECOVERY-KIT-TRANSFER-HASH-v1`, `OM-RECOVERY-KIT-ACK-BIND-v1`, the ordinary session AEAD, exact retransmit bytes and existing HPKE ceremony label when that older optional slot is used. Update `MAX_KIT_BYTES` in Core and every private decoder/encoder/receipt test; do not change the audited host syntax or W0 `RecoveryPolicy`.

## Vectors, security and metadata

- `KITS3-01`: reproduce the existing 253-character host and 4,342-byte kit **verbatim**; generate with/without the 32-byte independent snapshot key, max legal port and epoch encoding; admission succeeds under 8,192 and independently recomputed certificate/policy, with real fixed-randomness hybrid signatures.
- `KITS3-02`: lost promotion reply with `null` kit transfer, verified promotion, debt created, delayed kind-19/v2 kit, durable vault commit, v2 ACK, ACK loss, restart and exclusion. No record write until receipt. A sender MUST NOT treat the ceremony reply as a kit ACK.
- `KITS3-03`: 8,192 accepted only if semantically valid; 8,193 refused; invalid 254-character host refused; valid host plus oversized extensions refused **before new-code adoption**; old code preserved. Certificate/policy/locator/epoch mutations fail without storage/ACK. Existing v1 carrier and v1 kitless send stay negative.
- `KITS3-04`: new and old codecs agree on the unchanged 4,112-byte promotion cap; a 4,342-byte kit never silently enters it. Test sibling message envelope limits, replay, authenticated source/target and exact digest.

The Home sees session traffic length/timing and potentially new size buckets, not the certified Home hostname or scope inside E2EE; the sibling sees the entire kit and its sensitive capabilities. The larger private object is a **capacity/traffic metadata change** requiring explicit review of observable session length, padding and storage pressure; it does not relax the RecoveryStore's public 253-character hostname rule or add a public credential. Preserve the existing root-only and legacy-kit negative behavior.

## Gate and audited-text check

**No audited design text needs to change** for a private 8,192-byte cap plus conditional sibling transport if the audited recovery and transport envelopes already admit it. Verify that assumption against the full mapped design gate before implementation; if a normative audited transport ceiling below the resulting in-session message exists, amend it with operator/audit-owner approval or keep this feature blocked. ADR-0216 and ADR-0237 private delivery descriptions and the Core size constant **do require explicit change**. Two independent codecs, metadata review and the existing ADR-0216/0227/0237 gates remain mandatory. No release approval.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision.** The 8,192-byte private kit cap is new and does not touch another bound: [`recovery-record.cddl`](../spec/cddl/recovery-record.cddl) states no serialized-size bound for type 38; the only Core limit is the constant `MAX_KIT_BYTES` (4,096), which this ADR replaces when the gate opens; the ceremony `kit-transfer` slot (17..4112 bytes, [`controller-change.cddl`](../spec/cddl/controller-change.cddl)) is unchanged. Kind-19 version-2 field 9 (kit) and field 10 (policy) are exactly the fields ADR-0237 defines; the 8,192-byte limit is a semantic bound added to field 9, not a new key. `RecoveryKit` keys 9, 11 and 12 are the existing snapshot key, anchor and place certificate. No new label. Vector ids `KITS3-01` to `KITS3-04` appear nowhere in the repository.
- **Audited envelope check.** Searched the sealed-delivery profile (bucket plaintext sizes 1,024 / 4,096 / 16,384 / 65,536 / 131,072), the home API, the multi-device sync and session documents, ADR-0199, ADR-0216, ADR-0227, ADR-0232 and ADR-0237, and `spec/cddl/*`. No audited cap on an authenticated sibling in-session application message below 8,192 bytes was found; the sealed-delivery bucket that would carry it is 16,384 bytes. This was a targeted search, not the full mapped design gate. If the gate finds a normative ceiling below the resulting message, that part must be treated as needing the operator and a specialist, and the feature stays blocked, as the ADR's own gate section already says.
- **Status.** Imported as accepted by delegation, enablement gated, as with ADR-0237. It adds a private size cap and a conditional delivery path and does not change audited text or a security floor; the capacity and traffic-size metadata change is explicitly left to the specialist review.
- **Edits.** The finding-severity line and one sentence describing a constant refusing the kit were reworded neutrally; the status, date and decided-by lines were rewritten; this section was added. No ruling text was changed.

