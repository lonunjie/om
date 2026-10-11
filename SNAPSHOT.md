# OM Architecture Snapshot

Generated: **2026-10-11 PHT (UTC+08:00)**, after the operator accepted the `groups` unlock (ADR-0177) and set the PWA-canonical distribution direction (ADR-0178).

## State

- **Engineering is authorized for `core_kernel` (ADR-0169), `provider_reference` (ADR-0171), `reference_client_ui` (ADR-0172) and `groups` (ADR-0177) only**: `engineering_authorized=true`, 4/12 scopes. Every other scope is not authorized.
- **Review model:** Claude performs all reviews at the operator's direction (ADR-0167). Gates passed this way carry `pre_engineering_review_basis=operator_single_vendor_ADR-0167`, which is not vendor-independent review.
- **Design:** `design_open=0`. Review findings F-14..F-19 are fixed (ADR-0168, ADR-0170); F-20..F-23 are resolved by ADR-0172.
- **Verification:** 8 gates still have pre-engineering open. V-004, V-009 and V-019 passed pre-engineering on 2026-10-01 under the ADR-0167 model, which completes the evidence for `groups`; ADR-0177 (accepted 2026-10-11) unlocks it. V-006 passed pre-engineering the same day for the first-release web matrix; Windows Chrome/Edge is untested and carried pre-release (ADR-0172). V-018 is pre-release (ADR-0170). Pre-release items for groups stay open: the V-004 multi-client partition harness on two implementations, the V-009 corpus run by every implementation, the V-019 interoperable migration.
- **Ledgers:** 397 requirements / 397 trace rows, 151 threats, 200 decisions / 179 indexed ADRs, 40 security invariants.
- **Next:** build the `core_kernel`, `provider_reference`, `reference_client_ui` and `groups` crates/packages (web first, per [`ADR-0178`](decisions/ADR-0178-pwa-canonical-store-independent-distribution.md)) per [`ENGINEER_START_HERE.md`](docs/process/ENGINEER_START_HERE.md). V-006 pre-release: run the probe on Windows or drop Windows, publish the support matrix with regression tests.

## 2026-10-11 PHT — ADR-0179: default theme is OM Calm

- The default reference theme is named **OM Calm** (operator choice; the web client already used it). Display name only; stable ids and palette unchanged. Older ADR/evidence text saying OM Clear means OM Calm. D-200.

## 2026-10-11 PHT — ADR-0177 accepted; ADR-0178 PWA canonical

- **ADR-0177 accepted:** `groups` authorized (D-198). Cross-vendor re-check of V-004/V-009/V-019 stays a pre-release item.
- **ADR-0178:** the PWA is the canonical client; native shells are thin hosts of the same signed bundle; no store required (D-199, R-407, R-408, T-162). Motivated by the 2026-10-03 Bitchat India App Store removal and Android developer verification (volatile; re-verify before release).

## 2026-10-01 23:30 PHT — groups verification complete; ADR-0174..0176; ADR-0177 proposed

- **V-009:** [`v009-materializer`](verification/prototypes/v009-materializer/README.md): the Rust materializer reads the raw control chain and derives authority itself; the Python reference now covers every baseline kind; registers are epoch-major; expiry (R-235), mentions and forwarding (R-239) have expectations checked on both sides. 266 cases agree, 47 expectations and 3 forwarding vectors hold. Two bugs found by the random2 family (REF-4, MAT-1) are fixed (ADR-0174, R-397..R-399).
- **V-004 wire freeze:** [`v004-group-wire`](verification/prototypes/v004-group-wire/README.md) and [`group.cddl`](spec/cddl/group.cddl): GroupControlState 28, GroupControlIntent 29, GroupCommitBinding 30, GroupHistoryBundle 31, typed pin/poll bodies through an additive W1 event-body overlay. 383 vectors agree on the Rust and Node strict codecs, the frozen W0 corpus is unchanged, 105 semantic cases (19 bundle admission scenarios) agree on two checkers, two CDDL tools agree (ADR-0176, R-400..R-403, T-161). The signed-body limit of the W0 envelope (64 KiB) caps rooms at 512 members and bundles at 1024 items (F-27).
- **V-019:** [`v019-hybrid-pq-migration`](verification/prototypes/v019-hybrid-pq-migration/README.md): OpenMLS 0.9.0 has no ReInit, so the migration is an OM-level authenticated transition on `reboot`; 20 vectors, native and wasm32 byte-identical (ADR-0175, R-404..R-406).
- **Gates and unlock:** the three gates pass pre-engineering ([`review`](verification/reviews/2026-10-01-claude-sonnet-5-5-groups-review.md)); the `group_history` suite is specified. ADR-0177 proposes the `groups` unlock and waits for the operator.

## 2026-10-01 21:35 PHT — V-004 formal model and MLS ordering probe / ADR-0173

- **Model:** [`GroupCommit.tla`](verification/tla/GroupCommit.tla) (3 devices, a coordinator that may go offline or equivocate, racing reductions, bounded extra commits). The main config explores 676,654 states and passes safety and convergence under weak fairness; five mutations each give a counterexample ([`models-v004.json`](verification/tla/models-v004.json)).
- **Probe:** [`v004-mls-ordering`](verification/prototypes/v004-mls-ordering/README.md) runs OpenMLS 0.9.0 through ordered races, mutual removal, a partition fork with fork-resolution re-add, an external-commit roster gate, crash/restart and the past-epoch window. Native and wasm32-unknown-unknown (Node) agree byte for byte; iOS and Android are type-checked only.
- **Findings:** the design never named a winner among same-parent Commits (F-24); a merged MLS epoch cannot be rolled back, so a direct reduction in a partition forks (F-25); a removal on a losing branch is not a removal (F-26).
- **ADR-0173:** order key `(non-reducing, direct, DeviceID, binding hash)`, immediate acceptance, losing-branch obligations, roster-gated roster-neutral recovery, `sequencing_timeout` minimum 30 s. R-393..R-396, T-158..T-160, D-194.
- **Gate:** V-004 criteria 0, 1 and 4 are met (6/8). Open: the W1 wire freeze, the GroupHistoryBundle freeze, independent review.

## 2026-10-01 20:50 PHT — V-006 passes pre-engineering; `reference_client_ui` unlock / ADR-0172

- **Review:** [`2026-10-01-claude-sonnet-5-5-v006-review.md`](verification/reviews/2026-10-01-claude-sonnet-5-5-v006-review.md) passes V-006 for the first-release matrix (single-vendor model; the same session ran the probes, disclosed in the review).
- **Findings:** PRF is advertised but not returned by third-party providers (F-20), no first-release platform has a device-bound credential that can wrap the vault (F-21), Safari needs a focus wait between prompts (F-22). R-391 and R-392 added.
- **Matrix:** macOS Chrome/Safari/Firefox, iOS Safari, Android Chrome. Windows is untested and pre-release.
- **ADR-0172:** `reference_client_ui` is authorized.

## 2026-09-29 12:18 PHT — W0 provider/client review and `provider_reference` unlock / ADR-0170..0171

- **Review:** [`2026-09-29-claude-opus-5-5-w0-provider-client-review.md`](verification/reviews/2026-09-29-claude-opus-5-5-w0-provider-client-review.md) passes V-020, V-021, V-022 and V-023.
- **F-18:** a single-vendor block leaves about 42–48% of relationships alive even with the modeled fallback. PR-013 now needs at least 90% survivability before any resilience claim.
- **F-19:** the performance measurement plan gains sample-size, device-state, clock, fixture, CI-regression and no-release-telemetry controls.
- **V-018:** the participant study moves to pre-release (ADR-0170) because it needs the running client; it blocks any public or beta reference-client release.
- **V-006:** browser probe built (`verification/prototypes/v006-web-vault/`); macOS Chrome, Safari and Firefox recorded 2026-10-01 (PRF works everywhere; platform passkeys are iCloud-synced, not device-bound). Gate stays open.
- **ADR-0171:** `provider_reference` is authorized.

