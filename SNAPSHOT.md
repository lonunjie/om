# OM Architecture Snapshot

Generated: **2026-09-27 18:08 PHT (UTC+08:00)**, after completing V-001 ADR-0140/ADR-0142 runtime evidence

## 2026-09-27 18:08 PHT — V-001 current-tree runtime completion

The supplied `om-design-20260927-1723-PHT.zip` was imported as checkpoint `20260927-1723`; its omitted `.github/workflows/verify.yml` registry entry was repaired. The already accepted GPT-5.6 Sol F-9 review was carried forward unchanged.

Both remaining V-001 pre-engineering criteria now have current-tree runtime evidence:

- ADR-0140 AsyncKeyBundle: Rust tests exercise same-generation/core migration between provider pools, persistence of the freshness floor through device serialization/reload, lower-generation replay rejection after reload, equal-generation core equivocation rejection, and one-time-prekey transplantation rejection across pool, device, profile and generation contexts.
- ADR-0142 SealedDeliveryEnvelope: the bounded Rust prototype reuses the pinned draft-ietf-hpke-pq-05 MLKEM768-X25519 / HKDF-SHA256 / ChaCha20Poly1305 implementation. Native, wasip1 and Node/V8 tests cover route-bound info, wrapper AAD, profile-1 key/encapsulation sizes, all five padding buckets, expiry, wrong route/key, tampering, malformed padding and the outer-open versus sender-authentication boundary. No clear-sender fallback is accepted.
- The independent Python checker verifies the refreshed deterministic vector corpus, including sealed-delivery wire/context/seal/open semantics. The previous vectors were compared before regeneration; changes match the ADR-0140 wire amendment, refreshed current initiator-binding cases and new ADR-0142 vector.

The complete V-001 runner passed: 90/90 selftests on native, Wasmtime and Node/V8; Python vectors 236/236; pinned draft-05 HPKE KATs 33/33 on each Rust runtime and Python; NIST ACVP ML-KEM-768 80/80 on Rust native, Rust WASI and Python; 249 fresh-instance persistence steps / 367 delivered messages with byte-identical runtime schedules. See [`verification/results/v001-crypto-latest.json`](verification/results/v001-crypto-latest.json).

V-001 pre-engineering is **13/13 passed** and `pre_engineering_status=passed`. The overall gate remains open for independent cryptographic review and production-build exclusion proof; engineering remains unauthorized. V-005 and V-005's 16:44 checkpoint were not modified.

Repository-wide `make verify` also passed: phase-1 evidence covered **347,316 bounded cases**, and all **83 configured TLC runs** matched their expected outcomes; architecture validation, docs links and registry immutability passed.

## 2026-09-27 17:23 PHT — V-001 F-9 independent re-review

GPT-5.6 Sol independently re-reviewed the responder-side initiator-binding remediation for finding F-9 and **accepted** ADR-0135 §7 / ADR-0136 rule 4. The review confirms that mandatory `RelationshipContext` plus `binding::verify_initiator` bind inbound `IK_A` to exactly one active, non-quarantined DeviceCredential committed by the trusted relationship ACS head before session lookup, PQXDH/Double Ratchet secret-key work, key consumption, plaintext release or state commit. V-001 criterion 10 is now met, bringing V-001 to **11/13 pre-engineering criteria**.

This runner has no Rust/Cargo/Wasmtime and no outbound network. The ADR-0140 and ADR-0142 bounded/static checkers pass, but the two remaining V-001 runtime criteria were not self-passed. V-001 remains open pending current Rust/native/wasm AsyncKeyBundle evidence and the sealed-delivery HPKE runtime path. Engineering remains unauthorized.

## 2026-09-27 16:44 PHT — V-005 Rust/CDDL verification

The supplied checkpoint SHA-256 matched `efde9acf8b4da94af567fcc3a8af93ddbedc9d37939e23112093874303db39ac`. The full pinned V-005 runner passed: Rust 1.98.1 and Node v24.14.0 agree on all 735 vectors; Rust and Ruby CDDL 0.10.7/0.12.14 agree on all 399 applicable cases; Rust fixtures pass 14/14; and Python/Node R-280 checkers pass 16/16 sequences each. The 276 Ruby-generated examples cause zero unexpected Rust strict-codec rejections. See [`verification/results/v005-encoding-latest.json`](verification/results/v005-encoding-latest.json) and [`docs/reports/2026-09-27-v005-rust-cddl-verification.md`](docs/reports/2026-09-27-v005-rust-cddl-verification.md).

