# Security / Release Gates

A build cannot be presented as production secure until all applicable MUST gates pass.

## Protocol/core

- deterministic CBOR/CDDL/field registries frozen for v1 objects;
- golden positive/negative vectors;
- every parser bounded and fuzz target exists;
- no dev CryptoProvider in release graph;
- downgrade floors and migration vectors pass;
- AccountControlState TLA+ invariants pass and implementation tests cover counterexample classes;
- profile isolation tests pass.

## Cryptography

- current standards/errata rechecked;
- implementation library/protocol composition reviewed;
- KATs/upstream vectors in CI;
- independent cryptographic review plan completed for first public secure release;
- experimental/draft group PQ suites cannot masquerade as preferred stable profiles.

## Endpoint/web

- strict CSP; no remote runtime scripts/fonts/themes;
- Trusted Types enforcement where supported plus sink-free design;
- DeviceVault threat model and browser capability matrix tested;
- origin/provider migration tested as device transfer;
- no plaintext global indexes/logs.

## Content

- no HTML/JS/CSS message execution;
- RichTextV1 validation/fuzzing;
- attachment type/size/decompression limits;
- link preview privacy policy tested;
- view-once/disappearing deletion-closure tests.

## Supply chain

- locked dependencies/actions;
- SBOM;
- vulnerability/license policy;
- signed source/release tags;
- build provenance (SLSA/in-toto-compatible);
- TUF-style update metadata/rollback/freeze defense for distributions that own updates;
- reproducibility documented/tested where feasible.

## Product/accessibility

- shared accessibility release gates pass;
- every built-in theme passes light/dark/high-contrast/security-state checks;
- privacy/recovery settings match actual implementation behavior.

## Claim discipline

Do not use “bulletproof,” “unhackable,” “anonymous,” or “quantum-proof” as release claims. State the implemented threat model and profile status precisely.
