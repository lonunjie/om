# ADR-0125 — Portable archive format rulings

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator (delegated 2026-09-24: "draft the design gap rulings ... check it and accept it if good")
- **Source:** V-014 ([`v014-archive`](../verification/prototypes/v014-archive/README.md)). **Amends:**
  [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md), the manifest logical schema.

## Decision

1. **Manifest is canonical CBOR.** The normative manifest is `manifest.cbor`, because its integrity root is hashed and
   JSON is never hashed or signed. A JSON rendering, and the `portable/*.jsonl` projections, are optional and
   non-normative.
2. **Manifest fields.** The manifest adopts [`OM_ARCHIVE_V1_SPEC.md`](../verification/prototypes/v014-archive/OM_ARCHIVE_V1_SPEC.md) §5: a typed `record_inventory` with `path`,
   `size`, `sha256` and `present` (for partial exports), `critical_extensions`, `extensions`, and the §5.2
   integrity-root construction.
3. **Provenance evidence.** `imported-history-record` gains optional `link_evidence`. A `cryptographically_linked` or
   `bridge_attested` level without verifying evidence is downgraded to `foreign_export_label`.
4. **Secret-material registry.** [`spec/registries/om-secret-object-types.json`](../spec/registries/om-secret-object-types.json) lists the object types that must never
   appear in an archive. It includes handshake prekey secrets (ADR-0120, K11). A reader rejects the whole archive if
   one appears.
5. **Encryption profile.** `OMA-ENC-1-candidate` (spec §6) is the provisional archive encryption profile. It joins the
   crypto-profile review with A-023..A-028. An exporting device may sign a plain archive. An unsigned archive proves
   integrity only, not who exported it. Each record's provenance still rests on its own evidence (rule 3).
6. **Newer control state.** Archived control state that is newer than, or unknown to, the importer's live view is
   never applied. It is flagged for re-validation through the live protocol.

## Consequences

V-014 criterion 1 (a frozen spec) is still unmet. It needs the non-author review, the crypto-profile review of §6, and
this ADR reflected in the spec. The spec already implements rules 3 and 6.