The runner's gzip portability issue reproduced: decompressed canonical JSON matched byte-for-byte with SHA-256 `d5de1deaca350942b286a55d09de2748c234b37c26f77cad315d6c72bb46644a`, while compressed bytes differed in a gzip header byte. The harness now compares decompressed canonical JSON. V-005 pre-engineering is passed; overall V-005 remains open for the pre-release criterion. No other gate changed; `engineering_authorized=false`.

## 2026-09-27 13:08 PHT — verification continuation / crash-safe sealed delivery (ADR-0143)

The first Run-3 follow-up, a current-tree V-005 Rust/CDDL rerun, could not be executed in this runner: `cargo`/`rustc`/`rustup` are absent and outbound DNS cannot reach the pinned-toolchain download endpoint. That evidence remains explicitly open.

The next locally executable item, V-003 sealed-delivery session state, exposed a crash window in the original ADR-0142 replay prose: after durable `(delivery_key_id, delivery_id)` reservation but before authenticated inner durable acceptance, a crash followed by exact retransmission could be misclassified as a terminal replay and lose the message. ADR-0143 makes replay persistence a local `RESERVED` / `COMPLETE` / `REJECTED` state machine bound to the exact wrapper. An exact same-wrapper retry may resume `RESERVED`; same-key/id different-wrapper reuse is rejected; inner K2 session/dedupe state and `COMPLETE` commit crash-consistently before application release/ack; and `COMPLETE` replays are idempotent. No W0 wire object or primitive changes.

The focused executable model passes **15/15 scenarios** and catches **5/5 mutation controls**, including the crash-loss regression, clear-sender fallback, disabled route binding, treating outer open as sender authentication and cross-route delivery-id reuse. The existing V-003 sealed-delivery criterion is now met; V-003 remains open at **6/7** pre-engineering criteria solely for independent review. V-001 sealed-delivery HPKE/current AsyncKeyBundle runtime evidence, V-005 current Rust/CDDL agreement and V-015 real traces remain open. `engineering_authorized=false`.

## 2026-09-27 Run 3 — established 1:1 sealed delivery (ADR-0142)

ADR-0142 makes sealed delivery a W0/OM-Crypto-v1 requirement for relay/mailbox deposits in ACTIVE accepted 1:1 relationships. A new unsigned `SealedDeliveryEnvelope` (object type 23) HPKE-seals the complete, unchanged ordinary inner 1:1 envelope, so opening the outer wrapper never substitutes for the existing RelationshipContext, ACS/DeviceCredential, PQXDH or Double Ratchet authentication checks. There is no clear-sender fallback under CryptoProfile 1: unsupported or stale sealed-delivery state fails closed. First contact remains the existing `MessageRequest` / IntroGate flow rather than pretending an unaccepted relationship already has private delivery state.

To avoid making RelaySet scale with hybrid-HPKE key size, recipient devices publish individually signed, relationship-delivered, route-bound `SealedDeliveryKeyRecord` objects (type 22) instead of embedding a 1216-byte hybrid public key for every device × relay entry. `MessageRequestPayload` carries one request-scoped `reply_delivery_key` for the first post-acceptance sealed reply. CryptoProfile 1 pins the already-reviewed draft-ietf-hpke-pq-05 MLKEM768-X25519/HKDF-SHA256/ChaCha20Poly1305 suite, exact public/encapsulation lengths, domain-separated route/info/AAD bindings, fixed plaintext buckets, fresh 128-bit delivery IDs, durable replay reservation before plaintext release, same-route exact-wrapper retry rules, and fresh sealing when changing routes.

The metadata contract now treats established mailbox-class delivery as revealing neither stable sender/device identity nor Double Ratchet header shape by design. Residual timing, padding-bucket, source-network, volume and opaque route/key-identifier leakage remains explicitly declared. First contact, group-security upgrades, calls/live location, post-v1 ratchet upgrades, bots/plugins/bridges and unrelated later-wave work stay outside this pass.

