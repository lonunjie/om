# OM Architecture Hardening Phase Plan

- **Checkpoint:** 2026-09-23 09:20 PHT
- **State:** pre-engineering; engineering is not authorized

The former Gate A..K questions and architecture-design queue A-001..A-015 now have concrete design resolutions. Work remains split into typed ledgers.

## A. Architecture design queue — currently closed

`machine/architecture_design_queue.jsonl`

All current entries A-001..A-015 are resolved at the architecture-design level. A design item may be reopened only by explicit ADR/machine-contract update if verification or external standards evidence disproves a selected design.

## B. Architecture verification gates — blocks engineering

`machine/architecture_verification_gates.jsonl`

Selected architecture must now be proven. Current blocking gates are:

- V-001 production crypto path;
- V-002 AccountControlState formal model;
- V-003 one-to-one session model;
- V-004 group commit/control convergence;
- V-005 canonical encoding vectors;
- V-006 Web DeviceVault feasibility;
- V-007 SFrame/WebRTC interoperability;
- V-008 Space/Room permission authority;
- V-009 baseline event materialization/convergence;
- V-010 plugin sandbox;
- V-011 ServicePrincipal/agency authority;
- V-012 expressive-media/content-pack parser safety;
- V-013 bridge provenance/fidelity/loops;
- V-014 portable archive/import safety;
- V-015 metadata exposure/privacy paths;
- V-016 decentralized abuse/moderation authority;
- V-017 protocol longevity/0M continuity drill.

Architecture-focused prototypes, formal models, fuzz harnesses, packet/provider trace experiments and independent conformance implementations are allowed because they are evidence for these gates. Product engineering remains unauthorized.

## C. Public-release gates

`machine/public_release_gates.jsonl`

These include role/jurisdiction legal review for project-controlled services/distributions, RF patent text, trademark policy, store compliance, supply-chain ceremony, platform push/LAN matrices, LinkPreview security and provider-diversity evidence.

## D. Deferred product measurement

`machine/deferred_product_questions.jsonl`

Frameworks, exact breakpoints, animation timings, local DB crossover and visual theme naming stay bounded so they cannot accidentally dictate protocol/security architecture.

## Verification ordering

A practical order is:

1. authority/state-machine models — V-002, V-004, V-008, V-011, V-016;
2. protocol/wire foundations — V-005, V-009, V-013, V-014, V-015;
3. implementation feasibility boundaries — V-001, V-006, V-007, V-010, V-012;
4. cross-system continuity exercise — V-017 after the artifacts above are concrete enough to test.

Parallel work is fine where independent, but a gate may not be marked passed from prose alone.


## Phase-1 verification progress — 09:20 PHT

`verification/run_phase1.py` now supplies executable evidence for V-002, V-004, V-005, V-008, V-009, V-011, V-013, V-014, V-015, V-016 and V-017. The current run completed 7,772 bounded/adversarial cases, 14 cross-implementation deterministic-CBOR foundation vectors and a 60-artifact offline continuity dry run without a modeled invariant violation.

**All 17 blocking verification gates remain open.** Evidence accumulation is not equivalent to gate passage. In particular, TLC execution, frozen per-object wire registries, independent implementations and real browser/runtime/packet/parser experiments remain mandatory where specified.

## Engineering transition

A future ADR may set `engineering_authorized=true` only when the design queue has no open blocking item and every `blocks_engineering` verification gate is `passed`. The validator enforces this.
