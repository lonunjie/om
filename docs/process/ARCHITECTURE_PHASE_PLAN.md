# OM Architecture Hardening Phase Plan

- **Checkpoint:** 2026-09-23 07:18 PHT
- **State:** pre-engineering; engineering is not authorized

The former Gate A..K questions have concrete decisions in `ARCHITECTURE_RESOLUTIONS_20260923.md`. Work now falls into four ledgers.

## A. Architecture design queue — blocks engineering

`machine/architecture_design_queue.jsonl`

The ordinary messenger baseline, initial Spaces/Rooms/Communities model, expressive media/content packs/entitlement boundary, typed service-principal bot/LLM model and capability-sandboxed plugin architecture are now resolved at the design level. Remaining blocking design work is concentrated in external bridges plus import/export/escape; metadata privacy; decentralized abuse/moderation; and the 20-year protocol-longevity/anti-capture pass. Verification remains independently blocking.

## B. Architecture verification gates — selected design must be proven

`machine/architecture_verification_gates.jsonl`

Required before engineering authorization where marked blocking:

- production PQXDH/Triple-Ratchet Rust+WASM implementation path;
- AccountControlState formal model;
- one-to-one session-manager model;
- Group CommitCoordinator/control binding model;
- canonical encoding independent golden vectors;
- WebDeviceVault target-browser feasibility;
- SFrame/WebRTC interoperability profile.
- Space/Room permission-authority model.
- baseline event materialization/convergence model.

## C. Public-release gates

`machine/public_release_gates.jsonl`

These include role/jurisdiction legal review for project-controlled services/distributions, RF patent text, trademark policy, store compliance, supply-chain ceremony, platform push/LAN matrices, LinkPreview security and provider-diversity evidence.

## D. Deferred product measurement

`machine/deferred_product_questions.jsonl`

Frameworks, exact breakpoints, animation timings, local DB crossover and visual theme naming stay bounded so they cannot accidentally dictate protocol/security architecture.

## Engineering transition

A future ADR may set `engineering_authorized=true` only when design queue is empty/resolved and every `blocks_engineering` verification gate is `passed`. The validator enforces this.
