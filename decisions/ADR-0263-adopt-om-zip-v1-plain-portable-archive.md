# ADR-0263 — Adopt the OM-ZIP-v1 container, manifest and integrity rules (plain mode) for the portable archive

- **Status:** Delegated design decision (additive; independent review remains required). Encryption is not adopted.
- **Date:** 2026-11-23
- **Scope:** `portability` only: the container, the manifest, the integrity root, the reader algorithm and the import
  non-authority rules of [`OM_ARCHIVE_V1_SPEC.md`](../verification/prototypes/v014-archive/OM_ARCHIVE_V1_SPEC.md), plain mode.
- **Authority:** a delegated design decision under the operator's standing delegation to resolve plan questions from the
  constitution by additive ADR. It does not amend audited design text, lower a floor or change any wire already frozen.

## Context

The portability build plan (Q4) asks whether the archive adopts OM-ZIP-v1 as its normative container. The candidate
specification is complete and has two independent readers: the reference prototype and `crates/om-archive-reader`, written
from the specification alone, both replaying the same positive vectors and a hostile corpus. The design requires a public,
implementation-independent export and import that never becomes a source of authority.

## Decision

1. **Adopted as normative for plain mode:** sections 1 to 5 and 7 to 9 of the candidate specification, that is the strict ZIP
   subset, entry names and limits, the fixed entries, the canonical-CBOR manifest, the integrity root, record files, canonical
   objects as opaque exact bytes, blobs, the secret-material rejection (the registry in [`om-secret-object-types.json`](../spec/registries/om-secret-object-types.json)
   governs), the reader algorithm and the import non-authority rules. A writer emits plain-mode archives only until item 3.
2. **Per-object entries.** Canonical protocol objects are carried as opaque exact bytes with a declared `object_type` from
   [`om-object-types.json`](../spec/registries/om-object-types.json). The archive defines no per-object CDDL and never re-encodes an object; an importer verifies each
   object with its own live verifier. This is why no wire change is needed: the objects keep the frozen forms they already have.
3. **Encryption is not adopted.** `OMA-ENC-1-candidate` (section 6) stays a candidate until a reviewed profile exists. A reader
   returns `encrypted_unsupported`; a writer does not offer an encrypted portable export. `OMB-1` stays the only encrypted
   history format and is unchanged (plan Q2: kept as a separate lightweight backup).
4. **Versioning** follows section 9: a major change is a new format string and new labels; minor additions go in
   `extensions` or `critical_extensions`; an unknown critical item is rejected.
5. **Disappearing items** follow design section 9: an importer without expiry enforcement skips them (plan Q5).

## Consequences

- Slices 1 to 3 of the portability plan build the reader, the read-only import and the plain writer on this format.
- Nothing here changes a registry, a label or a CDDL file. The format string and the entries are the specification's.
- Independent review of the format and of any encryption profile remains required and is not claimed.

## Non-goals

This ADR does not adopt archive encryption, does not replace `OMB-1`, does not define foreign-import adapters, and does not
authorize any import path to change live control state, grant authority or fabricate native authorship.
