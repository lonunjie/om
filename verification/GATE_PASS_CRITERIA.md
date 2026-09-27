# Verification Gate Pass Criteria

- **Status:** Canonical verification-process view, **generated** by `tools/render_gates.py` from
  `machine/architecture_verification_gates.jsonl` (the machine ledger is authoritative; do not hand-edit)
- **Decisions:** ADR-0108, ADR-0109, ADR-0113, ADR-0115, ADR-0117, ADR-0134

## Rules

1. **Evidence is not passage.** A gate phase passes only when every listed criterion is met and recorded with evidence links.
2. **Independence.** Evidence produced solely by the author/agent who designed the thing under test cannot pass a phase. Each phase names the minimum independence:
   - `independent_tool`: a checker not written by the design author (TLC, fuzzers, sanitizers);
   - `independent_reviewer`: a named reviewer other than the author signs off in an ADR or review record (for pre-engineering phases a different-vendor AI agent counts, with a record in `verification/reviews/`: ADR-0117);
   - `independent_implementation`: a cleanly separated second implementation/model in a different language/runtime or by a different author agrees; organizational/vendor independence is required only when the gate also names `independent_reviewer`;
   - `upstream_kat`: official known-answer tests from the standard's publisher;
   - `external_review`: an external security/crypto review organization or qualified outside reviewer;
   - `participant_study` / `independent_facilitator`: moderated sessions with real target users, facilitated by someone other than the flow's designer.
3. **Two phases.** `pre_engineering_status` gates scope authorization; overall `status` = passed requires both phases and gates public releases/claims.
4. **Cross-wave feasibility (ADR-0113/0134).** A gate with `interface_impact` must reach `feasibility_status: evidenced` before the listed scopes can be authorized. Only that gate's explicit `feasibility_criteria` are required for this status; full later-wave gate passage is not.
5. **No-bounce execution (ADR-0134).** The verification executor completes all repo-local/non-review evidence first and batches independent-review-only items at the end. Verification prototypes under `verification/` are evidence, not product engineering.
6. **Waivers** (`waived_by_adr`) must name the ADR, the risk accepted and the compensating control.

## Summary

| Gate | Name | Scopes | Pre-engineering | Overall | Feasibility | W0 interface impact |
|---|---|---|---|---|---|---|
| V-001 | `production_crypto_path` | core_kernel | open | open | open | — |
| V-002 | `account_control_formal_model` | core_kernel | open | open | open | — |
| V-003 | `one_to_one_session_model` | core_kernel | open | open | open | — |
| V-004 | `group_commit_control_model` | groups | open | open | evidenced | core_kernel |
| V-005 | `canonical_encoding_vectors` | core_kernel | passed | open | open | — |
| V-006 | `web_device_vault_feasibility` | reference_client_ui | open | open | open | — |
| V-007 | `sframe_interop` | calls | open | open | open | — |
| V-008 | `space_room_permission_authority_model` | communities_publications | open | open | evidenced | core_kernel |
| V-009 | `baseline_event_materialization_convergence` | groups | open | open | evidenced | core_kernel |
| V-010 | `plugin_sandbox_capability_model` | plugins | open | open | open | — |
| V-011 | `service_principal_authority_and_automation_model` | services_bots | open | open | evidenced | core_kernel |
| V-012 | `expressive_media_and_pack_parser_safety` | expressive_media | open | open | open | — |
| V-013 | `bridge_provenance_fidelity_and_loop_model` | bridges | open | open | evidenced | core_kernel |
| V-014 | `portable_archive_interop_and_import_safety` | portability | open | open | evidenced | core_kernel |
| V-015 | `metadata_exposure_and_privacy_path_model` | core_kernel, provider_reference | open | open | open | — |
| V-016 | `decentralized_abuse_moderation_authority_model` | core_kernel, communities_publications | open | open | open | — |
| V-017 | `protocol_longevity_and_0m_continuity_drill` | none | open | open | open | — |
| V-018 | `security_ux_comprehension` | reference_client_ui | open | open | open | — |
| V-019 | `pq_group_reinit_migration` | groups | open | open | evidenced | core_kernel |
| V-020 | `home_onboarding_and_provider_portability` | provider_reference | open | open | open | — |
| V-021 | `endpoint_coercion_and_platform_leakage` | reference_client_ui | open | open | open | — |
| V-022 | `performance_budgets` | reference_client_ui, provider_reference | open | open | open | — |
| V-023 | `clienthost_update_lifecycle` | reference_client_ui | open | open | open | — |

## Gates

### V-001 — `production_crypto_path`

Demonstrate the selected OM-Crypto-v1 state-machine path, PQXDH + Double Ratchet (ADR-0118; no PQ post-compromise healing in v1, SPQR is OM-Crypto-v2), in Rust on native and wasm32, including state persistence/reload, deterministic vectors, one-time key material and dependency/licensing/update strategy, without relying on an unsupported external API contract.

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_implementation_or_upstream_kat; pre-release external_review
- **Execution class:** `repo_local_feasibility`; next executor role `verification_executor`
- **Pre-engineering independence status:** `satisfied`
- **Evidence:** `decisions/ADR-0118-production-crypto-dependency-licensing-and-update-strategy.md`; `decisions/ADR-0135-om-crypto-v1-construction-pinning.md`; `decisions/ADR-0136-core-kernel-review-remediation.md`; `decisions/ADR-0140-async-key-bundle-v1-privacy-freshness-and-context-binding.md`; `decisions/ADR-0142-sealed-delivery-v1-for-established-one-to-one.md`; `docs/reports/2026-09-26-security-equivalence-evidence-report.md`; `verification/bounded/async_prekey_adr0140_rules.py`; `verification/bounded/relationship_context_surface.py`; `verification/bounded/sealed_delivery_adr0142_rules.py`; `verification/prototypes/v001-crypto/README.md`; `verification/prototypes/v001-crypto/python/hpke_pq.py`; `verification/prototypes/v001-crypto/vectors/om-v001-proto-vectors.json`; `verification/results/async-prekey-adr0140-latest.json`; `verification/results/relationship-context-surface-latest.json`; `verification/results/sealed-delivery-adr0142-latest.json`; `verification/results/v001-crypto-latest.json`; `verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md`; `verification/reviews/2026-09-27-gpt-5-6-sol-v001-f9-re-review.md`

Pre-engineering criteria:

- [x] isolated verification prototype (not product code) of the OM-Crypto-v1 PQXDH + Double Ratchet state machine (ADR-0118; SPQR is v2) runs on native and wasm32 with state persistence/reload
- [x] deterministic vectors produced by the prototype are consumed by a second independent implementation or checked against upstream KATs
- [x] one-time key material handling and depletion behavior demonstrated without downgrade
- [x] dependency, licensing and update strategy recorded in an ADR
- [x] no reliance on an unsupported external library API contract
- [x] PQXDH DeviceIdentity binding vectors require both Ed25519 and ML-DSA-65 over the PQXDH identity/prekey binding and reject either-signature substitution (ADR-0131 rule 1)
- [x] last-resort PQXDH replay prototype persists an exact per-signed-prekey transcript ledger before acknowledgement, rejects duplicates across crash, and fails closed at the 4096 cap without eviction (ADR-0131 rule 2)
- [x] AES-256-GCM-SIV payload vectors verify the OM-DR-AEAD-KEY-v1 / OM-DR-AEAD-NONCE-v1 derivation and authenticated header context (ADR-0131 rule 3)
- [x] MessageRequest HPKE consumes draft-ietf-hpke-pq-05 MLKEM768-X25519/HKDF-SHA256/ChaCha20Poly1305 KATs plus OM context vectors; no ad-hoc hybrid implementation (ADR-0131 rule 9)
- [x] responder binds the inbound PQXDH initiator IK_A to exactly one active, non-quarantined DeviceCredential committed by the trusted relationship ACS head before any secret-key operation, with negative vectors for unknown, route-mismatched, revoked, quarantined, stale, substituted and ambiguous keys checked by the independent implementation (ADR-0136 rule 4, review F-9)
- [x] ADR-0135 §7 / ADR-0136 rule 4 responder binding accepted on re-review by a non-Anthropic reviewer (review F-9 reopened V-001)
- [ ] ADR-0140 AsyncKeyBundle v1 prototype persists a recipient high-water mark across reload/provider migration, rejects lower-generation replay and equal-generation core equivocation, and rejects one-time-prekey transplantation across device/profile/generation/provider-pool context
- [ ] SealedDeliveryEnvelope HPKE consumes the OM-Crypto-v1 pinned draft-ietf-hpke-pq-05 suite under ADR-0142 route-bound info/AAD and padding buckets, rejects wrong route/key/expiry, and never treats outer open as sender authentication

