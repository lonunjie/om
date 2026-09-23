# Verification Gate Pass Criteria

- **Status:** Canonical verification-process view, generated from `machine/architecture_verification_gates.jsonl` (the machine ledger is authoritative)
- **Checkpoint:** 2026-09-24 02:29 PHT
- **Decisions:** ADR-0108, ADR-0109

## Rules

1. **Evidence is not passage.** A gate phase passes only when every listed criterion is met and recorded with evidence links.
2. **Independence.** Evidence produced solely by the author/agent who designed the thing under test cannot pass a phase. Each phase names the minimum independence:
   - `independent_tool`: a checker not written by the design author (TLC, fuzzers, sanitizers);
   - `independent_reviewer`: a named reviewer other than the author signs off in an ADR or review record;
   - `independent_implementation`: a second implementation/model written by a different author or in a different language agrees;
   - `upstream_kat`: official known-answer tests from the standard's publisher;
   - `external_review`: an external security/crypto review organization or qualified outside reviewer;
   - `participant_study` / `independent_facilitator`: moderated sessions with real target users, facilitated by someone other than the flow's designer.
3. **Two phases.** `pre_engineering_status` gates scope authorization; overall `status` = passed requires both phases and gates public releases/claims.
4. **Waivers** (`waived_by_adr`) must name the ADR, the risk accepted and the compensating control.

## Gates

### V-001 — `production_crypto_path`

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_implementation_or_upstream_kat; pre-release external_review

Pre-engineering criteria:

- Rust feasibility prototype of the PQXDH + Triple Ratchet/SPQR-class state machine runs on native and wasm32 with state persistence/reload
- deterministic vectors produced by the prototype are consumed by a second independent implementation or checked against upstream KATs
- one-time key material handling and depletion behavior demonstrated without downgrade
- dependency, licensing and update strategy recorded in an ADR
- no reliance on an unsupported external library API contract

Pre-release criteria:

- independent cryptographic review of the integrated production path
- production build provably excludes dev/test CryptoProvider (R-072)

### V-002 — `account_control_formal_model`

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release external_review

Pre-engineering criteria:

- TLC (pinned tool) run of AccountControlState.tla with recorded state counts and no counterexample
- model covers recovery, fork/freeze, rollback, AccountRetirement terminality, LegacyPolicy contest/cancel and the device-link notice invariant
- regression/golden vectors derived from the model
- model reviewed by someone other than its author

Pre-release criteria:

- implementation CI replays every vector (R-070)
- independent security review of the AccountControlState implementation

### V-003 — `one_to_one_session_model`

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool

Pre-engineering criteria:

- executable model/fuzzer of the per-device 1:1 session manager covering crossed initiation, add/revoke races, stale/reset sessions, dedupe and persistence
- every discovered failing trace captured as a vector
- model reviewed by someone other than its author

Pre-release criteria:

- implementation fuzzing for an agreed CPU-hour budget with no unresolved crash or invariant violation

### V-004 — `group_commit_control_model`

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- formal model of CommitCoordinator replacement/equivocation and GroupControlIntent<->GroupCommitBinding under crash/partition with no silent authority fork
- candidate MLS library demonstrated to support the required external commit ordering on target runtimes

Pre-release criteria:

- multi-client partition/crash integration harness passes on two implementations or two runtimes

### V-005 — `canonical_encoding_vectors`

- **Scopes blocked:** core_kernel
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_implementation; pre-release independent_implementation

Pre-engineering criteria:

- CDDL, numeric-field and domain-separation registries frozen for every signed/hashed object used by the scope being unlocked (W0 first)
- two independently written encoders/decoders agree on positive and negative object vectors for those objects

Pre-release criteria:

- every conforming implementation in the release passes the complete vector suite

### V-006 — `web_device_vault_feasibility`

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool

Pre-engineering criteria:

- browser-matrix prototype of the WebDeviceVault wrapping hierarchy (WebAuthn PRF, non-extractable WebCrypto, passphrase fallback) with new-origin enrollment migration
- documented behavior for every unsupported/failed capability on the intended first-release browser matrix

Pre-release criteria:

- published support matrix with regression tests on each supported browser

### V-007 — `sframe_interop`

- **Scopes blocked:** calls
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- SFrame key schedule and counter/replay prototype interoperates across at least two browser engines and one native runtime through an SFU with reconnect and membership rekey

Pre-release criteria:

- full claimed matrix including simulcast/SVC where claimed

### V-008 — `space_room_permission_authority_model`

- **Scopes blocked:** communities_publications
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_tool, independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- formal model of Room/Space grants, restrictions, ceilings, governance quorum, bilateral links and revocation races with no self-escalation or transitive-authority counterexample

Pre-release criteria:

- implementation replays model-derived vectors

### V-009 — `baseline_event_materialization_convergence`

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_implementation; pre-release independent_implementation

Pre-engineering criteria:

- a second, independently written materializer (different author or language) agrees with the reference model on the full convergence corpus including malformed and authorization-race cases

Pre-release criteria:

- every conforming implementation passes the corpus

### V-010 — `plugin_sandbox_capability_model`

- **Scopes blocked:** plugins
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release external_review

