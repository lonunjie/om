## 2026-09-29 06:34 PHT — independent remediation / ADR-0157–0158

- OpenAI GPT-5.6 Sol independently reviewed Anthropic-authored ADR-0154–0156 and recorded the review under `verification/reviews/2026-09-29-gpt-5-6-sol-adr0154-0156-independent-review.md`.
- **ADR-0157:** closed future critical-extension downgrade divergence with the reserved `om.critical.*` namespace; split implementation-private `DeviceHistorySnapshot` from interoperable group `GroupHistoryBundle`; made scheduled-send queues compose-intent-only until actual send.
- **ADR-0158:** resolved A-055 with LanDeliveryProfile v1: 900 s epochs, K=8, purpose-separated pair/invite rendezvous, encrypted WebRTC signalling that hides SDP/ICE credentials, ordered/reliable DataChannel framing, 16 KiB chunks, bounded backpressure, ephemeral `om+lan://` RelayEntry reuse of the frozen sealed-delivery route binding, and ordinary IntroGate/MessageRequest first-contact authority.
- Added `spec/cddl/lan.cddl`, LAN numeric-field/domain-separation registrations, R-346..R-350 and T-136..T-138; updated V-004/V-005/V-024 criteria.
- A-055 is resolved and `design_open=0`; `local_delivery` remains unauthorized behind V-024. New OpenAI-authored ADR-0157/0158 changes still require different-vendor review; the consolidated `core_kernel` review still needs a non-OpenAI reviewer.
- Current-tree validation, docs, registry and 79-document continuity checks pass. Portable producers 1–30 passed under the aggregate runner before the execution wrapper stopped it while producer 31 was starting; producers 31–44 were then run directly and all passed. No product engineering was performed.

## 2026-09-28 23:00 PHT — independent architecture review (ADR-0154–0156)

- Imported the Run 10A drop as `checkpoint/20260928-2200` (its stale contract snapshot would have stamped it 20260927-0842) and reviewed it as the different-vendor architecture reviewer; report in `docs/reports/2026-09-28-independent-architecture-review-run10a.md`.
- **ADR-0154:** verifier runtime segment = one OS boot session; guarded wall-assisted gap credit for Recommended web verifiers with a final authenticated veto sweep (R-340, T-132, V-002 +1 criterion, 8/8 scenarios, 6/6 mutations).
- **ADR-0155:** non-critical registered extensions never invalidate the containing object; location cards create-only, single-point and suppressed after edits; list annotations bound to the carrying event (R-341, T-133, V-005 +1 criterion; extension 29/29 and rich-list 35/35 Python↔Node; both V-005 strict codecs aligned, corpus rerun unchanged).
- **ADR-0156:** LAN wire binding becomes open item A-055 under new W1 scope `local_delivery` and gate V-024; later-wave gates require frozen CDDL plus two-codec vectors; scheduled send and sync namespacing specified; design blockers now block only their listed scopes (R-342..R-345, T-134, T-135).
- Stale single-identity wording fixed; SNAPSHOT restructured; contract snapshot updated; portable phase-1 44/44.
- State: `design_open=1`, 24 gates (23 pre-engineering open), 0/12 scopes authorized; engineering remains unauthorized.

## 2026-09-28 22:00 PHT — Run 10A residual V1 boundary audit

- completed the final bounded residual post-V1-to-V1 scope audit after Runs 6A–9B;
- promoted **zero** additional features: the remaining deferrals now reflect real crypto/authority/privacy/parser/platform-policy/carrier/API/measurement costs rather than arbitrary sequencing;
- rechecked SMS/MMS/RCS against current Android/Google platform policy and APIs, including Android API 37 `AlternativeMessageTransportService`; the existing `TransportAdapter`/provenance boundary is sufficient to add carrier integrations later without core redesign;
- classified exact privacy-preset defaults, UI framework/breakpoint/theme/font/motion values and IndexedDB/OPFS crossover as measurement/product choices behind already-frozen abstractions, not missing V1 semantics;
- added no ADR, machine decision, requirement, threat, wire/schema change or verification criterion; Run-9B evidence and the active consolidated different-vendor review handoff remain unchanged.

## 2026-09-28 21:22 PHT — Run 9B final V1 platform evidence closure

- Added replaceable-Android-wake bounded evidence: 13/13 scenarios pass and 8/8 message/authority/linkage mutations are caught across FCM, UnifiedPush-compatible, persistent and `none` transitions.
- Added desktop/PersonRecord endpoint evidence: 12/12 scenarios pass and 10/10 semantic/profile/export mutations are caught across PWA/mobile/macOS/Windows/Linux capability surfaces and explicit local contact export.
- Added ADR-0153 / D-175 to correct the stale-registration privacy overclaim: unconfirmed provider-side retirement is residual metadata/wake exposure with provider-defined duration unless an enforceable finite lease is declared; obsolete wake handles never become fetch/message/account authority.
- Closed the new ADR-0152 repo-local criteria: V-015 is 16/16 and V-021 is 10/10; required independent-review and pre-release obligations remain open.
- Reactivated the consolidated different-vendor review handoff; engineering remains unauthorized.
- Refreshed all **43/43** portable producers in bounded execution; a monolithic retry was wrapper-terminated after producers 1–18 passed while producer 19 was starting, with no evidence failure.

## 2026-09-28 20:42 PHT — Run 9A final V1 platform-base promotion

- added ADR-0152: native desktop is a V1 reference-family target over shared Core/UI/PlatformAdapter semantics;
- made Android wake explicitly replaceable across FCM, UnifiedPush-compatible, persistent encrypted wake/fetch and none, with no identity/correctness authority;
- promoted explicit local PersonRecord cleanup/export UX with preview, permission, provenance preservation and no continuity-authority effect;
- reconciled stale current-design language for rich lists, account-private preference sync, desktop deferral, alternate Android wake and contact cleanup;
- added D-174 and R-337..R-339; intentionally reopened one repo-local criterion in V-015 and one in V-021 for Run 9B; independent review is paused and engineering remains unauthorized.

## 2026-09-28 19:37 PHT — Run 8B additional V1 evidence closure

