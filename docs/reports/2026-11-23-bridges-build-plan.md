# Bridges build plan (scope `bridges`, authorized by ADR-0260)

Date: 2026-11-23. Status: scope **authorized** on the operator's explicit instruction, 2026-11-23 ([`ADR-0260`](../../decisions/ADR-0260-authorize-bridges-engineering.md), Accepted). This is a plan; no product code for the scope exists yet.

## Design sources read

Read in full: [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) (including the metadata and verification rulings), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), the `bridges` scope contract, the V-013 gate record, the V-013 prototype README (listing and result summary), and [`ADR-0260`](../../decisions/ADR-0260-authorize-bridges-engineering.md). The service-principal document, read for [`the services plan`](2026-11-23-services-bots-build-plan.md), also applies because a bridge reuses that hosting and capability machinery.

**Not read, and every slice must read before it builds** (`python3 tools/design_gate.py needs <slice paths>`): [`BRIDGE_ADAPTER_CONTRACT.md`](../../verification/prototypes/v013-bridges/BRIDGE_ADAPTER_CONTRACT.md) (the candidate contract, rule IDs BAC-*, residual risks in its section 10), [`TRANSPORT_ADAPTERS_AND_INTEROP.md`](../architecture/network/TRANSPORT_ADAPTERS_AND_INTEROP.md), ADR-0124 and ADR-0129 in full, [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md), [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md), the groups documents on MLS membership and removal, the capability registry and the UX brief. The V-013 model was not re-run for this plan.

## 1. What the scope covers, per the design

A bridge exists only where OM crosses into a different messaging authority domain (Matrix, Telegram, email, SMS, IRC and the like). Independent OM clients are not bridges. The governing rule: **a bridge may translate semantics; it may never translate trust invisibly.**

- **Identity:** a typed `BridgePrincipal` with a random 256-bit `BridgeID` and a hash-chained `BridgeControlState` (controllers, threshold, endpoint credentials, adapter definitions). Distinct from `ServicePrincipal` because every bridge-authored event carries meaning from outside OM's authentication domain. A bridge endpoint is never an Account device and never votes in account control.
- **Foreign actors are never minted as OM users.** A bridged event is authored by the `BridgePrincipal` with `ForeignProvenance` (network namespace, opaque conversation and event refs, optional actor ref and display hint, observed and bridge-observed times, fidelity flags, origin fingerprint) in `ConversationEvent` key 14. Display names and avatars are bridge-attested hints.
- **Identity links are evidence, not merging.** `ForeignIdentityLink` lets a client show "Alice on Telegram, linked to Alice's OM identity"; it never lets a bridge author as the account.
- **Admission:** a bridge in a private Room is always a **full MLS member** and plaintext recipient; the roster and Room state say so (bridge id, networks, direction, mapped scope, `allowed_event_classes`, fidelity summary, approval evidence). `allowed_event_classes` limits export, never reading. Adding one is a high-risk change that may need quorum; removal stops future export and performs the key transition but cannot claw back plaintext already sent.
- **Manual share versus continuous export:** a user's one-shot forward is ordinary and shows the destination; continuous or automatic export of Room plaintext needs explicit bridge authorization and cannot hide behind a plugin's network permission.
- **Mapping and fidelity:** explicit `BridgeMapping` (one Room to zero or more foreign scopes only if policy permits, fan-out explicit, `ConversationID` never changes); `InteropCapabilities` per adapter; explicit `fidelity_flags` for every degradation; edits, deletes, reactions, replies, topics and polls translate only when stable targets exist, else degrade visibly.
- **Loops and duplicates:** an `InteropTrace` (origin, origin fingerprint, traversed bridge ids, hop count and limit, transformations); refuse on a repeated bridge id, an exhausted hop limit, an already materialized fingerprint in this Room through any bridge, or a policy reflection path. The fingerprint is keyed with a Room-scoped secret over (origin network, foreign origin id), so it is not stable across Rooms. Metadata-stripping paths yield `origin_unverified`; bridge-to-bridge relay through OM is off unless the Room opts in; a bridge exports only epochs within its own membership.
- **Honest security UI:** a bridged Room says bridging is active, which network, which `BridgePrincipal`, direction, exported classes, that OM E2EE ends at the bridge, and the external leg's claim separately. No single lock icon spanning both systems.
- **Bridges and services stay separate:** a bridge may invoke services only under separate capabilities; installing a service never makes it a bridge.
- **Hosting and discovery:** local or self-hosted creation, replaceable endpoint, direct signed descriptors, optional catalogs and domain aliases, no mandatory project registry; external platform requirements are labelled as external dependencies.

