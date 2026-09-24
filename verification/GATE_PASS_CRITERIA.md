# Verification Gate Pass Criteria

- **Status:** Canonical verification-process view, **generated** by `tools/render_gates.py` from
  `machine/architecture_verification_gates.jsonl` (the machine ledger is authoritative; do not hand-edit)
- **Decisions:** ADR-0108, ADR-0109, ADR-0113, ADR-0115, ADR-0117 (proposed)

## Rules

1. **Evidence is not passage.** A gate phase passes only when every listed criterion is met and recorded with evidence links.
2. **Independence.** Evidence produced solely by the author/agent who designed the thing under test cannot pass a phase. Each phase names the minimum independence:
   - `independent_tool`: a checker not written by the design author (TLC, fuzzers, sanitizers);
   - `independent_reviewer`: a named reviewer other than the author signs off in an ADR or review record (for pre-engineering phases a different-vendor AI agent counts, with a record in `verification/reviews/`: ADR-0117);
   - `independent_implementation`: a second implementation/model written by a different author or in a different language agrees;
   - `upstream_kat`: official known-answer tests from the standard's publisher;
   - `external_review`: an external security/crypto review organization or qualified outside reviewer;
   - `participant_study` / `independent_facilitator`: moderated sessions with real target users, facilitated by someone other than the flow's designer.
3. **Two phases.** `pre_engineering_status` gates scope authorization; overall `status` = passed requires both phases and gates public releases/claims.
4. **Cross-wave feasibility (ADR-0113).** A gate with `interface_impact` must reach `feasibility_status: evidenced` before the listed scopes can be authorized, even though its own scope is a later wave.
5. **Waivers** (`waived_by_adr`) must name the ADR, the risk accepted and the compensating control.

## Summary

| Gate | Name | Scopes | Pre-engineering | Overall | Feasibility | W0 interface impact |
|---|---|---|---|---|---|---|
| V-001 | `production_crypto_path` | core_kernel | open | open | open | — |
| V-002 | `account_control_formal_model` | core_kernel | open | open | open | — |
| V-003 | `one_to_one_session_model` | core_kernel | open | open | open | — |
| V-004 | `group_commit_control_model` | groups | open | open | open | core_kernel |
| V-005 | `canonical_encoding_vectors` | core_kernel | open | open | open | — |
| V-006 | `web_device_vault_feasibility` | reference_client_ui | open | open | open | — |
| V-007 | `sframe_interop` | calls | open | open | open | — |
| V-008 | `space_room_permission_authority_model` | communities_publications | open | open | open | core_kernel |
| V-009 | `baseline_event_materialization_convergence` | groups | open | open | open | core_kernel |
| V-010 | `plugin_sandbox_capability_model` | plugins | open | open | open | — |
| V-011 | `service_principal_authority_and_automation_model` | services_bots | open | open | open | core_kernel |
| V-012 | `expressive_media_and_pack_parser_safety` | expressive_media | open | open | open | — |
| V-013 | `bridge_provenance_fidelity_and_loop_model` | bridges | open | open | open | core_kernel |
| V-014 | `portable_archive_interop_and_import_safety` | portability | open | open | open | core_kernel |
| V-015 | `metadata_exposure_and_privacy_path_model` | core_kernel, provider_reference | open | open | open | — |
| V-016 | `decentralized_abuse_moderation_authority_model` | core_kernel, communities_publications | open | open | open | — |
| V-017 | `protocol_longevity_and_0m_continuity_drill` | none | open | open | open | — |
| V-018 | `security_ux_comprehension` | reference_client_ui | open | open | open | — |
| V-019 | `pq_group_reinit_migration` | groups | open | open | open | core_kernel |
| V-020 | `home_onboarding_and_provider_portability` | provider_reference | open | open | open | — |
| V-021 | `endpoint_coercion_and_platform_leakage` | reference_client_ui | open | open | open | — |
| V-022 | `performance_budgets` | reference_client_ui, provider_reference | open | open | open | — |

## Gates

### V-001 — `production_crypto_path`

Demonstrate the selected PQXDH + Triple Ratchet / SPQR-class OM-Crypto-v1 state-machine path in Rust on native and wasm32, including state persistence/reload, deterministic vectors, one-time key material and dependency/licensing/update strategy, without relying on an unsupported external API contract.

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_implementation_or_upstream_kat; pre-release external_review
- **Evidence:** `decisions/ADR-0118-production-crypto-dependency-licensing-and-update-strategy.md`; `verification/prototypes/v001-crypto/README.md`; `verification/results/v001-crypto-latest.json`