Pre-release criteria:

- [ ] independent cryptographic review of the integrated production path
- [ ] production build provably excludes dev/test CryptoProvider (R-072)

Mechanically unfinished pre-engineering work:

- ADR-0140 AsyncKeyBundle v1 prototype persists a recipient high-water mark across reload/provider migration, rejects lower-generation replay and equal-generation core equivocation, and rejects one-time-prekey transplantation across device/profile/generation/provider-pool context
- SealedDeliveryEnvelope HPKE consumes the OM-Crypto-v1 pinned draft-ietf-hpke-pq-05 suite under ADR-0142 route-bound info/AAD and padding buckets, rejects wrong route/key/expiry, and never treats outer open as sender authentication

### V-002 — `account_control_formal_model`

Model-check AccountControlState majority authorization, quarantine denominator, recovery, fork/freeze and rollback invariants; derive regression scenarios and golden vectors. Includes terminal AccountRetirement, LegacyPolicy contest/cancel and the non-silent device-link invariant (2026-09-24).

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release external_review
- **Execution class:** `repo_local_formal`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0136-core-kernel-review-remediation.md`; `verification/bounded/acs_authorization_table.py`; `verification/bounded/acs_tla_crosscheck.py`; `verification/bounded/continuity_assessment.py`; `verification/results/acs-authorization-table-latest.json`; `verification/results/acs-tla-crosscheck-latest.json`; `verification/results/authority-bounded-latest.json`; `verification/results/tlc-latest.json`; `verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md`; `verification/tla/AccountControl.tla`; `verification/tla/AccountControlState.cfg`; `verification/tla/AccountControlState.tla`; `verification/tla/RecoveryObservation.tla`; `verification/tla/models-v002-recovery-observation.json`; `verification/tla/models.json`; `verification/vectors/account-control-regression-vectors.json`; `verification/vectors/acs-authorization-table-vectors.json`; `verification/vectors/tla-AccountControl_mut_method_count.json`; `verification/vectors/tla-AccountControl_trap_challenge_resolve.json`; `verification/vectors/tla-AccountControl_trap_lift.json`; `verification/vectors/tla-AccountControl_trap_quarantine_freeze.json`; `verification/vectors/tla-AccountControl_trap_recover.json`; `verification/vectors/tla-RecoveryObservation_mut_trust_timestamp.json`; `verification/vectors/tla-RecoveryObservation_residual_withheld.json`; `verification/vectors/tla-RecoveryObservation_trap_late.json`; `docs/reports/2026-09-26-security-equivalence-evidence-report.md`; `decisions/ADR-0141-recovery-freshness-capability-hygiene-and-spec-consistency.md`; `verification/bounded/design_consistency_adr0141_rules.py`; `verification/results/design-consistency-adr0141-latest.json`

Pre-engineering criteria:

- [x] TLC (pinned tool) run of AccountControl.tla (formerly AccountControlState.tla) with recorded state counts and no counterexample
- [x] model covers recovery, fork/freeze, rollback, AccountRetirement terminality, LegacyPolicy contest/cancel and the device-link notice invariant
- [x] regression/golden vectors derived from the model
- [ ] model reviewed by someone other than its author
- [x] model covers ADR-0111 default recovery: delayed activation, veto by any live controller, immediate completion only with controller co-sign, RecoveryRoot as exactly one vote, revocation of all earlier controllers on completion
- [x] withheld RecoveryRequest trace: a verifier that first sees a recovery-alone transition late waits its own full policy interval; Home/custodian timestamps cannot accelerate acceptance (R-278, ADR-0131 rule 6)
- [x] model mirrors the ACCOUNT_CONTINUITY_AND_CONTROL §4.1 authorization table and vectors cover every transition class
- [x] successor pointer without constitutional authority is rejected even when syntactically valid (R-201)
- [x] recovery-vote retirement/revocation obeys delay+veto, and fork bridging requires om.acs.fork_ack with device-only majority (R-200, R-205)

Pre-release criteria:

- [ ] implementation CI replays every vector (R-070)
- [ ] independent security review of the AccountControlState implementation

Mechanically unfinished pre-engineering work:

- model reviewed by someone other than its author
- phase independence evidence is not yet recorded as satisfied

### V-003 — `one_to_one_session_model`

Model/fuzz the per-device 1:1 session manager across crossed initiation, add/revoke races, stale/reset sessions, dedupe and persistence.

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Execution class:** `repo_local_formal`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0120-one-to-one-session-manager-rules.md`; `decisions/ADR-0136-core-kernel-review-remediation.md`; `verification/prototypes/v003-sessions/README.md`; `verification/prototypes/v003-sessions/floor.py`; `verification/prototypes/v003-sessions/restore_redelivery.py`; `verification/results/v003-sessions-latest.json`; `verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md`; `verification/tla/OneToOneSessions.tla`; `verification/tla/models-v003.json`; `verification/vectors/one-to-one-floor-vectors.json`; `verification/vectors/one-to-one-session-regression-vectors.json`; `verification/vectors/tla-OneToOneSessions_K12.json`; `spec/protocol/one-to-one-session-state.logical.schema.json`; `verification/bounded/async_prekey_adr0140_rules.py`; `verification/results/async-prekey-adr0140-latest.json`; `decisions/ADR-0142-sealed-delivery-v1-for-established-one-to-one.md`; `verification/bounded/sealed_delivery_adr0142_rules.py`; `verification/results/sealed-delivery-adr0142-latest.json`; `decisions/ADR-0143-crash-safe-sealed-delivery-replay-and-retry.md`; `verification/prototypes/v003-sessions/sealed_delivery_state.py`; `verification/results/v003-sealed-delivery-latest.json`; `verification/vectors/sealed-delivery-session-vectors.json`

Pre-engineering criteria:

- [x] executable model/fuzzer of the per-device 1:1 session manager covering crossed initiation, add/revoke races, stale/reset sessions, dedupe and persistence
- [x] every discovered failing trace captured as a vector
- [ ] model reviewed by someone other than its author
- [x] restore+redelivery is covered by a dedicated executable scenario or an explicitly equivalent bounded trace, not only an informal argument
- [x] negative vector rejects a peer profile below the current security_generation and rejects a floor move to a lower generation after a SecurityAdvisory (R-206)
- [x] ADR-0140 recipient prekey freshness state merges monotonically across private multi-device state/restore: higher generation wins, equal generation with a different generation-core hash is a security conflict, and provider migration never resets the floor (R-298, R-299)
- [x] session state enforces per-device/per-route sealed-delivery key refresh/revocation, durable (delivery_key_id, delivery_id) replay reservation before plaintext release, same-route exact-wrapper retry semantics and no clear-sender fallback (ADR-0142)

Pre-release criteria:

- [ ] implementation fuzzing for an agreed CPU-hour budget with no unresolved crash or invariant violation

Mechanically unfinished pre-engineering work:

- model reviewed by someone other than its author
- phase independence evidence is not yet recorded as satisfied

### V-004 — `group_commit_control_model`

Formally exercise CommitCoordinator replacement/equivocation and GroupControlIntent <-> GroupCommitBinding crash/partition semantics so same-epoch races cannot silently fork authority.

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`, feasibility `evidenced`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release independent_implementation
- **Execution class:** `repo_local_formal_and_runtime_feasibility`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `verification/bounded/cross_wave_feasibility.py`; `verification/results/authority-bounded-latest.json`; `verification/results/cross-wave-feasibility-latest.json`

Pre-engineering criteria:

- [ ] formal model of CommitCoordinator replacement/equivocation and GroupControlIntent<->GroupCommitBinding under crash/partition with no silent authority fork
- [ ] candidate MLS library demonstrated to support the required external commit ordering on target runtimes
- [ ] reductions (revoked/quarantined device removal, member removal, self-leave, service/bridge removal, Space-delegated reductions) are live under coordinator failure or censorship; succession replacement is deterministic; a rebased non-reducing intent is re-validated at its new parent (R-220)
- [ ] account-control events: no sending after a learned revocation until the Remove is accepted, no adds under CONTROL_CONFLICT/SAFETY_FREEZE, device-add deduplication (R-221)
- [ ] two disconnected partitions independently create different direct reducing Commits from the same MLS epoch after sequencing_timeout; reconnection converges without silent key/state continuation on the losing branch
- [ ] stale reduction targets an immutable membership/service incarnation and becomes a no-op after that exact incarnation is gone; it cannot remove a later re-add (R-279)

Cross-wave feasibility criteria (only these are required for `feasibility_status=evidenced`):

- [x] existing W0 principal/event/control envelopes can represent GroupControlIntent, GroupCommitBinding, coordinator replacement and immutable-incarnation reductions without adding a new W0 signed-object primitive
- [x] a bounded crash/partition sketch demonstrates that same-epoch competing group commits can be detected and resolved without changing ConversationID or AccountControlState semantics

Pre-release criteria:

- [ ] multi-client partition/crash integration harness passes on two implementations or two runtimes

Mechanically unfinished pre-engineering work:

- formal model of CommitCoordinator replacement/equivocation and GroupControlIntent<->GroupCommitBinding under crash/partition with no silent authority fork
- candidate MLS library demonstrated to support the required external commit ordering on target runtimes
- reductions (revoked/quarantined device removal, member removal, self-leave, service/bridge removal, Space-delegated reductions) are live under coordinator failure or censorship; succession replacement is deterministic; a rebased non-reducing intent is re-validated at its new parent (R-220)
- account-control events: no sending after a learned revocation until the Remove is accepted, no adds under CONTROL_CONFLICT/SAFETY_FREEZE, device-add deduplication (R-221)
- two disconnected partitions independently create different direct reducing Commits from the same MLS epoch after sequencing_timeout; reconnection converges without silent key/state continuation on the losing branch
- stale reduction targets an immutable membership/service incarnation and becomes a no-op after that exact incarnation is gone; it cannot remove a later re-add (R-279)
- phase independence evidence is not yet recorded as satisfied

### V-005 — `canonical_encoding_vectors`

Publish deterministic-CBOR/CDDL golden vectors produced and consumed correctly by at least two independent implementations/encoders for every signed/hashed v1 object.

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `passed`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_implementation; pre-release independent_implementation
- **Execution class:** `repo_local_interop`; next executor role `verification_executor`
- **Pre-engineering independence status:** `satisfied`
- **Evidence:** `decisions/ADR-0119-freeze-w0-cddl-and-registries.md`; `decisions/ADR-0140-async-key-bundle-v1-privacy-freshness-and-context-binding.md`; `decisions/ADR-0141-recovery-freshness-capability-hygiene-and-spec-consistency.md`; `decisions/ADR-0142-sealed-delivery-v1-for-established-one-to-one.md`; `spec/cddl/`; `spec/registries/`; `verification/bounded/async_prekey_adr0140_rules.py`; `verification/bounded/design_consistency_adr0141_rules.py`; `verification/bounded/sealed_delivery_adr0142_rules.py`; `verification/cbor/node_intro_binding.mjs`; `verification/prototypes/v005-encoding/README.md`; `verification/prototypes/v005-encoding/gen/assemble.py`; `verification/prototypes/v005-encoding/gen/intro_binding.py`; `verification/prototypes/v005-encoding/run.sh`; `verification/results/async-prekey-adr0140-latest.json`; `verification/results/design-consistency-adr0141-latest.json`; `verification/results/sealed-delivery-adr0142-latest.json`; `verification/results/v005-encoding-latest.json`; `verification/results/v005-node-adr0141-latest.json`; `verification/results/v005-node-adr0142-latest.json`; `verification/vectors/v005-w0-object-vectors.json.gz`

Pre-engineering criteria:

- [x] CDDL, numeric-field and domain-separation registries frozen for every signed/hashed object used by the scope being unlocked (W0 first)
- [x] two independently written encoders/decoders agree on positive and negative object vectors for those objects
- [x] reciprocal IntroIdentityBinding positive/negative vectors encode ACCEPTED_UNBOUND -> ACTIVE only after both stable-account bindings verify (R-280)
- [x] MessageRequest HPKE vectors pin the exact draft-ietf-hpke-pq-05 suite/version named by OM-Crypto-v1 (R-281)
- [x] positive/negative vectors and both codecs cover MessageRequest, MessageRequestPayload, IntroIdentityBinding, RequestKey, ForkAck/om.acs.fork_ack, ContactDescriptor key 15 and IntroductionTicket key 11; AsyncKeyBundle without required key 8 is rejected
- [x] a later-wave rich-text node or attachment field inside a W0 body is rejected, while the same semantic content carried in a named non-critical extension beside a W0-valid body is accepted (R-237)
- [x] AsyncKeyBundle v1 vectors reject retired keys 3/4 and derive one-time-prekey signature input from DeviceID, CryptoProfile, generation, provider_pool_id, kind, key_id and public_key (ADR-0140)
- [x] canonical vectors and two independent strict codecs cover DeliveryKeyMaterial, SealedDeliveryKeyRecord, SealedDeliveryEnvelope and required MessageRequestPayload.reply_delivery_key, including OM-Crypto-v1 HPKE exact-length/padding-bucket negatives (ADR-0142)

Pre-release criteria:

- [ ] every conforming implementation in the release passes the complete vector suite

### V-006 — `web_device_vault_feasibility`

Prove the WebAuthn-PRF/non-extractable-WebCrypto DeviceVault hierarchy and migration behavior on the browser support matrix intended for the first reference PWA.

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Execution class:** `platform_empirical_feasibility`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0141-recovery-freshness-capability-hygiene-and-spec-consistency.md`; `verification/bounded/design_consistency_adr0141_rules.py`; `verification/results/design-consistency-adr0141-latest.json`

Pre-engineering criteria:

- [ ] browser-matrix prototype of the WebDeviceVault wrapping hierarchy (WebAuthn PRF, non-extractable WebCrypto, passphrase fallback) with new-origin enrollment migration
- [ ] documented behavior for every unsupported/failed capability on the intended first-release browser matrix
- [ ] PRF on synced (platform/password-manager) passkeys and signalCurrentUserDetails support recorded per browser/platform for the ADR-0111 recovery custodian, with the recovery-key fallback exercised where unsupported
- [ ] per browser/platform: availability of a device-bound (non-synced) WebAuthn credential for vault wrapping, and the separate-credential/separate-PRF-salt path from the RecoveryRoot exercised; reduced at-rest property shown where only a synced passkey exists (ADR-0129 rule 15)
- [ ] loss of the original PWA RP/ClientHost: passkey-only recovery is shown distribution-bound, and an origin-independent recovery path can enroll a fresh DeviceID at an unrelated origin/native client

