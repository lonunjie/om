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
- no provisioning or deployment path activates code that is not a verified, digest-pinned release (R-261);
- the SecurityAdvisory key is a delegated role of the release root; peer-affecting floors need the root threshold (R-263);
- reproducibility documented/tested where feasible.

## Product/accessibility

- shared accessibility release gates pass;
- every built-in theme passes light/dark/high-contrast/security-state checks;
- privacy/recovery settings match actual implementation behavior.

## Claim discipline

Do not use “bulletproof,” “unhackable,” “anonymous,” or “quantum-proof” as release claims. State the implemented threat model and profile status precisely.

- No censorship-resilience claim (CENSORSHIP_AND_NETWORK_RESILIENCE §1, BLUEPRINT §18A) until PR-013 passes (R-269).
- No post-quantum post-compromise-healing or "Triple Ratchet"/SPQR claim for OM-Crypto-v1 (ADR-0118), and no PQ
  claim for v1 groups (ADR-0107). The validator rejects both outside decision history, research and reviews.

## Which public-release gates apply (ADR-0130 rule 7, R-273)

Each `PR-###` record in [`machine/public_release_gates.jsonl`](../../machine/public_release_gates.jsonl) carries `applies_to` (engineering `scopes`,
`distribution_kinds` and `claims`, drawn from the vocabulary in [`machine/repository_contract.json`](../../machine/repository_contract.json) →
`public_release_vocabulary`), a `criteria` list and an `independence` list. A planned public release is declared in
[`repository_contract.json`](../../machine/repository_contract.json) → `public_releases` with its scopes, distribution kind and claims. The validator refuses
any release whose status is `authorized` or `published` while a PR gate that applies to it (a shared scope, its
distribution kind or one of its claims) is not `passed` or `waived_by_adr`. That makes the AGENTS.md rule "do not ship
a public binary whose trust reduces to one mutable CI workflow" machine-checked: every project distribution kind
requires PR-005 and PR-012.