Pre-engineering criteria:

- [x] Rust feasibility prototype of the OM-Crypto-v1 PQXDH + Double Ratchet state machine (ADR-0118; SPQR is v2) runs on native and wasm32 with state persistence/reload
- [x] deterministic vectors produced by the prototype are consumed by a second independent implementation or checked against upstream KATs
- [x] one-time key material handling and depletion behavior demonstrated without downgrade
- [x] dependency, licensing and update strategy recorded in an ADR
- [ ] no reliance on an unsupported external library API contract

Pre-release criteria:

- [ ] independent cryptographic review of the integrated production path
- [ ] production build provably excludes dev/test CryptoProvider (R-072)

What remains (needs someone other than the author):

- no unsupported external API contract: re-run the prototype's v1 path without the spqr crate (PQXDH + Double Ratchet only)
- criterion 2 independence: both implementations came from one agent (NIST ACVP KATs are upstream); needs a non-Anthropic review (ADR-0117)

### V-002 — `account_control_formal_model`

Model-check AccountControlState majority authorization, quarantine denominator, recovery, fork/freeze and rollback invariants; derive regression scenarios and golden vectors. Includes terminal AccountRetirement, LegacyPolicy contest/cancel and the non-silent device-link invariant (2026-09-24).

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release external_review
- **Evidence:** `verification/bounded/continuity_assessment.py`; `verification/results/authority-bounded-latest.json`; `verification/results/tlc-latest.json`; `verification/tla/AccountControl.tla`; `verification/tla/AccountControlState.cfg`; `verification/tla/AccountControlState.tla`; `verification/tla/models.json`; `verification/vectors/account-control-regression-vectors.json`; `verification/vectors/tla-AccountControl_trap_recover.json`

Pre-engineering criteria:

- [x] TLC (pinned tool) run of AccountControl.tla (formerly AccountControlState.tla) with recorded state counts and no counterexample
- [x] model covers recovery, fork/freeze, rollback, AccountRetirement terminality, LegacyPolicy contest/cancel and the device-link notice invariant
- [x] regression/golden vectors derived from the model
- [ ] model reviewed by someone other than its author
- [x] model covers ADR-0111 default recovery: delayed activation, veto by any live controller, immediate completion only with controller co-sign, RecoveryRoot as exactly one vote, revocation of all earlier controllers on completion

Pre-release criteria:

- [ ] implementation CI replays every vector (R-070)
- [ ] independent security review of the AccountControlState implementation

What remains (needs someone other than the author):

- model review by someone other than its author (a human, or a cross-vendor AI reviewer if ADR-0117 is accepted)

### V-003 — `one_to_one_session_model`

Model/fuzz the per-device 1:1 session manager across crossed initiation, add/revoke races, stale/reset sessions, dedupe and persistence.

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Evidence:** `verification/tla/OneToOneSessions.tla`; `verification/tla/models-v003.json`; `verification/prototypes/v003-sessions/README.md`; `verification/results/v003-sessions-latest.json`; `verification/vectors/one-to-one-session-regression-vectors.json`; `decisions/ADR-0120-one-to-one-session-manager-rules.md`

Pre-engineering criteria:

- [ ] executable model/fuzzer of the per-device 1:1 session manager covering crossed initiation, add/revoke races, stale/reset sessions, dedupe and persistence
- [x] every discovered failing trace captured as a vector
- [ ] model reviewed by someone other than its author

Pre-release criteria:

- [ ] implementation fuzzing for an agreed CPU-hour budget with no unresolved crash or invariant violation

What remains (needs someone other than the author):

- non-author review: a non-Anthropic agent or a human (ADR-0117)
- restore+redelivery TLC config unchecked (K11 covered by gap config + fuzzer)

### V-004 — `group_commit_control_model`

Formally exercise CommitCoordinator replacement/equivocation and GroupControlIntent <-> GroupCommitBinding crash/partition semantics so same-epoch races cannot silently fork authority.

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release independent_implementation
- **Evidence:** `verification/results/authority-bounded-latest.json`

Pre-engineering criteria:

- [ ] formal model of CommitCoordinator replacement/equivocation and GroupControlIntent<->GroupCommitBinding under crash/partition with no silent authority fork
- [ ] candidate MLS library demonstrated to support the required external commit ordering on target runtimes

Pre-release criteria:

- [ ] multi-client partition/crash integration harness passes on two implementations or two runtimes

### V-005 — `canonical_encoding_vectors`