Pre-release criteria:

- [ ] published support matrix with regression tests on each supported browser

Mechanically unfinished pre-engineering work:

- browser-matrix prototype of the WebDeviceVault wrapping hierarchy (WebAuthn PRF, non-extractable WebCrypto, passphrase fallback) with new-origin enrollment migration
- documented behavior for every unsupported/failed capability on the intended first-release browser matrix
- PRF on synced (platform/password-manager) passkeys and signalCurrentUserDetails support recorded per browser/platform for the ADR-0111 recovery custodian, with the recovery-key fallback exercised where unsupported
- per browser/platform: availability of a device-bound (non-synced) WebAuthn credential for vault wrapping, and the separate-credential/separate-PRF-salt path from the RecoveryRoot exercised; reduced at-rest property shown where only a synced passkey exists (ADR-0129 rule 15)
- loss of the original PWA RP/ClientHost: passkey-only recovery is shown distribution-bound, and an origin-independent recovery path can enroll a fresh DeviceID at an unrelated origin/native client
- phase independence evidence is not yet recorded as satisfied

### V-007 — `sframe_interop`

Prove the selected SFrame/WebRTC key schedule and counter/replay rules across required browsers/native runtimes, SFU forwarding, reconnect, simulcast/SVC where claimed, and membership rekey.

- **Scopes blocked:** calls
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Execution class:** `platform_empirical_feasibility`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`

Pre-engineering criteria:

- [ ] SFrame key schedule and counter/replay prototype interoperates across at least two browser engines and one native runtime through an SFU with reconnect and membership rekey
- [ ] call.offer from a principal without an accepted relationship causes no ringing, ICE gathering or TURN allocation; mesh uses relay-only candidates toward non-contacts (R-234)
- [ ] a non-joined Room member, service or bridge cannot decrypt call media merely by holding the Room MLS epoch; silent-SFU-listener trace fails closed (R-283)

Pre-release criteria:

- [ ] full claimed matrix including simulcast/SVC where claimed

Mechanically unfinished pre-engineering work:

- SFrame key schedule and counter/replay prototype interoperates across at least two browser engines and one native runtime through an SFU with reconnect and membership rekey
- call.offer from a principal without an accepted relationship causes no ringing, ICE gathering or TURN allocation; mesh uses relay-only candidates toward non-contacts (R-234)
- a non-joined Room member, service or bridge cannot decrypt call media merely by holding the Room MLS epoch; silent-SFU-listener trace fails closed (R-283)
- phase independence evidence is not yet recorded as satisfied

### V-008 — `space_room_permission_authority_model`

Model-check Room/Space capability grants, explicit restrictions, delegation ceilings, governance quorum, bilateral links and authority-parent propagation/revocation so no role/Space transition can silently self-escalate, create transitive authority, or bypass Room-local security state.

- **Scopes blocked:** communities_publications
- **Status:** pre-engineering `open`, overall `open`, feasibility `evidenced`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release independent_implementation
- **Execution class:** `repo_local_formal`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md`; `verification/bounded/cross_wave_feasibility.py`; `verification/results/authority-bounded-latest.json`; `verification/results/cross-wave-feasibility-latest.json`; `verification/results/v008-permissions-latest.json`; `verification/tla/ScopedPermissions.tla`; `verification/tla/models-v008-permissions.json`

Pre-engineering criteria:

- [ ] formal model of Room/Space grants, restrictions, ceilings, governance quorum, bilateral links and revocation races with no self-escalation or transitive-authority counterexample
- [ ] grant/restriction reductions target immutable grant/decision incarnation ids; delayed revoke cannot remove a later re-grant with the same semantic capability tuple (R-279)
- [ ] role-edit and assignment ceilings reject any role/grant whose effective or grantable scope exceeds the grantor's current ceiling (R-229)
- [ ] SpaceDelegatedIntent committed by a non-privileged member is rejected; stale Space heads fail closed; Space reductions remain live (R-230)
- [ ] invite links lapse when the issuer loses room.invite, and no governance/binding path can change CryptoProfile (R-225, R-227)

Cross-wave feasibility criteria (only these are required for `feasibility_status=evidenced`):

- [x] existing Principal, CapabilityGrant/Restriction and ConversationEvent authority-parent extension points can encode Room/Space roles, delegated intent and grant ceilings without changing W0 object identity or signature domains

Pre-release criteria:

- [ ] implementation replays model-derived vectors

Mechanically unfinished pre-engineering work:

- formal model of Room/Space grants, restrictions, ceilings, governance quorum, bilateral links and revocation races with no self-escalation or transitive-authority counterexample
- grant/restriction reductions target immutable grant/decision incarnation ids; delayed revoke cannot remove a later re-grant with the same semantic capability tuple (R-279)
- role-edit and assignment ceilings reject any role/grant whose effective or grantable scope exceeds the grantor's current ceiling (R-229)
- SpaceDelegatedIntent committed by a non-privileged member is rejected; stale Space heads fail closed; Space reductions remain live (R-230)
- invite links lapse when the issuer loses room.invite, and no governance/binding path can change CryptoProfile (R-225, R-227)
- phase independence evidence is not yet recorded as satisfied

### V-009 — `baseline_event_materialization_convergence`

Property-test/model the expanded baseline ConversationEvent materialization under duplicate, delayed, reordered and concurrent delivery for shared pins, moderator redaction, polls, threads/topics and message-topic moves, proving deterministic views and authorization handling without treating wall-clock time as security authority.

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`, feasibility `evidenced`
- **Independence:** pre-engineering independent_implementation; pre-release independent_implementation
- **Execution class:** `repo_local_interop`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0121-event-ordering-authorization-epoch-and-materialization.md`; `verification/bounded/cross_wave_feasibility.py`; `verification/prototypes/v009-materializer/README.md`; `verification/results/conversation-convergence-latest.json`; `verification/results/cross-wave-feasibility-latest.json`; `verification/results/v009-materializer-latest.json`

Pre-engineering criteria:

- [ ] a second, independently written materializer (different author or language) agrees with the reference model on the full convergence corpus including malformed and authorization-race cases
- [ ] the independent materializer derives authorization from the control chain rather than sharing the reference authorization adapter (G-V9-5)
- [ ] reaction and vote registers are authorization-epoch-major and a vote ordered after poll.close is ignored (R-236)
- [ ] expiry deletes at min(expiry, receipt + expires_after) and never after the absolute expiry bound (R-235)
- [ ] forwarding strips mentions and a mention of a non-member renders as inert text (R-239)

Cross-wave feasibility criteria (only these are required for `feasibility_status=evidenced`):

- [x] the W0 ConversationEvent envelope, event identity, authorization-parent reference and extension mechanism can carry later pins/polls/topics/moves/materialization semantics without changing W0 canonical object layout

Pre-release criteria:

- [ ] every conforming implementation passes the corpus

Mechanically unfinished pre-engineering work:

- a second, independently written materializer (different author or language) agrees with the reference model on the full convergence corpus including malformed and authorization-race cases
- the independent materializer derives authorization from the control chain rather than sharing the reference authorization adapter (G-V9-5)
- reaction and vote registers are authorization-epoch-major and a vote ordered after poll.close is ignored (R-236)
- expiry deletes at min(expiry, receipt + expires_after) and never after the absolute expiry bound (R-235)
- forwarding strips mentions and a mention of a non-member renders as inert text (R-239)
- phase independence evidence is not yet recorded as satisfied

### V-010 — `plugin_sandbox_capability_model`

Prototype and adversarially test the OM WebAssembly Component Model/WIT plugin host on intended native and PWA runtimes, proving no ambient filesystem/network/process/environment/secret-store/key/DOM authority, explicit permission-upgrade prompts, plaintext-read vs network-egress separation, draft vs send separation, crash isolation and enforceable resource budgets.

