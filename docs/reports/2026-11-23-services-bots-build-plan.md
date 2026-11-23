# Services, bots and agents build plan (scope `services_bots`, authorized by ADR-0258)

Date: 2026-11-23. Status: scope **authorized** on the operator's explicit instruction, 2026-11-23 ([`ADR-0258`](../../decisions/ADR-0258-authorize-services-bots-engineering.md), Accepted). This is a plan; no product code for the scope exists yet.

## Design sources read

Read in full: [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), the `services_bots` scope contract, the V-011 gate record and prototype listing, [`ADR-0258`](../../decisions/ADR-0258-authorize-services-bots-engineering.md).

Read in part: [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](../architecture/ecosystem/SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md) (headings, section 7 on cheap creation and unsolicited reach, section 13 on installation, section 14 on service-to-service, the `RepresentationGrant` shape), and the reserved-key comment of [`events.cddl`](../../spec/cddl/events.cddl).

**Not read, and every slice must read before it builds** (`python3 tools/design_gate.py needs <slice paths>`): the rest of the delegated-agency document (sections 3 to 6, 8 to 12, 15 to 16), ADR-0122, ADR-0124, ADR-0128 and ADR-0129 in full, [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md), [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md), the groups documents on MLS membership and `sequencing_timeout`, [`DECENTRALIZED_ABUSE_MODERATION.md`](../architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md) for the service-abuse rules, the capability registry and the UX brief. The V-011 models, broker corpus and injection corpus were not re-run for this plan.

## 1. What the scope covers, per the design

OM does not classify "human versus AI". It has typed authority principals, and "bot", "assistant", "agent" and "LLM" are presentation categories over `ServicePrincipal`.

- **Identity:** a random 256-bit `ServiceID`, hash-chained `ServiceControlState` (controllers, threshold, endpoint credentials), no root token, no central issuer. The service, its endpoints and its hosts can never be its own controllers (no self-bootstrap, ADR-0122). A signature proves continuity, not safety or "OM verified".
- **Authorship:** events carry a typed `author_principal` plus an endpoint id; a service endpoint is never a human `DeviceIdentity` and never counts toward human-device governance. Autonomous output is authored by the `ServicePrincipal` and labelled as automation; a user-confirmed send stays authored by the account.
- **Two cryptographic positions only:** **invocation-only** (not a member; a one-shot, user-initiated `ContextDisclosureBundle` over a 1:1 session with an endpoint the current `ServiceControlState` authorizes) or **full member** (holds the Room's MLS keys; the roster says it can read everything; a trigger filter limits automation, never reading). A third, device-local case, `local_component`, runs sandboxed on the user's device.
- **Installation versus data access:** installation scope (personal, Room, Space) is separate from data-access mode; a Space install never grants child-Room plaintext; adding a full-member service is a high-risk change that may need quorum.
- **Taint and egress:** any invocation that read conversation plaintext is tainted and gets no network egress without the Room's `plaintext_egress` policy (in a 1:1, both participants'); a local user's grant is not enough; content filtering is not a control.
- **Tools and injection:** LLM output is untrusted data; tool requests are typed, host-mediated and bounded by granted capabilities; no keys, vault, shell, filesystem or unrestricted network by default; no service approves its own escalation.
- **Automation:** explicit `AutomationRule`s with rate and budget bounds; lineage (`root_invocation_id`, `parent_invocation_id`, `origin_principal`, `hop_count`); service-originated events do not trigger services by default; bounded hops.
- **Revocation:** monotone and live; a revoked or migrated endpoint is removed from every Room by head carriage, member stop and removal, any member committing after `sequencing_timeout`; exposure is bounded by head delivery plus that timeout; no retroactive erasure is promised.
- **Delegated agency:** `RepresentationGrant` (issued by account authority, never by a messaging-only device, counterparty notice required) lets a service act "via" an account while staying `ServicePrincipal`-authored.
- **Discovery and creation:** permissionless; identity is cheap but unsolicited delivery is permissioned (IntroGate, Room admission, quotas, blocking); catalogs and verification are optional plural evidence; no project bot store.

## 2. Blocking gates and prerequisites

| # | Condition | State |
|---|---|---|
| 1 | Accepted ADR names the scope | Met: ADR-0258 |
| 2 | Gate V-011 pre-engineering | **Waived by ADR-0258 for engineering only.** Met repo-locally (6 of 7): host migration and alias model, tool-capability and injection corpus, the two service positions, `RepresentationGrant` rules, endpoint-removal liveness, immutable incarnations. **Open:** the ADR-0156 wire freeze for `ServicePrincipal`, `ServiceControlState`, `RepresentationGrant` and `ConversationEvent` key 13, and independent review |
| 3 | Feasibility: service principals fit the frozen Principal, capability and event model with no privileged authority class | Evidenced |
| 4 | `depends_on` | `groups` authorized |
| 5 | Validator | Passes (12 of 12 scopes) |

Pre-release: implementation tests replaying the model vectors and the injection corpus, plus the independent review.

## 3. What already exists