Publish deterministic-CBOR/CDDL golden vectors produced and consumed correctly by at least two independent implementations/encoders for every signed/hashed v1 object.

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_implementation; pre-release independent_implementation
- **Evidence:** `spec/cddl/`; `spec/registries/`; `verification/prototypes/v005-encoding/README.md`; `verification/results/v005-encoding-latest.json`; `decisions/ADR-0119-freeze-w0-cddl-and-registries.md`

Pre-engineering criteria:

- [ ] CDDL, numeric-field and domain-separation registries frozen for every signed/hashed object used by the scope being unlocked (W0 first)
- [ ] two independently written encoders/decoders agree on positive and negative object vectors for those objects

Pre-release criteria:

- [ ] every conforming implementation in the release passes the complete vector suite

What remains (needs someone other than the author):

- independence of the two codecs: a non-Anthropic agent review (ADR-0117) or a codec by a second author

### V-006 — `web_device_vault_feasibility`

Prove the WebAuthn-PRF/non-extractable-WebCrypto DeviceVault hierarchy and migration behavior on the browser support matrix intended for the first reference PWA.

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool

Pre-engineering criteria:

- [ ] browser-matrix prototype of the WebDeviceVault wrapping hierarchy (WebAuthn PRF, non-extractable WebCrypto, passphrase fallback) with new-origin enrollment migration
- [ ] documented behavior for every unsupported/failed capability on the intended first-release browser matrix
- [ ] PRF on synced (platform/password-manager) passkeys and signalCurrentUserDetails support recorded per browser/platform for the ADR-0111 recovery custodian, with the recovery-key fallback exercised where unsupported

Pre-release criteria:

- [ ] published support matrix with regression tests on each supported browser

### V-007 — `sframe_interop`

Prove the selected SFrame/WebRTC key schedule and counter/replay rules across required browsers/native runtimes, SFU forwarding, reconnect, simulcast/SVC where claimed, and membership rekey.

- **Scopes blocked:** calls
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- [ ] SFrame key schedule and counter/replay prototype interoperates across at least two browser engines and one native runtime through an SFU with reconnect and membership rekey

Pre-release criteria:

- [ ] full claimed matrix including simulcast/SVC where claimed

### V-008 — `space_room_permission_authority_model`

Model-check Room/Space capability grants, explicit restrictions, delegation ceilings, governance quorum, bilateral links and authority-parent propagation/revocation so no role/Space transition can silently self-escalate, create transitive authority, or bypass Room-local security state.

- **Scopes blocked:** communities_publications
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release independent_implementation
- **Evidence:** `decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md`; `verification/results/authority-bounded-latest.json`; `verification/results/v008-permissions-latest.json`; `verification/tla/ScopedPermissions.tla`; `verification/tla/models-v008-permissions.json`

Pre-engineering criteria:

- [ ] formal model of Room/Space grants, restrictions, ceilings, governance quorum, bilateral links and revocation races with no self-escalation or transitive-authority counterexample

Pre-release criteria:

- [ ] implementation replays model-derived vectors

What remains (needs someone other than the author):

- non-author review: a non-Anthropic agent or a human (ADR-0117)

### V-009 — `baseline_event_materialization_convergence`

Property-test/model the expanded baseline ConversationEvent materialization under duplicate, delayed, reordered and concurrent delivery for shared pins, moderator redaction, polls, threads/topics and message-topic moves, proving deterministic views and authorization handling without treating wall-clock time as security authority.

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_implementation; pre-release independent_implementation
- **Evidence:** `decisions/ADR-0121-event-ordering-authorization-epoch-and-materialization.md`; `verification/prototypes/v009-materializer/README.md`; `verification/results/conversation-convergence-latest.json`; `verification/results/v009-materializer-latest.json`

Pre-engineering criteria:

- [ ] a second, independently written materializer (different author or language) agrees with the reference model on the full convergence corpus including malformed and authorization-race cases

Pre-release criteria:

- [ ] every conforming implementation passes the corpus

What remains (needs someone other than the author):

- non-author review: a non-Anthropic agent or a human (ADR-0117)
- reference derives authorization from the control chain (G-V9-5) instead of the shared adapter
- second implementation of edits/deletes/reactions/receipts/replies

### V-010 — `plugin_sandbox_capability_model`

Prototype and adversarially test the OM WebAssembly Component Model/WIT plugin host on intended native and PWA runtimes, proving no ambient filesystem/network/process/environment/secret-store/key/DOM authority, explicit permission-upgrade prompts, plaintext-read vs network-egress separation, draft vs send separation, crash isolation and enforceable resource budgets.