- **Scopes blocked:** plugins
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release external_review
- **Execution class:** `platform_empirical_feasibility`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`

Pre-engineering criteria:

- [ ] prototype host on a native Wasm runtime and the PWA runtime; adversarial plugins demonstrate no ambient authority, enforced budgets, permission-upgrade prompts and plaintext-read vs egress separation
- [ ] adversarial plugin with content.read_current_conversation and net.fetch_origin_scoped in a conversation without plaintext_egress is blocked from egress, including via plugin private storage (ADR-0129 rule 2)
- [ ] plugin cannot emit an Account-authored event without per-send user confirmation; autonomous sending only as a local_component ServicePrincipal (ADR-0129 rule 1)
- [ ] PWA: plugins and mini-apps run on a separate registrable domain with cross-origin isolation and exact-origin message binding; content.* plugins disabled without site isolation; mini-app cannot occlude host chrome or render OM-like security prompts (ADR-0129 rule 8)
- [ ] remote-origin mini-app test treats plaintext postMessage/disclosure itself as egress; no claim that host fetch-cap denial contains plaintext after crossing origin (R-282)
- [ ] continuous 1:1 plaintext egress requires valid current RelationshipPolicyConsent from both participants; unilateral revoke or stale consent blocks egress (R-282)

Pre-release criteria:

- [ ] external review of the sandbox host

Mechanically unfinished pre-engineering work:

- prototype host on a native Wasm runtime and the PWA runtime; adversarial plugins demonstrate no ambient authority, enforced budgets, permission-upgrade prompts and plaintext-read vs egress separation
- adversarial plugin with content.read_current_conversation and net.fetch_origin_scoped in a conversation without plaintext_egress is blocked from egress, including via plugin private storage (ADR-0129 rule 2)
- plugin cannot emit an Account-authored event without per-send user confirmation; autonomous sending only as a local_component ServicePrincipal (ADR-0129 rule 1)
- PWA: plugins and mini-apps run on a separate registrable domain with cross-origin isolation and exact-origin message binding; content.* plugins disabled without site isolation; mini-app cannot occlude host chrome or render OM-like security prompts (ADR-0129 rule 8)
- remote-origin mini-app test treats plaintext postMessage/disclosure itself as egress; no claim that host fetch-cap denial contains plaintext after crossing origin (R-282)
- continuous 1:1 plaintext egress requires valid current RelationshipPolicyConsent from both participants; unilateral revoke or stale consent blocks egress (R-282)
- phase independence evidence is not yet recorded as satisfied

### V-011 — `service_principal_authority_and_automation_model`

Model/property-test permissionless ServiceGenesis, ServicePrincipal endpoint/control authorization (including no-self-bootstrap), host migration/revocation, alias/catalog replacement, RepresentationGrant impersonation boundaries, Account-confirmed vs autonomous authorship, installation/data-access modes, full-Room admission/revocation, local/remote LLM tool capabilities, cheap-identity/no-unsolicited-reach behavior, and automation lineage/hop/rate rules so no service can self-escalate, become a hidden plaintext recipient, impersonate an Account, or create unbounded bot-to-bot loops.

- **Scopes blocked:** services_bots
- **Status:** pre-engineering `open`, overall `open`, feasibility `evidenced`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Execution class:** `repo_local_formal`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md`; `verification/bounded/cross_wave_feasibility.py`; `verification/results/authority-bounded-latest.json`; `verification/results/cross-wave-feasibility-latest.json`; `verification/results/v011-services-latest.json`; `verification/tla/ServiceAuthority.tla`; `verification/tla/models-v011.json`

Pre-engineering criteria:

- [ ] formal/bounded model extended to host migration/revocation and alias replacement
- [ ] tool-capability prototype shows injected prompts cannot exceed granted capabilities or create unbounded automation loops
- [ ] model covers the two service positions (invocation-only, full member) and shows a trigger filter never narrows what a full member can read (ADR-0129 rule 3)
- [ ] RepresentationGrant issued by a messaging-only device is rejected; grant whose issuing control device is quarantined stops validating; counterparty-recipient grant without counterparty notice fails (ADR-0129 rules 4-5)
- [ ] revoked service endpoint is removed from every Room by any member after sequencing_timeout when the coordinator does not; removal rebases over forks (ADR-0129 rule 6)
- [ ] service endpoint/install removals target immutable endpoint/service-installation incarnations; stale removal cannot remove a later reinstallation (R-279)

Cross-wave feasibility criteria (only these are required for `feasibility_status=evidenced`):

- [x] ServicePrincipal and service-installation/endpoint incarnations fit the frozen Principal/capability/event model without adding a privileged authority class or changing W0 signatures

Pre-release criteria:

- [ ] implementation tests replay model vectors and injection corpus

Mechanically unfinished pre-engineering work:

- formal/bounded model extended to host migration/revocation and alias replacement
- tool-capability prototype shows injected prompts cannot exceed granted capabilities or create unbounded automation loops
- model covers the two service positions (invocation-only, full member) and shows a trigger filter never narrows what a full member can read (ADR-0129 rule 3)
- RepresentationGrant issued by a messaging-only device is rejected; grant whose issuing control device is quarantined stops validating; counterparty-recipient grant without counterparty notice fails (ADR-0129 rules 4-5)
- revoked service endpoint is removed from every Room by any member after sequencing_timeout when the coordinator does not; removal rebases over forks (ADR-0129 rule 6)
- service endpoint/install removals target immutable endpoint/service-installation incarnations; stale removal cannot remove a later reinstallation (R-279)
- phase independence evidence is not yet recorded as satisfied

### V-012 — `expressive_media_and_pack_parser_safety`

Fuzz and cross-implementation test expressive-media resource ceilings, decoder/pixel-frame budgets, ContentPack canonical manifests/signatures/update/fork lineage and malformed metadata/blob references so hostile packs/media fail safely without code execution, unbounded resource use, catalog dependence or signature ambiguity.

- **Scopes blocked:** expressive_media
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_tool, independent_implementation; pre-release independent_tool
- **Execution class:** `repo_local_fuzz_and_interop`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`

Pre-engineering criteria:

- [ ] candidate decoders and pack parsers fuzzed for an agreed CPU-hour budget under resource ceilings with no unresolved crash
- [ ] canonical ContentPack manifest/signature vectors agree across two implementations

Pre-release criteria:

- [ ] shipped parsers under continuous fuzzing

Mechanically unfinished pre-engineering work:

- candidate decoders and pack parsers fuzzed for an agreed CPU-hour budget under resource ceilings with no unresolved crash
- canonical ContentPack manifest/signature vectors agree across two implementations
- phase independence evidence is not yet recorded as satisfied

### V-013 — `bridge_provenance_fidelity_and_loop_model`

Adversarially model/test BridgePrincipal control and Room admission/removal, foreign provenance/identity-link spoofing, cross-network loop/reflection and duplicate suppression across multiple bridges, edit/delete/reaction/topic/poll target mapping, capability/fidelity-loss signaling, E2EE-boundary state and origin-fingerprint metadata exposure.

- **Scopes blocked:** bridges
- **Status:** pre-engineering `open`, overall `open`, feasibility `evidenced`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Execution class:** `repo_local_model`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0124-service-information-flow-and-bridge-provenance.md`; `verification/bounded/cross_wave_feasibility.py`; `verification/prototypes/v013-bridges/BRIDGE_ADAPTER_CONTRACT.md`; `verification/results/cross-wave-feasibility-latest.json`; `verification/results/v013-bridges-latest.json`; `verification/results/wire-boundary-bounded-latest.json`

Pre-engineering criteria:

- [ ] reviewed bridge adapter contract specification
- [x] loop/reflection-suppression model with at least two simulated bridges and fidelity-loss signaling
- [ ] bridge adapter contract states bridges are always full Room members and that allowed_event_classes limits export, not reading (ADR-0129 rule 3)

