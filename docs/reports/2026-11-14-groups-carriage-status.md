# Groups carriage (ADR-0189): status (2026-11-14)

Three packages implement [`ADR-0189`](../../decisions/ADR-0189-group-carriage-own-commit-acceptance-and-room-defaults.md). This file records each package as it lands. Package 1 is the wire spec only: no Core logic.

## Package 1: wire spec, registries and two-codec vectors

### Design sources read

Read in full, with the Read tool, before any edit: [`AGENTS.md`](../../AGENTS.md) "Design gate"; [`AGENT_ARCHITECTURE_RULES.md`](../process/AGENT_ARCHITECTURE_RULES.md); every document and ADR that
`python3 tools/design_gate.py needs` prints for [`spec/cddl/group-transport.cddl`](../../spec/cddl/group-transport.cddl), the four registries, [`machine/file_registry.json`](../../machine/file_registry.json) and the vectors file: seven domains (`crypto_1to1`, `discovery_first_contact`,
`identity_control_recovery`, `metadata_privacy`, `provider_deployment_supply_chain`, `relay_reachability`, `time_ordering_events`), 37 architecture documents (crypto, discovery, identity, delivery, client, storage, groups, providers, events),
[`PRIVACY_GOVERNANCE_AUTHORITY.md`](../constitution/PRIVACY_GOVERNANCE_AUTHORITY.md), [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md) and 59 ADRs; plus ADR-0177, ADR-0187 and ADR-0189.

Not read in full: nothing that the gate lists. Not opened at all: documents the gate does not list for these paths (the groups UX specs, [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md) beyond what ADR-0189 cites). [`group.cddl`](../../spec/cddl/group.cddl) was read for conventions, not audited line by line.

### What landed

| Item | Where |
|---|---|
| CDDL, moved from the ADR appendix unchanged in content, plus the `ConversationEventGroupCarriage` rule | [`spec/cddl/group-transport.cddl`](../../spec/cddl/group-transport.cddl) |
| Object types 32 `GroupRouteRecord` (signed, role `device`, label `OM-GROUP-ROUTE-SIG-v1`) and 33 `GroupTransportFrame` (unsigned), the `0xF04D` row | already registered by the ADR commit; verified, unchanged |
| Event kinds 15 `group.invite`, 16 `group.accept`, 17 `group.decline` | `enums.event_kind` and, as W1, `enum_value_waves.event_kind` in [`om-numeric-fields.json`](../../spec/registries/om-numeric-fields.json); body table in [`om-w1-event-bodies.json`](../../spec/registries/om-w1-event-bodies.json) |
| Enum `group_frame_kind` (1 commit, 2 leave_request, 3 app, 4 welcome, 5 admit_request, 6 route_update) | [`om-numeric-fields.json`](../../spec/registries/om-numeric-fields.json) |
| Structs | `GroupRouteRecord`, `GroupTransportFrame` (top level); `GroupRelease`, `GroupReleaseEvidence`, `GroupMemberRoute`; `FrameBodyCommit`, `FrameBodyLeaveRequest`, `FrameBodyWelcome`, `FrameBodyAdmitRequest`, `FrameBodyRouteUpdate`; `GroupInviteBody`, `GroupAcceptBody`, `GroupDeclineBody` |
| Generator, runner, CDDL checker | [`verification/prototypes/group-transport-wire/`](../../verification/prototypes/group-transport-wire/) (`run.sh`, [`gen_group_transport_vectors.py`](../../verification/prototypes/group-transport-wire/gen_group_transport_vectors.py), [`cddl_check_group_transport.py`](../../verification/prototypes/group-transport-wire/cddl_check_group_transport.py)) |
| Vectors (130: 25 accept, 105 reject) | [`verification/vectors/group-transport-vectors.json.gz`](../../verification/vectors/group-transport-vectors.json.gz) |
| Product-codec replay | [`crates/om-codec/tests/group_transport_vectors.rs`](../../crates/om-codec/tests/group_transport_vectors.rs); new `Registry::load_w1()` in `crates/om-codec/src/schema.rs` |

Results: the V-005 Rust codec and the Node strict harness agree on all 130 vectors (verdict, error class, re-encoding, derived signature input) under the W1 overlay; Rust `cddl` 0.10.7 and Ruby `cddl` 0.12.14 agree on 107 CDDL checks and
the Ruby-generated examples are accepted by the Rust codec; the product `om-codec` replays the same file exactly. The frozen W0 corpus replays unchanged (`w0_vectors` passes).

### For package 2