## 2026-09-29 11:50 PHT — core_kernel review and engineering unlock / ADR-0167..0169

- **ADR-0167:** the operator made Claude the sole reviewer. A recorded review pass (brief, re-executed evidence, adversarial checks, findings) now satisfies pre-engineering `independent_reviewer`, labelled honestly in the ledger. Pre-release and external review still need humans.
- **Review:** [`2026-09-29-claude-opus-5-5-core-kernel-review.md`](verification/reviews/2026-09-29-claude-opus-5-5-core-kernel-review.md) re-ran `make verify`, the V-001/V-003/V-005 toolchains and the provider traces.
- **ADR-0168 F-14 (blocker):** one stolen device could permanently lock a two-controller account through a quarantine-induced freeze. A freeze resolution now has normal-mode majority authority: a co-signed recovery vote counts once, and non-signing controllers may be revoked. Table (63 vectors), TLA/table equivalence (98,496 cases) and TLC (new pass config plus golden trap) agree.
- **ADR-0168 F-15/F-16/F-17:** the V-005 harness is scoped to W0 and its corpus regenerated (758 vectors, Rust = Node); W3 `sig_role` 7 no longer widens W0 validity (`enum_value_waves`); the `geo:` token grammar is normative. A clean-room Rust implementation agrees with the extension corpora (64/64 verdicts and bytes).
- **ADR-0169:** `core_kernel` is authorized. All its gates and V-017 passed, feasibility is evidenced, and its conformance suites are specified.

## 2026-09-29 10:21 PHT — consolidation of the parallel design lines / ADR-0166

Two OpenAI sessions worked in parallel from the same reviewed export (`76ad8f2`). The independent-review line
(`checkpoint/20260929-0634` → `checkpoint/20260929-0902`: ADR-0157..0160) and the Run line (`checkpoint/20260929-0900`,
Runs 1–7) reused the same ADR/ledger IDs for different content. ADR-0166 merges them into one tree.

- **Trunk:** the independent-review line keeps its IDs and semantics.
- **Renumbered Run line:** ADR-0161 terminal DeviceID revocation / mutable-record convergence / GroupHistoryBundle
  isolation; ADR-0162 Omnibar, visual and theme hardening on the ADR-0159 shell; ADR-0163 implementation-readiness and
  crash/retry contract; ADR-0164 dangerous algorithms; ADR-0165 protocol evolution and agent guardrails. Ledger IDs moved
  to D-182..186, R-361..386 and T-139..155.
- **Not adopted:** Run 1's `!` critical-ID class, `AccountHistorySnapshot` naming and TLS-exporter LAN binding, because
  the reviewed ADR-0157/0158 govern them. Run 3's OM Calm/Evergreen/Warm Slate/Violet palettes were also dropped in
  favour of ADR-0159's OM Clear/Paper/Iris/Graphite. Dependent Run text (ADR-0164 §8 LAN admission, ADR-0165
  criticality, V-004 GroupHistoryBundle criterion) was rewritten to the trunk's terms.
- **Guard:** `validate_0166_consolidation_guardrails` stops the superseded variants from returning.
- **Evidence:** the UI evidence now checks OM Clear contrast (38/38); dangerous-algorithm evidence checks the ADR-0158
  LAN profile (49/49); `make verify-drop` passes 49/49 producers.

## 2026-09-29 09:02 PHT — R16C V-008 Space/Publication wire freeze

- Froze the V-008 W3 wire surface in ADR-0160: `SpaceControlState` type 24, `SpaceDelegatedIntent` 25, `PublicationControlState` 26 and `PublicationEvent` 27.
- Added immutable numeric fields/enums and domain-separation labels; Publication uses dedicated signature role 7 rather than reusing device identity semantics.
- Typed `moderation.redact`, `topic.create`, `topic.update`, `topic.status.set` and `message.topic.set` bodies in [`events.cddl`](spec/cddl/events.cddl).
- Two independently written Python/Node codecs agree on 19/19 vectors (9 positive, 10 negative), including canonical body and SignedEnvelope bytes.
- V-008 is now 6/6 repo-local criteria; independent review remains required, so engineering remains unauthorized.

Detailed report: [`docs/reports/2026-09-29-r16c-v008-space-publication-wire-freeze.md`](docs/reports/2026-09-29-r16c-v008-space-publication-wire-freeze.md).

## 2026-09-29 08:50 PHT — R16B repo-local executable/spec/vector evidence

R16B continues the no-bounce rule: close only evidence whose semantics are already fixed, and do not manufacture protocol, independent-review, browser/device or human evidence.

- **V-004:** 531 fixed-semantics bounded cases close reductions liveness/revalidation, account-control event rules and immutable-incarnation stale reductions; repo-local progress is 3/7. Formal crash/partition/equivocation, target-runtime MLS ordering, disconnected same-epoch partition convergence and wire freeze remain open.
- **V-008:** existing independent-tool TLC is credited after ADR-0122 resolved its former cascade/subdelegation design gaps; 24,637 new bounded authority cases pass. Repo-local progress is 5/6; wire freeze remains.
- **V-011:** existing host-migration/alias TLC and tool-capability corpus are credited, with 68 new bounded membership/representation/removal/incarnation cases. Repo-local progress is 6/7; wire freeze remains.
- **V-020:** both missing provider-resilience criteria are now evidenced; repo-local progress is 9/9. Independent review still blocks pre-engineering passage.
- **V-024:** independent Python/Node codecs agree on 9 positive + 8 negative LAN vectors; passive-observer and offline-first-contact models pass. Repo-local progress is 3/4; real-browser WebRTC feasibility remains.
- **Deliberate non-closures:** V-013 wire freeze would allocate new canonical later-wave bytes; V-014 archive freeze still depends on review of its candidate encryption profile. R16B does not disguise either design choice as verification.
- **Overall:** 128/173 pre-engineering criterion rows now carry repo-local evidence. Gate-level state remains 24 overall-open / 23 pre-engineering-open; `engineering_authorized=false`, 0/12 scopes authorized.

Detailed report: [`docs/reports/2026-09-29-r16b-repo-local-evidence.md`](docs/reports/2026-09-29-r16b-repo-local-evidence.md).

## 2026-09-29 08:22 PHT — R16A verification-gate triage and static closure

R16A inventories all 24 architecture verification gates and closes only unambiguous repo-local/static gaps. Engineering remains unauthorized.

- **V-005:** ADR-0157 critical-prefix convergence now has explicit Python + Node evidence (8/8 cases), the strict Node/Rust reference-codec sources fail closed on unknown `om.critical.*` even with wire `critical=false`, and the vector generator includes the downgrade case. V-005 is now 13/13 repo-local criteria; its required independent review/phase evidence remains open.
- **V-013:** bridge contract now explicitly says a private-Room bridge is a full Room/MLS plaintext member and `allowed_event_classes` limits export, not reading. V-013 is 2/4; independent contract review and bridge wire freeze remain open.
- **V-020:** existing V-023 verifier-first/update lifecycle evidence and PC-I01..PC-I03 are credited to the verified-release provisioning criterion. V-020 is 7/9; blocked-vendor/suffix simulation and sponsor-revocation/non-sponsor-route modeling remain open.
- **Triage result:** `core_kernel` has no remaining repo-local model/vector criteria: V-001 pre-engineering passed; V-002/V-003/V-005/V-015/V-016 plus always-required V-017 are waiting on independent-review/phase evidence rather than more local architecture work.
- **State unchanged at gate level:** 24 overall-open gates, 23 pre-engineering-open gates, `engineering_authorized=false`, 0/12 scopes authorized. Human/device/runtime obligations are not approximated.

