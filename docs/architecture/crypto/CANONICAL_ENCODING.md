# OM Canonical Encoding

## Decision direction

Use a strict deterministic CBOR profile derived from RFC 8949 for canonical protocol/control objects.

## OM-CBOR-v1 constraints

- RFC 8949 core deterministic encoding requirements;
- definite lengths only;
- no floating-point values in signed/security-sensitive objects;
- no duplicate map keys;
- valid UTF-8 text only;
- bounded nesting, collection counts and byte lengths before allocation;
- numeric field labels in frozen wire schemas where useful;
- unknown extensions handled only through explicit critical/non-critical extension rules;
- reject non-canonical encodings for objects that are signed/hashed canonically;
- sign/hash exact canonical bytes with an explicit ASCII domain-separation prefix and profile/object version.

Human text is not silently Unicode-normalized on receipt because normalization can alter user-authored content. Security-sensitive identifiers use their own normalization/confusable policy.

## Never sign JSON

JSON remains acceptable for logical schemas, tooling and debug output but is not the signed canonical representation.

## Decoder discipline

All input is hostile even if encrypted/signed. Decoder limits and canonicality checks happen before deep allocation or business logic. Golden positive/negative vectors are mandatory before wire freeze.
