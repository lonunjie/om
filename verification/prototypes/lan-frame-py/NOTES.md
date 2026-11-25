# LanFrameV1 implementation notes

## Scope and interpretations

- Only `BRIEF.md`, [`lan.cddl`](../../../spec/cddl/lan.cddl), [`common.cddl`](../../../spec/cddl/common.cddl) and [`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md) were used as specification inputs. The codec covers `lan-frame-v1` only, not the separate invite or signal envelopes or other common objects. No network access or third-party packages are used.
- The eight frame maps are closed, with all their listed fields mandatory. The general common-schema extensions comment does not add key 23 to these explicitly enumerated LAN maps; all unlisted keys are rejected, including otherwise recognized LAN labels belonging to other kinds.
- `device-id` is exactly 16 bytes, `blob-id` exactly 32 bytes, and `profile-id` is 1 through 65535 inclusive, from [`common.cddl`](../../../spec/cddl/common.cddl). Every size/range is inclusive. `uint` values in this codec are limited to 0 through 2^64-1, with no tagged bignum representation.
- `encode` accepts exact built-in `dict`, `int` and `bytes` types, not subclasses or equivalent coercible types. In particular, Python `bool` is not an integer here. `decode` accepts exact built-in `bytes`, not mutable bytes or buffer-like objects. These choices avoid coercion and type ambiguity. A Python dict cannot represent duplicate equal keys that were already collapsed before it was passed in; duplicate wire keys are checked explicitly.
- Map order follows the brief's shorter-encoded-key-first, then bytewise rule. For shortest-form unsigned CBOR integers this is equivalent to unsigned numeric order, and all actual schema labels fit one byte. Thus the encoder sorts integer keys, while the decoder compares actual encoded keys after checking shortest form. No general-purpose CBOR or alternate CBOR profile is implemented.
- Byte strings are opaque and may contain any octets. Zero-filled identifiers, repeated identifiers, arbitrary digest/tag bytes, and boundary offsets are accepted if structurally valid. No cross-frame, cryptographic, uniqueness, freshness, capability, receipt, authority, transfer-progress or other application-state checks are implied by this codec. In particular, an offset need not be consistent with a chunk length or another frame's total, and `total_bytes` does not trigger any payload allocation. Those checks are outside the attached single-frame schema. A tag in the CBOR sense is forbidden; the schema's `receipt_tag` is simply a 32-byte string.
- The 131068-byte cap applies to the entire input before parsing, including any trailing bytes. The maximum legal map has six entries and the maximum legal byte string is 16384 bytes. Counts above six and declared byte-string lengths above 16384 are rejected before iteration or slicing; other byte-string sizes are checked against their individual field rules after parsing. Truncated data is checked before slicing, and the parser never descends recursively. No syntactically or semantically invalid frame is returned or normalized. Every valid frame is naturally smaller than the input limit, so no extra output-size check is required after encode validation.

## Error reason choices and precedence

The brief specifies `ValueError` and short snake_case reasons, but no fixed vocabulary or precedence for inputs with multiple defects. The following choices are deterministic. They are local codec diagnostics, not LAN error-frame codes.

| Reason | Meaning |
| --- | --- |
| `input_type` | Decode argument is not exact `bytes`. |
| `empty_input` | Zero input bytes. |
| `input_too_long` | Input length exceeds 131068. |
| `truncated` | Missing initial byte, argument bytes, or required byte-string payload. |
| `indefinite_length` | Initial-byte additional information 31 (also rejects stray break and invalid uses on integer majors). |
| `invalid_head` | Reserved additional information 28, 29 or 30. |
| `not_map` | Top-level CBOR major is not map, or encode argument is not exact `dict`. |
| `non_shortest` | An allowed integer or length argument uses a wider form than necessary. |
| `key_set` | Map exceeds six pairs, required discriminator is absent, or exact kind-specific labels differ. |
| `invalid_key` | Key major is not unsigned integer, or encode key is not an exact unsigned 64-bit Python `int`. |
| `duplicate_key` | An already-seen decoded key is repeated. |
| `key_order` | A nonduplicate decoded key is not strictly greater in canonical encoded order. |
| `unsupported_type` | Value is text, array, map, CBOR tag (including bignums), float, or simple value. |
| `integer_range` | Negative CBOR integer or an ordinary integer field outside its inclusive bounds. |
| `byte_string_size` | A byte string exceeds the global byte-string bound or violates the field's size constraint. |
| `trailing_bytes` | Any bytes follow the parsed top-level map. |
| `value_type` | An otherwise allowed scalar (or encode-side Python object) is not of the field's required type. |
| `invalid_version` | Version is not 1 after its type check. |
| `unknown_kind` | Kind is not one of 1 through 8 after its type check. |

Decode precedence: input type, empty input, input limit; then initial-byte validity, major-type eligibility, argument completeness and minimality, and the top-level map count limit. For each pair in input order, validate its key header/major/argument, check duplicate before order, then read its value (including global byte-length limit before payload completeness). A negative integer is rejected after its argument has been checked. Unsupported major types are rejected without reading their bodies; their malformed contents cannot change the reason. A reserved additional-information value or 31 is classified at initial-byte inspection before major-type eligibility. A count above six is `key_set` even if its claimed contents are absent, duplicate or out of order. A declared byte string above 16384 is `byte_string_size` even if its payload is absent. No scan for additional errors is performed.

Once the map has been read, trailing bytes are rejected before schema validation. Shared schema validation checks the exact dict type, maximum count, key type/range, presence of labels 0 and 1, version type/value, kind type/value, exact key set, then the remaining fields in schema order (type before bounds). Consequently a malformed discriminator can precede another missing or extra key. Encode uses this same schema validation without wire-level checks; invalid version/kind integers receive the discriminator-specific reason, even if they are outside uint64. Multi-defect reason ordering is a deliberate local choice, not a claim that the sources prescribe it.

## Fixtures and verification

- [`make_vectors.py`](make_vectors.py) uses independent fixture constructors and explicit wire fragments rather than importing or calling `encode`. Both positive expected bytes and hostile bytes are assembled independently of the codec. Negative reasons are assigned in fixture definitions, not obtained by calling `decode`. The vectors include all eight kinds, all requested boundaries, shortest-form transitions, malformed heads, truncation, disallowed majors, duplicate/out-of-order keys, wrong types, missing/foreign fields, and size/range violations.
- `vectors.json` contains 28 valid and 198 hostile cases, with lowercase wire/byte-string hex and decimal-string keys. Negative cases use JSON `null` for `frame`; positive cases use JSON `null` for `reason`. Integers remain JSON integers even at uint64 maximum; use an integer-preserving JSON reader rather than a floating-point-only one. The generator writes to its own directory and produces deterministic contents.
- To regenerate and test from the extracted `lanframe/` directory:

  ```sh
  python3.11 -B make_vectors.py
  python3.11 -B -m unittest -v
  ```

- All six unittest methods passed in the available Python 3.13.5 environment, including every positive and hostile fixture, exact JSON equality, Python-side validation, and proper-prefix rejection. Python 3.11 was not installed in the execution environment; the source was additionally parsed with the Python 3.11 grammar via `ast.parse(feature_version=(3, 11))`. All code uses standard-library APIs available in Python 3.11; native 3.11 execution remains unverified.