- added ADR-0151 to resolve the rich-list known-schema-rejection versus non-critical semantic-fallback ambiguity without changing valid ADR-0150 encodings;
- added LAN/WAN EventID + blob-backfill reconciliation evidence: 8/8 scenarios, 6/6 mutations caught;
- added `om.rich-list.v1` two-implementation evidence: 34 vectors, 34/34 Python↔Node verdict and deterministic-CBOR agreement;
- added LAN privacy/resource trace: 9/9 scenarios, 6/6 mutations caught;
- added PWA/Android/iOS reference-family capability-parity evidence: 8/8 scenarios, 5/5 mutations caught;
- reconciled repo-local gate progress to V-003 13/14, V-005 11/11, V-015 15/15 and V-021 9/9; independence records remain open and engineering remains unauthorized.
- all **41/41** portable evidence producers pass in bounded batches; architecture/docs/registry/continuity checks are clean at 851 current files and 79/79 continuity documents.

## 2026-09-28 19:10 PHT — ADR-0150 / Run 8A

- Promoted optional privacy-preserving offline-LAN message/file delivery into V1 without making local discovery universal or authoritative.
- Registered bounded `om.rich-list.v1` list presentation with deterministic W0 fallback binding and no RichTextV1 grammar change.
- Made Android and iOS explicit V1 reference-family targets while preserving PWA-first sequencing.
- Added D-172, R-334..R-336 and T-129..T-131; reopened one repo-local criterion each in V-003/V-005/V-015/V-021 for Run 8B.
- Paused the consolidated different-vendor review until Run 8B evidence is current.

## 2026-09-28 18:21 PHT — Run 7B ADR-0149 adversarial evidence closure

- added causal/revocation/restore account-private-sync hardening evidence: 8/8 scenarios, 6/6 mutations caught; V-003 is 12/13 with independent model review remaining;
- amended the voice/location extension corpus to ADR-0149 integer-key payloads and 24 vectors with 24/24 Python/Node agreement; added registry/object/criticality/schema hardening evidence; V-005 is 10/10 and pre-engineering passed;
- added ProfileExecutionContext TOCTOU evidence: 8/8 scenarios across async paths/independent windows, 5/5 retargeting mutations caught; V-021 is 8/8 repo-local with independent review remaining;
- added update role/key/policy authorization evidence: 13/13 scenarios, 8/8 substitution/threshold/root-chain mutations caught; V-023 is 9/9 repo-local with independent review remaining;
- reactivated the consolidated different-vendor core-kernel reviewer handoff; engineering remains unauthorized.

## 2026-09-28 17:33 PHT — Run 7A ADR-0149 promoted-V1 adversarial hardening

- registered/object-scoped/schema-bound `om.voice-note.v1` and `om.location-card.v1` using integer-keyed canonical extension structs;
- bound static-location fallback to the exact canonical `geo:<lat-fixed-7>,<lon-fixed-7>` token and defined mismatch as ignore-extension/render-W0-fallback;
- hardened account-private sync with authenticated per-origin causal context, future-HLC quarantine, revocation checks, tombstone-wins anti-resurrection, draft-parent binding and stale-backup non-republication;
- required immutable `ProfileExecutionContext` for profile-sensitive async work and independent windows so profile switching cannot retarget callbacks;
- hardened update agility to distinct authorized role/key/algorithm/root-policy thresholds with sequential dual-threshold root rotation;
- added D-171 / R-330..R-333 / T-125..T-128 and deliberately reopened one Run-7B criterion each in V-003, V-005, V-021 and V-023; external review is paused and engineering remains unauthorized.

## 2026-09-28 17:18 PHT — Run 6B ADR-0148 evidence closure

- added bounded account-private sync evidence: 6/6 scenarios, 4/4 forbidden mutations caught; V-003 is now 11/12 with independent model review remaining;
- added a 20-vector `om.voice-note.v1` / `om.location-card.v1` corpus with independent Python/Node deterministic-CBOR/semantic agreement; V-005 is now 9/9 and pre-engineering passed;
- completed V-021 repo-local evidence (7/7) for personal-grant notices, PlatformAdapter capability mapping, ClientHost root/transparency constraints and multi-profile isolation; independent review remains open;
- completed V-023 repo-local update-lifecycle/signature-agility evidence (8/8), with 15/15 scenarios and 5/5 security mutations caught; independent review and live pre-release drills remain open;
- reactivated the consolidated different-vendor `core_kernel` review handoff; engineering remains unauthorized;
- refreshed all 33 portable evidence producers successfully in bounded batches.

## 2026-09-28 16:30 PHT — Run 6A ADR-0148 V1 product-base promotion

- promoted visible multi-IdentityProfile switching into the V1 reference-client baseline while preserving strict profile/push/cache/deep-link isolation;
- promoted bounded same-profile account-private E2EE sync for drafts plus selected non-authoritative preferences, with concurrent-draft loss protection and explicit authority/security/provider/push/vault exclusions;
- standardized non-critical `om.voice-note.v1` and one-shot `om.location-card.v1` presentation extensions beside W0-valid fallbacks, without changing the frozen W0 body grammar;
- made first-public-release software metadata/verifiers algorithm-agile and ML-DSA-65 co-signature-ready under owner-pinned root policy, without turning distribution keys into OM protocol identity;
- intentionally reopened local evidence obligations in V-003, V-005, V-021 and V-023 for Run 6B; the consolidated different-vendor review handoff is paused and engineering remains unauthorized.

## 2026-09-28 15:22 PHT — Run 5B ADR-0147 executable closure

- added crash-safe verifier-local recovery timer evidence: 5/5 normative restart/restore scenarios pass and 2/2 forbidden acceleration mutations are caught;
- added bootstrap lifecycle/restore evidence: 5/5 scenarios pass and 4/4 mutations are caught for hidden request expiry, ninth-after-prune, stale OPEN/RESERVED/private-key restore and stale replay-only restore;
- reconciled the ADR-0146 harness so MessageRequest v1 no longer carries a fictitious independent `request_expires` input;
- closed the new repo-local V-002/V-003/V-015 criteria, yielding 9/10, 10/11 and 14/14 respectively; remaining blockers are independent review / phase independence;
- reactivated the consolidated different-vendor review handoff; engineering remains unauthorized.
- all 29/29 portable evidence producers pass in bounded batches; validator/docs/79-document continuity checks pass.