- Registry names: object types `GroupRouteRecord` (32), `GroupTransportFrame` (33); structs above; kinds 15 to 17 through the W1 overlay. In Rust use `om_codec::schema::Registry::load_w1()` (W1 decoder: kinds 15 to 17 valid and typed); `Registry::load()` is the W0 decoder and rejects them (`value_range`).
- A `GroupTransportFrame.body` is a bstr. For kinds 1, 2, 4, 5, 6 it holds the canonical CBOR of the matching `FrameBody*` struct; for kind 3 it holds one raw MLS PrivateMessage. Typed validation of that inner map is **not** done by any codec (see Q1); package 2 must decode and check the `FrameBody*` structs itself.
- Fixtures per frame kind and per body: ids `GroupTransportFrame/<kind>` (`commit`, `leave_request`, `app`, `welcome`, `admit_request`, `route_update`, `commit-without-added-devices`), `GroupRouteRecord/{typical,four-relays,short-lived}`, `ConversationEvent/group-{invite,accept,accept-two-devices,decline,...}`. Decode with `python3 -c` over the gz file; the `value` and `body_value` fields give the diagnostic form.
- Semantic fixtures (wire-valid bytes, rule owed to the Core semantic layer): vector ids starting `.../semantic-`, each with a `semantic_rule` field: `stale-route-in-accept`, `route-for-another-device-id`, `invite-without-required-capabilities`, `accept-for-unknown-invite-id`, `welcome-without-accepted-invite`, `welcome-member-route-for-another-device`, `commit-frame-non-roster-sender`.
- Regenerate: `verification/prototypes/group-transport-wire/run.sh [--no-cddl] [--update-vectors]` (about 45 seconds with CDDL).

### ADR-0189 negatives: where each is covered

| ADR negative | Coverage |
|---|---|
| Welcome frame over the bucket | wire: `GroupTransportFrame/neg/welcome-over-the-bucket` (`size_limit`, object over 131,068 bytes); body bound 131,000 on the product codec (`frame_body_boundary_on_the_product_codec`) |
| Route for another device id | semantic fixtures `route-for-another-device-id`, `welcome-member-route-for-another-device`; wire part: device id 15 and 17 bytes, missing |
| Stale route | semantic fixture `stale-route-in-accept` (the wire carries `expires_at`; staleness needs the receiver clock) |
| Welcome without a preceding accepted invite | semantic fixture `welcome-without-accepted-invite` |
| Accept for an unknown `invite_id` | semantic fixture `accept-for-unknown-invite-id` |
| Frame from a non-roster device | semantic fixture `commit-frame-non-roster-sender` (a frame names no sender; MLS authenticates it) |

Besides these, 105 wire negatives: generic mutations (unsorted or duplicate keys, non-minimal integers and lengths, indefinite length, truncation, trailing bytes, size limit, envelope faults), frame kind 0 and 7, body empty, missing or mistyped fields, route relays 0 and 5,
relay entry bounds, profile-1 key length (1216), kind 15 to 17 body bounds (room name 1 to 256 bytes, member count 1 to 512, 1 to 16 releases), nested release faults, and kind 18.

### Questions the design leaves open (not decided here)

**Q1. Typed frame bodies.** The codec schema grammar has no "CBOR inside a bstr" type, so `GroupTransportFrame.body` is validated only as a bstr of 1 to 131,000 bytes and the `FrameBody*` structs are registered but unreachable by the two codecs. Closing this means a new schema type (for example `{cbor: Struct}` plus a dispatch on `frame_kind`) in both prototype codecs and `om-codec`, which changes the frozen type grammar and needs an ADR. The nested pieces that do exist as typed values (`GroupRelease`, route and credential envelopes) are exercised through the Accept event. Stopped there.

**Q2. Accept size.** ADR-0189 1.6 says an Accept must fit one sealed bucket (131,068 bytes), but the Accept is a ConversationEvent, whose registered limit is 65,536 bytes (type 10), and up to 16 releases can each carry a credential of up to 32 KiB and a route record of up to 16 KiB. Which limit wins is not stated. I left the registry as is: Accept is bounded at 65,536.

**Q3. `required_capabilities`.** "Kinds 15 to 17 carry `["om.group.v1"]`" is a cross-field rule (kind against field 10). CDDL and the registry cannot express it; it is the semantic fixture `invite-without-required-capabilities`. No semantic checker exists for it yet.

**Q4. Unbounded MLS byte strings.** The ADR CDDL leaves KeyPackage, Commit and Welcome as bare `bstr`. The registry needs bounds, so they carry the enclosing object limits (KeyPackage 0 to 65,536, Commit and Welcome 0 to 131,000). No minimum was invented, so an empty KeyPackage is wire-valid and MLS parsing must reject it.

### Findings (no decision needed)

