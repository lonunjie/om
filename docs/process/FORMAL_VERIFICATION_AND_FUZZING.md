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

## 2026-09-23 08:21 new cross-cutting verification gates

The completed design queue adds three blocking proof programs:

- **V-015 metadata exposure/privacy paths:** instrument provider/network traces across relay/group/discovery/IntroGate/push/blob/calls/services/bridges/migration and verify that canonical-ID hiding and UI/provider metadata claims match observed reality;
- **V-016 decentralized abuse/moderation:** model authorization/revocation/expiry, Space delegation, selective evidence, SafetyAdvisory lifecycle/conflicts/scoped references, provider denial, service/bridge abuse and Sybil/resource behavior;
- **V-017 protocol longevity/0M:** perform independent continuity-bundle recovery, multi-generation/fork compatibility, crypto-break/release-root/provider disappearance tabletop/technical exercises and dominant-extension capture tests.

These are architecture-verification activities and may use purpose-built model checkers, fuzzers, packet traces, independent decoders and bounded prototypes even though product engineering remains unauthorized.


## 2026-09-23 09:20 phase-1 verification evidence

The first verification-evidence checkpoint is recorded under `verification/`. It is deliberately **evidence-only**: all V-001..V-017 gates remain open and engineering remains unauthorized.

Executed evidence in this checkpoint:

- **7,772 bounded/adversarial cases** across AccountControlState, CommitCoordinator/group-control, Space/Room authority, baseline event convergence, ServicePrincipal/delegated agency, bridge/archive/metadata boundaries, scoped moderation and continuity invariants;
- **14 deterministic-CBOR foundation vectors** produced identically by independently written Python and Node.js encoders;
- **60-artifact offline continuity dry run** covering current protocol JSON plus selected normative/machine continuity material.

Important limits:

- the TLA+ AccountControlState model is authored, but TLC was not available in this execution environment and V-002 therefore remains open;
- V-005 remains open because every signed/hashed v1 object still needs a frozen CDDL/numeric-field/domain-separation registry and object-specific two-implementation vectors;
- bounded reference models do not substitute for runtime, browser, cross-implementation, packet-trace, parser-fuzz or independent continuity evidence required by the respective gates.

`verification/results/phase1-summary.json` is the machine-readable summary.