## 2026-09-28 14:43 PHT — Run 5A ADR-0147 normative lifecycle/restore closure

- froze the MessageRequest bootstrap acceptance deadline as the authenticated minimum of reply-route and reply-delivery-key expiry, with no hidden local request-expiry value;
- made the eight-candidate OPEN budget durable and non-replenishable across rejection/GC, pinned replay-only expiry/GC, and excluded live bootstrap state from historical restore;
- defined crash-safe conservative monotonic-duration persistence for recovery-alone activation and related security-critical local durations;
- intentionally left the new V-002/V-003/V-015 evidence criteria open for Run 5B.

## 2026-09-28 10:10 PHT — Run 4 final bootstrap/resource closure

- accepted ADR-0146 / D-168 / R-318..R-319 / T-119..T-120 and added M8;
- atomically terminalized all losing `RESERVED` bootstrap candidates on every MessageRequest exit from `OPEN` before request-scoped key destruction;
- clarified `reply_route` as one logical response with exact retries and at most 8 distinct candidate wrappers while open, rather than one physical deposit;
- bound sealed-wrapper expiry to delivery-key/route/request acceptance and bounded replay/tombstone retention without imposing a globally short offline-delivery lifetime;
- added focused V-003/V-015 executable evidence and removed stale historical wording that still called K2/EventID dedupe-before-ack unspecified.

## 2026-09-28 09:17 PHT — Run 3 ADR-0145 final bootstrap closure

- closed the lost-ack liveness gap by recognizing an existing exact `COMPLETE` replay before live delivery-key lookup/HPKE, allowing completed bootstrap private-key destruction without breaking idempotent retry;
- made exactly one logical bootstrap response authoritative per MessageRequest, with first valid crash-consistent commit winning recipient-device races and different/losing wrappers unable to create another branch;
- made winning bootstrap acceptance, authenticated recipient binding/imported established delivery state, inner dedupe, replay `COMPLETE` and request `COMPLETE_REPLAY_ONLY` closure one crash-consistent transition;
- added ADR-0145 / D-167 / R-317 / T-118 and focused V-003 evidence: 5/5 scenarios pass and 3/3 deliberate mutations are caught;
- V-003 is now 8/9 with only independent review remaining; engineering remains unauthorized.


## 2026-09-28 08:57 PHT — Run 2 ADR-0144 executable closure

- added `sealed_delivery_adr0144_composition.py`, a focused V-003 bounded harness for request-scoped `reply_delivery_key` semantics and stale-ACS × `RESERVED` composition;
- 12/12 scenarios pass and 7/7 deliberate security mutations are caught, including premature stale-state `REJECTED`, wrong route/profile/expiry, key/id reuse, cross-request transplant and authority promotion;
- marked the ADR-0144 local V-003 criterion met, making V-003 7/8 with only independent review remaining;
- activated the consolidated different-vendor review brief and compatibility handoff; engineering remains unauthorized.


## 2026-09-28 08:12 PHT — Run 1 pre-review semantic hardening (ADR-0144)

- made MessageRequest `reply_delivery_key` fresh/request/profile/route/expiry scoped with explicit lifecycle disposal and no promotion into relationship authority;
- composed sealed-delivery `RESERVED` replay state with stale trusted-ACS refresh so temporary authentication indeterminacy cannot become premature terminal `REJECTED`;
- added R-315/R-316, T-115..T-117, M7 and strengthened M6/V-003 traceability;
- reconciled A-020 with ADR-0111;
- added one intentionally-open V-003 local composition-evidence criterion for Run 2; external review is staged until that evidence closes.
## 2026-09-28 05:30 PHT — Run 3 pre-Opus reconciliation

- reconciled V-002/V-003/V-015/V-016/V-017 progress prose to the authoritative gate ledger without changing gate passage;
- reconciled stale V-016 prototype metadata/README to ADR-0121/0122/0123 while preserving recorded TLC outcomes;
- rewrote the final independent-review brief around `make verify-drop` for raw ZIPs and full `make verify` only in a tool-complete Git worktree;
- converted `verification/CLAUDE_OPUS_5_5_HANDOFF.md` into a stable redirect to the consolidated independent-review brief;
- removed stale V-002 evidence aliases for the retired `AccountControlState` files and updated the TLA+ pin example to the current `AccountControl.tla` / `AccountControl.cfg`;
- refreshed agent/verification guidance and snapshot state for the final pre-Opus checkpoint;
- reran `make verify-drop`: 24/24 portable producers plus validator/docs/79-document continuity manifest passed; stronger committed Rust/TLC evidence was preserved because this runner lacks that toolchain/network.

## 2026-09-27 18:08 PHT — V-001 ADR-0140 and ADR-0142 runtime completion

- added bounded sealed-delivery HPKE verification under the existing V-001 prototype, reusing the pinned draft-ietf-hpke-pq-05 hybrid suite with ADR-0142 route-bound info/AAD, padding buckets and explicit sender-authentication separation;
- added runtime checks for all five padding buckets, wrong route/key/expiry, route/info and AAD tampering, encapsulation/ciphertext tampering, malformed buckets, and no clear-sender fallback;
- added ADR-0140 freshness persistence/reload, cross-provider migration, lower-generation rollback, equal-generation equivocation and device/profile/generation/provider-pool one-time-key transplantation cases;
- reviewed the intentional vector drift, regenerated the canonical corpus and extended the independent Python checker and current-tree result report;
- full V-001 runner passed on native, wasip1 and Node/V8: 90/90 selftests, Python vectors 236/236, HPKE KATs 33/33 per runtime, ACVP 80/80 per implementation, and persistence 249 steps / 367 messages;
- repository-wide `make verify` passed phase-1 evidence (347,316 bounded cases), all 83 configured TLC outcomes, architecture validation, docs checks and registry immutability;
- V-001 pre-engineering is 13/13 passed; the overall gate remains open for separate pre-release obligations and engineering remains unauthorized.

## 2026-09-27 17:23 PHT — V-001 F-9 independent re-review