Pre-engineering criteria:

- prototype host on a native Wasm runtime and the PWA runtime; adversarial plugins demonstrate no ambient authority, enforced budgets, permission-upgrade prompts and plaintext-read vs egress separation

Pre-release criteria:

- external review of the sandbox host

### V-011 — `service_principal_authority_and_automation_model`

- **Scopes blocked:** services_bots
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- formal/bounded model extended to host migration/revocation and alias replacement
- tool-capability prototype shows injected prompts cannot exceed granted capabilities or create unbounded automation loops

Pre-release criteria:

- implementation tests replay model vectors and injection corpus

### V-012 — `expressive_media_and_pack_parser_safety`

- **Scopes blocked:** expressive_media
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_tool, independent_implementation; pre-release independent_tool

Pre-engineering criteria:

- candidate decoders and pack parsers fuzzed for an agreed CPU-hour budget under resource ceilings with no unresolved crash
- canonical ContentPack manifest/signature vectors agree across two implementations

Pre-release criteria:

- shipped parsers under continuous fuzzing

### V-013 — `bridge_provenance_fidelity_and_loop_model`

- **Scopes blocked:** bridges
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- reviewed bridge adapter contract specification
- loop/reflection-suppression model with at least two simulated bridges and fidelity-loss signaling

Pre-release criteria:

- real adapters and multi-bridge traces meet the model

### V-014 — `portable_archive_interop_and_import_safety`

- **Scopes blocked:** portability
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- frozen om-archive-v1 specification and vectors
- hostile archive corpus
- one reference reader/writer prototype passes the corpus

Pre-release criteria:

- two independent implementations round-trip
- real foreign importers pass authority/provenance tests

### V-015 — `metadata_exposure_and_privacy_path_model`

- **Scopes blocked:** core_kernel, provider_reference
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool

Pre-engineering criteria:

- MetadataExposureDescriptor written for every provider role in the scope being unlocked, including the sponsored starter mailbox
- trace-model assertions for each descriptor and a packet-capture test plan

Pre-release criteria:

- real packet/provider/browser traces for normal and privacy-enhanced paths match every descriptor

### V-016 — `decentralized_abuse_moderation_authority_model`

- **Scopes blocked:** core_kernel, communities_publications
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_tool

Pre-engineering criteria:

- IntroGate/block/Sybil/resource model for the W0 surfaces
- Room timeout/remove/ban race and evidence model before communities_publications is unlocked

Pre-release criteria:

- adversarial load and abuse tests against reference providers

### V-017 — `protocol_longevity_and_0m_continuity_drill`

- **Scopes blocked:** none (always required pre-engineering; blocks public production claims)
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation, external_review

Pre-engineering criteria:

- Protocol Continuity Bundle assembly and verification script
- registry-immutability checks
- written tabletop drills: founder/forge/domain loss, crypto primitive break, release-root loss, dominant-extension capture, vendor-wide provider block and vulnerability response

Pre-release criteria:

- independent rebuild of a compatible implementation from a mirror
- two coexisting protocol generations/fork lineages
- live disaster drill with no hidden universal choke point

### V-018 — `security_ux_comprehension`

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering participant_study, independent_facilitator; pre-release participant_study

Pre-engineering criteria:

- moderated sessions with at least 12 non-technical participants (including at least 3 aged 60+ and 3 with low digital literacy, in at least 2 languages) on non-production prototypes
- flows: onboarding tier (T0/T1), continuity check, device-link notice, suspend device, contact continuity change, bridge E2EE boundary, full-Room service disclosure
- per flow at least 80% task success and correct answer to the flow's security-comprehension question; failures redesigned and retested

Pre-release criteria:

- repeat on the real client, plus assistive-technology users for accessibility

### V-019 — `pq_group_reinit_migration`

- **Scopes blocked:** groups
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- prototype authenticated ReInit from the v1 MLS suite to an experimental hybrid-PQ suite preserving ConversationID and GroupControlState, rejecting downgrade, with vectors

Pre-release criteria:

- interoperable migration across two implementations once ADR-0107 triggers are met

### V-020 — `home_onboarding_and_provider_portability`

- **Scopes blocked:** provider_reference
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release independent_implementation

Pre-engineering criteria:

- provider conformance suite specification
- bounded model of sponsorship quotas, revocation and no-chain rules
- T0->T2/T3 migration model preserving relationships
- vendor-wide block tabletop

Pre-release criteria:

- second reference Home with no cloud-vendor dependency passes provider conformance
- live vendor-wide block drill
- zero-billing enforcement tests on the reference cloud tier

### V-021 — `endpoint_coercion_and_platform_leakage`

- **Scopes blocked:** reference_client_ui
- **Status:** pre-engineering `open`, overall `open`
- **Independence:** pre-engineering independent_reviewer; pre-release external_review

Pre-engineering criteria:

- per-platform capability matrix for every control in ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md section 4
- device-link notice invariant in the AccountControlState model
- deletion-closure checklist including OS notification and snapshot surfaces

Pre-release criteria:

- forensic-style audit on each shipped platform: backup exclusion, snapshot suppression, plaintext residue after quick lock and disappearing expiry