Current executable evidence is clean for the tools available here: the V-005 corpus is **735 vectors (71 positive, 664 negative)** and the independent Node strict decoder agrees with the Python generator on all 735 verdict/error classes; the metadata model covers **216 flows × 12 observer roles with 0 violations**; and the dedicated ADR-0142 structural/bounded checker passes, including route isolation, replay, downgrade and metadata consistency checks. Cargo/Rust and the CDDL two-codec rerun are unavailable in this runner, so V-001/V-003/V-005 tool-complete criteria remain open. V-015 still requires real packet/provider/browser traces and independence evidence. Engineering remains unauthorized.

Final Run-3 exported-drop validation reports **774 current files; `design_open=0`; `verification_open=23`; `pre_engineering_open=23`; `engineering_authorized=false`; scopes authorized `0/11`**.

## 2026-09-27 Run 2 — security semantics and frozen-spec consistency (ADR-0141)

ADR-0141 closes the post-Run-1 semantic contradictions without changing OM's operatorless authority model. `om.acs.legacy_policy` is now critical/fail-closed everywhere current, including CDDL, registry, A-030 resolution and V-005 vectors. Provider bearer capabilities now have normative transport/redaction/no-store rules. RecoveryRecord v1 carries authenticated monotonic `record_generation`, `recovery_epoch` and `last_known_acs_head`; restored provider routes/capabilities/descriptors are advisory until revalidated, and stale backups cannot lower a surviving high-water mark. RecoveryRoot compromise is explicitly separated into authority vs metadata-confidentiality consequences, with optional independently held `RelationshipRecoveryKey` custody for Maximum Privacy.

Frozen-machine drift was repaired: assigned W0 domain-separation labels and AccountControlState `device_credentials` no longer say proposed; the default `skew_policy=default` tolerance is frozen at 300 seconds; stale A-035/MLS-exporter call-key text now matches ADR-0132's participant-only SFrame design; passkey privacy wording no longer overclaims unlinkability. The architecture validator and dedicated ADR-0141 checker now guard these semantic agreements, including the corrected A-030 resolution and current V-005 evidence.

The V-005 corpus is now **681 vectors (69 positive, 612 negative)**. The regenerated Python corpus and Node strict harness agree on all 681 expected verdict/error classes, including a positive critical-LegacyPolicy vector and a negative non-critical encoding. Rust/CDDL two-codec execution was not possible because this runner has no Rust toolchain, so that criterion remains explicitly open. No sealed-delivery/Run-3 work was performed. Engineering remains unauthorized.

Final Run-2 exported-drop validation reports **769 current files; `design_open=0`; `verification_open=23`; `pre_engineering_open=23`; `engineering_authorized=false`; scopes authorized `0/11`**.

## 2026-09-27 Run 1 — AsyncKeyBundle/prekey remediation (ADR-0140)

ADR-0140 re-freezes AsyncKeyBundle v1 before release: provider-visible `acs_generation`/`acs_state_hash` are removed and retired keys 3/4 are permanently reserved; DeviceCredential authority is checked against relationship-pinned peer ACS. Signed-prekey generation is one monotonic namespace per DeviceID + CryptoProfile across providers, with a durable recipient high-water mark plus equal-generation generation-core hash to detect rollback/equivocation. One-time-prekey signatures bind device, profile, generation, provider pool, kind, key id and public key. Provider generation revocation is operational cleanup, not the security boundary; backup/private-sync freshness merges monotonically.

The provider/recovery contracts, logical session state, CDDL/registries, metadata model, V-001 prototype source, V-005 canonical corpus and gate criteria were updated consistently. Current bounded ADR-0140 evidence passes, and the regenerated 679-vector V-005 corpus matches the committed gzip; the Node strict checker reports zero mismatches. The current runner has no Rust toolchain, so amended V-001 native/wasm execution/vector regeneration and V-005 Rust/CDDL execution were **not** claimed. V-005 two-codec pre-engineering agreement is explicitly reopened pending that tool-complete rerun.

This is a **Run 1 checkpoint, not the end of the multi-run remediation**. Sealed delivery and the Run 2 specification/security-consistency items remain outside this pass. Engineering remains unauthorized.