- GPT-5.6 Sol independently accepted the ADR-0135 §7 / ADR-0136 rule 4 responder-side `IK_A` binding remediation for review finding F-9;
- recorded V-001 criterion 10 met and added the focused review as evidence, bringing V-001 to 11/13 pre-engineering criteria;
- reran the ADR-0140 and ADR-0142 bounded/static checks successfully; no current Rust/native/wasm execution is claimed because this runner lacks Rust/Cargo/Wasmtime and outbound network;
- the only remaining V-001 pre-engineering items are the ADR-0140 current runtime rerun and ADR-0142 sealed-delivery HPKE runtime evidence; engineering remains unauthorized.

## 2026-09-27 16:44 PHT — V-005 Rust/CDDL verification

- reproduced the cross-gzip false failure and changed the runner to compare decompressed canonical JSON; no vectors changed;
- ran the pinned Rust strict codec, Node strict codec, Rust CDDL 0.10.7 and Ruby CDDL 0.12.14 against the current 735-vector corpus;
- Rust and Node had zero mismatches; both CDDL validators agreed on 399 applicable cases; all 276 generated examples had zero unexpected Rust strict-codec rejections;
- recorded Cargo and canonical corpus SHA-256 in the machine result, and marked V-005 pre-engineering 8/8 passed with independence satisfied;
- overall V-005 remains open for its pre-release criterion; other gates and engineering authorization are unchanged.

## 2026-09-27 13:08 PHT — verification continuation / ADR-0143 crash-safe sealed delivery

- attempted the Run-3 next item first: the current V-005 Rust/CDDL rerun remains blocked because this runner has no Rust toolchain and outbound DNS cannot reach rustup; no historical result was relabeled current;
- added a focused V-003 sealed-delivery state model covering key refresh/overlap/revocation, expiry, route binding/migration, exact retry, replay collision, persistence failure, no clear fallback and inner-auth authority;
- found and corrected a crash-consistency ambiguity where a crash after durable outer replay reservation but before inner acceptance could make an exact retry look terminal and lose the message;
- accepted ADR-0143 / D-165 / R-314: durable replay state is `RESERVED` / `COMPLETE` / `REJECTED`, exact-wrapper `RESERVED` retries resume, and inner K2 state plus `COMPLETE` commit before app-visible release/ack;
- focused V-003 result is 15/15 passing scenarios with 5/5 deliberate mutation controls caught; the existing sealed-delivery state criterion is now met, leaving V-003 open only for independent review (6/7 pre-engineering criteria);
- V-001 sealed-delivery HPKE execution, V-005 Rust/CDDL agreement and V-015 real traces remain open; engineering remains unauthorized.

## 2026-09-27 11:19 PHT — Run 3 established 1:1 sealed delivery (ADR-0142)

- promoted sealed delivery into W0 for ACTIVE accepted 1:1 relay/mailbox deposits, with no clear-sender fallback under CryptoProfile 1 and no weakening of inner sender/session authentication;
- added signed route-bound `SealedDeliveryKeyRecord` (type 22) and unsigned HPKE `SealedDeliveryEnvelope` (type 23), keeping large hybrid delivery keys out of `RelaySet`;
- added the one-request `reply_delivery_key` bootstrap to `MessageRequestPayload` so the first post-acceptance response can already be sealed without treating first contact as established traffic;
- froze the existing draft-ietf-hpke-pq-05 MLKEM768-X25519/HKDF-SHA256/ChaCha20Poly1305 suite for sealed delivery, exact key/encapsulation lengths, route/info/AAD domains, padding buckets, fresh delivery IDs, replay-before-release and cross-route reseal rules;
- changed established mailbox metadata descriptors/models so sender identity/device correlators and Double Ratchet header shape are absent by design while residual timing/size/source-network/volume/opaque-route leakage remains declared;
- expanded V-005 to 735 vectors (71 positive / 664 negative); Python generation + Node strict decoding agree on every current verdict/error class, and the dedicated ADR-0142 plus metadata bounded checks pass;
- Cargo/Rust and the current CDDL two-codec rerun are unavailable in this runner, and real V-015 packet/provider/browser traces remain open; engineering remains unauthorized.

## 2026-09-27 08:42 PHT — Run 2 security semantics/spec consistency (ADR-0141)

- made `om.acs.legacy_policy` critical/fail-closed across current CDDL, registries, machine queue/decisions/requirements and conformance semantics while keeping activation later-wave/off by default;
- added normative bearer-capability transport hygiene: encrypted/authenticated authorization transport, no URL/query/fragment/cookie placement, no-referrer/cross-origin leakage, `no-store`, and log/analytics/crash redaction;
- defined RecoveryRecord v1 monotonic record generation/recovery epoch/last-known ACS observation, stale-snapshot rollback handling and mandatory revalidation of provider-mutable data;
- made RecoveryRoot metadata-confidentiality exposure explicit and added optional independently held `RelationshipRecoveryKey` split custody for Maximum Privacy;
- corrected passkey privacy wording, froze all assigned W0 domain labels and the default 300-second skew, and removed stale `proposed` semantics from frozen AccountControlState fields;
- reconciled stale A-035 group-call text with ADR-0132 participant-only SFrame media-key delivery and strengthened semantic closure guardrails;
- regenerated V-005 to 681 vectors (69 positive / 612 negative); Python generator + Node strict harness agree on all 681 current cases; Rust/CDDL two-codec evidence remains open because Cargo is unavailable;
- Run 3 sealed delivery and later v1-scope conveniences remain outside this checkpoint; engineering remains unauthorized.

## 2026-09-27 08:08 PHT — Run 1 AsyncKeyBundle/prekey remediation (ADR-0140)

- pulled the unlinkable AsyncKeyBundle design into v1: retired provider-visible ACS keys 3/4; relationship-pinned ACS now supplies device authority;
- added cross-provider signed-prekey generation rollback/equivocation state and monotonic restore/private-sync semantics;
- context-bound one-time prekeys to device/profile/generation/provider pool/kind/key id/public key and documented compromise rotation;
- updated provider/recovery contracts, metadata descriptors, CDDL/registries, logical schemas, V-001 source and V-005 vectors;
- regenerated 679 V-005 vectors and passed the current Node/Python checks plus the dedicated ADR-0140 bounded checker;
- no Rust toolchain was available, so current V-001 native/wasm/vector evidence and V-005 Rust/CDDL agreement were not claimed; V-005 two-codec passage is intentionally reopened;
- sealed sender/SPQR and Run 2+ remediation are outside this checkpoint; engineering remains unauthorized.