Detailed matrix: [`docs/reports/2026-09-29-r16a-verification-gate-triage.md`](docs/reports/2026-09-29-r16a-verification-gate-triage.md).

## 2026-09-29 08:05 PHT — R15 UI/UX architecture integration / ADR-0159

R15 reconciles the standalone UI/UX design track into the authoritative architecture rather than maintaining a parallel design tree. Architecture authority wins conflicts; presentation complexity remains progressively disclosed.

- **Shell and interaction:** Chats is the default compact root; optional destinations are user-composable; the Omnibar is the universal local-first find/do surface; compact conversation entry morphs between composer and Omnibar rather than stacking competing text-entry surfaces.
- **Authority boundary:** semantic actions resolve to typed Core intents, local presentation state or explicit PlatformAdapter capability. The Action Registry is not a protocol/RPC authority path.
- **V1 reconciliation:** scheduled send, eligible-device private draft sync, Spaces/Rooms/nesting, threads/Topics, Publication, polls, stickers/bounded loops, rich text, disappearing/view-once, ContactCapsule/MessageRequest, privacy-aware link previews, and selective report/evidence UX are represented without becoming mandatory default navigation.
- **Trust/onboarding:** T0–T4 Home/sponsor/client-origin disclosure, recovery-on-by-default, Protection profiles, device Suspend/quarantine, pending/final revocation and Safety Freeze are integrated. Calls use timeline notices rather than pretending live signalling is a durable `ConversationEvent`.
- **Detailed UX system:** semantic motion/color, OM Clear and preset theme pairs, icon/media contracts, accessibility/input mapping, responsive capability layouts, messaging/call/security state machines, notification attention rules and local adaptation contracts now live under `docs/ux-design/`.
- **Traceability:** ADR-0159 / D-181 and R-351..R-360 make the merge machine-traceable. No new verification gate was created.
- **Validation:** architecture validator passes at 1,298 current files; docs/registry and 81-document continuity checks pass; R15 and inherited UI/UX semantic/token validators pass; all 44 portable phase-1 producers were observed passing in bounded execution. PWA reference v3 passes 17/17 automated Chromium checks; Firefox/WebKit and AT/physical-device evidence remain unclaimed.

## 2026-09-29 06:34 PHT — independent remediation review / ADR-0157–0158

OpenAI GPT-5.6 Sol reviewed Anthropic-authored ADR-0154–0156 and accepted ADR-0154 while finding three additional semantic gaps in ADR-0155/0156 plus completing the already-tracked A-055 LAN design item. Review record: [`2026-09-29-gpt-5-6-sol-adr0154-0156-independent-review.md`](verification/reviews/2026-09-29-gpt-5-6-sol-adr0154-0156-independent-review.md).

- **ADR-0157:** reserves `om.critical.*` so future mandatory extensions fail closed across registry versions even if a sender clears the wire critical bit; separates implementation-private `DeviceHistorySnapshot` from interoperable `GroupHistoryBundle`; and makes scheduled-send queues compose-intent-only until actual send.
- **ADR-0158 / A-055:** freezes LanDeliveryProfile v1 at 900-second epochs, K=8 fixed-cardinality tags, purpose-separated pair/invite derivations, seed-encrypted WebRTC signalling that hides SDP/ICE credentials, ordered/reliable DataChannel framing, <=16 KiB chunks, bounded backpressure, ephemeral `om+lan://` RelayEntry reuse of `OM-SEALED-DELIVERY-ROUTE-v1`, ordinary SealedDeliveryEnvelope/MLS payloads, and ordinary IntroGate/MessageRequest first-contact authority.
- **Verification honesty:** A-055 is design-resolved, but V-024 remains open for two-codec vectors, passive-observer and first-contact mutation models, real-browser feasibility (or an honest unavailable result) and independent review. ADR-0157/0158 are OpenAI-authored and therefore this same review cannot independently approve them.
- **Portable current-tree evidence:** aggregate producers 1–30 passed before the execution wrapper stopped while producer 31 was starting; producers 31–44 were then run directly and all passed. No producer failed. This is recorded as split execution, not misrepresented as one uninterrupted aggregate run.
## Parallel Run line (Runs 1–7), consolidated by ADR-0166

The sections below are the historical record of the parallel Run line as delivered in `checkpoint/20260929-0900`, with its IDs already renumbered by ADR-0166 (ADR-0158..0162 → ADR-0161..0165, D-180..184 → D-182..186, R-349..374 → R-361..386, T-138..154 → T-139..155). Run 1's ADR-0157 (`!` critical-ID class, `AccountHistorySnapshot`, TLS-exporter LAN binding) was **not adopted**; the reviewed ADR-0157/0158 above govern those topics. Run 3's OM Calm/Evergreen/Warm Slate/Violet palettes were not adopted either. Counts quoted below are Run-line counts, not current state.

### 2026-09-29 — Run 6 / ADR-0164 dangerous algorithms

Run 6 adds a normative implementation-order supplement for the highest-risk transitions: AccountControlState child validation; 1:1/sealed-delivery receive/replay; group Commit journaling/recovery; RelaySet/BlobLocationRecord branch selection; HistoryBundle import isolation; extension dispatch; LAN channel/frame admission; and provider/network unknown-outcome reconciliation. Each algorithm states its validation order, state transitions, crash/failure behavior and forbidden shortcuts. The machine contract is [`machine/dangerous_algorithm_contracts.json`](machine/dangerous_algorithm_contracts.json); the bounded implementation-readiness harness passes **49/49 cases**. No wire field, cryptographic primitive or authorization gate changed.

Run-6 machine additions are D-185, R-376..R-380 and T-148..T-151. Current ledgers contain **359 requirements / 359 traces, 140 threats, 183 decisions / 161 ADRs**. Architecture validation reports **894 current files**, `design_open=0`, 24 verification gates overall-open, 23 pre-engineering phases open and **0/12 scopes authorized**. The portable phase-1 suite now contains **47/47 producers**, all passing in bounded batches. The continuity normative manifest is **82/82**.

### 2026-09-29 — Run 5 implementation-readiness hardening (ADR-0163)

ADR-0163 converts several previously scattered implementation obligations into one canonical transaction/retry contract without changing frozen W0 wire bytes. Mutations that can cross a crash or network uncertainty boundary must preserve a stable logical operation identity and security context before the first external effect when required; a timeout/disconnect after transmission is an **unknown outcome**, not a rejection, and safe retries reuse the same semantic identity rather than creating a second operation. Durable local commit precedes irreversible local release or success acknowledgement.

For groups, the prior crash/retry checklist is now an explicit local journal: `INTENT_DURABLE -> CANDIDATE_PREPARED -> PUBLISHED_UNCONFIRMED -> ACCEPTED`, with `SUPERSEDED`/`ABANDONED` terminal alternatives. Exact prepared MLS Commit bytes/hash and tentative post-state are persisted before publication; relay acknowledgement is not canonical acceptance; application messages cannot leave a tentative branch; duplicate accepted delivery is idempotent; and a losing branch is inert and must be rebased/revalidated from canonical state. These rules do not close V-004: formal crash/partition analysis, MLS runtime feasibility, wire freeze/two-codec evidence and independent review remain external verification obligations.

