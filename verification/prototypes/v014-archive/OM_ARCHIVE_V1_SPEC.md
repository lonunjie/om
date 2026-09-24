# OM Portable Archive v1 (`om-archive-v1`): frozen-candidate specification

- **Status:** frozen *candidate* for gate V-014. It is not frozen until reviewed (independent_reviewer) and adopted
  by ADR. The encryption profile `OMA-ENC-1-candidate` additionally needs OM cryptographic profile review
  ([`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../../../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) §8).
- **Author:** written by a Claude agent. The design package was authored by a ChatGPT design agent and Claude agents;
  ADR-0117 (proposed) decides whether an AI reviewer counts as independent.
- **Normative sources:** [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../../../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) §§4–20, [`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md),
  [`spec/protocol/om-archive-manifest.logical.schema.json`](../../../spec/protocol/om-archive-manifest.logical.schema.json), [`spec/protocol/imported-history-record.logical.schema.json`](../../../spec/protocol/imported-history-record.logical.schema.json).
- **Reference prototype:** `omarchive/` (Rust) in this directory. Vectors: `vectors/om-archive-v1-vectors.json`.

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119 / RFC 8174.

## 1. Scope and non-goals

An om-archive-v1 file carries **data and evidence, never authority** (§7 of the source doc). It holds exact canonical
bytes of OM protocol objects, blobs, and portable records such as imported foreign history. It MUST NOT carry
long-term private keys (§5.6). Importing an archive MUST NOT grant any Account, Room, Service or Bridge authority (§8).

This spec does not define the per-object CDDL of OM canonical objects (V-005). Canonical objects are opaque exact
bytes with a declared `object_type`, and an importer verifies them with its own object verifier.

## 2. Conventions

- `||` means concatenation. `u64be(x)` is the 8-byte big-endian encoding.
- `H(x)` is SHA-256. `HKDF(ikm, salt, info)` is HKDF-SHA256 (RFC 5869) with a 32-byte output.
- **Canonical CBOR** means RFC 8949 §4.2.1 core deterministic encoding, restricted to the OM-CBOR-v1 profile
  ([`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md)). The rules are: definite lengths only, shortest-form integers and lengths, and map keys
  sorted bytewise by their encoded form with no duplicates. Only unsigned and negative integers, byte strings, UTF-8
  text strings, arrays, maps, `true`, `false` and `null` are allowed. No floats, no tags, no other simple values. The
  item must be the whole input (no trailing bytes). Nesting depth must be ≤ 16.
- Decoders MUST reject any violation and MUST check collection counts against the remaining input *before*
  allocating.

## 3. Container: OM-ZIP-v1 (a strict ZIP subset, PKWARE APPNOTE 6.3.10)

### 3.1 Entries

- Every entry is **stored** (compression method 0) and MUST NOT be encrypted. Compressed size MUST equal uncompressed
  size.
- The general-purpose flag MUST be exactly `0x0800` (UTF-8 names). Flags for encryption (bits 0, 6, 13) and data
  descriptors (bit 3) are rejected.
- Version-needed MUST be ≤ 45. Disk numbers MUST be 0. Internal attributes MUST be 0. File comments MUST be empty.
- External attributes:
  - if version-made-by's high byte is 3 (Unix), the upper 16 bits MUST be a regular-file mode (`S_IFREG`) and the
    low 16 bits MUST be 0;
  - otherwise, the external attributes MUST be 0.

  So symlinks, directories and devices are not representable.
- The only allowed extra field is ZIP64 (`0x0001`). It carries exactly the 8-byte fields whose 32-bit counterparts are
  `0xFFFFFFFF`: in the central directory, in the order uncompressed size, compressed size, offset; in the local
  header, both sizes.
- Writers MUST set: DOS date `0x0021` (1980-01-01), time 0, version-made-by `0x0314` (or `0x032D` when ZIP64),
  external attributes `0o100644 << 16`. Readers MUST ignore the DOS time/date and the low byte of version-made-by.

### 3.2 Layout (exactly one byte layout is valid)

The entries are laid out in this order:

1. local header + data, for each entry in order, starting at offset 0 with no gaps;
2. the central directory, immediately after the last entry's data, in the same order;
3. if ZIP64 is in use, the ZIP64 end-of-central-directory record (size field 44) and then the ZIP64 locator (total
   disks 1), each immediately following the previous record;
4. the end-of-central-directory record, with a comment length of 0, as the **final 22 bytes of the file**.

Readers MUST reject prepended data, appended data, comments, gaps, overlap, out-of-order local headers, a central
directory count or size that disagrees with its contents, and multi-disk archives. Each local header MUST equal its
central record in: name, CRC-32, sizes, method, flags and version-needed. Readers MUST verify the CRC-32 of every
entry.

### 3.3 Entry names

A name matches `om-archive-v1/` followed by 1–16 segments separated by `/`. Each segment matches
`[a-z0-9][a-z0-9._-]{0,127}`. The total length is ≤ 255 bytes.

This grammar alone excludes:

- absolute paths, `..` and `.`;
- backslashes, drive letters and NUL;
- non-ASCII look-alikes such as U+2024 and U+FF0F;
- case-variant duplicates and directory entries.

Readers MUST also reject exact duplicate names. Readers MUST NOT map entry names to filesystem paths. The logical
path is only a key into the manifest.

### 3.4 Size limits

Readers MUST enforce configurable limits and check them before reading data. The limits are: entry count, per-entry
size, total size and manifest size. The count is also checked for plausibility against the central-directory size
(each record is ≥ 46 bytes). A conforming reader MUST support archives of at least 1,000,000 entries, 64 GiB per
entry and 1 TiB in total. It MAY use lower limits when a user or policy asks.

### 3.5 ZIP64

Writers MUST use ZIP64 records if and only if they are required (≥ 65535 entries, or any size or offset
≥ `0xFFFFFFFF`). Readers MUST accept ZIP64 records even when they were not required.

## 4. Archive structure

### 4.1 Fixed entries

1. The first entry MUST be `om-archive-v1/format` with content exactly `om-archive-v1\n` (14 bytes). Other content
   starting with `om-archive-v` → `UnsupportedVersion`. Otherwise → `NotOmArchive`.
2. The second entry is either `om-archive-v1/manifest.cbor` (plain mode) or `om-archive-v1/header.cbor` (encrypted
   mode). Anything else → `UnexpectedLayout`.

### 4.2 Plain mode

The manifest is followed by one entry per payload item, named `om-archive-v1/` followed by its logical path, in
payload order (§4.3). A plain archive has **integrity** (every item is hash-bound by the manifest) but **no
authenticity**. Anyone can rewrite it consistently. Canonical objects inside remain independently verifiable.

### 4.3 Payload order and logical paths

| Kind | Logical path | Inventory |
|---|---|---|
| canonical object | `canonical/objects/<hex(H(bytes))>` | `canonical_object_inventory` |
| blob | `blobs/<hex(blob_id)>` (only if `present`) | `blob_inventory` |
| record file | `records/<stem>.cbor` | `record_inventory` |

Payload order is: all canonical objects, then the present blobs, then the record files. Within each group, items
are sorted by logical path (bytewise).

### 4.4 Encrypted mode

After `header.cbor` (§6.1) come the entries `om-archive-v1/e/00000000` (the encrypted manifest) and
`om-archive-v1/e/NNNNNNNN` for payload item N (1-based, 8 decimal digits, in payload order). Entry names reveal
only the item count and the ciphertext sizes.

## 5. Manifest (canonical CBOR map, text keys)

| Key | Type | Req | Meaning |
|---|---|---|---|
| `format_version` | text `"om-archive-v1"` | yes | another value → `UnsupportedVersion` (checked first) |
| `archive_id` | bytes(16) | yes | random per archive; MUST equal the header's in encrypted mode |
| `export_scope` | map | yes | exporter-defined description (§9 of the source doc) |
| `source_account_hint` | text / null | no | display hint only |
| `included_conversations` | [text] | no | |
| `generated_at_hint` | uint | yes | wall-clock hint, never ordering or authority |
| `exporter_implementation` | text | no | |
| `provenance_profiles` | [text] | yes | |
| `encryption_profile` | null (plain) / `"OMA-ENC-1-candidate"` | yes | MUST match the container mode |
| `canonical_object_inventory` | [ {`path`, `object_type`: text, `size`: uint, `sha256`: bytes(32)} ] | yes | `path` MUST be `canonical/objects/` + hex(`sha256`) |
| `blob_inventory` | [ {`path`, `blob_id`: bytes(32), `media_type`: text, `present`: bool, `size`, `sha256`} ] | yes | `present=false` = deliberately omitted (partial export) |
| `record_inventory` | [ {`path`, `record_type`: text, `size`, `sha256`} ] | yes | |
| `integrity_root` | bytes(32) | yes | §5.2 |
| `critical_extensions` | [text] | yes | a v1 reader MUST reject any entry (`UnknownCritical`) |
| `extensions` | map | yes | non-critical; readers MUST ignore unknown content |

### 5.1 Manifest validation rules

- Every map (the manifest and each inventory item) has **exactly** the listed keys. Missing or unknown keys →
  `Schema`.
- Every inventory is sorted strictly by `path` (so there are no duplicates).
- Every path matches §3.3 once `om-archive-v1/` is prepended, and it starts with its kind's prefix.

### 5.2 Integrity root

`integrity_root = H("OM-ARCHIVE-v1/integrity-root" || 0x00 || canonical([archive_id, canonical_object_inventory,
blob_inventory, record_inventory]))`

A reader MUST recompute it (`IntegrityRoot`). It MUST then check every payload item's size (`EntrySize`) and SHA-256
(`EntryHash`) against the inventory before exposing any item. The set and order of payload entries must match the
inventory exactly: `MissingEntry` if one is absent, `UnexpectedEntry` if one is unlisted or misplaced.

### 5.3 Record files

`record_type = "imported_history"` is a canonical-CBOR array of `ImportedHistoryRecord` maps. The fields follow
[`imported-history-record.logical.schema.json`](../../../spec/protocol/imported-history-record.logical.schema.json), plus an optional `link_evidence` (text): the logical path of a
canonical object that backs a strong `provenance_level`.

### 5.4 Canonical objects

These are exact bytes as received or produced. Readers MUST NOT re-encode them. Round-trip re-export MUST reproduce
byte-identical objects.

### 5.5 Blobs

Blobs are stored as decrypted plaintext. They are hostile input for any later decoder, which must validate them
before preview ([`RICH_TEXT_AND_CONTENT_SECURITY.md`](../../../docs/architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md)).

### 5.6 Secret material

A reader MUST reject the whole archive (`SecretMaterial`) if any `object_type` is in the secret registry. The current
registry is: `om.account_root.private`, `om.device_identity.private`, `om.device_vault`, `om.recovery_secret`,
`om.mls.private_state`, `om.session.private_state`, `om.service_controller.private`, `om.bridge_controller.private`.
Writers MUST NOT emit them. A sealed recovery artifact is a separate format (§7 of the source doc).

## 6. Encryption profile `OMA-ENC-1-candidate`

The profile uses only reviewed constructions: HKDF-SHA256, Argon2id (RFC 9106), HMAC-SHA256, ChaCha20-Poly1305
(RFC 8439), and the STREAM chunking construction as profiled by the age format.

### 6.1 Header

`header.cbor` is a canonical CBOR map with exactly these keys: `format_version` (`"om-archive-v1"`), `profile`
(`"OMA-ENC-1-candidate"`), `archive_id` (bytes 16), `chunk_size` (uint, MUST be 65536), `recipients` (array of 1–16
recipients) and `header_mac` (bytes 32).

### 6.2 Recipients

The archive key `K` is 32 random bytes. It is wrapped as
`wrapped_key = ChaCha20-Poly1305(KEK, nonce = 0^12, K, aad = "OM-ARCHIVE-v1 wrap" || 0x00 || archive_id)` (48 bytes).

- `{type: "passphrase-argon2id", salt: bytes(16), m_kib, t, p, wrapped_key}`, where
  `KEK = Argon2id_v0x13(passphrase, salt, m_kib, t, p, len 32)`.
  - Parameter floor: `m_kib ≥ 65536` and `t ≥ 3`.
  - Parameter ceiling: `m_kib ≤ 4194304`, `t ≤ 10`, `1 ≤ p ≤ 8`.
  - Readers MUST check both before running the KDF (`KdfParams`). This blocks both a downgrade and a denial of
    service through the header.
  - The committed vectors use `m_kib=8, t=1, p=1`. Only a reader explicitly in `insecure_test_kdf` mode accepts them.
- `{type: "raw-key", key_id: bytes(8), wrapped_key}`, where `KEK = HKDF(raw32, archive_id, "OM-ARCHIVE-v1 raw-kek")`
  and `key_id = first 8 bytes of H("OM-ARCHIVE-v1 raw-key-id" || 0x00 || raw32)`. This is meant for a
  high-entropy recovery code.
- An unknown recipient `type` → `UnknownCritical`. No matching key → `NoMatchingKey`. A wrong passphrase and a wrong
  key are indistinguishable by design.

### 6.3 Header MAC

`header_mac = HMAC(HKDF(K, archive_id, "OM-ARCHIVE-v1 header-mac"), canonical(header without header_mac))`.

A reader verifies it after unwrapping `K` (`HeaderMac`). The MAC binds the profile, chunk size and recipient set.

### 6.4 Entry encryption

For entry index `i` (0 = manifest), `key_i = HKDF(K, archive_id, "OM-ARCHIVE-v1 entry" || u64be(i))`.

The plaintext is split into chunks of exactly 65536 bytes. The last chunk holds 0–65536 bytes, and it is empty only
when the whole plaintext is empty. Chunk `c` is encrypted with ChaCha20-Poly1305 under `key_i`, with an empty AAD
and the nonce `u88be(c) || last_flag`, where `last_flag` is 0x01 for the final chunk and 0x00 otherwise.

Consequences:

- truncation at a chunk boundary, mid-chunk truncation, chunk reordering or replay, and bit flips all fail
  authentication;
- entry swapping fails, because each index has its own key;
- a manifest spliced in from another archive fails, because `K` and `archive_id` differ.

Any failure → `Decrypt`. The reader exposes nothing from an archive that fails.

## 7. Reader algorithm (normative order)

1. Parse the container (§3). Any violation → `Zip(<class>)`.
2. Check the `format` entry (§4.1).
3. In plain mode, check the manifest size, then parse and validate the manifest (§5).

   In encrypted mode:

   1. parse the header;
   2. validate the KDF parameters;
   3. unwrap `K` with the supplied keys;
   4. verify `header_mac`;
   5. decrypt `e/00000000` under the manifest size limit;
   6. parse the manifest, then check that its `archive_id` equals the header's.
4. Check the entry set and order against the inventory. Decrypt each payload item, then verify its size and SHA-256.
5. Report blobs with `present=false` in `missing_blobs`. This is a partial archive, not an error.

A reader MUST treat an archive as all-or-nothing. It MUST NOT write any entry to disk under an attacker-chosen path.

## 8. Import semantics (non-authority)

An importer holds live protocol state and an object verifier. It MUST NOT let an archive change live state.

- **Canonical objects.** An object that fails verification is kept only as opaque bytes (`Unverified`), never as
  native history. A verified control-state object older than the live version is historical evidence
  (`StaleControl`). A verified control-state object that is newer than live, or unknown to it, MUST be revalidated
  through the live protocol (`ControlNeedsLiveValidation`). It is never applied from the archive.
- **Imported history.**
  - Records never become OM ConversationEvents.
  - Fields that claim native OM authorship or identity cause the record to be quarantined. These fields are
    `author_principal`, `author_account_id`, `author_endpoint_id`, `event_id`, `conversation_id` and `signature`.
    Unknown fields and unknown `provenance_level` values are also quarantined.
  - `cryptographically_linked` requires `link_evidence` naming a verifying `foreign_identity_link` object, and
    `bridge_attested` requires a verifying bridge-attestation object. Without that evidence the record is
    **downgraded** to `foreign_export_label`. Export never upgrades provenance.
- **Keys and credentials.** Importing yields no signing keys, device enrolment, delivery capabilities or Room send
  rights. In the prototype the importer takes live state by shared reference and returns a view with an always-empty
  `authority_changes`.

## 9. Versioning

- A major change means a new `format_version` string, a new `format` entry content, and new domain-separation
  strings.
- Minor, backward-compatible additions go only in `extensions`, which old readers ignore, or in
  `critical_extensions`, which old readers reject.
- New recipient types are critical by construction, as are new `record_type` values that carry authority semantics.
- A reader MUST NOT guess the meaning of an unknown critical item.

## 10. Vectors and corpus

- `vectors/om-archive-v1-vectors.json` holds five positive archives, with their hex bytes, SHA-256 and the keys needed
  to open them: plain, passphrase, raw key, forced ZIP64 and partial. All keys are insecure test values derived from
  public labels.
- The prototype's `omarchive-corpus` regenerates them byte-for-byte and generates the hostile corpus
  deterministically (67 cases; the README table gives the expected error class of each).

## 11. Known limitations of this candidate

- The canonical-object format is a test stand-in (`testobj.rs`) until V-005 freezes per-object CDDL.
- Import verifies a stale-versus-live control *version*. It does not re-run full governance validation.
- Only one reader/writer exists. Pre-release needs a second independent implementation.
- Real foreign-format importers (HTML, JSON, SQLite exports) are not included.
- Plain mode gives no authenticity. An optional exporter signature is left for a later profile.
