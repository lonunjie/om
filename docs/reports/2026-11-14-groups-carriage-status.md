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