The Core/UI contract now distinguishes `PendingNetwork` from `PendingReconciliation`, and provider `request_id` idempotency must survive provider process restart for the advertised retry horizon. A new machine contract covers all 12 scopes, separating authorization dependencies from integration prerequisites and recording normative sources, restart contracts, retry/error semantics and forbidden shortcuts. The derived implementation layers are `core_kernel`; then `reference_client_ui`, `provider_reference`, `groups`, `portability`, `expressive_media`, `local_delivery`; then `calls`, `communities_publications`, `services_bots`, `plugins`, `bridges`. They are dependency guidance only, never engineering authorization.

Run 5 also corrected stale handoff wording around the 12-scope gate graph and the already-selected one-to-one/session/group semantics. Machine additions are D-184, R-372..R-375 and T-146..T-147. `design_open=0` and engineering remains unauthorized. Portable evidence passes **46/46 producers** in bounded execution; continuity remains **81/81**. Current ledgers contain **354 requirements / 354 traces, 136 threats, 182 decisions / 160 ADRs**. Final architecture validation reports **889 current files, 24 gates overall-open, 23 pre-engineering phases open, and 0/12 scopes authorized**.

### 2026-09-29 08:45 PHT — Run 4 integration/adversarial release-candidate pass (no new ADR)

Run 4 re-read the current handoff, machine ledgers, deferred product queue, licensing/registry machinery and the semantic corrections from ADR-0157..0162. No new protocol/product architecture defect requiring an ADR survived the pass. Four integration defects were fixed instead: (1) Q-048/DP-011 still described curated theme names/tokens as future work even though ADR-0162 fixed OM Calm, Evergreen, Warm Slate and Violet; (2) DP-007 still said exact theme resource/count limits were deferred even though the theme schema already freezes 256 assets and a 32 MiB per-asset declared-byte ceiling, so only aggregate/decoded/platform/catalog limits remain deferred; (3) [`ENGINEER_START_HERE.md`](docs/process/ENGINEER_START_HERE.md) still listed eleven scopes and omitted `local_delivery`/V-024; and (4) the file registry contained a vanished GitHub workflow and a UX-JSON license drift that ordinary `make check` did not detect.

The fast verification path now runs `tools/om_registry.py check`. [`LICENSE.md`](LICENSE.md), REUSE policy and the file registry consistently classify structured `docs/ux-design/*.json` artifacts under Apache-2.0 while prose/design documentation remains CC-BY-SA-4.0. A Run-4 validator guard keeps the theme-deferral and 12-scope handoff boundary from silently regressing. Engineering remains unauthorized; no V-* independence/runtime obligation is self-approved.

Run-4 validation is clean: architecture/docs/file-registry checks pass at **885 current files**, `design_open=0`, 24 overall-open verification gates, 23 pre-engineering phases open and **0/12 scopes authorized**; continuity is **81/81**; and all **46/46 portable phase-1 producers pass** in bounded execution (1–30, then 31–46 after the wrapper timeout).

### 2026-09-29 08:20 PHT — Run 3 product/UI integration audit (ADR-0162)

ADR-0162 removes a canonical-product contradiction between the older three-destination shell and the newer conversation-first direction. The default shell is now the conversation workspace plus an Omnibar; Calls and People remain first-class contextual/searchable surfaces and may be user-pinned, but are not required persistent destinations. Omnibar learning is local to an IdentityProfile, uses bounded ranking facts rather than durable raw-query history by default, never sends a query merely because the user typed, never becomes trust/identity/authorization evidence, and cannot let asynchronously arriving results replace an already armed keyboard target. Destructive or security-sensitive commands retain ordinary Core intent and confirmation requirements regardless of learned rank.

The visual system now has explicit functional semantic roles, an accessible OM Calm light/dark seed palette, a theme-proof semantic icon contract, and curated preset families. Untrusted theme `icon` assets are decorative only; alternate semantic icon families require reviewed client assets. Activated theme bundles require both light and dark variants and safe relative asset paths, with bounded asset sizes. Density, motion intensity, reduced-motion and alert/accessibility behavior remain explicit user-owned controls. Call UI keeps a user-pinned visual participant stable across active-speaker changes and treats face/hands visibility as an accessibility concern.

Executable ADR-0162 evidence passes **38/38 cases**, covering OM Calm contrast pairs, theme path rejection, profile-local adaptive ranking, exact-match priority, armed-result stability, destructive-command confirmation and the decorative-only theme-icon boundary. The portable drop suite passes **46/46 producers** in two bounded invocations because the execution wrapper ended the first after producers 1–23 passed; producers 24–46 then all passed. The normative continuity manifest now passes **81/81 documents**. Current machine state after this checkpoint: **884 current files; 350 requirements / 350 requirement traces; 134 threats; 181 decisions / 159 ADRs; `design_open=0`; engineering remains unauthorized.**

### 2026-09-29 07:20 PHT — Run 2 protocol/security semantic audit (ADR-0161)

ADR-0161 closes four security-relevant semantic gaps found by a cross-document authority/convergence audit. First, `permanently_revoked_devices` is now a monotone DeviceID tombstone set across both messaging and control roles; returning hardware or restored installations use a fresh DeviceID/credential. Second, frozen W0 RelaySet and BlobLocationRecord now have exact previous-hash/+1-version branch semantics with out-of-order reconsideration and deterministic effective-record selection (highest version, then smallest canonical hash among currently eligible terminal signers), so network arrival order cannot create durable route/location disagreement. Third, Room HistoryBundle is bound to Room, author, recipient membership incarnation and GroupControlState/MLS epoch, while its items live in a bundle-local inert namespace and cannot execute any live ConversationEvent/Room/control reducer. Fourth, ACS `minimum_crypto_profile` is pinned to the `one_to_one` security-generation axis; group-only profiles are invalid for that field and Room/group downgrade floors remain independent.

The ACS model passes **16,200 cases with 0 violations and 57 vectors** after adding an explicit revoked-messaging-relink negative. The ADR-0161 bounded protocol model passes reordered branch-convergence, continuity/signer negatives and HistoryBundle state-isolation cases. The V-003 floor model passes **17/17 vectors**, including mixed one-to-one/group generation cases and rejection of a group-only ACS minimum. The portable drop suite passes **45/45 producers** in bounded execution. V-002 and V-003 gain evidenced repo-local criteria but remain open for independent review; V-004 gains a stronger still-open HistoryBundle wire-freeze criterion. Current machine state: **876 current files; 343 requirements / 343 requirement traces; 131 threats; 180 decisions / 158 ADRs; 79/79 continuity documents**. Engineering remains unauthorized.

### 2026-09-29 06:07 PHT — Run 1 semantic follow-up (ADR-0157)

The follow-up audit found three cross-file semantic issues that ordinary design-closure/link checks did not detect. ADR-0157 closes all three at the design level without authorizing engineering. First, future critical key-23 extensions now use a registry-independent `!` namespace so mixed-version clients fail closed even if a sender clears the wire critical bit. Second, ADR-0156's sync namespacing is narrowed to account-private records: implementation-owned same-account history is `AccountHistorySnapshot`, while Room `HistoryBundle` is an interoperable groups-scope object and joins V-004's wire-freeze obligation. Third, A-055 is resolved with a concrete W1 LAN profile that reuses existing `RelayEntry`, sealed delivery, MessageRequest/IntroGate, AsyncKeyBundle and encrypted-blob semantics rather than creating parallel authority.