- **W1 enum values.** Adding kinds 15 to 17 to `event_kind` as plain W0 values broke the frozen W0 corpus negative `kind-15-unregistered`. I used the existing ADR-0168 F-16 mechanism (`enum_value_waves`, marked W1) and added one overlay rule: [`om-w1-event-bodies.json`](../../spec/registries/om-w1-event-bodies.json) `enum_values` lifts the marker for a W1 decoder; [`verification/prototypes/v004-group-wire/overlay.py`](../../verification/prototypes/v004-group-wire/overlay.py) and `Registry::load_w1()` implement it. A W0 decoder rejects kinds 15 to 17, a W1 decoder accepts and types them. This is tooling for an additive registry change, not a design change; flagged for review.
- **Signature roles are not a codec check.** A `GroupRouteRecord` envelope signed with the controller role is accepted by both codecs; `signed.roles` is enforced when signatures are verified, not at decode. Package 2 must verify role `device` there.
- **Prototype string limit.** The V-005 prototype codecs cap every string at 64 KiB, so they cannot decode a frame body over 65,536 bytes; only the product codec does (`limits_for` widens it per type). The shared vectors therefore avoid bodies above that, and the boundary is a product-codec test.
- **Device id width.** `GroupRouteRecord` and the release structs use the 16-byte `device-id` of [`common.cddl`](../../spec/cddl/common.cddl) (and `DeviceCredential`), as the ADR says; [`group.cddl`](../../spec/cddl/group.cddl) still uses 16 to 32 bytes for succession and committer devices.
- **V-004 additive check fails at HEAD, before this change.** `verification/prototypes/v004-group-wire/run.sh` reports FAIL on [`additive.py`](../../verification/prototypes/v004-group-wire/additive.py): labels were inserted mid-list in [`om-domain-separation.json`](../../spec/registries/om-domain-separation.json) by earlier commits against its pre-freeze base `31e35af`. All V-004 codec comparisons (383 vectors, 758 W0 vectors, 105 semantic cases) pass. I did not touch it and did not commit its regenerated result file.

## Operator-delegated decisions on Q1 to Q4 (2026-11-14)

The operator asked for the best decision balancing ease of use with strongest security. Basis: ADR-0189 (no new authority, no new metadata), CRYPTOGRAPHY_AND_AGILITY (never silently downgrade; fail closed), and the rule that the frozen codec grammar is not changed for a convenience.

- **Q1 (typing of `GroupTransportFrame.body`):** keep the frozen codec grammar unchanged. The codec checks only the outer bstr and its size. Core must decode the inner `FrameBody*` structure strictly (canonical CBOR, unknown fields rejected, kind must match the `group_frame_kind`) and drop the frame as invalid on any mismatch before it touches MLS state. No ADR is needed; a later schema grammar change can move the check into the codec without changing behaviour.
- **Q2 (Accept size):** the ConversationEvent limit of 65,536 bytes wins. An Accept is tiny, so this costs nothing, and the stricter limit is the safe reading. ADR-0189 section 1.6's bucket sentence is read as an upper bound for frames, not a requirement to pad an Accept to 131,068 bytes.
- **Q3 (`required_capabilities` for kinds 15 to 17):** enforced in Core, not in CDDL. An event of kind 15 to 17 without `om.group.v1` is rejected and no group state changes. Peers without the capability never see a group invitation as a chat message; they see nothing actionable.
- **Q4 (bare MLS byte strings):** the codec keeps the enclosing object limits. Core rejects an empty KeyPackage, Commit or Welcome and anything the MLS engine cannot parse, with no partial state change, so an empty value is wire-valid but never accepted.
- **Also (from package 1 notes):** Core verifies role `device` on every `GroupRouteRecord` signature, and enforces the semantic negatives shipped as fixtures (non-roster sender, stale route, route for another device, Welcome without an accepted invite, Accept for an unknown `invite_id`). Each is a drop with no state change and no error shown to the person beyond the normal "could not join" state.

## Package 2a: routes, frames and the relationship lane in Core

Built by a builder-only session, then design-gate read, reviewed and committed by a separate committer session (see "Design sources read by the committer" and "Package 2a rulings"). Core never produces the group wire this package adds until a host calls the new `Core::group_*` methods, so no existing behaviour changes.

### Design sources read, and not read (builder)

Read in full: this report (Package 1 and "Operator-delegated decisions on Q1 to Q4"); [`ADR-0189`](../../decisions/ADR-0189-group-carriage-own-commit-acceptance-and-room-defaults.md) (the whole text, including the appendix CDDL); the repository [`AGENTS.md`](../../AGENTS.md) and its Design gate (given as session context). Read as code: `crates/om-core/src/groups/flow.rs` (types and the receive path), `groups.rs` (head), `groups/binding.rs`, `groups/store.rs`, `flows.rs` (compose, release_queued, seal, open, on_event), `events.rs`, `route.rs`, `crypto/delivery.rs`, `engine.rs` group steps, `tests/groups_core.rs`, and the registries for object types 2, 7, 28, 32, 33, `ConversationEvent` and the `Frame*` structs.

**Not read by the builder** (the brief asked for them; the builder ran out of room and did not open them; the committer read them, next section): ADR-0177, ADR-0187, [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md), [`docs/process/AGENT_ARCHITECTURE_RULES.md`](../process/AGENT_ARCHITECTURE_RULES.md), and every document the design gate lists for the touched paths. `journal.rs` was only skimmed. The committer must read them before committing; nothing below was checked against them.

### Design sources read by the committer