- **Scopes blocked:** plugins
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release external_review

Pre-engineering criteria:

- [ ] prototype host on a native Wasm runtime and the PWA runtime; adversarial plugins demonstrate no ambient authority, enforced budgets, permission-upgrade prompts and plaintext-read vs egress separation

Pre-release criteria:

- [ ] external review of the sandbox host

### V-011 — `service_principal_authority_and_automation_model`

Model/property-test permissionless ServiceGenesis, ServicePrincipal endpoint/control authorization (including no-self-bootstrap), host migration/revocation, alias/catalog replacement, RepresentationGrant impersonation boundaries, Account-confirmed vs autonomous authorship, installation/data-access modes, full-Room admission/revocation, local/remote LLM tool capabilities, cheap-identity/no-unsolicited-reach behavior, and automation lineage/hop/rate rules so no service can self-escalate, become a hidden plaintext recipient, impersonate an Account, or create unbounded bot-to-bot loops.

- **Scopes blocked:** services_bots
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Evidence:** `decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md`; `verification/results/authority-bounded-latest.json`; `verification/results/v011-services-latest.json`; `verification/tla/ServiceAuthority.tla`; `verification/tla/models-v011.json`

Pre-engineering criteria:

- [ ] formal/bounded model extended to host migration/revocation and alias replacement
- [ ] tool-capability prototype shows injected prompts cannot exceed granted capabilities or create unbounded automation loops

Pre-release criteria:

- [ ] implementation tests replay model vectors and injection corpus

What remains (needs someone other than the author):

- non-author review: a non-Anthropic agent or a human (ADR-0117)

### V-012 — `expressive_media_and_pack_parser_safety`

Fuzz and cross-implementation test expressive-media resource ceilings, decoder/pixel-frame budgets, ContentPack canonical manifests/signatures/update/fork lineage and malformed metadata/blob references so hostile packs/media fail safely without code execution, unbounded resource use, catalog dependence or signature ambiguity.

- **Scopes blocked:** expressive_media
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_tool, independent_implementation; pre-release independent_tool

Pre-engineering criteria:

- [ ] candidate decoders and pack parsers fuzzed for an agreed CPU-hour budget under resource ceilings with no unresolved crash
- [ ] canonical ContentPack manifest/signature vectors agree across two implementations

Pre-release criteria:

- [ ] shipped parsers under continuous fuzzing

### V-013 — `bridge_provenance_fidelity_and_loop_model`

Adversarially model/test BridgePrincipal control and Room admission/removal, foreign provenance/identity-link spoofing, cross-network loop/reflection and duplicate suppression across multiple bridges, edit/delete/reaction/topic/poll target mapping, capability/fidelity-loss signaling, E2EE-boundary state and origin-fingerprint metadata exposure.

- **Scopes blocked:** bridges
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Evidence:** `decisions/ADR-0124-service-information-flow-and-bridge-provenance.md`; `verification/prototypes/v013-bridges/BRIDGE_ADAPTER_CONTRACT.md`; `verification/results/v013-bridges-latest.json`; `verification/results/wire-boundary-bounded-latest.json`

Pre-engineering criteria:

- [ ] reviewed bridge adapter contract specification
- [x] loop/reflection-suppression model with at least two simulated bridges and fidelity-loss signaling

Pre-release criteria:

- [ ] real adapters and multi-bridge traces meet the model

What remains (needs someone other than the author):

- non-author review: a non-Anthropic agent or a human (ADR-0117)

### V-014 — `portable_archive_interop_and_import_safety`

Round-trip om-archive-v1 across at least two independent implementations and test exact canonical-object preservation, stale control-state non-authority, missing/partial blobs, encrypted-archive corruption/wrong-key behavior, hostile archive paths/decompression/media/foreign formats, fabricated foreign authorship, read-only import attempting live authority, and inability to clone DeviceIdentity/account authority from ordinary history archives.

- **Scopes blocked:** portability
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Evidence:** `decisions/ADR-0125-portable-archive-format-rulings.md`; `verification/prototypes/v014-archive/OM_ARCHIVE_V1_SPEC.md`; `verification/results/v014-archive-latest.json`; `verification/results/wire-boundary-bounded-latest.json`

Pre-engineering criteria:

- [ ] frozen om-archive-v1 specification and vectors
- [x] hostile archive corpus
- [x] one reference reader/writer prototype passes the corpus

Pre-release criteria:

- [ ] two independent implementations round-trip
- [ ] real foreign importers pass authority/provenance tests

What remains (needs someone other than the author):

- freeze spec: review by a non-author (ADR-0117), ADR, crypto-profile review, V-005/ADR-0119
- reviewer sign-off on corpus adequacy

### V-015 — `metadata_exposure_and_privacy_path_model`

Instrument/model normal and privacy-enhanced paths across relay, MLS/group control, KeyMaterialProvider, discovery/OHTTP split, IntroGate, push, blob, calls, services, bridges and archive migration; prove no accidental canonical AccountID/ConversationID/group-identifier exposure in normal routing, verify per-provider capability/randomized wrapping, validate MetadataExposureDescriptor claims against traces, and document remaining timing/size/IP/collusion leakage.

- **Scopes blocked:** core_kernel, provider_reference
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Evidence:** `docs/process/METADATA_TRACE_TEST_PLAN.md`; `spec/metadata/w0-metadata-exposure-descriptors.json`; `verification/bounded/metadata_trace_model.py`; `verification/results/metadata-trace-latest.json`; `verification/results/wire-boundary-bounded-latest.json`

Pre-engineering criteria:

- [x] MetadataExposureDescriptor written for every provider role in the scope being unlocked, including the sponsored starter mailbox
- [x] trace-model assertions for each descriptor and a packet-capture test plan

Pre-release criteria:

- [ ] real packet/provider/browser traces for normal and privacy-enhanced paths match every descriptor

What remains (needs someone other than the author):

- review of the descriptors, trace model (mutation-tested: 5/5 seeded mis-claims caught) and test plan by someone other than their author

### V-016 — `decentralized_abuse_moderation_authority_model`

Model/property-test block/revocation/Sybil behavior, IntroGate resource abuse, Room/Space timeout-remove-ban authorization and delegation revocation, malicious moderators, selective report evidence, SafetyAdvisory signature/supersession/expiry and scoped subject references, conflicting labelers, provider denial boundaries, service/bridge abuse and fork/escape so no safety mechanism creates hidden universal authority or global identity tracking.

- **Scopes blocked:** core_kernel, communities_publications
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Evidence:** `decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md`; `verification/bounded/introgate_sybil_model.py`; `verification/results/authority-bounded-latest.json`; `verification/results/introgate-sybil-latest.json`; `verification/results/v016-room-moderation-latest.json`; `verification/tla/RoomModeration.tla`; `verification/tla/models-v016-room-moderation.json`

Pre-engineering criteria:

- [ ] IntroGate/block/Sybil/resource model for the W0 surfaces
- [ ] Room timeout/remove/ban race and evidence model before communities_publications is unlocked

Pre-release criteria:

- [ ] adversarial load and abuse tests against reference providers

What remains (needs someone other than the author):

- non-author review: a non-Anthropic agent or a human (ADR-0117)

### V-017 — `protocol_longevity_and_0m_continuity_drill`

Run the 20-year continuity exercise with founder/forge/domain/provider unavailable, independent Protocol Continuity Bundle recovery, immutable registry snapshots, two protocol generations/fork lineages, crypto-profile deprecation and catastrophic-authentication-break tabletop cases, release-root loss/rotation, dominant proprietary extension attempt, provider migrations and independent build/conformance paths; demonstrate no hidden single universal continuity choke point.

- **Scopes blocked:** none (always required pre-engineering; blocks public production claims)
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation, external_review
- **Evidence:** `docs/process/CONTINUITY_TABLETOP_DRILLS.md`; `tools/continuity_bundle.py`; `verification/results/continuity-dry-run-latest.json`

Pre-engineering criteria:

- [x] Protocol Continuity Bundle assembly and verification script
- [x] registry-immutability checks
- [x] written tabletop drills: founder/forge/domain loss, crypto primitive break, release-root loss, dominant-extension capture, vendor-wide provider block and vulnerability response

Pre-release criteria:

- [ ] independent rebuild of a compatible implementation from a mirror
- [ ] two coexisting protocol generations/fork lineages
- [ ] live disaster drill with no hidden universal choke point

What remains (needs someone other than the author):

- review of the bundle script and the six drills by someone other than their author (independent_reviewer)

### V-018 — `security_ux_comprehension`

Demonstrate that non-technical users complete and correctly understand OM's security-critical flows without infrastructure or cryptographic vocabulary.

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering participant_study, independent_facilitator; pre-release participant_study
- **Evidence:** `docs/process/SECURITY_UX_STUDY_PROTOCOL.md`

