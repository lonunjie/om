# v014-archive: om-archive-v1 spec, hostile corpus and reference reader/writer

Feasibility prototype for V-014. Not product code. Not for production.

Written by a Claude agent from [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../../../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) §§4–20, [`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md) and the logical
schemas `om-archive-manifest` and `imported-history-record`. The design package was authored by a ChatGPT design agent
and Claude agents; ADR-0117 (proposed) decides whether an AI reviewer counts as independent. The code is Rust and
does not derive from [`verification/bounded/wire_boundary_models.py`](../../bounded/wire_boundary_models.py), which is Python.

## Contents

| Path | What |
|---|---|
| [`OM_ARCHIVE_V1_SPEC.md`](OM_ARCHIVE_V1_SPEC.md) | Frozen-candidate spec: OM-ZIP-v1 container, manifest, canonical CBOR, `OMA-ENC-1-candidate` encryption, integrity, reader algorithm, import semantics, versioning |
| `omarchive/src/{zip,cbor,crypto,archive,import}.rs` | Reference reader/writer and read-only importer |
| `omarchive/src/testobj.rs` | INSECURE test stand-in for canonical signed objects (V-005 has not frozen them) |
| `omarchive/src/bin/omarchive-corpus.rs` | Deterministic vector and corpus generator, and harness |
| `vectors/om-archive-v1-vectors.json` | 5 byte-exact positive vectors: plain, passphrase, raw key, ZIP64, partial |

## Run

`./run.sh` takes about 6 s after the first build (the first build compiles the crates in about 30 s). It needs Rust
1.98.1, pinned in [`rust-toolchain.toml`](../../../rust-toolchain.toml), and the crates pinned in `Cargo.lock`: `chacha20poly1305`, `hkdf`, `hmac`,
`sha2`, `argon2`, `ed25519-dalek` and `crc32fast`. `./run.sh --long` runs a 200,000-mutant sweep.
Regenerate the vectors with `omarchive-corpus --vectors vectors/om-archive-v1-vectors.json --write-vectors`.

## Results (`verification/results/v014-archive-latest.json`)

- **Hostile cases: 67, all rejected with the expected class.**
  - Path attacks: `..`, absolute, backslash, drive letter, U+2024/U+FF0F look-alikes, case variant, NUL, directory,
    over-long name.
  - Entry types: symlink, directory mode, DOS directory bit.
  - Duplicate name.
  - Resource attacks: a real fixed-Huffman DEFLATE bomb (64 MiB), overlapping entries, declared huge size, entry-count
    bomb, count larger than the directory, total-size limit.
  - Container tricks: trailing data, prepended polyglot, EOCD comment, multi-disk, truncated, empty,
    data-descriptor flag, PKWARE-encryption flag, CRC mismatch, local/central mismatch, unknown extra field.
  - Format and version: missing format entry, future major version (format entry and manifest), wrong layout.
  - Non-canonical CBOR: unsorted keys, duplicate key, float, indefinite length, trailing bytes, non-minimal length,
    deep nesting.
  - Manifest integrity: unknown critical extension, unknown field, integrity-root mismatch, entry-hash mismatch,
    removed entry, unlisted entry, manifest too large, inventory path traversal.
  - Secret key material, in plain and encrypted archives.
  - Keys and KDF: wrong passphrase, wrong raw key, no key, KDF below the floor, KDF-parameter DoS, KDF-parameter
    tamper, chunk-size tamper, header-MAC tamper, unknown recipient type.
  - Ciphertext: truncated at a chunk boundary, truncated mid-chunk, corrupted byte, swapped entries, replayed chunk,
    manifest spliced from a foreign archive.
- **Positive cases: 6 accepted.** Plain, passphrase, raw key, multi-chunk, ZIP64 records, and a partial archive whose
  missing blob is reported.
- **Import checks: 9 pass.**
  - Stale archived Room control state is classified `StaleControl` and never applied.
  - A newer archived control state is flagged for live validation, and live state is untouched.
  - Forged and type-confused objects stay `Unverified`.
  - Forged `cryptographically_linked` / `bridge_attested` levels are downgraded.
  - Fabricated native authorship (`author_principal`, `signature`) is quarantined.
  - Canonical objects verify exactly.
  - Plain and encrypted round trips are byte-identical.
- **Mutation sweep: 20,000 deterministic mutants of the vectors.** 0 panics, 0 silent corruptions. The 33 accepted
  mutants changed only header bytes the reader is specified to ignore.
- **Determinism:** two consecutive runs produce byte-identical results JSON.

## Design gaps found (proposed design updates)

1. **The manifest must not be JSON.** [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../../../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) §4 names `archive-manifest.json`, but
   `integrity_root` is a hash, and [`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md) forbids hashing or signing JSON. *Proposal:* the manifest
   is canonical CBOR (`manifest.cbor`); a JSON rendering is optional and non-normative. The same applies to the
   `portable/*.jsonl` projections: normative records are canonical CBOR.
2. **The logical schema lacks fields the format needs.** [`om-archive-manifest.logical.schema.json`](../../../spec/protocol/om-archive-manifest.logical.schema.json) has no
   `record_inventory`, `critical_extensions` or `extensions`. It also leaves inventory items as untyped objects.
   *Proposal:* adopt the §5 table, including per-item `path`/`size`/`sha256`, `present` for partial exports, and a
   defined integrity-root construction.
3. **`imported-history-record` cannot carry provenance evidence.** It has no field linking a strong
   `provenance_level` to verifiable evidence, so an importer cannot tell a real `cryptographically_linked` record from a
   forged one. *Proposal:* add optional `link_evidence` (the archive path of a verifying link or attestation object).
   Unbacked strong levels are downgraded.
4. **A secret-material registry is needed.** §7 bans private keys but gives no machine-checkable rule. *Proposal:* a
   versioned registry of secret object types (§5.6). Readers reject the whole archive if one appears.
5. **The archive encryption profile is unnamed.** §8 defers "password/KDF/AEAD choices". *Proposal:*
   `OMA-ENC-1-candidate` (Argon2id with a floor and a ceiling checked before the KDF runs, HKDF, and age-style
   ChaCha20-Poly1305 STREAM), for crypto-profile review. Also decide whether plain archives need an optional exporter
   signature; today they have integrity only.
6. **Live-validation semantics for newer control state.** §10 says an archive "cannot override newer" state, but it is
   silent on archived control state that is *newer* than the importer's view. *Proposal (implemented):* the importer
   never applies it and flags it for re-validation through the live protocol.

Registry note: the new files need `python3 tools/om_registry.py sync` (this agent may not edit `machine/`).