Read in full, in this session, before committing: [`AGENTS.md`](../../AGENTS.md) "Design gate"; [`AGENT_ARCHITECTURE_RULES.md`](../process/AGENT_ARCHITECTURE_RULES.md); [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md); [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md); every document the design gate printed for the changed `.rs` files, in all six domains: `crypto_1to1` (CRYPTOGRAPHY_AND_AGILITY, PRODUCTION_CRYPTO_INTEGRATION_GATE, ASYNC_KEY_MATERIAL, ONE_TO_ONE_SESSION_MANAGEMENT, MULTI_DEVICE_SYNC, CANONICAL_ENCODING), `discovery_first_contact` (DISCOVERY_FABRIC, INVITES_INTROGATE_AND_SPAM, CONTACT_SHARING, INTRODUCTION_AND_SERVICE_ACCOUNTS, KEY_TRANSPARENCY_AND_DIRECTORY), `identity_control_recovery` (IDENTITY_PROTOCOL, IDENTITY_CONTINUITY, ACCOUNT_CONTINUITY_AND_CONTROL, RECOVERY_AND_BACKUP_POLICY, IDENTITY_DISCOVERY_VERIFICATION, IDENTIFIER_SPOOFING_AND_DISPLAY, ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY), `metadata_privacy` (PRIVACY_GOVERNANCE_AUTHORITY, MULTI_RELAY_AND_DELIVERY_RESILIENCE, NOTIFICATIONS_AND_PUSH, STORAGE_AND_BLOB_ARCHITECTURE, SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION, METADATA_THREAT_MODEL), `relay_reachability` (RELAY_AND_DELIVERY_ARCHITECTURE, MULTI_RELAY_AND_DELIVERY_RESILIENCE, REACHABILITY_AND_OBJECT_REPAIR), `time_ordering_events` (EVENT_MODEL, TIME_AND_EXPIRY_SEMANTICS, CANONICAL_ENCODING, MESSENGER_BASELINE_V1, SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION); and every ADR the gate listed for those domains plus ADR-0177, ADR-0187 and ADR-0189 (ADR-0002, 0003, 0004, 0008, 0010, 0011, 0012, 0019 to 0022, 0025 to 0030, 0033, 0040, 0063, 0065 to 0067, 0072, 0073, 0075, 0076, 0087, 0088, 0098, 0105 to 0107, 0111, 0118 to 0121, 0124, 0126 to 0130, 0140, 0157, 0159, 0177, 0187, 0189). Nothing the gate listed was left unread. The code was reviewed against them for the three delegated items; no line-by-line audit of every Package 2a function against every document was done, and ADR-0121, 0122, 0173, 0176 were read only where cited (0121 in full; 0122, 0173, 0176 through GROUP_COMMIT section 3.2 and the ADR-0189 text, not as files).

Review result: the design settles none of the three items, and nothing in the code contradicts a normative passage beyond the two gaps recorded as D36 and D37 in [`2026-10-28-relay-status.md`](2026-10-28-relay-status.md). Notes: (1) ONE_TO_ONE_SESSION_MANAGEMENT section 3 (K7, ADR-0144) is the precedent for item (b): an inbound item that needs a fresher peer head is held unprocessed, not refused. (2) ADR-0189 section 3 shows an invitation "in the 1:1 thread"; Core keeps lane messages out of the timeline, archive and last-message line, and the groups UI is expected to present the invitation from group state. (3) The self-signed `signed-DeviceCredential` evidence envelope is not specified either way; the receiver already trusts only a body the validated head commits to, which is the safe reading.

### What was built

| Scope item | State |
|---|---|
| (1) `GroupRouteRecord` mint, role `device` verify, replacement after removal | Done. A route is minted from one host route offer (the deposit capability the adapter obtained with `POST /v1/relay/deposit-capabilities`, `Core::supply_routes`), with a fresh HPKE key bound to that entry, signed role `device` (`RouteRecord::sign`). `RouteRecord::open` verifies role `device` only, the named device, the route binding to the first relay entry and freshness; any other role, device, binding or an expired key is a drop. `group_route_replace` mints a fresh route and retires older ones; `group_routes_expire` drops them after the overlap and returns the capabilities for the host to revoke. Core sets "route owed" on every `MemberRemoved` (`group_carriage.replaceOwed`). |
| (2) Frame seal/unseal, strict inner decode | Done. `encode_frame` / `decode_frame` for all six kinds, `seal_frame` (ADR-0142 envelope, same buckets), `Core::group_frame_receive`. The inner `FrameBody*` is checked by the new `om_codec::Registry::check_struct` (no grammar change: it applies the existing field tables to a registered struct), so unknown fields, non-canonical bytes and a body of another kind fail as `frame_body_invalid` before any MLS or group state is read; empty MLS parts are `empty_mls_object`. |
| (3) Invite, Accept, Decline as kinds 15 to 17 | Done. They are queued as hidden 1:1 messages (`Msg.lane`), so they use the real ratchet, retry and fan-out, and never show in the timeline or the archive. W1 registry check, `["om.group.v1"]` required (without it: rejected as a structural failure), Accept bounded at 65,536 bytes. |
| (4) Semantic negatives | Done for all five as drops with no state change, each tested: non-roster sender, stale route, route for another device, Welcome without an accepted invite, Accept for an unknown `invite_id`. |
| (5) Two-Core tests | `crates/om-core/tests/groups_carriage.rs`, 12 tests, pass with and without `--features groups-openmls`: invite, accept, Welcome delivered and application frame, decline, and each negative. |