## 2026-09-26 final architecture design closure (ADR-0139)

Set 2 is complete. OM's architecture is now **design-frozen**: the explicit design queue remains at zero, the whole current canonical architecture/process/constitution/spec tree has been swept for unresolved normative ambiguity, requirement/threat/decision/security-invariant traceability is machine-checked, and the final adversarial pass covers all eleven engineering scopes.

ADR-0139 closes two final semantic ambiguities: SecurityAdvisory lifecycle may remove only an advisory-added recommendation and can never lower the stored relationship security floor; and frozen W0 CDDL + frozen registries/domain-separation rules are the authority for accepted signed/hashed wire bytes, while logical JSON schemas remain modeling/documentation views only. The pass also reconciles stale references to already-resolved A-032/A-035/A-045/A-046 and makes `OMA-ENC-1-candidate` explicitly verification-only until V-014 freezes a reviewed production archive-encryption profile.

The design-only closure audit reports **288 requirements / 288 trace rows / 0 missing or duplicate traces**, **93 threats / 0 invalid references**, **161 decisions through D-161 / ADR-0139**, **32 security invariants / 32 machine mappings**, **43/43 design-queue items closed**, and **167 canonical files swept / 0 unresolved ambiguity markers**. The 11-scope adversarial report found no additional blocking architecture choice after these repairs.

The Protocol Continuity Bundle manifest also remains closed: 79 normative architecture/constitution documents, zero manifest drift, and a disposable-probe 372-file bundle verifies offline.

This is not an implementation-security claim. TLC, Rust, Wasm/native builds, real browser/platform tests, fuzzing, performance, independent cryptographic review and human studies remain verification/engineering obligations. **Engineering is not authorized.** Future architecture work should require an actual contradiction/new requirement/independent finding rather than another speculative hardening cycle.

## 2026-09-26 design closure set 1 (ADR-0138)

The first half of the final architecture-closure work is complete. ADR-0138 resolves A-030/A-031/A-036/A-054 and
repairs two cross-boundary authority/privacy contradictions discovered while closing them. LegacyPolicy has narrow
guardian authority and verifier-local dormancy/contest semantics. **Historical note:** ADR-0140 later superseded this
section's W0 AsyncKeyBundle-linkage cut by shipping the unlinkable bundle design in v1; v1 MLS groups explicitly make no deniability
claim; and a later sealed-sender delivery profile hides the sender identity/ratchet header without weakening inner E2E
authentication.

Routine provider administration now uses independent provider/resource-scoped opaque capabilities rather than
AccountID/DeviceCredential/ACS authentication. RecoveryRecord storage has separate read/write capabilities and an
opaque locator derived from RecoveryRoot plus provider scope, while actual RecoveryRequest fan-out explicitly records
the AccountID/recovery-attempt disclosure to the participating mailbox Home. Design-only bounded/structural checks pass;
no TLC, Rust, wasm/native runtime or product-build work was performed.

This is **Set 1, not the final closure checkpoint**. Set 2 remains: the whole-repository normative-ambiguity sweep,
traceability closure audit, 11-scope adversarial attack pass, any resulting architecture repairs, and the definitive
engineering handoff. Engineering remains unauthorized.

## 2026-09-26 security-equivalence evidence pass (point 6)

The point-6 handoff has now been executed as a consolidated verification/equivalence pass. The new report is
[`2026-09-26-security-equivalence-evidence-report.md`](docs/reports/2026-09-26-security-equivalence-evidence-report.md).
It is verification evidence, **not** ADR-0117 independent approval of ADR-0137, because GPT-5.6 Sol also authored the
preceding ADR-0137 hardening. No engineering scope is unlocked.

Fresh executable/bounded evidence now shows: **66,816** ACS selective-freeze candidates with zero table-only or
TLA-only acceptances in the covered domain; mandatory `RelationshipContext` on all public prototype inbound 1:1
boundaries with established/reload/ambiguity negatives present; **216 metadata trace cases / 0 violations** plus
human/machine observer-matrix drift checks; separate Privacy Pass regressions for `r=1`, reused nontrivial `r=2`,
direct signing and the CSPRNG path; and provider negative-capability/authority-laundering mutations that all fail as
intended.