## 2026-09-26 23:45 PHT — final architecture design closure (ADR-0139)

- completed Set 2: whole-canonical-tree normative ambiguity sweep, full requirement/threat/decision/invariant traceability closure, 11-scope adversarial architecture pass and definitive engineering handoff;
- fixed the final semantic ambiguities: SecurityAdvisory lifecycle cannot lower an established relationship floor, and frozen W0 CDDL/registries/domain-separation rules outrank logical modeling schemas for wire bytes;
- reconciled stale current-document references to already-resolved A-032/A-035/A-045/A-046 and clarified that `OMA-ENC-1-candidate` is verification-only until V-014 freezes a reviewed production archive profile;
- added `machine/security_invariant_traceability.json` and the design-only Set-2 closure auditor; all 288 requirements are traced, all 93 threats have valid references/mitigations, all 32 named security invariants are mapped, and all 161 decisions are indexed;
- final adversarial review covers authority laundering, optional security context, stable correlators, fail-open/ABA/downgrade/recovery/provider-colocation/multi-device/multi-account/portability/offline/update/resource-exhaustion boundaries across all 11 engineering scopes; no additional blocking architecture choice remains;
- replaced the stale inactive/pending-freeze engineering handoff with a design-frozen source-of-truth/precedence and architecture-change contract;
- Protocol Continuity Bundle manifest check remains clean at 79 normative architecture/constitution docs; a 372-file disposable-probe bundle builds and verifies offline;
- no TLC, Rust, Wasm/native build or product engineering was run; all engineering scopes remain unauthorized and `engineering_authorized=false`.

## 2026-09-26 23:02 PHT — design closure set 1 (ADR-0138)

- resolved the remaining design queue A-030/A-031/A-036/A-054: narrow verifier-local LegacyPolicy retirement; a later unlinkable-account AsyncKeyBundle target; explicit no-deniability claim for v1 MLS groups; and a later sealed-sender delivery profile;
- replaced routine provider AccountID/DeviceCredential/ACS administration with independent provider/resource-scoped opaque capabilities that confer no account, recovery or E2E authority;
- repaired RecoveryRecord access with distinct RecoveryRoot-derived read/write capabilities and an opaque locator, while explicitly declaring AccountID/recovery-attempt exposure during actual RecoveryRequest mailbox fan-out;
- added ADR-0138, the sealed-sender profile, D-160, R-290..R-295, T-100..T-101 and design-only Set-1 evidence;
- design-only bounded/structural checks pass, including 216 metadata traces / 12 roles / 0 violations; no TLC, Rust, wasm/native runtime or product engineering was run;
- architecture validator reports `design_open=0`, all 23 verification gates remain open, and `engineering_authorized=false`;
- this checkpoint intentionally stops before Set 2: whole-repository ambiguity/traceability/adversarial closure and the definitive engineering handoff.

## 2026-09-26 16:52 PHT — ADR-0137 security-equivalence evidence pass

- upgraded the ACS/TLA cross-check from one-way trace replay to bounded **bidirectional accepted-set equality**: 66,816 freeze-resolution candidates, zero table-only or TLA-only acceptances, plus explicit freeze/fork negatives;
- strengthened V-001 relationship binding with a public-ingress structural verifier and new established-session, persisted/reloaded-state and ambiguous-credential negative selftests; the structural evidence passes, while fresh Rust execution remains pending because this runner has no Rust toolchain;
- made W0 auth-layer metadata frequencies exact and mutation-tested, added 216-flow trace coverage and executable human/machine observer-matrix drift detection;
- split Privacy Pass issuance evidence into fresh/distinct, repeated nontrivial `r=2`, `r=1`, direct-sign and actual CSPRNG-path controls; every intended regression is caught;
- added provider negative-capability conformance checks covering key authority, recovery-authority laundering, directory/ACS type separation, IntroGate minting and role-colocation union semantics;
- the pinned TLC JAR was unavailable, so current-tree TLC was not freshly rerun; historical committed TLC results were left intact rather than overwritten;
- this is verification evidence, not ADR-0117 independent approval of ADR-0137. No gate was self-passed and engineering remains unauthorized.

## 2026-09-26 14:32 PHT — security invariants and core-boundary hardening (ADR-0137)

- made `spec/protocol/account-control-transition-policy.json` the machine-readable ACS authorization source; selective safety-freeze restoration now requires a parent device majority, self-challenge by every restored controller, and a child that regains voting majority;
- made inbound 1:1 relationship context non-optional in the V-001 prototype API and removed the unpinned decrypt path;
- added `METADATA_OBSERVER_MATRIX.md`, authentication-layer `device_auth` correlators and matching W0 trace coverage;
- defined W0 referral issuance unlinkability precisely and added a repeated-nontrivial-blinding-factor negative control;
- added provider/subsystem negative-capability contracts and `SECURITY_INVARIANTS.md`;
- added D-159, R-285..R-289 and T-095..T-099;
- **did not run point 6 / the cross-model equivalence pass**; its complete next-agent brief is `verification/reviews/BRIEF-2026-09-26-security-equivalence-pass.md`;
- design-side bounded ACS/metadata/privacy/abuse checks pass; TLC/cross-model evidence was not regenerated and Rust/wasm tooling was unavailable; engineering remains unauthorized.

## 2026-09-26 PHT — core_kernel independent-review remediation (ADR-0136)

- imported gpt-5.6-sol's core_kernel review (`checkpoint/20260926-0043`): all 7 items rejected, 9 findings (3 blockers);
- **F-1** any live controller vetoes recovery regardless of method count (TLA `veto_any` passes; `mut_method_count` caught);
- **F-2/F-3** device-only lift, threshold-disabling quarantine freezes, freeze resolution counts challenge signatures, lift rule state-derived; `acs_tla_crosscheck.py` replays every AccountControl trace through the §4.1 table;
- **F-4/F-9** responder binds IK_A to exactly one active committed DeviceCredential before any secret-key operation (session rule K12; V-001 `verify_initiator` + 11 vectors checked by the Python implementation; V-003 `pass-forged-initiator` 9.7M states, K12 gap reproduced); ADR-0135 §7 composed authentication argument; V-001 reopened pending re-review;
- **F-5** `stable_device_correlator` declared for KeyMaterialProvider, mailbox-class Homes (sender IK in every envelope, found while fixing) and push provider; A-054 opened;
- **F-6/F-7** RFC 9578 type 0x0002 with 48-byte-salt PSS and id-RSASSA-PSS SPKI key id; RFC 9474/9578 KATs; blind issuance and issuer-transcript test;
- **F-8** explicit continuity-bundle normative manifest with `check-manifest` and drill D7 (offline reconstruction);
- `make verify` passes; engineering remains unauthorized.

