# Go reader for OM-CBOR-v1 (independent second-language check)

A standard-library-only Go decoder and canonical encoder for the OM-CBOR-v1 profile of
[`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md), written from the profile text and RFC 8949
section 4.2 without reading the Python, Rust or Node codecs, and replayed against the frozen wire corpora.

```sh
cd verification/prototypes/go-cbor-reader
go run . ../bridges-wire/vectors.json ../plugins-wire/vectors.json ../services-wire/vectors.json
go test ./...
```

Last run (Go 1.27 on macOS, offline): every positive wire decodes and re-encodes byte for byte (bridges 27/27, plugins 24/24,
services 21/21); every `non_canonical` and `max_bytes` negative is rejected with a named reason (10/10, 14/14, 5/5); the
remaining semantic negatives are valid canonical CBOR and decode (88, 102, 75). The services corpus was not part of the
original build brief, so it is an unseen-corpus check.

## What this is and is not

- A CBOR-layer check only: canonical integers and lengths, definite lengths, no floats, tags or negative integers, valid UTF-8,
  no duplicate or unsorted map keys, trailing bytes, and resource limits. It performs no schema, signature, role, state or
  extension-criticality checks, so it is not a full conformance run and does not by itself close gate V-005.
- The implementation limits (8,192-byte item, 1,024 collection entries, nesting 32) are conservative choices of this reader,
  not normative OM limits: the profile gives none. Map keys are unsigned integers or text (two bridge vectors use text keys).
- One `max_bytes` bridge vector is rejected for trailing bytes, not size, because the reader has no per-object size limit.
- Not independently reviewed. A second reader agreeing with the corpus is evidence of corpus consistency, not of design
  correctness.
