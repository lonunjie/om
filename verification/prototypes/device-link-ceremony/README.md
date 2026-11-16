# Device-link ceremony vectors (ADR-0192)

Feasibility evidence, not product code. Fixed inputs, deterministic output: `verification/vectors/device-link-ceremony-vectors.json.gz`.

- [`gen_device_link_ceremony_vectors.py`](gen_device_link_ceremony_vectors.py) makes the vectors with its own minimal canonical CBOR encoder (unsigned integers, byte strings,
  text strings, arrays), `hashlib`/`hmac` and a pure-Python XChaCha20-Poly1305. It shares no code with `crates/om-core`.
  The CBOR codec is this generator's encoder, not the repository's strict codec: the construction uses only those four types, and the
  canonical form of each is fixed by [`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md) (shortest head, definite length).
- [`check_node.mjs`](check_node.mjs) recomputes every expected value with `node:crypto` and a second encoder.
- `crates/om-core/tests/device_link_vectors.rs` replays the file against `om_core::crypto::link_ceremony`.
- `./run.sh` regenerates and compares with the committed file, then runs the Node check; `--update-vectors` rewrites it.

Contents: context, ceremony id, PRK, the four keys, both associated-data values, the comparison code (and digit cases at the leading-zero
and modulus edges), the approval binding, one sealed message per direction with a fixed nonce, the polling id, and negatives (each bound
input changes every key; a re-encoded request changes the code; a code MAC is not a binding; each binding field changes the tag; a
message under the other direction's key or associated data differs).