## 2. Blocking gates and prerequisites

| # | Condition | State |
|---|---|---|
| 1 | Accepted ADR names the scope | Met: ADR-0260 |
| 2 | Gate V-013 pre-engineering | **Waived by ADR-0260 for engineering only.** Met repo-locally: the loop and reflection model with two simulated bridges and fidelity signalling (16 scenarios, 43,936 states in the contract scenarios), and the contract statement that bridges are full members and `allowed_event_classes` limits export. **Open:** independent contract review and the ADR-0156 wire freeze for `BridgePrincipal`, `BridgeControlState` and `ConversationEvent` key 14 |
| 3 | Feasibility: provenance, scoped fingerprints and loop metadata fit the frozen event model without changing AccountID, ConversationID or W0 event identity | Evidenced |
| 4 | `depends_on` | `groups` authorized |
| 5 | Validator | Passes (12 of 12 scopes) |

Pre-release: real adapters and multi-bridge traces meeting the model, plus the independent contract review.

## 3. What already exists

| Piece | Where | Gap |
|---|---|---|
| Reserved event key 14 `foreign_provenance` (W0 rejects it) | [`spec/cddl/events.cddl`](../../spec/cddl/events.cddl) | No CDDL for bridge objects |
| MLS Rooms, membership, removal liveness | `crates/om-core/src/groups/*` | No bridge endpoint type or bridge installation state; the service endpoint work in the services plan comes first |
| Capability registry (`room.add_bridge`, export policy ids) | [`spec/registries/om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) | Enforcement for bridge admission not built |
| Candidate adapter contract, explicit-state loop explorer | `verification/prototypes/v013-bridges` | Evidence, not product |

## 4. Slices

Each slice reads its design-gate output first, ends with `make check`, the touched crate tests and a commit with `Design-Read` and `Design-Departures`. Heavy runs go through `bin/lonunjie-heavy`.

| Slice | Content | Done when |
|---|---|---|
| 0 | **Adapter contract and wire freeze.** Get the candidate [`BRIDGE_ADAPTER_CONTRACT.md`](../../verification/prototypes/v013-bridges/BRIDGE_ADAPTER_CONTRACT.md) independently reviewed and adopted by ADR; CDDL, registered fields and labels, positive and negative vectors and two independently written codecs agreeing for `BridgePrincipal`, `BridgeControlState`, `BridgeMapping`, `InteropTrace`, `ForeignProvenance`, `ForeignIdentityLink` and key 14; product Rust codec in `om-codec` | Both codecs agree on every vector; key 14 stays rejected by W0 receivers (ADR-0128 rule 18) |
| 1 | **Bridge control and typed authorship (pure Core).** Hash-chained state, controller and no-self-bootstrap rules shared with services, endpoint rotation, rollback rejection, immutable incarnations; `author_principal` of type bridge; endpoints excluded from human quorums | A bridge can never produce an event that reads as an account's or a service's |
| 2 | **Provenance and loop engine.** Origin fingerprint with the Room-scoped secret, per-Room dedup across bridges, trace refusal rules, `origin_unverified`, hop limit, membership-interval export bound, bridge-to-bridge relay off by default | The V-013 scenarios run as tests on product code; independent bridges importing the same foreign event dedupe only on a matching fingerprint, never on text, name and time |
| 3 | **Admission and Room state.** Bridge as full MLS member with visible roster state, mapping state, quorum for the high-risk change, removal with key transition, revocation races, export classes enforced by the host, stale mapping failing closed | Removal stops export within the stated bound; a stale mapping cannot send; no path hides a bridge as an account or service author |
| 4 | **Fidelity and translation rules.** `InteropCapabilities`, `fidelity_flags`, edit, delete, reaction, reply, topic and poll translation rules with explicit degradation; unknown targets never attach to an unrelated message | Every degraded translation carries a flag the UI can show; deletes never claim erasure |
| 5 | **Identity links.** `ForeignIdentityLink` with proof methods per network, expiry and revocation, forged-link rejection, display of "linked" separate from native authorship | A forged or stale link never changes authorship or merges accounts |
| 6 | **Reference adapter and host.** One adapter, chosen under Q1, running as a replaceable endpoint outside the client, with no project-hosted gateway; it exercises the loop engine against a real foreign network in a test environment | A two-bridge round trip on test networks with no loops and correct fidelity flags |
| 7 | **UI.** The honest security-state surface (active bridge, network, principal, direction, exported classes, "OM encryption protects this message to Bridge X"), admission and removal flows, fidelity markers, link display; accessibility and reduced motion per the UX brief | UX brief definition of done; Playwright on three engines |
| 8 | **Evidence.** Replay the model scenarios against product code, fuzz every bridge object decoder and adapter boundary, an independent implementation of the vectors, real adapters with multi-bridge traces, independent contract review | Recorded; nothing claimed as passed that needs independent review |

Slice 1 reuses the service-principal engine, so the `services_bots` slices 0 to 2 should land first or together. Slices 0 to 2 are pure Core and wire work and testable on emeth4.

## 5. Constitution rules that bind it

C-88 (interoperation with bridges is a goal within explicit boundaries and user consent), C-65 (optional bridges may use proprietary external systems; none is mandatory), C-01 (no project-hosted bridge fleet or endpoint), C-12, C-18 (stewards never inherit Bridge authority), C-20. The ADR-0260 constraints add: a bridge is always a visible full member, and an unknown external-send outcome is reconciled or retried only through that network's safe idempotency mechanism, never duplicated under a fresh identity.

## 6. Questions for the operator or an ADR by delegation

| ID | Question | Safest default |
|---|---|---|
| Q1 | Which foreign network does the reference adapter target first? | Matrix, because it has stable event ids and a public specification, so the loop and fidelity rules can be exercised honestly |
| Q2 | Who is the independent reviewer of the adapter contract? | A different-vendor AI review recorded under `verification/reviews/` (ADR-0117 basis), plus a human reviewer before any release |
| Q3 | Default hop limit and fingerprint secret rotation | Smallest values that satisfy the model; record them in an additive ADR before slice 2 |
| Q4 | Should a one-shot "forward to another network" ship before continuous bridging? | Yes: it needs no bridge principal and still shows the destination boundary |
| Q5 | Retry horizon for outbound operation identity across restart | Use the supported horizon the contract names; no new number without an ADR |

## 7. Routing and budget

Slice 0 is well specified once the contract is reviewed and fits a mid tier worker with a complete spec; slices 1 to 3 are authority and information-flow logic and stay on the session model, each paying the design-gate reading cost. Run the usage gate from the lonunjie [`AGENTS.md`](../../AGENTS.md) before dispatching.

## 8. What this plan did not do

It wrote no code, ran no tests, edited no ledger and amended no ADR. Nothing in the Q-list changes audited text.

## Update 2026-11-25: wasm host surface, handle-based commands

Design sources read for this slice: the six domain document sets listed by `design_gate.py needs` for `crates/om-wasm` (the constitution through its digest); not read in full: documents outside those domains.

`om-wasm` (feature `scope-engines`, off in every release) gained `room_services_do`, `bridge_room_do` and `lan_transfer_do`. State sits in the host instance under a caller-chosen handle (64 per kind); control heads arrive as already-validated facts. Tests cover the human-quorum rule (a bridge principal never counts), the protected capability, export blocking (class, stale, revoked) and receipt-only confirmation. Still open: space-binding state handle, the TypeScript binding, and the separate build variant.