Two dynamic reruns remain tool-dependent rather than silently claimed: the pinned TLC JAR is absent, and this runner
has no Rust toolchain. Existing committed TLC/Rust/wasm evidence remains historical; the new current-tree Rust tests
therefore still need compilation/execution on a tool-complete runner. Independence review of F-10..F-13 also remains
required. Engineering remains unauthorized.

## 2026-09-26 security invariants and core-boundary hardening (ADR-0137)

The post-remediation independent re-review found F-10..F-13. This design pass applies their architectural remedies
without self-closing the findings: one normative ACS transition policy with threshold-restoring selective freeze
resolution; mandatory `RelationshipContext` on every inbound 1:1 decrypt path; authentication-layer metadata
observations; explicit W0 issuer-to-redemption unlinkability with fresh independent Blind-RSA factors; and provider
negative-capability contracts. The focused cross-model/equivalence pass is now executed as bounded/structural evidence; see the point-6 section above and
[`BRIEF-2026-09-26-security-equivalence-pass.md`](verification/reviews/BRIEF-2026-09-26-security-equivalence-pass.md) for the acceptance checklist.

Design-side bounded checks and the strengthened cross-model/equivalence evidence pass. Fresh TLC and Rust dynamic
execution remain unavailable in this runner and are explicitly carried as tool-dependent follow-up evidence. Engineering remains unauthorized.

## 2026-09-26 core_kernel review remediation (ADR-0136)

gpt-5.6-sol's independent review ([`2026-09-26-gpt-5-6-sol-core-kernel.md`](verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md))
rejected all seven core_kernel queue items, with 9 findings. The executor applied all of them (ADR-0136; each finding
has a `Resolution:` line). One finding was widened: the sender device key at the front of every 1:1 envelope is a
stable per-device correlator visible to mailbox Homes (A-054).
- V-001 is reopened (F-9): its new responder-binding criterion is met, and its re-review criterion is open.
- V-002, V-003, V-015, V-016 and V-017 wait on re-review of the changed artifacts only:
  [`BRIEF-2026-09-26-core-kernel-re-review.md`](verification/reviews/BRIEF-2026-09-26-core-kernel-re-review.md).
- Engineering is still not authorized.

## 2026-09-26 verification executor pass (ADR-0134)

Every repo-local `core_kernel` criterion now has passing evidence. V-001 and V-005 are `passed`. V-002, V-003,
V-015, V-016 and V-017 are open **only** for independent review. V-004, V-008, V-009, V-011, V-013, V-014 and V-019
have `feasibility_status=evidenced`. ADR-0135 pins the OM-Crypto-v1 bytes. Report:
[`2026-09-26-verification-executor-report.md`](docs/reports/2026-09-26-verification-executor-report.md). The single
remaining step before a `core_kernel` unlock ADR is
[`BRIEF-2026-09-26-core-kernel-independent-review.md`](verification/reviews/BRIEF-2026-09-26-core-kernel-independent-review.md).
Engineering is still not authorized.

## State

OM remains in **architecture verification / pre-engineering**. **Engineering is not authorized.** The blocking architecture-design queue is closed; remaining blockers are objective V-* evidence/review obligations.

- blocking architecture design items: **0**;
- open non-blocking/later-wave design items: **0** — A-030/A-031/A-036/A-054 are resolved by ADR-0138;
- verification gates V-001..V-023: **23 open**;
- engineering scopes authorized: **0 / 11**; `engineering_authorized=false`;
- machine ledgers: **298 requirements, 100 threats, 163 decisions through D-163 / ADR-0141; 33 security invariants / 33 mappings**.

## 11:54 verification-process hardening

ADR-0134 removes the remaining process ambiguity before handing execution to the repo agent:

- evidence is separated into design-complete, repo-local pre-engineering, post-scope implementation, and genuinely external/human phases;
- isolated `verification/` prototypes are explicitly allowed before scope authorization but are non-production evidence only;
- every W0-impacting later-wave gate now has explicit `feasibility_criteria`, so W0 does not need full W1-W4 gate passage;
- substantive requirements formerly hidden only in free-text `remaining` notes were promoted into canonical gate criteria;
- free-text `remaining` lists were removed; unfinished work is derived mechanically from canonical criteria;
- `criteria_met` must exactly match canonical criteria, and a passed pre-engineering phase must also record satisfied independence;
- independent-review-only work is batched after the verification executor exhausts repo-local evidence, preventing model-to-model ping-pong;
- [`verification/CLAUDE_OPUS_5_5_HANDOFF.md`](verification/CLAUDE_OPUS_5_5_HANDOFF.md) is the one-way execution brief for the next repo pass.

## What changed on 2026-09-25

ADR-0131 closes the remaining W0 core design questions and three newly found cross-boundary gaps:

- dual Ed25519 + ML-DSA-65 binding for PQXDH identity/prekey material;
- exact durable last-resort PQXDH replay ledger with a fail-closed 4096-per-prekey cap;
- AES-256-GCM-SIV Double Ratchet payload profile and explicit unencrypted-header metadata exposure;
- exact v1 PQ wording: hybrid-PQ handshake/HNDL protection, no PQ post-compromise healing;
- recovery-alone activation measured from **each verifier's own first observation**, defeating withheld-request maturity claims without a trusted clock;
- every live/rebasable reduction targets an immutable incarnation, closing stale-reduction/ABA attacks;
- reciprocal `IntroIdentityBinding`, with `ACCEPTED_UNBOUND -> ACTIVE` only after both stable AccountID bindings verify;
- MessageRequest HPKE pins `draft-ietf-hpke-pq-05` MLKEM768-X25519/HKDF-SHA256/ChaCha20Poly1305 rather than inventing an OM hybrid.

ADR-0132 closes referral-token, group-call and executable-extension boundary questions:

- RFC 9578 publicly verifiable token type 0x0002 for recipient-issued referral admission;
- per-sender SFrame call keys distributed only to joined participant DeviceIDs; Room MLS epoch possession alone does not reveal media keys;
- plaintext crossing into a remote mini-app origin is egress itself;
- continuous 1:1 plaintext egress is explicit bilateral signed relationship policy.

ADR-0133 closes the provider/distribution design blockers:

- normative Home API v1, CDDL bodies, typed error registry, real ProviderCapabilities JSON Schema with `durability_class`, and missing provider interface contracts;
- Rekor-v2/C2SP-checkpoint-compatible release transparency with two independent monitor observations while preserving the owner-pinned TUF/root as authority;
- all W0 conformance suites now contain concrete case lists and are registered `specified`.

## Verification status

The point-6 equivalence/conformance evidence pass is complete for every check executable in this runner:

- ACS accepted-set equivalence: **66,816 bounded candidates, 0 table-only, 0 TLA-only**;
- ACS authorization table: **16,200 cases, 56 vectors, 0 violations**;
- RelationshipContext public-ingress structural/mutation checks: **pass**;
- metadata trace model: **216 cases / 12 roles / 0 violations**, with auth-layer mutations caught;
- metadata descriptor/observer-matrix alignment: **pass**;
- Privacy Pass issuance KAT/freshness/linkability controls: **pass**, including repeated nontrivial `r=2`;
- provider negative-capability conformance/mutations: **pass**.

Fresh TLC state-space execution and compilation/execution of the newly expanded Rust V-001 tests are still outstanding
because this runner lacks the pinned TLC JAR and Rust toolchain. Historical committed TLC/Rust/wasm results were left
intact and are not presented as current-tree reruns.

Repository integration checks after the pass:

- `python3 tools/validate_architecture.py` — **pass**;
- `python3 tools/om_docs.py check` — **pass**;
- `python3 tools/om_registry.py check` — **pass**;
- continuity manifest — **78 normative docs, 0 problems**;
- offline continuity bundle — **369 files, integrity verification pass**.

Current validator state: **748 current files; design_open=4; verification_open=23; pre_engineering_open=22; engineering_authorized=false; scopes_authorized=0/11**.

## Next work

Run the two tool-dependent completion commands on a runner with the pinned dependencies (`python3 tools/run_tlc.py` and
`verification/prototypes/v001-crypto/run.sh`), then send the consolidated F-10..F-13 evidence to a genuinely independent
ADR-0117 reviewer. Do not begin product engineering or treat this executor's evidence as independent approval.