The V-005 generator now adds 23 `future-critical-cleared-bit` negative vectors, one for every affected top-level object. The strict Node decoder rejects all 23 as `unknown_critical_extension`; the portable presentation-extension Python/Node pair also agrees 30/30 with a dedicated cleared-bit `!` case. The Rust strict decoder source is updated too, but this environment has no Rust toolchain, so the new V-005 two-codec criterion remains open rather than being self-certified.

The portable phase-1 drop suite is current at **44/44 producers passed**, completed in two bounded invocations because the execution wrapper terminated the first invocation after producers 1–29 passed while producer 30 was starting; producers 30–44 then all passed. No producer failed.

### 2026-09-29 Run 6 — ADR-0164

Dangerous algorithm ordering and failure-path contracts are frozen as a normative pre-engineering supplement. The design remains closed and engineering remains unauthorized.

### 2026-09-29 Run 7 — protocol evolution / agent guardrails

ADR-0165 freezes protocol evolution/change control: published IDs and meanings are immutable; profile negotiation is security_generation-first with deterministic equal-generation tie-break; extension criticality is immutable; incompatible behavior requires a new lineage/profile/extension or explicit fork; and normative changes use a machine-owned strongest-applicable change class. The repository adds a protocol-evolution policy/validator, a 35-case bounded sanity harness, and agent guardrails. Engineering remains unauthorized.

### Run 7 final machine state

The Run 7 release-candidate checkpoint is **901 current files**, **365 requirements**, **144 threats**, **184 decisions / 162 indexed ADRs**, and **40 security invariants**. `design_open=0`, `verification_open=24`, `pre_engineering_open=23`, `engineering_authorized=false`, and **0/12 scopes** are authorized. The continuity manifest is **82/82**; portable phase-1 evidence is **48/48**; ADR-0165 protocol-evolution sanity evidence is **35/35**. Engineering, independent/formal/two-codec/runtime/real-LAN verification and human review remain separate and open where their gates say so.


## 2026-09-28 23:00 PHT — independent architecture review (ADR-0154–0156)

Claude Opus 5.5 reviewed the Run 10A drop as the different-vendor architecture reviewer. Report: [`2026-09-28-independent-architecture-review-run10a.md`](docs/reports/2026-09-28-independent-architecture-review-run10a.md). The verdict is **ready for engineering for the W0 scopes after this review**: 0 blockers, and the 4 MAJOR defects are fixed or confined to a later scope. No W0 wire bytes change.

- **ADR-0154 (F-01):** recovery-alone timers now define a runtime segment as one OS boot session. Web verifiers under Recommended may credit wall-clock gaps forward of a high-water mark, ending in an authenticated veto sweep. Before this, recovery was effectively unreachable for PWA contacts. V-002 +1 criterion, evidenced.
- **ADR-0155 (F-02/F-03):** a non-critical registered extension can no longer make an event invalid, so validity no longer depends on registry version. Location cards and list annotations are bound to the text they were validated against, across edits. V-005 +1 criterion, evidenced (29/29 and 35/35 Python↔Node; V-005 corpus rerun unchanged).
- **ADR-0156 (F-04..F-08):** the LAN delivery wire binding becomes A-055 under new scope `local_delivery` and gate V-024. Every later-wave gate must freeze its wire objects as CDDL with two-codec vectors, and `calls` needs specialist review of its non-contact key channel. Scheduled send and sync-record namespacing are specified, and open design items now block only the scopes they list.
- The portable phase-1 suite passes 44/44 producers in one run. The run-10A boundary verdict (promote nothing) is independently confirmed.

## 2026-09-28 22:00 PHT — Run 10A residual V1 boundary audit

Run 10A completes a deliberately narrow final sweep for post-V1 features that might still be safe to promote without new authority, cryptography, wire semantics or external gatekeepers. **No additional feature is promoted.** SMS/MMS/RCS remains the strongest residual candidate, but the already-frozen `TransportAdapter` + capability/provenance/security-class boundary removes later redesign pressure while current Android/Google policy still makes full carrier integration platform/default-handler dependent. Mutual-contact PSI, LegacyPolicy activation and additional expressive-media codecs retain substantive privacy/authority/parser reasons to remain later. Exact privacy presets, UI framework/breakpoints/themes/fonts/motion and IndexedDB/OPFS selection remain evidence-driven product/implementation choices behind stable V1 abstractions.

No ADR, machine decision, requirement, threat, protocol/schema or verification criterion changes in this pass. Run-9B evidence remains current; the consolidated different-vendor `core_kernel` review handoff stays active; engineering remains unauthorized. The detailed rationale and current platform references are recorded in [`2026-09-28-run10a-residual-v1-boundary-audit.md`](docs/reports/2026-09-28-run10a-residual-v1-boundary-audit.md). The V1 feature boundary is now intentionally complete for external architecture review; future scope promotion should be triggered by new evidence/standards/platform capability or an independent-review finding rather than another speculative sweep.

## 2026-09-28 21:22 PHT — Run 9B / ADR-0153 final V1 platform evidence closure

Run 9B closes the two repo-local evidence obligations intentionally opened by ADR-0152. Replaceable Android wake/privacy passes **13/13 scenarios** and catches **8/8** message/authority/linkage mutations across FCM, UnifiedPush-compatible, persistent and `none` modes; desktop/PersonRecord endpoint safety passes **12/12 scenarios** and catches **10/10** platform/profile/export mutations. ADR-0153 resolves the one design overclaim exposed by the wake model: when provider-side retirement cannot be confirmed, OM does not invent a universal finite retention bound; obsolete wake state is residual metadata exposure only and never message-fetch/account/relationship authority.

Repo-local progress is now **V-015 16/16** and **V-021 10/10**. Their required independent-review/pre-release records remain open and are not self-approved. The consolidated different-vendor `core_kernel` review brief is active again; engineering remains unauthorized. Portable evidence is fresh across **43/43 producers** in bounded batches; the monolithic attempt was cut off by the execution wrapper after 1–18 passed while producer 19 was starting, with no test failure. Architecture validation reports **859 current files**, `design_open=0`, `verification_open=23`, `pre_engineering_open=22`, `engineering_authorized=false`, and **0/11** scopes authorized; docs/registry are clean and continuity is **79/79**. Current ledger totals are **330 requirements / 330 trace rows, 121 threats, 175 decisions / 153 ADRs**.

## 2026-09-28 20:42 PHT — Run 9A / ADR-0152 final V1 platform-base promotion

Run 9A promotes the last low-risk deferred platform/product foundations into V1 without changing OM wire formats or authority. Native desktop joins PWA/Android/iOS as a V1 reference-family target over shared Core/UI/PlatformAdapter semantics; Android background wake is explicitly replaceable (`fcm | unifiedpush | persistent | none`) so FCM remains convenience rather than protocol correctness/identity infrastructure; and PersonRecord cleanup/export becomes explicit local V1 UX with preview/permission/provenance and no AccountID/RelationshipBinding continuity effect. Stale current-design wording for rich lists, account-private sync, desktop deferral, alternate Android wake and contact cleanup has been reconciled.

Run 9A intentionally adds one new repo-local criterion to V-015 and one to V-021 for Run 9B. Prior Run-8B evidence remains valid. External different-vendor review is paused until the new wake/privacy and desktop/contact-safety evidence is current. Engineering remains unauthorized. Machine progress is **V-015 15/16 repo-local** and **V-021 9/10 repo-local**; all other prior repo-local counts are unchanged. Current ledger totals are **330 requirements / 330 trace rows, 121 threats, 174 decisions / 152 ADRs**; architecture validation reports **853 current files**, `design_open=0`, `verification_open=23`, `pre_engineering_open=22`, `engineering_authorized=false`, and **0/11** scopes authorized. Documentation, registry and the **79-document** continuity manifest are clean; the drop preflight has no problems.