| Piece | Where | Gap |
|---|---|---|
| Reserved event keys 13 (representation) and 14, typed `author_principal` | [`spec/cddl/events.cddl`](../../spec/cddl/events.cddl) | No CDDL for service objects; W0 rejects key 13 |
| MLS Rooms, membership, endpoint, rejoin, removal liveness | `crates/om-core/src/groups/*` | No service endpoint type, no service installation state |
| Capability registry including `room.install_service`, `room.set_plaintext_egress` and the control-kind mapping | [`spec/registries/om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) | Enforcement for service installs not built |
| Model, broker corpus and injection corpus | `verification/prototypes/v011-services` | Evidence, not product |
| First-contact admission (IntroGate, MessageRequest) | `om-core` intro and flows | A service as a sender is not yet a typed principal there |

## 4. Slices

Each slice reads its design-gate output first, ends with `make check`, the touched crate tests and a commit with `Design-Read` and `Design-Departures`. Heavy runs go through `bin/lonunjie-heavy`.

| Slice | Content | Done when |
|---|---|---|
| 0 | **Wire freeze.** CDDL, registered numeric fields and domain-separation labels, positive and negative vectors and two independently written codecs agreeing (V-005 method, ADR-0165) for `ServicePrincipal`, `ServiceControlState`, `ServiceDefinition`, `ServiceInstallation`, `RepresentationGrant`, `ContextDisclosureBundle` and `ConversationEvent` key 13; product Rust codec in `om-codec` | Both codecs agree on every vector; key 13 stays rejected by W0 receivers (ADR-0128 rule 18) |
| 1 | **Service control engine (pure Core).** Hash-chained `ServiceControlState`, controller threshold, no-self-bootstrap rule, endpoint credentials, atomic `migrate_endpoint`, rollback rejection, immutable installation and endpoint incarnations | The V-011 model cases run as tests on product code; a stale removal cannot remove a later reinstall |
| 2 | **Typed authorship.** `author_principal` and endpoint id enforced end to end; service endpoints excluded from human-device quorums; automation labelling is a model fact the UI cannot omit | A service can never produce an event that reads as an account's; messaging-only devices cannot issue a `RepresentationGrant` |
| 3 | **Invocation-only path.** `ContextDisclosureBundle` construction from user-selected context, 1:1 session to an authorized endpoint, disclosure receipt, minimal scope hints, streaming as ephemeral invocation state, final result only via an authorized `message.create` or a user-confirmed draft | Nothing but the selected items leaves; partial frames never become events; cancel stops future frames |
| 4 | **Full-member path.** Installation as a typed MLS member, visible roster state, quorum for the high-risk change, trigger filter as installation data, removal via Room control plus MLS, endpoint-revocation carriage to every Room (head carriage, stop, removal after `sequencing_timeout`) | Revocation removes the endpoint in every Room within the stated bound; a Room that never receives the head keeps its documented staleness |
| 5 | **Capability broker, taint and egress.** Host-mediated typed tool actions, taint tracking for any invocation that read plaintext, `plaintext_egress` enforcement including the 1:1 both-participants rule, no keys or vault, no self-escalation, capability growth after an update needs approval | The injection corpus cannot exceed granted capabilities or create unbounded loops; a tainted invocation has no egress without the policy |
| 6 | **Automation and loop safety.** `AutomationRule`, rate and budget bounds, lineage fields, default ignore of service-originated events, hop limit | Two services that mention each other stop at the hop limit; a rule cannot read beyond its installation grant |
| 7 | **Representation.** `RepresentationGrant` issuance by account authority, counterparty notice for a counterparty-recipient grant, scoped actions, expiry and revocation, "via" display data | A grant from a quarantined control device stops validating; no representation makes a service an account device |
| 8 | **Hosts and UI.** Typed intents through wasm and platform packages; install flow showing the real requested capabilities and data-access mode and stating "this service can read all Room content" for full members; per-invocation disclosure summary; revoke, remove and "who can this read" views; accessibility per the UX brief | UX brief definition of done; Playwright on three engines |
| 9 | **Evidence.** Replay of the model vectors and injection corpus against product code; parser fuzzing of every service object decoder under the agreed budget; an independent implementation of the vectors; independent review of the authority and information-flow model | Recorded; nothing claimed as passed that needs independent review |

Slices 0 to 2 are pure Core and wire work and testable on emeth4. Slices 3 to 5 reuse the existing group and session machinery and should land behind the capability flag only.

## 5. Constitution rules that bind it

C-88 (interoperation with services, bots and LLM agents is a goal within explicit security boundaries and user consent), C-01 (no project bot cloud or catalog), C-12 (authority never moves to a host or provider), C-18 (stewards never inherit Service authority), C-20 (no new primitive). The ADR-0258 constraints add: a service never masquerades as an account, plaintext read never implies egress or Room authority, and no grant is widened on retry.

## 6. Questions for the operator or an ADR by delegation

| ID | Question | Safest default |
|---|---|---|
| Q1 | Do local LLMs ship in the first release of this scope, or only remote and `local_component` services? | Selected-context remote invocation and `local_component` first; no bundled model |
| Q2 | The `ServiceDefinition` manifest signing and the catalog attestation format are only conceptual in the design | Freeze only what slice 0 needs; catalogs stay out (C-01) |
| Q3 | Default quorum for adding a full-member service to a Room | Use the Room's existing high-risk threshold; a 1-of-1 Room is 1-of-1 |
| Q4 | Rate and budget defaults for automation and hop count | Smallest values that make the model vectors meaningful, recorded in an additive ADR |
| Q5 | Wording of the "can read all Room content" notice and "automation" label | Take it from the UX brief copy rules; no new brand terms |

## 7. Routing and budget

Slice 0 is well specified with checkable vectors and fits a mid tier worker with a complete spec; slices 1, 2, 4, 5 and 7 are authority and information-flow logic and stay on the session model, each paying the design-gate reading cost. Run the usage gate from the lonunjie [`AGENTS.md`](../../AGENTS.md) before dispatching.

## 8. What this plan did not do

It wrote no code, ran no tests, edited no ledger and amended no ADR. Nothing in the Q-list changes audited text.