Wired end to end in 2a: Invite, Accept, Decline (lane); Welcome out and in; application frame out and in; Commit frame in. Encoded, sealed, opened and strictly decoded but **not** wired to the flow: Commit and LeaveRequest out, admit request, route update (`frame_kind_not_wired`).

### Files changed

New: [`crates/om-core/src/groups/carriage.rs`](../../crates/om-core/src/groups/carriage.rs) (wire objects, routes, seals, lane bodies, persisted `GroupCarriageRec`), [`crates/om-core/src/groups/carry.rs`](../../crates/om-core/src/groups/carry.rs) (glue to the flow: evidence checks, Welcome, App, Commit in), [`crates/om-core/src/flows/group_lane.rs`](../../crates/om-core/src/flows/group_lane.rs) (lane send and receive in the 1:1 transaction), [`crates/om-core/tests/groups_carriage.rs`](../../crates/om-core/tests/groups_carriage.rs).

Changed: `crates/om-codec/src/schema.rs` (`Registry::check_struct`); `crates/om-core/src/groups.rs` (two `pub mod`); `crates/om-core/src/groups/flow.rs` (a Welcome may arrive for an invite whose ConversationID is still empty, and the join stores it: ADR-0189 1.1 says the invite carries none); `crates/om-core/src/engine.rs` (`group_lane_send`, `group_lane_pump`, `group_frame_send`, `group_frame_receive`, `group_route_replace`, `group_routes_expire`; `group_step` made generic; route-owed hook); `crates/om-core/src/flows.rs` (a kind 15 to 17 event goes to the lane before the W0 check); `crates/om-core/src/events.rs` (`encode_full` with key 10, `parse_group_lane`); `crates/om-core/src/state.rs` (`Msg.lane`, `AppState.group_carriage`, both serde-default); `crates/om-core/src/flows/multi.rs`, `crates/om-core/src/flows/ceremony.rs` (`lane: false` in `Msg` literals); `crates/om-core/src/backup.rs` and `engine.rs` (lane messages are not archived, shown or counted as the last message); [`crates/om-core/Cargo.toml`](../../crates/om-core/Cargo.toml) (dev-dependency `flate2`, already in the lockfile through om-codec) and `Cargo.lock`; [`machine/file_registry.json`](../../machine/file_registry.json) (four new files, by `om_registry.py sync`).

Results: `cargo test -p om-core`, `cargo test -p om-core --features groups-openmls`, `cargo test -p om-wasm`, `cargo fmt`, `cargo clippy -p om-core --all-targets` (0 warnings) and `make check` all pass in the working tree. wasm and TypeScript were not touched; the TypeScript host does not yet call the new methods.

### Design domains the committer must read

`python3 tools/design_gate.py needs` over every changed file prints the same six domains: `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events` (the documents and ADRs listed under them), plus ADR-0177, ADR-0187, ADR-0189, [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) and [`AGENT_ARCHITECTURE_RULES.md`](../process/AGENT_ARCHITECTURE_RULES.md). Suggested trailers once read: `Design-Read: crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events` and `Design-Departures:` the items under "Deviations to confirm" below, or `none` if the operator rules them as silence.

### Deviations to confirm and open questions

1. **Signed credential and head evidence (needs a ruling).** The CDDL carries `signed-DeviceCredential` and `signed-AccountControlHead`, but Core stores only unsigned credential bodies and the account state chain; it never produced either object. I did this: a device signs its **own** credential with role `device` (registry allows `device`, `controller`, `recovery`), which proves nothing, so a receiver trusts a credential only when its body is one the validated head of that account commits to (`peer_credentials` / own `acs_credentials`); and an Accept's head is a type 2 `AccountControlHead` signed with role `controller`, which only a **control** device can produce (a messaging-only device refuses with `no_controller_head`). The receiver checks it against the relationship's pinned head. Question: who mints the head for a messaging-only device, and is a self-signed credential acceptable as the evidence envelope?
2. **Other members' credentials (needs a ruling).** A Welcome's member routes and a Commit's added devices can name devices of accounts the receiver holds no validated head for. I fail closed: a Welcome is accepted only when every member route is a device of the inviter's account or of the invitee's own, and a Commit frame with `added` devices is dropped (`added_evidence_unverifiable`). A Room with three or more accounts therefore cannot be joined yet. ADR-0189 1.4(b) says the receiver maps devices through a validated head but does not say where it gets heads of members it has no 1:1 relationship with.
3. **Overlap length.** ADR-0189 1.3 says "a bounded overlap" and gives no value; `ROUTE_OVERLAP_S` is 3600 s, a placeholder policy constant. Same for `ROUTE_TTL_S` (30 days) and `route_not_newer` (a replacement must outlive the stored route; the ADR gives no versioning).
4. **An Accept releases one device.** The flow's `siblings` (releases of the person's other devices) need own-device sync; only the acting device is released (`no_own_release` if absent).
5. **Kinds 15 to 17 from an own other device** are accepted and ignored (own-device sync of them is not specified); the peer's other devices receive them through the normal fan-out.
6. **Invite carries no ConversationID** (ADR-0189 1.1), so the flow's invite record starts with an empty id and takes it from the Welcome (the one edit in `flow.rs`).
7. **"Welcome without an accepted invite"** is reached in two ways in Core: the route exists only after the Accept, and a Welcome naming another invite on that route is `route_for_another_room`; a replay after joining finds the route re-keyed to the Room. The literal `welcome_without_accepted_invite` branch exists and is covered only by those routes into it, not by a dedicated test (a Core cannot hold the invite route without having accepted).
8. **Capability failure is a rejection, a semantic drop is an acceptance.** A kind 15 to 17 event that fails the codec, lacks `om.group.v1` or comes from the wrong author is rejected like any malformed event (it counts as an integrity failure on the relationship); a well-formed event the group layer refuses (unknown invite, stale route, uncommitted credential, head mismatch) is accepted and dropped so it is not replayed. Confirm this split.

