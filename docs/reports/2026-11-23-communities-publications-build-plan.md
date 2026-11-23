# Communities and publications build plan (scope `communities_publications`, authorized by ADR-0261)

Date: 2026-11-23. Status: scope **authorized** on the operator's explicit instruction, 2026-11-23 ([`ADR-0261`](../../decisions/ADR-0261-authorize-communities-publications-engineering.md), Accepted), after the `publication` conformance suite was specified (cases PUB-01 to PUB-14). This is a plan; no product code for the scope exists yet.

## Design sources read

Read in full: [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md), [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), the `communities_publications` scope contract, the V-008 and V-016 gate records, [`ADR-0261`](../../decisions/ADR-0261-authorize-communities-publications-engineering.md), and the first sixty lines of [`space-publication.cddl`](../../spec/cddl/space-publication.cddl) with the V-008 prototype README.

Read in part: the headings and the Room and Space moderation sections of [`DECENTRALIZED_ABUSE_MODERATION.md`](../architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md).

**Not read, and every slice must read before it builds** (`python3 tools/design_gate.py needs <slice paths>`): the remainder of [`space-publication.cddl`](../../spec/cddl/space-publication.cddl), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md), ADR-0121, ADR-0122, ADR-0128 and ADR-0160 in full, [`om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json), [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md), the rest of the moderation document, the metadata threat model, and the UX brief. The V-008 prototype and the Room moderation model were not re-run for this plan.

## 1. What the scope covers, per the design

Three separate object classes, deliberately not one "group": a **Space** (organizational container, shown to people as a Community), **Rooms** (interactive, MLS for private ones, identified by their existing `ConversationID`) and **Publications** (public signed one-to-many feeds).

- **Space control state:** a hash-chained `SpaceControlState` (type 24) with profile, membership, role definitions, child links and governance. Authority forks freeze privileged advancement; authority-reducing transitions stay applicable (ADR-0122); no winner by timestamp, arrival order or provider.
- **Child links are bilateral affiliation.** A Space never owns a Room or Publication it lists; joining a Space joins no Room; Space membership can be admission evidence a Room opts into, and a Room performs its own membership and MLS transition.
- **Delegation by explicit binding only.** A `SpaceAuthorityBinding` names exactly which Room capabilities the Space may manage, at most one authority-parent Space per Room, never `room.manage_security_policy`, `room.add_bridge`, `room.install_service` or `room.manage_governance` unless the Room grants them, never a CryptoProfile change. Space authority reaches a Room as a `SpaceDelegatedIntent` that any Room member may commit; reductions use the sequencing-timeout liveness rule; admission checks fail closed past `freshness_bound`.
- **Capabilities are authority, roles are bundles.** Delegation ceilings stop self-escalation (role edit, role assignment and `grantable_scope` each checked); restrictions win over grants; bans and restrictions end only through committed lifts; a `ModerationDecision` is a record and changes no Room state.
- **Topics and threads** are room-local organization (`topic.create`, `topic.update`, `topic.status.set`, `message.topic.set`) with no new roster or MLS state; follow and mute are local preference. Forum, chat and announcement are presentation modes; the security rule comes from capabilities.
- **Governance without a permanent owner:** one-person start, later quorum; orphaned governance freezes only what needs that authority, and people fork to a successor.
- **Private announcement Rooms** are ordinary MLS Rooms with `room.post` restricted; no new broadcast cryptosystem in V1.
- **Publications:** `PublicationControlState` (type 26) and `PublicationEvent` (type 27) under a subordinate signing key (role 7) with a per-key monotonic `seq`, `prev_post_hash` chain and `control_state_hash`; revocation records the last valid `(key, seq)`; subscribers reject rollback; followers are not members and are never disclosed; tombstones retract but do not erase.
- **Not in V1:** a mandatory homeserver, a Space-wide MLS roster, inheritance of child membership, a global admin bit, a permanent owner, transitive nested-Space authority, ambient bot or bridge plaintext from a parent Space, a private subscriber-broadcast cryptosystem, and any project-run discovery, hosting or moderation.

## 2. Blocking gates and prerequisites

| # | Condition | State |
|---|---|---|
| 1 | Accepted ADR names the scope | Met: ADR-0261 |
| 2 | Gate V-008 pre-engineering | **Waived by ADR-0261 for engineering only.** Met repo-locally: the authority model, incarnation and ceiling checks, delegated-intent and invite-lapse checks; the W3 wire is frozen (ADR-0160) with two independent codecs agreeing. **Open:** independent reviewer evidence on the authority model |
| 3 | Gate V-016 pre-engineering | Passed |
| 4 | ADR-0130 guard: `publication` conformance suite specified | Met (PUB-01 to PUB-14) |
| 5 | `depends_on` | `groups` authorized |
| 6 | Validator | Passes (12 of 12 scopes) |

Pre-release: the model-derived vectors replayed by an independent implementation, adversarial load and abuse tests against reference providers (V-016), and the independent review.

## 3. What already exists

| Piece | Where | Gap |
|---|---|---|
| Frozen W3 wire and typed event bodies | [`spec/cddl/space-publication.cddl`](../../spec/cddl/space-publication.cddl), registries | No product Rust codec for types 24 to 27 |
| Room groups: MLS Rooms, endpoint, rejoin, journal, binding | `crates/om-core/src/groups/*`, `flows/group_lane.rs` | No Space layer, no capability-registry enforcement for Space delegation, no topics, no moderation events beyond what groups needed |
| Capability registry (Room and Space ids, kind-to-capability map) | [`spec/registries/om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) | Space ids and delegated-intent mapping unused by code |
| Two-codec Space wire prototype and the Room moderation TLC model | `verification/prototypes/v008-space-wire`, `v016-room-moderation` | Evidence, not product |

## 4. Slices

Each slice reads its design-gate output first, ends with `make check`, the touched crate tests and a commit with `Design-Read` and `Design-Departures`. Heavy runs go through `bin/lonunjie-heavy`.

| Slice | Content | Done when |
|---|---|---|
| 0 | **Confirm the freeze and write the PUB cases as tests.** Product Rust codec in `om-codec` for types 24 to 27 and the typed event bodies, replaying both prototypes' vectors; implement PUB-01 to PUB-14 as failing-first conformance tests against a Publication validator skeleton; run the V-008 negative vectors | The Rust codec agrees with both prototype codecs on every vector; PUB cases exist and fail for the right reasons |
| 1 | **Publication validator and feed (pure Core).** Per-key `seq` and hash chain, revocation last-valid `(key, seq)`, rollback rejection, control-head fork freeze, tombstones, idempotence and equivocation detection, subscriber state as local data | PUB-01 to PUB-14 pass; a mirror that withholds data can delay but not rewind |
| 2 | **Space control state engine.** Hash-chained transitions under prior-state governance, governance thresholds, fork freeze and authority-reducing rebase, orphaned-governance behavior, role definitions with ceiling checks (the three rules), restrictions over grants, immutable incarnation ids so a stale revoke cannot remove a re-grant | The V-008 negative cases each fail closed on product code; a revoke targets an exact incarnation |
| 3 | **Bilateral child links and Room binding.** Link proposal and acceptance, either side revokes, `SpaceAuthorityBinding` with the exact capability list and `freshness_bound`, refusal of protected capabilities, `SpaceDelegatedIntent` creation and member commit, admission by Space membership failing closed when stale | A Space cannot claim an unconsenting Room; a delegated reduction commits after `sequencing_timeout`; no binding can change a CryptoProfile |
| 4 | **Topics, threads and room organization.** `topic.*` and `message.topic.set` events and reducers, the reply and thread anchors, forum and announcement presentation modes derived from capabilities, local follow and mute | Authorization follows the kind-to-capability table; moving a message keeps EventID and context |
| 5 | **Moderation.** `restriction.set`, `membership.remove`, `membership.ban`, `moderation.redact`, committed lifts only, `ModerationDecision` as a record with no state effect, the V-016 race cases against product code, and the C-16 limits on any evidence disclosure | A decision alone changes nothing; bans end only by a committed lift; stale reductions cannot hit a later incarnation |
| 6 | **Binding to hosts and wasm.** Typed intents and views through `om-wasm`, `om-core-wasm` and the platform packages; Publication fetch and mirror use only existing transport and storage | Intents exist for create, link, bind, post, moderate; no new endpoint |
| 7 | **UI.** Community view with Rooms and Publications, admission and join flows that state "joining the Community does not join its Rooms", role and restriction views that answer "why can they do this", forum and announcement modes, Publication follow and "publisher retracted this" copy, orphan and fork guidance; accessibility and reduced motion per the UX brief | UX brief definition of done; Playwright on three engines |
| 8 | **Evidence.** Fuzzing of control-state and publication decoders under the agreed budget, independent implementation of the model vectors, abuse and load tests against the reference Home, independent review of the authority model | Recorded; nothing claimed as passed that needs independent review |

Slices 0 and 1 are pure Rust and testable on emeth4. Publication (slices 0 and 1) does not depend on Space code and can ship first behind its flag.

## 5. Constitution rules that bind it

C-01 (no required operator, so no project-run moderation, hosting or discovery), C-12 (principals hold authority, never providers or hosts), C-16 (abuse control without surveillance, no universal decryption), C-18 (stewards never inherit user, Room, Service or Bridge authority), C-20, and the ADR-0261 constraints.

## 6. Questions for the operator or an ADR by delegation

| ID | Question | Safest default |
|---|---|---|
| Q1 | Where does the Space-level capability mapping for delegated intents live while the registry is only a summary in the prose? | Follow [`om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) exactly; any gap is a written question, not a guess |
| Q2 | `sequencing_timeout` and `freshness_bound` numeric defaults | Use the values already fixed for group liveness in the groups design; add none that are new |
| Q3 | Maximum Space size and Publication mirror limits for V1 | Bound by the existing limits profile; record the numbers in an additive ADR before slice 2 |
| Q4 | Which large-Room scale claims are allowed in copy | None beyond what MLS and runtime verification show; the design forbids promising unbounded subscribers |
| Q5 | Whether Publications ship before Spaces | Yes: slices 0 and 1 are independent and lower risk |

## 7. Routing and budget

Slices 0 and 1 are well specified with checkable cases and fit a mid tier worker; slices 2, 3 and 5 are authority and moderation logic and stay on the session model, each paying the design-gate reading cost. Run the usage gate from the lonunjie [`AGENTS.md`](../../AGENTS.md) before dispatching.

## 8. What this plan did not do

It wrote no code, ran no tests, edited no ledger and amended no ADR. Nothing in the Q-list changes audited text.