Cross-wave feasibility criteria (only these are required for `feasibility_status=evidenced`):

- [x] bridge provenance, scoped origin fingerprints and loop-suppression metadata fit the frozen ConversationEvent extension/provenance model without changing AccountID, ConversationID or W0 event identity

Pre-release criteria:

- [ ] real adapters and multi-bridge traces meet the model

Mechanically unfinished pre-engineering work:

- reviewed bridge adapter contract specification
- bridge adapter contract states bridges are always full Room members and that allowed_event_classes limits export, not reading (ADR-0129 rule 3)
- phase independence evidence is not yet recorded as satisfied

### V-014 — `portable_archive_interop_and_import_safety`

Round-trip om-archive-v1 across at least two independent implementations and test exact canonical-object preservation, stale control-state non-authority, missing/partial blobs, encrypted-archive corruption/wrong-key behavior, hostile archive paths/decompression/media/foreign formats, fabricated foreign authorship, read-only import attempting live authority, and inability to clone DeviceIdentity/account authority from ordinary history archives.

- **Scopes blocked:** portability
- **Status:** pre-engineering `open`, overall `open`, feasibility `evidenced`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Execution class:** `repo_local_interop`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0125-portable-archive-format-rulings.md`; `verification/bounded/cross_wave_feasibility.py`; `verification/prototypes/v014-archive/OM_ARCHIVE_V1_SPEC.md`; `verification/results/cross-wave-feasibility-latest.json`; `verification/results/v014-archive-latest.json`; `verification/results/wire-boundary-bounded-latest.json`

Pre-engineering criteria:

- [ ] frozen om-archive-v1 specification and vectors
- [x] hostile archive corpus
- [x] one reference reader/writer prototype passes the corpus

Cross-wave feasibility criteria (only these are required for `feasibility_status=evidenced`):

- [x] om-archive-v1 can preserve canonical W0 objects, unknown named extensions and stable event/object identities without introducing a second authority representation or altering W0 encodings

Pre-release criteria:

- [ ] two independent implementations round-trip
- [ ] real foreign importers pass authority/provenance tests

Mechanically unfinished pre-engineering work:

- frozen om-archive-v1 specification and vectors
- phase independence evidence is not yet recorded as satisfied

### V-015 — `metadata_exposure_and_privacy_path_model`

Instrument/model normal and privacy-enhanced paths across relay, MLS/group control, KeyMaterialProvider, discovery/OHTTP split, IntroGate, push, blob, calls, services, bridges and archive migration; prove no accidental canonical AccountID/ConversationID/group-identifier exposure in normal routing, verify per-provider capability/randomized wrapping, validate MetadataExposureDescriptor claims against traces, and document remaining timing/size/IP/collusion leakage.

- **Scopes blocked:** core_kernel, provider_reference
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Execution class:** `repo_local_privacy_model`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0136-core-kernel-review-remediation.md`; `docs/process/METADATA_TRACE_TEST_PLAN.md`; `spec/metadata/w0-metadata-exposure-descriptors.json`; `verification/bounded/metadata_trace_model.py`; `verification/bounded/metadata_w0_rules.py`; `verification/results/metadata-trace-latest.json`; `verification/results/metadata-w0-rules-latest.json`; `verification/results/wire-boundary-bounded-latest.json`; `verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md`; `docs/reports/2026-09-26-security-equivalence-evidence-report.md`; `docs/architecture/providers/METADATA_OBSERVER_MATRIX.md`; `decisions/ADR-0140-async-key-bundle-v1-privacy-freshness-and-context-binding.md`; `verification/results/async-prekey-adr0140-latest.json`; `decisions/ADR-0141-recovery-freshness-capability-hygiene-and-spec-consistency.md`; `verification/bounded/design_consistency_adr0141_rules.py`; `verification/results/design-consistency-adr0141-latest.json`; `decisions/ADR-0142-sealed-delivery-v1-for-established-one-to-one.md`; `docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md`; `verification/bounded/sealed_delivery_adr0142_rules.py`; `verification/results/sealed-delivery-adr0142-latest.json`

Pre-engineering criteria:

- [x] MetadataExposureDescriptor written for every provider role in the scope being unlocked, including the sponsored starter mailbox
- [x] trace-model assertions for each descriptor and a packet-capture test plan
- [x] two IdentityProfiles on one install never present the same push subscription or token to two parties unless the push provider's descriptor declares the linkage (ADR-0129 rule 14)
- [x] shared-blob mode for expressive assets has a BlobStore descriptor line; default per-send encryption yields no shared BlobID across conversations (ADR-0129 rule 10)
- [x] LAN trace: rendezvous only for marked contacts, tags retired on block/relationship end, fixed-cardinality advertisement, epoch rotation with link-layer address where available; the model shows advertisement size and cross-epoch sets reveal neither contact count nor linkage (R-270, R-271)
- [x] descriptors declare roles co-located on one operator's Home as one observer (descriptor_mailbox_linkage, contact_graph_edge) and state no non-collusion between them (R-265)
- [x] OM-Crypto-v1 established-relationship relay trace/model uses SealedDeliveryEnvelope so mailbox-class Homes receive no sender AccountID/DeviceID/identity key/Double Ratchet header/ConversationID; timing, padding bucket, source-network metadata and opaque route/key identifiers remain declared, while first contact remains MessageRequest/IntroGate (ADR-0142)
- [x] first-contact trace proves neither stable AccountID is exposed through IntroGate before acceptance and reciprocal binding occurs only inside accepted relationship (R-280)
- [x] MessageRequest/IntroIdentityBinding trace proves IntroGate sees no sender AccountID, DeviceID or intro private/public identity material beyond the admission surface, and stable AccountID binding occurs only after acceptance (R-202, R-203, R-280)
- [x] KeyMaterialProvider W0 trace uses a per-relationship fetch capability; AsyncKeyBundle v1 exposes no provider-visible ACS generation/hash after ADR-0140, while stable subject_device_id, provider_pool_id, timing and inventory-shape leakage remain declared (R-209, R-291, R-298..R-300)
- [x] group fan-out outer-encrypts every recipient copy; relays never see raw MLS group_id/epoch and group_id is unrelated to ConversationID (R-226)
- [ ] provider-resource administration traces contain no AccountID/DeviceCredential/ACS auth material for routine descriptor/key/relay/recovery-record/blob administration, while RecoveryRequest fan-out explicitly records AccountID + recovery_attempt exposure (R-287, R-294, R-295)

Pre-release criteria:

- [ ] real packet/provider/browser traces for normal and privacy-enhanced paths match every descriptor

Mechanically unfinished pre-engineering work:

- provider-resource administration traces contain no AccountID/DeviceCredential/ACS auth material for routine descriptor/key/relay/recovery-record/blob administration, while RecoveryRequest fan-out explicitly records AccountID + recovery_attempt exposure (R-287, R-294, R-295)
- phase independence evidence is not yet recorded as satisfied

### V-016 — `decentralized_abuse_moderation_authority_model`

Model/property-test block/revocation/Sybil behavior, IntroGate resource abuse, Room/Space timeout-remove-ban authorization and delegation revocation, malicious moderators, selective report evidence, SafetyAdvisory signature/supersession/expiry and scoped subject references, conflicting labelers, provider denial boundaries, service/bridge abuse and fork/escape so no safety mechanism creates hidden universal authority or global identity tracking.