## 2026-09-25 11:54 PHT — verification lifecycle hardened for one-way execution

- accepted ADR-0134: design closure, repo-local pre-engineering evidence, post-scope implementation evidence and genuinely external evidence are now explicit phases;
- eliminated the verification chicken-and-egg ambiguity: non-production feasibility prototypes under `verification/` are permitted, but evidence requiring the real blocked implementation belongs to pre-release;
- added explicit `feasibility_criteria` for V-004/V-008/V-009/V-011/V-013/V-014/V-019, so `core_kernel` needs only proof that frozen W0 interfaces can carry later waves, not full later-wave gate passage;
- promoted hidden V-* obligations from stale `remaining` notes into canonical criteria and removed free-text remaining lists;
- strengthened the validator: exact criterion matching, independence required for pre-engineering passage, and explicit feasibility criteria required for interface-impact gates;
- added `verification/CLAUDE_OPUS_5_5_HANDOFF.md`: the verification executor must exhaust repo-local evidence before one batched review queue, preventing agent-to-agent ping-pong;
- no engineering/environment verification was run in this pass; architecture validator and document/link consistency checks pass; engineering remains unauthorized.

## 2026-09-25 08:59 PHT — blocking architecture design queue closed

- accepted ADR-0131..ADR-0133 and resolved A-024..A-028, A-032, A-035, A-045..A-047 plus new A-048..A-053 findings; **open blocking design items = 0**;
- pinned core security decisions: dual PQXDH credential binding, exact replay ledger, AES-256-GCM-SIV, explicit v1 ratchet-header exposure, verifier-local recovery activation, incarnation-bound reductions, reciprocal first-contact identity binding, and version-pinned PQ/T HPKE;
- froze W0 referral Privacy Pass profile, participant-isolated group-call SFrame key distribution, remote-mini-app egress semantics and bilateral 1:1 plaintext-egress consent;
- froze Home API v1/CDDL/error registry, ProviderCapabilities JSON Schema and Rekor/C2SP-style release transparency; W0 conformance suites are specified;
- phase-1 executable verification passed with 347,316 bounded/adversarial cases; full `make verify` could not complete TLC because the isolated runner could not download the pinned TLC dependency; no gate was waived or auto-passed;
- engineering remains unauthorized pending V-* proof/review criteria.

## 2026-09-24 23:30 PHT — architecture review R1–R4 applied

- four independent architecture reviews (ADR-0117 records in `verification/reviews/`) applied as ADR-0126..ADR-0130;
- ledgers: R-200..R-277, T-050..T-089, A-030..A-036 and A-045..A-047, D-148..D-152; new gate V-023 (ClientHost update lifecycle);
- `machine/conformance_suites.json` registers every `conformance:<suite>` target; public release gates gain `applies_to`, `criteria` and `independence`, and planned releases are declared in the contract; the validator enforces both;
- V-005: 619 W0 vectors (ADR-0126 objects included) agree across the Rust and Node codecs and both CDDL tools;
- the RELEASE_WAVES §3 scope table is generated from the ledger; stale text on crypto (PQXDH + Double Ratchet), recovery default and onboarding corrected;
- `engineering_authorized=false`, all scopes unauthorized.

## 2026-09-24 02:29 PHT — independent hardening review (gaps, blind spots, weak spots)

- preserved the complete 09:20 current tree under `history/20260923-0920/`;
- added `HARDENING_REVIEW_20260924.md` with findings F-01..F-17 and their fixes;
- **fixed a circular engineering lock:** gates are two-phase (pre-engineering / pre-release) with independence levels, and engineering unlock is scope-granular across waves W0..W4 (`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`, ADR-0108/0109). Nothing is authorized;
- **onboarding and 0M:** Home tiers T0..T4 with a vendor-neutral sponsored starter mailbox as the first-run path from invites; vendor monoculture declared a 0M risk; second non-cloud reference provider required (ADR-0102, V-020, PR-013);
- **endpoint threats:** coercive/proximate adversaries, non-silent device linking, and a platform leakage matrix covering backups, snapshots, keyboards, spellcheck, extensions and notifications (ADR-0103, V-021);
- **honesty fixes:** PQ status is per conversation type (v1 groups are classical) with a ReInit path and objective adoption triggers (ADR-0107, V-019); recovery is informed-off with a ContinuityAssessment (ADR-0106) plus 1,620 bounded continuity cases;
- **lifecycles:** decentralized vulnerability response without a kill switch (ADR-0104, PR-012); account retirement/deletion/legacy (ADR-0105); public-surface illegal-content process (PR-014);
- added `machine/requirement_traceability.json` (all 182 requirements traced) and `machine/threat_register.jsonl` (T-001..T-040); validator guards enforce gate schema, scopes, traceability, threat references and the no-PQ-group-overclaim rule;
- added V-018..V-021, PR-012..PR-014, R-161..R-182, D-124..D-131, A-016..A-022 (resolved); R-104 superseded by R-176;
- `design_open=0`, `verification_open=21`, `engineering_authorized=false`, all scopes unauthorized.

## 2026-09-23 09:20 PHT — phase-1 architecture verification evidence

- preserved the complete 08:21 current tree under `history/20260923-0821/`;
- added a TLA+ `AccountControlState` model/configuration and transition regression vectors without claiming TLC execution;
- added bounded authority/state/convergence models covering V-002/V-004/V-008/V-009/V-011/V-016;
- added bounded bridge/archive/metadata models for V-013/V-014/V-015 and an offline continuity dry run for V-017;
- added two independently written Python/Node.js deterministic-CBOR foundation encoders with 14 agreeing fixtures for V-005 groundwork;
- executed 7,772 bounded/adversarial cases plus the CBOR and continuity checks with no modeled invariant violation;
- documented the V-005 gap: per-object CDDL/numeric-field/domain-separation registries still need to be frozen before that gate can pass;
- added standards/runtime feasibility reconnaissance for V-001/V-006/V-007/V-010/V-012;
- kept every V-001..V-017 gate open and `engineering_authorized=false`; evidence is not gate passage.