### Package 2a rulings (operator delegation 2026-11-14: "best decision: easy to use while maintaining the best security", fail closed, no silent downgrade)

- **(a) Messaging-only devices must be able to join groups.** Ruled: the device uses the validated account-control head it received through the device-link enrollment notice and chain, refusing with `no_controller_head` only if that cache is missing. State today: Core keeps the ACS chain and credentials, but it never stores a controller-signed `AccountControlHead` (the enrollment notice, object type 8, carries generation and hash but no head signature), so a messaging-only device has nothing to cache and still refuses. Not fixed in 2a. **2b work:** a control device signs a head whenever the chain advances and delivers it with the enrollment notice and every later advance (own-device sync); a messaging-only device stores the newest validated one and `carry_own_head` returns it; or, if the operator prefers, an ADR-0189 amendment that lets an Accept carry the ACS chain tip instead. Recorded as D36.
- **(b) A Welcome naming accounts the receiver has no head for must be held pending, not refused for good.** Ruled: fetch the missing heads through the existing peer-head refresh/fetch (multi-device phase 2) and retry. State today: the Welcome is dropped (`credential_not_committed`, and a Commit frame with added devices is `added_evidence_unverifiable`), and the existing refresh only covers accounts the device has a relationship with. Not fixed in 2a. **2b work:** a pending-Welcome record (wrapper bytes held, sealed under the route's key, bounded count and age), a head fetch for non-relationship accounts, and retry on arrival; the Welcome's `RESERVED` treatment follows ADR-0144. Until then a Room with three or more accounts cannot be joined (fail closed). Recorded as D37.
- **(c) Route overlap 3600 s and route lifetime 30 days: accepted as v1 defaults.** Done: named constants `ROUTE_OVERLAP_S` and `ROUTE_TTL_S` in [`crates/om-core/src/groups/carriage.rs`](../../crates/om-core/src/groups/carriage.rs), their comments now say the operator accepted them. `route_not_newer` (a replacement must outlive the stored record) stays as built; the ADR gives no versioning, so this is a safe-direction rule.

### What 2b and 3 need

- **2b (rulings first):** items (a) and (b) above (D36, D37). Then **G10 and Room defaults:** `commit_accepted` as the three conditions of ADR-0189 section 2 replacing the host's `CommitAccepted` report (the tests still report it by hand, marked in `welcomed()`); Room defaults per protection profile. Commit out needs the **signed `GroupControlIntent` envelope** (label `OM-GROUP-INTENT-SIG-v1`), which Core does not produce yet: the flow's `Commit.intent` is the body and the actor is vouched for by the committer's binding. Same for LeaveRequest out.
- **3 (host and driver):** the host side of the device lane: depositing a `Sealed.wrapper` at `Sealed.entry`, polling the per-Room mailbox of each `MyRoute` (`mailbox`) and handing wrappers to `group_frame_receive`, revoking returned capabilities (`group_routes_expire`), and `route_update` and admit-request wiring. The lane itself rides the existing 1:1 delivery, so Invite, Accept and Decline need only `group_lane_send` after a flow step and `group_lane_pump` after each poll. The TypeScript `GroupCarriage` port and the wasm bridge methods for the new `Core::group_*` calls are not written. A route offer must be available before an Accept, a Welcome or an App frame (`no_route_offer` otherwise); the driver should top the pool up as it does for 1:1.

## Package 2b: messaging-only join, pending Welcomes, Core-owned commit acceptance, Room defaults, signed intents

Built by a builder-only session in `crates/om-core` (plus two match arms in `crates/om-wasm/src/bridge.rs` so the crate compiles). Committed by a separate committer session after the design gate. No ADR was written or changed: D36 needed no new authority, and D37 stays inside ADR-0144's held-wrapper precedent.

### Design sources read, and not read (builder)

Read: this report (Package 1, Q1 to Q4 rulings, 2a, the 2a rulings); [`ADR-0189`](../../decisions/ADR-0189-group-carriage-own-commit-acceptance-and-room-defaults.md); ADR-0177; ADR-0187; [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md); [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md); [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md); [`AGENT_ARCHITECTURE_RULES.md`](../process/AGENT_ARCHITECTURE_RULES.md); the 2a code (`carriage.rs`, `carry.rs`, `flow.rs`, `engine.rs`, the ceremony and multi-device flows).

Not read in full: [`MULTI_DEVICE_SYNC.md`](../architecture/crypto/MULTI_DEVICE_SYNC.md) and [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) were only searched with grep; the other documents the gate lists for the changed paths (the six domains below) were not read by the builder. The committer then read all six domains' documents and ADRs, ADR-0177, 0187, 0189, GROUP_CHAT, GROUP_COMMIT, SECURITY_PRIVACY_PROFILES and AGENT_ARCHITECTURE_RULES in full.

### What was built

| Scope item | State |
|---|---|
| 1. D36 messaging-only join | Done, no ADR. A device that holds no controller key cannot sign an `AccountControlHead`, so the approver of a device link now also signs a controller head over the new tip and sends it in the enrollment/approval payload; the new device validates it and caches it (`head_cache`). `carry_own_head` uses the controller head if present, else the cache, else fails closed with `no_controller_head`. The messaging-only device signs only what a device may sign (credential evidence is the controller's head). Test: `a_messaging_only_device_joins_a_room`. |
| 2. D37 pending store | Done for accounts the device has a relationship with: a Welcome or Commit naming a device whose head is missing or stale is held (`MAX_PENDING` 8, `PENDING_TTL_S` 7 days), `group_pending_refresh` asks for the head through the existing relationship-bound `ask_head`, `group_pending_retry` re-runs every held frame on a copy of the state and keeps the result only on success. Nothing is accepted unverified. For an account with no relationship there is no anchor: held until it expires (question Q5). Test: `a_welcome_naming_an_unanchored_account_is_held_and_then_expires`. Test for the head arriving: `a_held_welcome_applies_once_the_missing_head_arrives`. |
| 3. G10 `commit_accepted` | Done. `Core::group_settle` accepts an own Commit once it is durable, its parent is still the canonical tip and no better commit was received; no timer, no relay acknowledgement or order. A lost race is `CommitSuperseded`. `Core::group_do(CommitAccepted)` now refuses with `acceptance_is_core_owned`; the tests' manual acceptance is gone. Frames for the device lane are queued in a Core-owned outbox (`group_outbox`, `group_outbox_done`). |
| 4. Room profile defaults | Done as Core state and intents. New `crates/om-core/src/groups/room.rs` (Recommended, Hardened, Maximum Privacy bundles, `Setting`, `diff`, `Notice`). `Create` is Recommended (one-tap join, members can add, no history for new members, disappearing off); `CreateWithProtection` and `RoomSetting` change them. Tightening needs no confirmation; any loosening is refused with `loosening_needs_confirmation` unless `confirmed`; every change is stored as a Notice and emitted as `PolicyChanged` on every member's device with `by_account` and `loosened`. New reducer ops ADMISSION_SET, SECURITY_POLICY_SET, DISAPPEARING_TIMER_SET, and an MLS update-commit (`CommitPlan::Update`, fake and OpenMLS `self_update`). |
| 5. Signed `GroupControlIntent` (type 29) | Done for Commit and LeaveRequest out and in (role `device`, label `OM-GROUP-INTENT-SIG-v1`, verified against the directory key). Route updates are wired out and in (`group_route_update`). `admit_request` is not wired (Q5). |

Bug found and fixed on the way: `RoleDef::value` did not order its grant set by encoded bytes, so adding `room.invite` to a role produced a state the strict codec refuses (`set_order`). It now sorts.

Results (in the working tree): see "Verification" at the end of this section.

### Files changed (exact, uncommitted)

New: `crates/om-core/src/groups/room.rs`, `crates/om-core/tests/groups_carriage_2b.rs` (8 tests, with and without `groups-openmls`). Both registered with `python3 tools/om_registry.py sync`.

Changed: `crates/om-core/src/engine.rs`, `crates/om-core/src/flows/ceremony.rs`, `crates/om-core/src/flows/multi.rs`, `crates/om-core/src/groups.rs`, `crates/om-core/src/groups/carriage.rs`, `crates/om-core/src/groups/carry.rs`, `crates/om-core/src/groups/fake.rs`, `crates/om-core/src/groups/flow.rs`, `crates/om-core/src/groups/journal.rs`, `crates/om-core/src/groups/mls.rs`, `crates/om-core/src/groups/openmls.rs`, `crates/om-core/src/state.rs`, `crates/om-core/tests/groups.rs`, `crates/om-core/tests/groups_carriage.rs`, `crates/om-core/tests/groups_core.rs`, `crates/om-core/tests/groups_flow.rs`, `crates/om-core/tests/groups_loopback.rs`, `crates/om-wasm/src/bridge.rs` (JSON arms for the new events and the Notices view, and the old `commit_accepted` command now runs one `group_settle` pass; no new bridge API), [`machine/file_registry.json`](../../machine/file_registry.json), and this report.

### Design gate output for the committer

`python3 tools/design_gate.py needs <changed files>` lists six domains, each with its documents and ADRs: `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events`. The committer reads every document and ADR it prints (run the command again for the full list) and writes `Design-Read:` with those six ids and `Design-Departures:` per the D-numbers below.

### Deviations confirmed by the committer (D38 to D44, in the relay-status table)

Committer review (2026-11-14): the builder's proposals were checked against ADR-0189, ADR-0128, ADR-0177 decision 3, GROUP_COMMIT 3.2 and the 1:1 session-management passages. D42 is dropped as a design deviation (an engineering transition, listed under Package 3). Q5 became D44. Added in review: a fail-closed guard, `quorum_not_supported`, so a loosening is refused when the Room's governance threshold is above 1 (Core collects no quorum), and the missing D37 test (`a_held_welcome_applies_once_the_missing_head_arrives`).

- **D38 (T-161):** policy and role ops are sequenced as non-reducing; ADR-0177 decision 3 (recomputing `reducing`) is open.
- **D39:** no quorum collected on loosening; refused above threshold 1.
- **D40:** a lane Accept with a stale pinned head is dropped, not held.
- **D41:** a Commit before its Welcome is dropped; no ordering buffer.
- **D43:** `admit_request` not wired (depends on D44).
- **D44:** no anchor for the head of an account with no relationship to the receiver (was Q5).

### Questions for the operator (the audited text is not amended)

- **Floor (D39):** ADR-0128 rule 8, the capability registry and GROUP_CHAT say governance never lowers an established floor. ADR-0189 section 3 allows loosening the privacy-profile fields with explicit confirmation and an announcement. Does "floor" include the privacy profile of a Room? Core follows ADR-0189 (confirmed, announced loosening) until ruled.
- **Q5/D44:** how does a receiver anchor the head of an account it has no relationship with (Welcome carries the signed head, or a directory fetch)?

### What Package 3 needs

- The host calls `group_settle` after every durable step and at start-up; drains `group_outbox` (deposit each `Sealed.wrapper`) then `group_outbox_done`; calls `group_pending_refresh` and `group_pending_retry` after each poll; calls `group_route_update` after a removal.
- wasm and TypeScript bridge: the new APIs (`group_settle`, `group_outbox`, `group_outbox_done`, `group_pending_retry`, `group_pending_refresh`, `group_route_update`), the new intents (`CreateWithProtection`, `RoomSetting`), the `Notices` query, the new events (`held`, `policy_changed`, `room_setting_blocked`, `commit_superseded`), and `CommitAccepted` removed.
- Driver: route-offer top-up (`supply_routes`), polling per-Room mailboxes, and the end-to-end test with a real relay.
- Remove the `commit_accepted` shim (former D42) from the bridge and TypeScript; own-device head sync on later chain advances (D36); admit_request once D44 is ruled; a frame ordering buffer (D41); governance quorum collection (D39).
- UI copy for the three profiles and the loosening confirmation (from the UX brief; none was written here).

### Verification (builder, working tree, uncommitted)

`cargo test -p om-core` and `cargo test -p om-core --features groups-openmls` (no failures, including the 7 new tests), `cargo test -p om-wasm`, `cargo fmt`, `cargo clippy -p om-core -p om-wasm --all-targets` (0 warnings, default features; also clean with `--features groups-openmls` for om-core) and `make check` all pass. Committer re-ran all of these with the added test (8 in `groups_carriage_2b.rs`).

## Operator-delegated rulings on the 2b questions (2026-11-14)

Basis: easy by default, strongest security as opt-in, fail closed, no new authority or metadata, no change to audited text.

1. **"Floor" (ADR-0128 rule 8, GROUP_CHAT).** The floor is the cryptographic and authority baseline (MLS properties, signed control, no silent downgrade of those), not a Room's chosen privacy profile. A Room's profile may be loosened only by the explicit, confirmed, announced action ADR-0189 describes (marked `loosened` to every member); it is never silent. Core keeps its current behaviour. Where a Room's governance threshold is above 1 the existing `quorum_not_supported` refusal stays (fail closed) until quorum collection is built (D39).
2. **D44 (anchoring the head of an account with no relationship).** The inviter's Welcome release evidence carries the signed account head for every account it names. The receiver verifies it by chain descent from the head it pinned in the Invite from the inviter's account, never by a directory lookup (no new metadata, no new authority). A head that does not verify leaves the Welcome held until it expires (current behaviour). Implementation belongs in package 3 together with `admit_request` (D43).