## 2026-09-28 19:37 PHT — Run 8B / ADR-0151 additional V1 evidence closure

Run 8B closes the four repo-local evidence obligations intentionally opened by ADR-0150. Offline-local route reconciliation passes **8/8** scenarios and catches **6/6** route/authority/durability mutations; LAN privacy/resource evidence passes **9/9** scenarios and catches **6/6** mutations; `om.rich-list.v1` has **34/34 Python↔Node** verdict/CBOR agreement; and PWA/Android/iOS reference-family parity passes **8/8** scenarios with **5/5** semantic-fork mutations caught. ADR-0151 resolves the one ambiguity exposed by the rich-list negatives: malformed registered schema rejects the containing object, while schema-valid presentation-semantic failure ignores the whole non-critical enhancement and leaves authenticated W0 RichText unchanged.

Repo-local progress is **V-003 13/14**, **V-005 11/11**, **V-015 15/15**, **V-021 9/9**. Their required independence records remain open and are not self-approved. The consolidated different-vendor `core_kernel` review brief is active again; engineering remains unauthorized. Portable evidence is fresh across **41/41 producers** in bounded batches; architecture validation reports **851 current files**, `design_open=0`, `verification_open=23`, `pre_engineering_open=22`, `engineering_authorized=false`, and **0/11** scopes authorized, with the continuity manifest clean at **79/79**.

## 2026-09-28 19:10 PHT — Run 8A / ADR-0150 additional V1-base promotion

ADR-0150 promotes three more low-risk foundations into V1 without adding protocol authority or reopening OM-Crypto-v1/W0 RichText: optional privacy-preserving same-LAN message/file delivery uses ordinary EventID/session/blob semantics and honest storage-durability state; registered non-critical `om.rich-list.v1` adds bounded ordered/unordered list presentation over exact W0 paragraph-marker fallbacks; and Android/iOS become explicit V1 reference-family targets while PWA remains the first vertical slice. Automatic LAN discovery remains platform-capability-gated and native shells remain non-privileged adapters over shared Core/UI.

Run 8A intentionally opens one new repo-local criterion in V-003, V-005, V-015 and V-021 for Run 8B. Prior Run-7B evidence remains valid; external different-vendor review is paused until those local checks are current. Engineering remains unauthorized. Machine progress is **V-003 12/14**, **V-005 10/11**, **V-015 14/15**, **V-021 8/9**. Current ledger totals are **327 requirements / 327 trace rows, 121 threats, 172 decisions / 150 ADRs**; architecture validation reports **839 current files**, `design_open=0`, `verification_open=23`, `pre_engineering_open=22`, `engineering_authorized=false`, and **0/11** scopes authorized.

## 2026-09-28 18:21 PHT — Run 7B / ADR-0149 adversarial evidence closure

Run 7B closes the four repo-local evidence obligations intentionally reopened by ADR-0149. Account-private sync passes **8/8** causal/revocation/restore scenarios and catches **6/6** shortcut mutations. The amended presentation-extension corpus now contains **24 vectors with 24/24 Python↔Node agreement**, while a registry hardening check proves object scope, exact criticality/schema, integer-key payloads, cross-object rejection and nonempty mismatched-location-fallback ignore semantics. ProfileExecutionContext evidence passes **8/8** scenarios and catches **5/5** retargeting mutations across async profile-sensitive paths and independent windows. Release authorization passes **13/13** role/key/root-chain scenarios and catches **8/8** key/role/threshold/substitution mutations.

Machine progress is now **V-003 12/13**, **V-005 10/10 and pre-engineering passed**, **V-021 8/8 repo-local**, and **V-023 9/9 repo-local**. V-003, V-021 and V-023 remain open only where their independent-review/phase-independence requirements say they should. The consolidated different-vendor `core_kernel` handoff is active again; engineering remains unauthorized.

## 2026-09-28 17:33 PHT — Run 7A / ADR-0149 promoted-V1 adversarial hardening

Run 7A hardens the ADR-0148 V1 promotions before external review. `om.voice-note.v1` and `om.location-card.v1` are now canonical registered/object-scoped/schema-checked extensions; location fallback is cryptographically carried in the same event and semantically tied to the exact E7 point through a canonical `geo:` token. Account-private sync now has causal/revocation/restore anti-resurrection semantics rather than timestamp-only convergence, including 300-second remote-future HLC quarantine, tombstone-wins for concurrent stale writes and explicit draft-resolution parent binding. Profile-sensitive async work captures immutable `ProfileExecutionContext`, and release-signature thresholds bind distinct authorized role/key/algorithm/root-policy tuples rather than algorithm tags.

Machine state is intentionally reopened for Run 7B: **V-003 11/13**, **V-005 9/10**, **V-021 7/8**, **V-023 8/9**. The consolidated different-vendor review handoff is paused and engineering remains unauthorized. Run-6B evidence remains historical/current for the first-order semantics it covered; the new ADR-0149 criteria are not claimed until Run 7B.

## 2026-09-28 17:18 PHT — Run 6B / ADR-0148 V1 base evidence closure

Run 6B supplies the executable/structural evidence intentionally deferred by ADR-0148 Run 6A. Account-private sync passes **6/6** bounded scenarios and catches **4/4** forbidden mutations: timestamp-only draft loss, authority-state injection, revoked-device delivery and cross-profile merge. The optional voice/location extension corpus contains **20 vectors** with **20/20 Python↔Node verdict/encoding agreement**, including W0 fallback and unknown non-critical handling. Endpoint/profile evidence covers multi-profile cache/draft/sync/notification/deep-link isolation plus the older V-021 grant-notice, PlatformAdapter capability-map and ClientHost transparency/root invariants. ClientHost/update evidence passes **15/15** lifecycle scenarios and catches **5/5** downgrade/root/transparency mutations.

Machine progress after evidence reconciliation is **V-003 11/12**, **V-005 9/9 and pre-engineering passed**, **V-021 7/7 repo-local**, and **V-023 8/8 repo-local**. V-003, V-021 and V-023 remain phase-open only where their independent-review requirements say they should; no OpenAI-authored run self-approves those reviews. The consolidated different-vendor `core_kernel` handoff is active again. Engineering remains unauthorized.

Portable evidence is fresh across **33/33 producers**, run in three bounded batches (1–11, 12–22, 23–33) to avoid the execution-wrapper cutoff observed in earlier long monolithic runs; no producer failed.

Final Run-6B integrity state is **826 current files**, `design_open=0`, `verification_open=23`, `pre_engineering_open=21`, `engineering_authorized=false`, and **0/11** engineering scopes authorized. Documentation checks are clean and the continuity manifest matches **79/79** normative documents.

## 2026-09-28 16:30 PHT — Run 6A / ADR-0148 V1 base promotion

Run 6A pulls five low-risk foundations into the V1 baseline without changing OM-Crypto-v1, AccountControlState authority or the frozen W0 ConversationEvent body grammar: reference-client multi-profile switching; bounded account-private draft/selected-preference sync; optional non-critical `om.voice-note.v1`; optional one-shot `om.location-card.v1`; and owner-root-controlled algorithm-agile/PQ-ready software-release signatures. Live location, SPQR, PQ MLS, stories/status, guaranteed server-side scheduling, payments, mini-apps and massive private-broadcast crypto remain deferred.

The change is normative only. New repo-local evidence is intentionally open for Run 6B in V-003 (sync), V-005 (extension vectors), V-021 (profile-switch isolation) and V-023 (release-signature agility). The consolidated different-vendor review handoff is paused until those checks are closed/reconciled. Engineering remains unauthorized.


