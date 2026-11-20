# ADR-0247 — Independent sheet custody of the exact signed checkpoint envelope

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-20). The design is accepted as an additive specification. Code may not ship or be enabled until vectors `RK2D-01` to `RK2D-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registry rows it names are added in the same change, and a specialist signs off. Qualifies ADR-0242 (additive pointer there). The new digest label and the version-3 sheet definition are registered only with the sheet spec change; W0 and type-39 CDDL are untouched.
- **Class:** New owner-sheet format version; original W0 ACS hash and v2 sheet meanings immutable.
- **Answers:** RK2B-01 signature-only envelope substitution.
- **Priority:** High.

## Ruling

The existing v2 `OwnerRecoveryCheckpoint` pins AccountID, generation and the **unsigned** ACS body hash. It does not pin signature bytes. For a **newly issued** owner sheet that must reject *every different signed envelope*, independently custody an additional **32-byte exact-signed-envelope commitment**. Do not change `OM-ACS-HASH-v1` (which deliberately identifies bodies rather than signatures), reinterpret existing v2 owner sheets, or treat AEAD-carried envelope bytes as independent custody.

Register new private-to-sheet digest label **`OM-ACS-SIGNED-ENVELOPE-PIN-v1`**, a `DS-HASH` of the **exact canonical encoded `signed-AccountControlState` envelope bytes** (including the ordered signature entries, signer metadata and signature bytes). Store only its 32-byte digest value in a **version 3 full owner recovery sheet** alongside the existing v2 canonical `OwnerRecoveryCheckpoint` bytes. At sheet creation the issuer freezes the exact verified signed envelope, computes both body hash and envelope commitment from the same bytes, and displays/custodies the sheet only after those bytes are durably pinned; signing/adding an additional signature later produces a different envelope and needs a newly issued sheet. All v3 secret and relationship sheets of a paired lineage commit **the same checkpoint bytes and envelope commitment**, and each checksums the complete layout.

## Exact proposed non-CBOR sheet delta (`spec/protocol/recovery-sheet.md`)

```text
New version=0x03, kind=0x01 (root) or 0x03 (relationship):
  version(1) | kind(1) | secret(32) | scope(32) | port(2)
  | host_len(1) | host(host_len) | canonical OwnerRecoveryCheckpoint CBOR(82..90)
  | signed_envelope_pin_value(32) | sheet_check(4)
Whole length = 105 + host_len + checkpoint_len.
  signed_envelope_pin_value = digest-value(DS-HASH(
    "OM-ACS-SIGNED-ENVELOPE-PIN-v1", exact signed ACS envelope bytes)).
  sheet_check uses existing OM-RECOVERY-SHEET-CHECK-v1 on all preceding bytes.
```

Version-2 layouts, OwnerRecoveryCheckpoint type-39 CDDL and type-39 90-byte cap remain unchanged. No new type-39 key and no W0 signature change. Parsing v3 must require exact lengths, recognized kind, canonical checkpoint, constant-time checksums and exact envelope-pin comparison **before** using a supplied envelope as the starting state. Re-serialized/altered/signature-reordered envelopes with an equal unsigned-body hash fail the v3 envelope check; an envelope *matching* the sheet pin is a starting authority state, not independent proof of historical Q→checkpoint W0 signature validity. Existing v2 sheets retain their narrower body-pin guarantee and must not advertise detection of signature-only changes. A v2-only reader refuses v3, never strips the final commitment.

## Vectors, metadata and gate

- `RK2D-01`: fixed signed checkpoint with previous-key signature and full real hybrid envelope bytes, exact unsigned body digest, signed-envelope commitment, v3 binary, Base32 and QR bytes; without Q history a v3 sheet still anchors the exact envelope.
- `RK2D-02`: same ACS body, one signature byte changed to another *well-formed signature-entry encoding*; body hash unchanged, envelope commitment different and v3 refuses before suffix verification. Also test signature-entry reorder, addition/removal, duplicate and re-signing under other valid randomness.
- `RK2D-03`: legacy v2 sheet does not gain signature-envelope identity by inference, and a v3/v2 pair cannot be mixed; altered sheet/truncation/checksum, unknown version/kind fail.
- `RK2D-04`: atomic issuance/crash/restart, rollback/fork at checkpoint, and a later ordinary child signed with P's committed rotated credential.

The new digest remains solely in the owner-custodied secret/relationship sheets, not in ACS, head gossip, Home or RecoveryRecord. Loss or disclosure of the owner sheet has its existing threat model. New hash label and sheet-version registration, two independent real hybrid-signature codecs, UX/backup-custody review and audit-owner acceptance are required. **Audited W0 text does not change; the normative owner-sheet specification must be additively extended.** No retroactive strong guarantee for saved v2 sheets or release approval.