- **Scopes blocked:** core_kernel, communities_publications
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Execution class:** `repo_local_formal`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md`; `decisions/ADR-0136-core-kernel-review-remediation.md`; `verification/bounded/abuse_w0_rules.py`; `verification/bounded/introgate_sybil_model.py`; `verification/bounded/privacy_pass_issuance.py`; `verification/results/abuse-w0-rules-latest.json`; `verification/results/authority-bounded-latest.json`; `verification/results/introgate-sybil-latest.json`; `verification/results/v016-room-moderation-latest.json`; `verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md`; `verification/tla/RoomModeration.tla`; `verification/tla/models-v016-room-moderation.json`; `verification/vectors/privacy-pass-blind-rsa-vectors.json`; `docs/reports/2026-09-26-security-equivalence-evidence-report.md`; `verification/bounded/provider_negative_capabilities.py`; `verification/results/provider-negative-capabilities-latest.json`

Pre-engineering criteria:

- [x] IntroGate/block/Sybil/resource model for the W0 surfaces
- [x] Room timeout/remove/ban race and evidence model before communities_publications is unlocked
- [x] RFC 9578 token-type 0x0002 referral admission: IntroGate verifies with issuer public key but cannot mint; token is single-use, issuer-epoch-bound, and old epoch stops after bounded overlap (ADR-0132 rule 1)
- [x] stale moderation/member reduction cannot target a later incarnation after re-add/regrant (R-279)
- [x] IntroGate model covers referral-token admission into reserved capacity, per-IntroGate max_uses, and MessageRequest size rejection before expensive work (R-210, R-214)
- [x] unsolicited group add without an accepted invitation is rejected (R-224)
- [x] bans and restrictions end only through committed lifts; a ModerationDecision alone changes no Room state (R-231)
- [x] call.offer from a principal without an accepted relationship is dropped before call setup (R-234)
- [x] providers never resolve AccountID-keyed advisories to delivery capabilities and safety-reference v2 encoding is injective (R-238)

Pre-release criteria:

- [ ] adversarial load and abuse tests against reference providers

Mechanically unfinished pre-engineering work:

- phase independence evidence is not yet recorded as satisfied

### V-017 — `protocol_longevity_and_0m_continuity_drill`

Run the 20-year continuity exercise with founder/forge/domain/provider unavailable, independent Protocol Continuity Bundle recovery, immutable registry snapshots, two protocol generations/fork lineages, crypto-profile deprecation and catastrophic-authentication-break tabletop cases, release-root loss/rotation, dominant proprietary extension attempt, provider migrations and independent build/conformance paths; demonstrate no hidden single universal continuity choke point.

- **Scopes blocked:** none (always required pre-engineering; blocks public production claims)
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation, external_review
- **Execution class:** `repo_local_continuity`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `decisions/ADR-0136-core-kernel-review-remediation.md`; `docs/process/CONTINUITY_TABLETOP_DRILLS.md`; `spec/continuity/om-crypto-v1-bundle-manifest.json`; `tools/continuity_bundle.py`; `verification/results/continuity-dry-run-latest.json`; `verification/reviews/2026-09-26-gpt-5-6-sol-core-kernel.md`

Pre-engineering criteria:

- [x] Protocol Continuity Bundle assembly and verification script
- [x] registry-immutability checks
- [x] written tabletop drills: founder/forge/domain loss, crypto primitive break, release-root loss, dominant-extension capture, vendor-wide provider block and vulnerability response

Pre-release criteria:

- [ ] independent rebuild of a compatible implementation from a mirror
- [ ] two coexisting protocol generations/fork lineages
- [ ] live disaster drill with no hidden universal choke point

Mechanically unfinished pre-engineering work:

- phase independence evidence is not yet recorded as satisfied

### V-018 — `security_ux_comprehension`

Demonstrate that non-technical users complete and correctly understand OM's security-critical flows without infrastructure or cryptographic vocabulary.

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering participant_study, independent_facilitator; pre-release participant_study
- **Execution class:** `human_participant_study`; next executor role `human_study_team`
- **Pre-engineering independence status:** `open`
- **Evidence:** `docs/process/SECURITY_UX_STUDY_PROTOCOL.md`

Pre-engineering criteria:

- [ ] moderated sessions with at least 12 non-technical participants (including at least 3 aged 60+ and 3 with low digital literacy, in at least 2 languages) on non-production prototypes
- [ ] flows: onboarding tier (T0/T1), recovery on (where it lives) and turning recovery off, recovery veto notice, continuity check, device-link notice, suspend device, contact continuity change, bridge E2EE boundary, full-Room service disclosure
- [ ] per flow at least 80% task success and correct answer to the flow's security-comprehension question; failures redesigned and retested
- [ ] flows add: counterparty notice for a representing service, Connected services review and revoke, and 'this service can read all Room content' for a full-member service with a trigger filter (ADR-0129)
- [ ] theme-compiler test: a glyph-remapping theme font does not change any reserved surface (ADR-0129 rule 17)
- [ ] onboarding from a sponsored invite: the user chooses native or one of several independent app origins and can say who serves their app (R-260)
- [ ] comprehension: 'your inviter can see when you get messages and who else on their home you talk to, and can close this mailbox after 30 days' notice' is understood; the co-located-home disclosure is understood (R-265, R-267)

Pre-release criteria:

- [ ] repeat on the real client, plus assistive-technology users for accessibility

Mechanically unfinished pre-engineering work:

- moderated sessions with at least 12 non-technical participants (including at least 3 aged 60+ and 3 with low digital literacy, in at least 2 languages) on non-production prototypes
- flows: onboarding tier (T0/T1), recovery on (where it lives) and turning recovery off, recovery veto notice, continuity check, device-link notice, suspend device, contact continuity change, bridge E2EE boundary, full-Room service disclosure
- per flow at least 80% task success and correct answer to the flow's security-comprehension question; failures redesigned and retested
- flows add: counterparty notice for a representing service, Connected services review and revoke, and 'this service can read all Room content' for a full-member service with a trigger filter (ADR-0129)
- theme-compiler test: a glyph-remapping theme font does not change any reserved surface (ADR-0129 rule 17)
- onboarding from a sponsored invite: the user chooses native or one of several independent app origins and can say who serves their app (R-260)
- comprehension: 'your inviter can see when you get messages and who else on their home you talk to, and can close this mailbox after 30 days' notice' is understood; the co-located-home disclosure is understood (R-265, R-267)
- phase independence evidence is not yet recorded as satisfied

### V-019 — `pq_group_reinit_migration`

Prove the v1 group wire can migrate by authenticated ReInit to a hybrid-PQ MLS profile without changing ConversationID or group authority and without downgrade.

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`, feasibility `evidenced`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Execution class:** `repo_local_feasibility`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `verification/bounded/cross_wave_feasibility.py`; `verification/results/cross-wave-feasibility-latest.json`

Pre-engineering criteria:

- [ ] prototype authenticated ReInit from the v1 MLS suite to an experimental hybrid-PQ suite preserving ConversationID and GroupControlState, rejecting downgrade, with vectors

Cross-wave feasibility criteria (only these are required for `feasibility_status=evidenced`):

- [x] CryptoProfile/critical-extension and group ReInit extension points can name a future hybrid-PQ MLS profile while preserving ConversationID and GroupControlState identity; no W0 wire field needs reinterpretation

Pre-release criteria:

- [ ] interoperable migration across two implementations once ADR-0107 triggers are met

Mechanically unfinished pre-engineering work:

- prototype authenticated ReInit from the v1 MLS suite to an experimental hybrid-PQ suite preserving ConversationID and GroupControlState, rejecting downgrade, with vectors
- phase independence evidence is not yet recorded as satisfied

### V-020 — `home_onboarding_and_provider_portability`

Prove zero-signup sponsored onboarding, bounded sponsorship, migration between Home tiers without AccountID change, and survival of a vendor-wide provider block with a second non-cloud reference provider.