## 2026-09-28 15:22 PHT — Run 5B / ADR-0147 executable closure and reviewer reactivation

Run 5B supplies the executable evidence intentionally deferred by ADR-0147 Run 5A, without changing wire semantics or reopening architecture. The new crash-safe verifier-timer model passes **5/5** restart/restore scenarios and catches **2/2** forbidden acceleration mutations: crediting reboot downtime from wall time and trusting restored historical elapsed state. The new bootstrap lifecycle/restore model passes **5/5** scenarios and catches **4/4** mutations: an independent hidden `request_expires`, candidate-slot replenishment after rejected-state pruning, stale `OPEN`/`RESERVED`/private-key resurrection, and stale `COMPLETE_REPLAY_ONLY` resurrection.

The older ADR-0146 harness was also reconciled to ADR-0147 and no longer carries a fictitious independent `request_expires` input; request acceptance is the authenticated route/key minimum. The portable phase-1 runner now executes both ADR-0147 harnesses. Machine-ledger repo-local progress is **V-002 9/10**, **V-003 10/11**, and **V-015 14/14**. Their remaining blockers are independent-review / phase-independence records; the consolidated different-vendor review handoff is active again. Engineering remains unauthorized.

Portable evidence is fresh across **29/29 producers**. This execution environment terminated the single monolithic runner during producer 12, so the same producer list was completed in bounded batches: producers 1–11 had already passed and producers 12–29 all passed on rerun; no producer failed. Final integrity checks report **813 current files, `design_open=0`, `verification_open=23`, `pre_engineering_open=21`, `engineering_authorized=false`, 0/11 scopes authorized**; docs checks pass and the **79/79** continuity-manifest check passes.


## 2026-09-28 14:43 PHT — Run 5A / ADR-0147 normative lifecycle/restore closure

A fresh semantic pass found four latent gaps after ADR-0146: the model used an independent `request_expires` value that does not exist on the wire; the eight-candidate lifetime budget did not explicitly survive rejection/GC; `COMPLETE_REPLAY_ONLY -> EXPIRED` lacked a precise terminal deadline; and verifier-local recovery timers lacked conservative reboot/crash semantics. Historical backup behavior for the newer bootstrap state was also not explicit.

ADR-0147 closes those choices without a wire change. Bootstrap acceptance is derived as `min(reply_route.expires_at, reply_delivery_key.expires_at)`; candidate slots are non-replenishable while `OPEN`; replay-only state ends at the derived request deadline plus frozen skew while exact winner acknowledgement is still bounded by the wrapper's own expiry; historical restore cannot reactivate request-scoped bootstrap key/request/replay state; and durable verifier-local recovery intervals use a persisted monotonic elapsed-time lower bound that cannot be accelerated by wall clock or reboot downtime.

This is deliberately a **5A normative checkpoint, not review-ready evidence closure**. V-002, V-003 and V-015 gain new repo-local criteria for Run 5B and those criteria are currently unchecked. The prior different-vendor review handoff is paused until the restart/restore/deadline/candidate-GC evidence is added and the ledger is refreshed. Current machine state is **315 requirements, 114 threats, 169 decisions / 147 indexed ADRs, `design_open=0`, `verification_open=23`, `pre_engineering_open=21`, `engineering_authorized=false`, 0/11 scopes authorized**. Engineering remains unauthorized.


## 2026-09-28 10:10 PHT — Run 4 / ADR-0146 final bootstrap/resource closure

A final adversarial pass found three concrete composition gaps after ADR-0145: losing `RESERVED` candidates could survive winner/key retirement, the historical “one-use” `reply_route` wording could be interpreted as one physical deposit despite retry/race requirements, and authenticated wrapper expiry was not explicitly bounded by delivery-key/route/request authority lifetime. ADR-0146 closes all three without changing wire bytes.

Every transition out of MessageRequest `OPEN` now atomically terminalizes non-winning `RESERVED` candidates before request-scoped key destruction, and completion/cancel/decline/expiry/terminal invalidation race on the same request state. `reply_route` means one logical bootstrap response with exact-wrapper retries and at most **8** distinct candidate wrappers while `OPEN`; exact retries do not consume candidate slots. `SealedDeliveryEnvelope.expires_at` must fit inside its delivery key and bound route, plus the pending request acceptance window for bootstrap, before a new `RESERVED` record is allocated. Replay tombstones never extend authority.

Focused ADR-0146 evidence passes **8/8 scenarios** and catches **4/4 deliberate mutations** and is part of V-003 and V-015. V-003 is **9/10** with only independent model review unchecked; V-015 has **13/13** repo-local criteria evidenced and remains phase-open only for independent review. Engineering remains unauthorized.

Portable evidence is fresh across **27/27 producers**, executed in three bounded batches to avoid this runner's previously observed monolithic-duration cap. Architecture validation, docs checks and the **79/79** continuity-manifest check pass.


## 2026-09-28 09:17 PHT — Run 3 / ADR-0145 final bootstrap retry/race closure

A post-Run-2 adversarial pass found one concrete liveness/race gap: immediate destruction of the completed request-scoped bootstrap private key could make a lost-ack exact `COMPLETE` retry unreachable if key lookup preceded replay lookup, and ADR-0144 still allowed incompatible interpretations of whether more than one logical bootstrap response could exist. ADR-0145 closes both without a wire change.

Each MessageRequest now accepts **exactly one logical bootstrap response**. Multiple recipient-device candidates may race, but the first valid requester-side crash-consistent commit wins and atomically binds the authenticated recipient binding/imported established delivery state, inner/session dedupe, replay `COMPLETE`, and request transition to `COMPLETE_REPLAY_ONLY`. Losing/different wrappers cannot create a second branch. Existing exact `COMPLETE` retries are matched from durable replay fingerprint/route metadata before live key lookup, so the completed bootstrap private key may be destroyed immediately while a lost-ack byte-identical retry remains idempotently acknowledgeable. `RESERVED` retains key material needed to resume.

Focused ADR-0145 evidence passes **5/5 scenarios** and catches **3/3 deliberate mutations**: legacy key-before-replay ordering, accepting a second logical bootstrap after completion, and splitting the completion/request-close commit. V-003 is therefore **8/9**, with only independent model review unchecked. Engineering remains unauthorized.

Portable evidence is fresh across **26/26 producers** using bounded batches after this runner terminated the monolithic command at producer 15. Architecture validation, docs checks and the **79/79** continuity-manifest check pass; the split-execution note is recorded in [`phase1-drop-summary.json`](verification/results/phase1-drop-summary.json).

## 2026-09-28 08:57 PHT — Run 2 / ADR-0144 executable closure and reviewer activation

The focused ADR-0144 V-003 composition harness now passes **12/12** request-scope/stale-ACS scenarios and catches **7/7** deliberate security mutations. The evidence covers exact `reply_delivery_key` route/profile/expiry binding, delivery-key-id and key-pair non-reuse, cross-request transplant rejection, prohibition on stable-authority promotion, bootstrap retirement, `RESERVED -> authenticated stale-ACS refresh -> COMPLETE`, definitive post-refresh rejection, and same-key/id collision while the original exact wrapper remains resumable.

The canonical V-003 ADR-0144 criterion is now met, bringing V-003 to **7/8** pre-engineering criteria. Its only unchecked criterion is independent model review, and phase independence therefore remains open. The consolidated different-vendor reviewer brief is active again. No engineering scope is authorized and `engineering_authorized=false`.

Run 2 adds no wire fields, cryptographic primitives, provider authority or new architecture decision; it supplies the executable evidence explicitly deferred by ADR-0144 Run 1 and reconciles the reviewer-facing state to the machine ledger.