Pre-engineering criteria:

- [ ] moderated sessions with at least 12 non-technical participants (including at least 3 aged 60+ and 3 with low digital literacy, in at least 2 languages) on non-production prototypes
- [ ] flows: onboarding tier (T0/T1), recovery on (where it lives) and turning recovery off, recovery veto notice, continuity check, device-link notice, suspend device, contact continuity change, bridge E2EE boundary, full-Room service disclosure
- [ ] per flow at least 80% task success and correct answer to the flow's security-comprehension question; failures redesigned and retested

Pre-release criteria:

- [ ] repeat on the real client, plus assistive-technology users for accessibility

What remains (needs someone other than the author):

- run moderated sessions with >=12 non-technical participants (>=3 aged 60+, >=3 low digital literacy, >=2 languages) with an independent facilitator, using the protocol; this needs people and cannot be done by an agent

### V-019 — `pq_group_reinit_migration`

Prove the v1 group wire can migrate by authenticated ReInit to a hybrid-PQ MLS profile without changing ConversationID or group authority and without downgrade.

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- [ ] prototype authenticated ReInit from the v1 MLS suite to an experimental hybrid-PQ suite preserving ConversationID and GroupControlState, rejecting downgrade, with vectors

Pre-release criteria:

- [ ] interoperable migration across two implementations once ADR-0107 triggers are met

### V-020 — `home_onboarding_and_provider_portability`

Prove zero-signup sponsored onboarding, bounded sponsorship, migration between Home tiers without AccountID change, and survival of a vendor-wide provider block with a second non-cloud reference provider.

- **Scopes blocked:** provider_reference
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation
- **Evidence:** `docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md`; `docs/process/CONTINUITY_TABLETOP_DRILLS.md`; `verification/bounded/home_tiers_model.py`; `verification/results/home-tiers-latest.json`

Pre-engineering criteria:

- [x] provider conformance suite specification
- [x] bounded model of sponsorship quotas, revocation and no-chain rules
- [x] T0->T2/T3 migration model preserving relationships
- [x] vendor-wide block tabletop
- [x] vendor-block model computes the fraction of relationships with a non-Cloudflare fallback route for realistic invite graphs under Cloudflare-first T1 (drill D5, gap G5)

Pre-release criteria:

- [ ] second reference Home with no cloud-vendor dependency passes provider conformance
- [ ] live vendor-wide block drill
- [ ] zero-billing enforcement tests on the reference cloud tier

What remains (needs someone other than the author):

- review of the conformance suite, models and vendor-block tabletop by someone other than their author (independent_reviewer)

### V-021 — `endpoint_coercion_and_platform_leakage`

Prove non-silent device linking and per-platform leakage controls for every LocalDataClass on each shipped platform.

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release external_review
- **Evidence:** `docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md`; `verification/tla/AccountControl.tla`

Pre-engineering criteria:

- [x] per-platform capability matrix for every control in ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md section 4
- [x] device-link notice invariant in the AccountControlState model
- [x] deletion-closure checklist including OS notification and snapshot surfaces

Pre-release criteria:

- [ ] forensic-style audit on each shipped platform: backup exclusion, snapshot suppression, plaintext residue after quick lock and disappearing expiry

What remains (needs someone other than the author):

- independent review of the matrix (documented from platform docs as of mid-2026, entries marked 'verify' are uncertain) and of the deletion-closure checklist

### V-022 — `performance_budgets`

Show OM meets the measurable speed, battery, memory and size budgets in MOTION_AND_PERFORMANCE.md on reference device classes.

- **Scopes blocked:** reference_client_ui, provider_reference
- **Status:** pre-engineering `open`, overall `open`, feasibility `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool
- **Evidence:** `docs/architecture/ux/MOTION_AND_PERFORMANCE.md`; `docs/process/PERFORMANCE_MEASUREMENT_PLAN.md`; `verification/results/v001-crypto-latest.json`; `verification/results/v005-encoding-latest.json`

Pre-engineering criteria:

- [x] budgets and reference device classes recorded in MOTION_AND_PERFORMANCE.md
- [x] V-001 and V-005 prototypes record crypto and codec costs on native and wasm32 against the budgets; unattainable budgets revised by ADR
- [ ] measurement harness plan (lab devices + CI emulation) reviewed

Pre-release criteria:

- [ ] every budget measured on real reference devices for the release, results published
- [ ] CI regression checks on emulated low-end profiles

What remains (needs someone other than the author):

- review of the measurement plan by someone other than its author
