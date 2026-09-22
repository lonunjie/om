# Formal Verification, Fuzzing and Adversarial Testing

## 1. Scope

OM does not attempt formal proof of the entire product. Spend formal methods where state-machine mistakes can create irreversible authority/security failures.

## 2. AccountControlState — mandatory TLA+ model

Model before production implementation:

- device enrollment;
- control-device promotion;
- emergency quarantine;
- contest/safety freeze;
- permanent revocation;
- constitutional threshold changes;
- recovery-authorized transition;
- stale state/rollback;
- concurrent competing transitions;
- control-head observations.

Required invariants:

1. a compromised constitutional minority cannot promote itself into a majority without pre-change valid authorization;
2. quarantine alone never changes the constitutional quorum denominator;
3. a permanently revoked controller cannot regain authority through an older state/backup;
4. accepted generation/state hash never rolls back silently;
5. two incompatible successor states cannot both become locally final under the same valid parent without conflict/freeze detection;
6. recovery cannot bypass the RecoveryPolicy committed by the authoritative parent except by the explicit documented lost-all-devices recovery transition;
7. ordinary messaging devices cannot perform constitutional transitions unless promoted.

Run TLC in CI on bounded configurations and preserve counterexample traces as regression fixtures.

## 3. Property-based tests

Use property testing for:

- event convergence under reorder/duplicate/drop;
- deterministic encoding round trips;
- invite/ticket expiry/replay;
- RelaySet failover/idempotency;
- rich-text parser/validator invariants;
- LocalDataClass deletion closure;
- profile isolation.

## 4. Fuzz every public parser

At minimum fuzz:

- deterministic CBOR decoder;
- ContactDescriptor/Invite/ContactCapsule;
- relay envelope/ticket;
- ConversationEvent/RichTextV1;
- backup/blob manifests;
- theme bundle parser;
- deep-link parser;
- attachment metadata/type sniffing.

Fuzz goals include no panic, no unbounded allocation, no pathological CPU, and deterministic reject/accept.

## 5. Differential/conformance tests

Golden vectors are language-independent. At least one tiny independent reference/vector checker should exist outside `om-core` to prevent “the implementation defines the spec.”

## 6. Tooling floor

Rust CI should include current equivalents of:

- `cargo test`;
- clippy/rustfmt;
- Miri where applicable;
- sanitizers for FFI/native paths;
- `cargo-fuzz`/libFuzzer targets;
- dependency vulnerability/license review (`cargo audit`/RustSec and a policy tool such as cargo-deny/cargo-vet or equivalent).

Web CI should include:

- unit/component tests;
- browser E2E across supported engines;
- accessibility automation plus manual keyboard/screen-reader gates;
- CSP/Trusted Types tests;
- malicious-theme/rich-text corpus.

## 7. Crypto tests

Production CryptoProfile requires:

- upstream KATs/test vectors;
- OM profile composition vectors;
- downgrade/migration tests;
- compromised-key/PCS scenarios;
- malformed key/ciphertext rejection;
- cross-version interoperability;
- constant-time/side-channel review appropriate to implementation/library.

Do not infer cryptographic correctness from successful happy-path messaging tests.