- **Scopes blocked:** provider_reference
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Execution class:** `repo_local_provider_model`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md`; `docs/process/CONTINUITY_TABLETOP_DRILLS.md`; `verification/bounded/home_tiers_model.py`; `verification/results/home-tiers-latest.json`; `decisions/ADR-0141-recovery-freshness-capability-hygiene-and-spec-consistency.md`; `verification/bounded/design_consistency_adr0141_rules.py`; `verification/results/design-consistency-adr0141-latest.json`

Pre-engineering criteria:

- [x] provider conformance suite specification
- [x] bounded model of sponsorship quotas, revocation and no-chain rules
- [x] T0->T2/T3 migration model preserving relationships
- [x] vendor-wide block tabletop
- [x] vendor-block model computes the fraction of relationships with a non-Cloudflare fallback route for realistic invite graphs under Cloudflare-first T1 (drill D5, gap G5)
- [x] Home API frozen in spec/protocol/home-api.md + spec/cddl/home-api.cddl: scoped request authentication, typed om-home-errors registry, versioning, real ProviderCapabilities JSON Schema with durability_class and explicit provider interfaces (ADR-0133, R-275, R-284)
- [ ] blocked-vendor and blocked-suffix simulation (for example all workers.dev locators and one vendor's IP ranges blocked): fraction of established relationships and of first contacts that still succeed, and which fallbacks carried them (R-021, R-028, R-045)
- [ ] no provisioning path activates code that is not a verified release: template verifier-first flow and update controller modeled, conformance PC-I01..PC-I03 specified (R-261)
- [ ] sponsor revocation overlap and non-sponsor second route in the sponsorship model; PC-D06/PC-D07 specified (R-266, R-267, R-260)

Pre-release criteria:

- [ ] second reference Home with no cloud-vendor dependency passes provider conformance
- [ ] live vendor-wide block drill
- [ ] zero-billing enforcement tests on the reference cloud tier

Mechanically unfinished pre-engineering work:

- blocked-vendor and blocked-suffix simulation (for example all workers.dev locators and one vendor's IP ranges blocked): fraction of established relationships and of first contacts that still succeed, and which fallbacks carried them (R-021, R-028, R-045)
- no provisioning path activates code that is not a verified release: template verifier-first flow and update controller modeled, conformance PC-I01..PC-I03 specified (R-261)
- sponsor revocation overlap and non-sponsor second route in the sponsorship model; PC-D06/PC-D07 specified (R-266, R-267, R-260)
- phase independence evidence is not yet recorded as satisfied

### V-021 — `endpoint_coercion_and_platform_leakage`

Prove non-silent device linking and per-platform leakage controls for every LocalDataClass on each shipped platform.

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release external_review
- **Execution class:** `documented_platform_capability_review`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md`; `verification/tla/AccountControl.tla`

Pre-engineering criteria:

- [x] per-platform capability matrix for every control in ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md section 4
- [x] device-link notice invariant in the AccountControlState model
- [x] deletion-closure checklist including OS notification and snapshot surfaces
- [ ] failing trace: a personal egress, representation or automation grant created on one device with no notice on the account's other devices is rejected (ADR-0129 rule 7)
- [ ] PlatformAdapter SecureSurface, BackupExclusion, InputPrivacy, NotificationRemoval and sensitive Clipboard availability recorded per platform in PLATFORM_CAPABILITY_MATRIX.md (ADR-0129 rule 18)
- [ ] ClientHost controller: owner-pinned root cannot be changed by release metadata; deploy refused without two transparency observations (ADR-0129 rule 12)

Pre-release criteria:

- [ ] forensic-style audit on each shipped platform: backup exclusion, snapshot suppression, plaintext residue after quick lock and disappearing expiry

Mechanically unfinished pre-engineering work:

- failing trace: a personal egress, representation or automation grant created on one device with no notice on the account's other devices is rejected (ADR-0129 rule 7)
- PlatformAdapter SecureSurface, BackupExclusion, InputPrivacy, NotificationRemoval and sensitive Clipboard availability recorded per platform in PLATFORM_CAPABILITY_MATRIX.md (ADR-0129 rule 18)
- ClientHost controller: owner-pinned root cannot be changed by release metadata; deploy refused without two transparency observations (ADR-0129 rule 12)
- phase independence evidence is not yet recorded as satisfied

### V-022 — `performance_budgets`

Show OM meets the measurable speed, battery, memory and size budgets in MOTION_AND_PERFORMANCE.md on reference device classes.

- **Scopes blocked:** reference_client_ui, provider_reference
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Execution class:** `measurement_plan_and_prototype_metrics`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`
- **Evidence:** `docs/architecture/ux/MOTION_AND_PERFORMANCE.md`; `docs/process/PERFORMANCE_MEASUREMENT_PLAN.md`; `verification/results/v001-crypto-latest.json`; `verification/results/v005-encoding-latest.json`

Pre-engineering criteria:

- [x] budgets and reference device classes recorded in MOTION_AND_PERFORMANCE.md
- [x] V-001 and V-005 prototypes record crypto and codec costs on native and wasm32 against the budgets; unattainable budgets revised by ADR
- [ ] measurement harness plan (lab devices + CI emulation) reviewed

Pre-release criteria:

- [ ] every budget measured on real reference devices for the release, results published
- [ ] CI regression checks on emulated low-end profiles

Mechanically unfinished pre-engineering work:

- measurement harness plan (lab devices + CI emulation) reviewed
- phase independence evidence is not yet recorded as satisfied

### V-023 — `clienthost_update_lifecycle`

Specify and test the ClientHost update lifecycle that is the W0 PWA endpoint trust root: release discovery and mirrors, signed-metadata verification, the verifier-first provider-template update flow, service-worker lifecycle, rollback/freeze protection, the owner-local minimum-version floor, broken/compromised-origin migration, credential scoping, long-neglected host upgrade, and the release-transparency log/monitor roles.

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool, external_review
- **Execution class:** `repo_local_update_lifecycle_design`; next executor role `verification_executor`
- **Pre-engineering independence status:** `open`

Pre-engineering criteria:

- [ ] release discovery/mirror rules and the signed-metadata verification path outside the downloaded bundle, for an owner-pinned root (CLIENTHOST §4 rule 1)
- [ ] verifier-first provider-template deployment and update flow: staged immutable build, atomic switch, bounded rollback, fail closed on invalid signatures or rollback (R-261)
- [ ] service-worker lifecycle: cache versioning, old-client compatibility window, staged activation, recovery from a broken worker
- [ ] rollback/freeze protection and the owner-local minimum-version floor (no remote floor)
- [ ] broken/compromised-origin migration as new-device enrollment, and credential scoping for deploy credentials
- [ ] offline/long-neglected host upgrade behavior
- [ ] release-transparency profile uses Rekor-v2/C2SP-compatible signed checkpoints, inclusion/consistency verification, two independent signed monitor observations and batch/mirrorable retrieval; origin compromise is outside same-origin verifier trust (ADR-0133, R-274)

Pre-release criteria:

- [ ] live update and rollback drill on the reference PWA across the supported browser matrix, including a withheld-transparency case and a compromised-origin migration

Mechanically unfinished pre-engineering work:

- release discovery/mirror rules and the signed-metadata verification path outside the downloaded bundle, for an owner-pinned root (CLIENTHOST §4 rule 1)
- verifier-first provider-template deployment and update flow: staged immutable build, atomic switch, bounded rollback, fail closed on invalid signatures or rollback (R-261)
- service-worker lifecycle: cache versioning, old-client compatibility window, staged activation, recovery from a broken worker
- rollback/freeze protection and the owner-local minimum-version floor (no remote floor)
- broken/compromised-origin migration as new-device enrollment, and credential scoping for deploy credentials
- offline/long-neglected host upgrade behavior
- release-transparency profile uses Rekor-v2/C2SP-compatible signed checkpoints, inclusion/consistency verification, two independent signed monitor observations and batch/mirrorable retrieval; origin compromise is outside same-origin verifier trust (ADR-0133, R-274)
- phase independence evidence is not yet recorded as satisfied