Portable evidence is fresh across **25/25** producers. The environment terminated the single long `make verify-drop` process after producer 15 despite all reached producers passing, so producers 16–25 were rerun in bounded batches and all passed; the refreshed drop summary records that split execution rather than pretending the monolithic command completed. Architecture/docs/gate consistency checks pass.

## 2026-09-28 08:12 PHT — Run 1 / semantic hardening before external review

ADR-0144 closes two pre-review semantic gaps without changing W0 wire objects: first-contact `reply_delivery_key` material is now explicitly fresh and scoped to one pending request/profile/exact reply route with bounded expiry/disposal and no stable-authority promotion; and an already-`RESERVED` sealed wrapper whose inner authentication is temporarily undecidable only because trusted peer ACS may be stale remains `RESERVED` with no release/ack until authenticated monotonic refresh yields a definitive result.

Security traceability now names the crash-atomicity/bootstrap/stale-head threats (T-115..T-117), adds requirements R-315/R-316, strengthens M6 and adds M7. V-003's existing ADR-0142/0143 criterion now states the full crash-safe replay contract. A separate ADR-0144 local-evidence criterion was added and is intentionally open in this Run-1 checkpoint; therefore this package is **not yet ready for the different-vendor reviewer**. Run 2 must add the executable negative/cross-product evidence and re-establish review-only remaining work.

The architecture design queue remains closed and engineering remains unauthorized. V-003 is **6/8** at this intermediate checkpoint: the ADR-0144 local composition criterion and the independent-review criterion are the two unchecked items. A-020's historical wording was reconciled with ADR-0111 so it no longer embeds the superseded recovery-off-by-default premise.

## 2026-09-28 05:30 PHT — Run 3 / final pre-Opus reconciliation

Run 3 reconciles the human-facing verification state to the authoritative machine ledger and the Run-2 portable/full verification split. No protocol semantics were changed. The old V-016 prototype prose still described gaps that ADR-0121/0122/0123 had already resolved; its criteria/README/result metadata now reflect those accepted resolutions while preserving the recorded TLC run outcomes. V-016 has **9/9** repo-local pre-engineering criteria evidenced and remains open only for ADR-0117 independent review; provider load testing remains pre-release.

The final `core_kernel` review queue is now exact: V-002 is **8/9** with only its independent-review criterion unchecked, V-003 is **6/7** with only its independent-review criterion unchecked, V-015 is **12/12**, V-016 is **9/9**, and V-017 is **3/3**; all five still have phase independence open. V-001 and V-005 already have passed pre-engineering phases. Cross-wave feasibility required for `core_kernel` remains evidenced for V-004/V-008/V-009/V-011/V-013/V-014/V-019. Engineering remains unauthorized.

A final evidence-path audit also removed two dead V-002 aliases for the retired `AccountControlState.tla` / `.cfg` files and records the current [`AccountControl.tla`](verification/tla/AccountControl.tla) + [`AccountControl.cfg`](verification/tla/AccountControl.cfg) pair. No model result changed.

The Opus handoff now distinguishes environments correctly: run `make verify-drop` in a raw exported ZIP; run `make verify` only after import into a Git worktree with the full Rust/TLC/history toolchain. The compatibility handoff file now redirects to the single consolidated final-review brief instead of asking Opus to repeat the completed verification-executor pass.

Current machine/design closure remains **305 requirements, 104 threats, 165 decisions / 143 indexed ADRs, 34 security invariants, 43/43 design-queue items closed, `design_open=0`, `engineering_authorized=false`, 0/11 scopes authorized**.

Run-3 `make verify-drop` passes all **24/24 portable evidence producers** plus architecture validation, docs checking and the **79/79** continuity-manifest check; the portable summary remains **347,316 bounded cases**. This environment has no Rust/Cargo and cannot fetch the pinned TLC JAR, so Run 3 does not replace the stronger committed Rust/TLC evidence.

## 2026-09-27 18:48 PHT — post-Luna verification cleanup / final independent-review queue

The 18:09 Luna drop was inspected rather than accepted on summary alone. Its V-001 evidence is internally consistent: all 13 pre-engineering criteria are recorded met, while the overall V-001 gate correctly remains open for its separate pre-release obligations. Luna is an OpenAI model, so its runtime work does **not** satisfy ADR-0117's different-vendor reviewer requirement for the GPT/Sol-authored remediation chain. The remaining independent-review batch therefore targets a non-OpenAI reviewer (for example Claude Opus 5.5).

Two repository-quality issues were repaired before that review. First, clean design-drop extraction omitted the gitignored V-005 Rust binary while [`cross_wave_feasibility.py`](verification/bounded/cross_wave_feasibility.py) assumed it already existed; the checker now builds the V-005 Rust codec on demand when Cargo is available. Second, the Python V-003 reference machine lagged the TLA K12 initiator-binding model; it now models `BindInitiator` / `ForgeInitiation`, and a focused deterministic smoke check reproduces `NoMisattributedSession` with K12 disabled and rejects the forged initiator with K12 enabled. The full pinned Hypothesis V-003 run still needs the tool-complete independent-review runner because this environment cannot fetch its pinned Python runtime/dependencies.

V-015's last non-review pre-engineering criterion is now evidenced by [`verification/bounded/provider_resource_admin_trace.py`](verification/bounded/provider_resource_admin_trace.py): **8** routine provider-administration traces plus **3** RecoveryRequest fan-out traces pass with zero violations, and **7/7** security mutations are caught. V-015 therefore has all **12/12 repo-local criteria** evidenced but remains pre-engineering-open solely for independent review. A single consolidated handoff is in [`verification/reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md`](verification/reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md); the raw drop uses `make verify-drop`; full `make verify` is run only after import into a tool-complete Git worktree. Engineering remains unauthorized.

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

## State as of 2026-09-26 (historical; the current State is at the top)

OM remains in **architecture verification / pre-engineering**. **Engineering is not authorized.** The blocking architecture-design queue is closed; remaining blockers are objective V-* evidence/review obligations.

- blocking architecture design items: **0**;
- open non-blocking/later-wave design items: **0** — A-030/A-031/A-036/A-054 are resolved by ADR-0138;
- verification gates V-001..V-023: **23 overall-open**; pre-engineering phases open: **21**;
- engineering scopes authorized: **0 / 11**; `engineering_authorized=false`;
- machine ledgers: **307 requirements, 107 threats, 166 decisions / 144 indexed ADRs; 35 security invariants / 35 mappings**.

## 11:54 verification-process hardening

ADR-0134 removes the remaining process ambiguity before handing execution to the repo agent:

- evidence is separated into design-complete, repo-local pre-engineering, post-scope implementation, and genuinely external/human phases;
- isolated `verification/` prototypes are explicitly allowed before scope authorization but are non-production evidence only;
- every W0-impacting later-wave gate now has explicit `feasibility_criteria`, so W0 does not need full W1-W4 gate passage;
- substantive requirements formerly hidden only in free-text `remaining` notes were promoted into canonical gate criteria;
- free-text `remaining` lists were removed; unfinished work is derived mechanically from canonical criteria;
- `criteria_met` must exactly match canonical criteria, and a passed pre-engineering phase must also record satisfied independence;
- independent-review-only work is batched after the verification executor exhausts repo-local evidence, preventing model-to-model ping-pong;
- [`verification/CLAUDE_OPUS_5_5_HANDOFF.md`](verification/CLAUDE_OPUS_5_5_HANDOFF.md) is the stable compatibility entry point; it redirects to the consolidated final independent-review brief.

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