## 2026-09-23 07:56 PHT — 0M / permissionless services / bridges / escape hardening

- added **0M** as the seventh canonical OM meaning: zero parties have total control;
- made ServiceID creation local/permissionless with optional replaceable hosting, discovery and catalogs rather than a BotFather-style root issuer;
- replaced a human-vs-bot cognitive distinction with authority-based authorship and scoped `RepresentationGrant` for autonomous personal agents;
- resolved A-011 with typed BridgePrincipal, bridge-attested foreign provenance, explicit E2EE termination, fidelity mapping and loop lineage;
- resolved A-012 with public `om-archive-v1`, history/authority separation and provenance-safe foreign imports;
- added V-013/V-014 and expanded V-011;
- preserved the complete 07:18 tree under `history/20260923-0718/`;
- engineering remains unauthorized.

## 2026-09-23 07:18 PHT — Extension ecosystem / bots / expressive-media hardening

- resolved A-001/A-002/A-003/A-004/A-009 at architecture-design level;
- froze bounded inert WebP expressive-media profiles and self-contained conversation rendering;
- added portable immutable ContentPack releases, update lineage, provenance/fork semantics and optional entitlement proofs while keeping commerce external;
- added typed ServicePrincipal/ServiceID + ServiceInstallation architecture for bots, agents and local/remote LLMs;
- generalized ConversationEvent authorship to typed PrincipalRef + authenticated endpoint rather than forcing automation into human AccountIDs;
- froze capability-sandboxed WebAssembly Component Model/WIT local plugins plus isolated web apps and anti-fragmentation rules;
- added V-010..V-012 so plugin sandboxing, service authority/automation and media/pack parser safety remain verification-blocking;
- engineering remains unauthorized.

# 2026-09-22 23:02 PHT — architecture hardening / anti-capture checkpoint

- superseded the 14:34 engineering transition; returned phase to architecture hardening / pre-engineering;
- added canonical project philosophy/anti-capture constitution;
- selected layered licensing/contribution direction: MPL core, AGPL reference network services, Apache normative protocol/conformance artifacts, CC BY-SA documentation, DCO/inbound-equals-outbound and no ordinary broad copyright assignment;
- defined SeedLife/lonunjie as initial steward, with founder-only S0 custody accepted until trustworthy maintainers emerge and succession rails prepared now;
- made asynchronous handshake/MLS key-material publication a first-class provider/state-machine boundary;
- added one-to-one multi-device session management above the cryptographic ratchet;
- added MLS same-epoch Commit ordering and atomic OM group-policy/MLS binding requirements;
- added catastrophic RelaySet reachability recovery and stable BlobID/mutable BlobLocationRecord repair;
- added unified clock/time/expiry semantics;
- added self-hosted ClientHost update/deployment lifecycle;
- canonicalized AccountID as random 256-bit stable data, AccountControlState terminology, and one current ProviderCapabilities schema;
- added machine repository contract, architecture map/file registry/stewardship registry and executable drift validator;
- added ADR-0069 onward and new requirements/open questions/protocol logical schemas;
- preserved the complete 14:34 checkpoint under `history/20260922-1434/`.

# 2026-09-22 13:42 PHT — shared UI / OwnMessenger design checkpoint

- Added one shared `om-ui` architecture across PWA/iOS/Android/Desktop official clients.
- Added adaptive Compact/Medium/Expanded/Wide layout system and Chats/Calls/People IA.
- Added complete onboarding/discovery/share-contact/message-request/call/security/recovery/device/OM Home UX.
- Added semantic MotionSystem and perceived-performance targets.
- Added data-only theme/font architecture with CSP/Trusted Types hardening and theme-proof security semantics.
- Added shared accessibility/input release gates and PlatformAdapter capability contract.
- Added ADR-0051 through ADR-0058, design seed artifacts and current UI references.

# Changelog — 2026-09-21 master snapshot

## 2026-09-21 21:32 PHT — continuity / trust / personal-bootstrap checkpoint

- added AccountControlState, two-stage revocation, controller-promotion safeguards and head gossip;
- made recovery optional/off-by-default and separated identity/history/provider recovery;
- made user-owned Cloudflare + workers.dev the initial personal bootstrap model;
- reduced no-payment Cloudflare core to Worker + SQLite DO; R2 optional;
- added signed ContactCapsule Share Contact flow;
- added update/supply-chain trust, privacy profiles, notification policy, backup anti-rollback, retention and extensibility design;
- added ADR-0026..0042, logical schemas, requirements/open questions and current research references.

## 2026-09-21 10:08 PHT — Discovery Fabric / resilience checkpoint

- Replaced the implicit global-looking username model with an explicit Resolver + namespaced DirectoryProvider architecture.
- Added `ContactDescriptor` as the common output of directory, domain, QR/link, contact-card and LAN discovery.
- Added optional OHTTP/ODoH-inspired oblivious directory lookup.
- Added revocable/scoped Invite design and explicit offline-revocation limits.
- Added public `IntroductionServiceDescriptor`, short-lived `IntroductionTicket`, and IntroGate anti-spam/DDoS boundary.
- Added first-class multi-relay `RelaySet`, distinct capability per relay, automatic failover and optional redundant delivery.
- Added privacy-preserving rotating LAN rendezvous and offline-local-mode direction.
- Added modular censorship/network-resilience architecture with pluggable transports; explicitly no uncensorability guarantee.
- Added ADR-0019 through ADR-0025 and five new logical protocol schemas.
- Preserved prior 08:01 PHT checkpoint under `history/20260921-0801/`.


This is a design changelog, not a software release changelog.

## Philosophy and governance

The architecture now explicitly rejects the idea that safety should depend on a permanently benevolent universal platform operator. OM still permits authorities, but each authority is deliberately narrow. Users control their own relationships; group administrators control their own groups; relays control their own service; directories control their own namespace; media providers control media transport allocation; none of those powers should compose into universal content or identity control.

