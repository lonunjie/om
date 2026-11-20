# ADR-0246 — Certified kit bound must not silently alter frozen type 38

- **Status:** Proposed (2026-11-20), explicit operator, audit-owner and specialist decision required. It touches frozen registry row type 38 and a new type 45, and the certified-kit size and traffic metadata review; nothing is registered, `spec/registries` and CDDL are untouched, and it is not accepted by delegation. Qualifies ADR-0241 (additive pointer there): ADR-0241's 8,192-byte bound stays gated and its positive reproduction of a 4,342-byte kit as a type-38 item must not be relied on until this is decided.
- **Class:** New private object lineage proposal to preserve immutable registered meanings; recovery delivery remains gated.
- **Answers:** KITS3-03/04, the omitted type-38 registry cap and all private admission paths.
- **Priority:** High.

## Finding

[`spec/registries/om-object-types.json`](../spec/registries/om-object-types.json) declares `status: frozen (ADR-0119)` and says `max_bytes` is checked on the encoded top-level item **before decoding**. Type 38 `RecoveryKit` has `max_bytes: 4096`; Core `crates/om-core/src/recovery_record.rs` also has `MAX_KIT_BYTES = 4096`, and its decoder invokes the registry check. ADR-0241's 8,192-byte claim does **not** currently cover this row or the vault, restore, promotion, sibling and registry admission paths. Raising type 38's registered `max_bytes` to 8,192 is an **audited normative text/accepted-byte change**, not an editorial clarification or a permissible delegated ADR edit. Do not silently patch the frozen value.

## Preferred non-mutating ruling (subject to acceptance)

Keep type 38/version 1 and its 4,096-byte maximum immutable for pre-existing local kits. Reserve previously unassigned **type 45 `CertifiedRecoveryKit`**, a new private `om.recovery_secret` object with top-level `max_bytes: 8192`. Its payload matches the ADR-0237 certified fields but changes the object tag to 45 and requires key 12's independently verified place certificate for kitless-target admission. A new certified issuer produces type 45 **while the RecoveryRoot is still available** and never promises an uncertified legacy type-38 kit is equivalent. ADR-0241's `KITS3-01` type-38 4,342-byte sample must be preserved as a **negative historical specimen**, then regenerated with type 45 as a new positive (different bytes and transfer digest). This explicitly qualifies ADR-0241's “verbatim” instruction; it is not bit-compatible with type 38.

```cddl
; recovery-record.cddl; do NOT redefine RecoveryKit/type 38
CertifiedRecoveryKit = {
  0: 45, 1: 1,
  2: bstr .size 32, 3: tstr .size (1..253), ? 4: 1..65535,
  5: bstr .size 32, 6: bstr .size 32, 7: bstr .size 32,
  8: bstr .size 32, ? 9: bstr .size 32, 10: uint,
  ? 11: recovery-anchor,  ; ADR-0230 map {AccountID, generation, ACS digest}
  12: bstr .cbor signed-RecoveryKitPlaceBinding,
  ? 23: extensions,
}
; max encoded CertifiedRecoveryKit bytes = 8192 (including all fields).
; New kind-19/v2 admission:
; 9: bstr .cbor CertifiedRecoveryKit (contents <= 8192 bytes)
; ADR-0216 promotion kit-transfer ciphertext cap remains 4112 bytes.
```

All version-2 sender, receiver, receipt, recovery-key issuance, DeviceVault persistence/load, snapshot, state restoration and local backups-exclusion checks MUST use the registry's type-45 bound and a hard pre-parse bound (reject 8,193 bytes). Source and target eligibility, current-policy commitment, epoch, certificate, locator and root-derived method verification still apply. Vault write precedes v2 ACK; a promotion oversized for 4,112 uses `null` with durable kit-delivery debt. Legacy type-38 decode stays <=4,096 and never becomes kitless certified admission. Sender refuses a valid but over-cap kit *before new-code confirmation/child adoption* and preserves the old key.

An alternative **only by explicit audit-owner approval** is to amend type 38's frozen registry cap to 8,192, with a documented new normative profile/migration rule; that is an audited-text change and is NOT authorized by this ADR draft. Do not combine both alternatives silently.

## Labels, vectors and metadata

No new cryptographic labels; retain `OM-RECOVERY-KIT-PLACE-BIND-SIG-v1`, `OM-RECOVERY-KIT-TRANSFER-HASH-v1`, `OM-RECOVERY-KIT-ACK-BIND-v1`. Type 45 changes the transfer-hash input; regenerate exact canonical digests/signatures. Register type 45 in [`om-object-types.json`](../spec/registries/om-object-types.json) and field IDs in [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json) only when accepted; do not edit type 38.

- `KITS4-01`: current 4,342-byte type-38 byte string fails registry admission; corresponding type-45 kit (including 253-character legal hostname and valid certificate) fits 8,192 and passes.
- `KITS4-02`: complete type-45 and v2 sibling carriage at exactly 8,192 (only if semantic validity can attain the boundary) and 8,193 rejection at every entry point; malformed 254-character hostname, invalid signatures/extension and oversized issuer fail before adoption.
- `KITS4-03`: 4,342 bytes never enters 4,112-byte ceremony ciphertext, null promotion creates debt; authenticated sibling delivery, ACK loss/restart and excluded target preserve lifecycle.
- `KITS4-04`: independent codecs and registry checks agree on typed version 38 vs 45; no plaintext kit in Home, backup, log or wrong session; measure sealed-delivery bucket and vault pressure.

An encrypted certified kit's greater size can disclose a different traffic bucket to the Home; the other controller and a compromised vault see sensitive Home/locator capabilities. This capacity and metadata change requires a specialist review. Existing sealed-delivery 16,384-byte bucket is not itself proof of all in-session envelope/MTU limits. **No audited W0 text changes**, but adding a new normative registry row/type and changing ADR-0241's gated path requires operator/change-control approval and full transport design gate. No release approval.