A recurring review question is now:

> **Where did authority go?**

Every new component must answer that question and show that it does not silently re-centralize OM.

## Cryptography

Crypto agility is upgraded from a library abstraction into a protocol property. OM negotiates approved, versioned CryptoProfiles and records downgrade floors. Account continuity is no longer allowed to depend on a particular signature/KEM family. PQ/hybrid support is preferred, but draft or experimental suites remain explicitly replaceable.

## Relay and metadata

AccountID is never a message-routing address. Accepted relationships receive random opaque delivery capabilities that can rotate. Per-destination transport wrappers should prevent easy cross-relay correlation of identical group ciphertext.

## Storage

Delivery queue, local database, hot sync, attachment blobs, cold archive, and recovery are separate storage classes. R2 is an example BlobStore, not “the OM database.”

## Calls

The old “WebRTC later” note is replaced by a complete media architecture: encrypted signaling through OM, SFrame above WebRTC/SRTP, adaptive LAN/P2P/TURN/SFU routing, strict no-silent-privacy-downgrade behavior, and provider-independent `MediaProvider`/`RoutePolicy` interfaces.

## Cloudflare

Cloudflare is documented as a strong zero/low-friction reference backend:

- Workers/Durable Objects: possible relay implementation;
- R2: possible encrypted blob store;
- Realtime STUN/TURN/SFU: possible call connectivity/media provider.

OM core does not branch on “Cloudflare.” Provider quotas/pricing are discovered/configured operationally and must not become wire semantics.

## Carrier messaging

SMS/MMS/RCS remain architectural adapters but are removed from first implementation scope. Store/carrier approvals must not block OM-native messaging or calls.

## 2026-09-22 07:34 PHT — Operatorless core / client ecosystem checkpoint

- Made the operatorless nature of OM explicit: no mandatory OM-operated runtime service.
- Added PWA-first/not-PWA-only client strategy and shared portable OM Core direction.
- Added operatorless provisioning and Deploy-to-Cloudflare/template path.
- Added local EvidencePackage export and removed any architectural requirement for a central AbuseDesk.
- Split protocol/project/deployment/provider/distribution legal/operational roles.
- Added optional convenience-provider/business model without protocol privilege.
- Added Creator-Disappearance Gate.
- Added ADR-0043 through ADR-0050 and three new logical schemas.

## 2026-09-22 14:34 PHT — Engineering handoff / final design-gap checkpoint

- moved phase from architecture consolidation to architecture candidate + engineering/adversarial validation;
- added engineering start/handoff/repository documents;
- added ClientHost/DeviceVault/PWA migration architecture;
- added multi-IdentityProfile isolation now, visible multi-account switching later;
- added semantic RichTextV1 and hostile-content/link-preview safety design;
- froze durable-control-signature vs message-deniability direction;
- added LocalDataClass and privacy-safe diagnostics;
- selected deterministic CBOR direction for canonical signed/wire objects;
- made AccountControlState TLA+ model a production implementation gate;
- added production crypto integration gate and current security/crypto re-audit;
- required light/dark/Auto for all themes and optional distinct profile theme suggestions;
- added ADR-0059..0068 and new logical schemas;
- preserved 13:42 checkpoint under `history/20260922-1342/`.

## 2026-09-23 05:45 PHT

- resolved former Q-001..Q-061 architectural choices and replaced generic open-question tracking with typed resolution/design/verification/release/deferred ledgers;
- removed country-specific canonical legal gating; adopted jurisdiction-neutral role-scoped operational/legal boundaries;
- strengthened standard-license warranty/liability and downstream-operator separation without field-of-use restrictions;
- made the six meanings of OM canonical in `NAMING.md` and project philosophy;
- added pre-engineering architecture queue for expressive media/stickers/GIF-like animation, portable external content packs, creator-commerce boundaries, bots/LLMs, communities/spaces/rooms, broadcast channels, roles/topics/plugins/bridges and first-release messenger baseline;
- added ADR-0079..0083 and new machine requirements/decisions;
- preserved the 2026-09-22 23:02 checkpoint under `history/20260922-2302/`.

## 2026-09-23 06:15 PHT — Messenger baseline + Spaces/Rooms/Communities hardening

- froze ordinary first-release messenger semantics and separated shared protocol state from local/account-private UX;
- unified interactive private groups/Rooms on ConversationID;
- defined Spaces/Communities as independent-object containers with bilateral links and explicit authority-parent delegation;
- selected capability grants + restrictions + delegation ceilings and no protocol-required magical owner/Admin bypass;
- selected layered replies/threads/Topics;
- separated private announcement Rooms from signed public Publications;
- resolved A-005/A-006/A-007/A-008/A-010 and added V-008 authority-model plus V-009 baseline-event convergence verification;
- added blocking A-012 import/export escape, A-013 metadata threat model, A-014 decentralized abuse/moderation hardening and A-015 protocol longevity so the broader hardening agenda remains machine-visible;
- preserved the complete 05:45 current tree under `history/20260923-0545/`;
- engineering remains unauthorized.

## 2026-09-23 08:21 PHT — metadata / decentralized safety / 0M longevity hardening

- resolved A-013 with a dedicated metadata threat model: canonical IDs remain above provider routing; private delivery uses scoped opaque capabilities/randomized outer envelopes; exposure is declared per role/path; stronger anonymity/padding remains explicit policy;
- resolved A-014 with layered user/Room/Space/provider enforcement, selective evidence, optional signed SafetyAdvisories, labeler-scoped pseudonymous subject references and no universal moderator/reputation/master blocklist;
- resolved A-015 with Protocol Continuity Bundles, immutable registry meanings, explicit generation/fork lineage, crypto/project/provider failure migration and dominant-client/extension anti-capture rules;
- added `METADATA_THREAT_MODEL.md`, `DECENTRALIZED_ABUSE_MODERATION.md`, `PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md` and supporting logical schemas/research references;
- added ADR-0098..ADR-0101, decisions D-120..D-123, requirements R-146..R-160 and verification gates V-015..V-017;
- closed the current A-001..A-015 design queue while keeping engineering unauthorized pending all blocking verification gates;
- preserved the complete 07:56 tree under `history/20260923-0756/`.
