# Multi-device Core: phases 2 to 4 status and open questions (2026-11-04)

Scope: `core_kernel` (`crates/om-core`) with the minimum in `packages/om-core-wasm` and `packages/om-provider-http`. Commits:
phase 2 `eeeb4d0` (peer head refresh and head fetch), phase 3 `689c8b5` (per-device fan-out, self-sync, crossed
initiation), phase 4 (device-link ceremony, this change). Deviation D24 in
[`2026-10-28-relay-status.md`](2026-10-28-relay-status.md).

## Design sources read

Read: [`AGENTS.md`](../../AGENTS.md) (Design gate), [`ENGINEER_START_HERE.md`](../process/ENGINEER_START_HERE.md), the [`design_gate.py`](../../tools/design_gate.py) list for `crates/om-core`
(domains `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`,
`time_ordering_events`) as far as these were needed: ONE_TO_ONE_SESSION_MANAGEMENT sections 3 and 4 (K4 to K12),
ACCOUNT_CONTINUITY_AND_CONTROL 4.1, 9 and 12, MULTI_DEVICE_SYNC (notice requirement), ENDPOINT_COERCION 2, ASYNC_KEY_MATERIAL
2.1 and 3, ADR-0143, ADR-0144, ADR-0181 (confirmation nonces), the W0 CDDL for `DeviceEnrollmentNotice`, the domain
registry, the platform-api device types and the UX brief lines on linking.

Not read (stated, not claimed): ADR-0181 in full, IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS and
[`dangerous_algorithm_contracts.json`](../../machine/dangerous_algorithm_contracts.json), CRYPTOGRAPHY_AND_AGILITY, ASYNC_KEY_MATERIAL beyond 2.1 and 3, the discovery
documents, the metadata_privacy, relay_reachability and time_ordering_events documents in full, most listed ADRs, the
TLA+ models and remaining V-003 files, home-wire and the Cloudflare Home source beyond greps, the full `control.rs`
`authorize` body, and the existing tests in detail.

## Phase 4: what is built

`OfferDeviceLink`, `RequestDeviceLink`, `ApproveDeviceLink`, `RejectDeviceLink` are real (`flows/ceremony.rs`). The existing
control device offers `om:link/...`; the new device asks on an empty profile; both show the same six-digit code
(`GetLinkOffer`, `GetLinkRequest`, `DevicesView.linkRequests`); a confirmed approval appends one `link_messaging` successor
(re-verified with `verify_successor`), signs a `DeviceEnrollmentNotice`, delivers it to every other active device (or
refuses the approval and changes nothing), hands over the active conversations and tells contacts the new head. The new
device adopts nothing unless the chain lists its own rebuilt credential and the notice verifies; it then publishes keys,
sets up its side of each conversation and introduces itself through the approving device, which forwards the
introduction to the contact (account update, field 4). Other devices verify the notice and advance their own head only
along a verified descent (`KIND_ENROLLED = 11`, siblings only). A messaging device cannot offer or approve; revoke
already removes a device. Transport and the host calls (`OpenQueue`, polling and acknowledging with an explicit
capability) are in D24; the provider adapter implements them over the Home's existing queue endpoints.

Evidence: `tests/device_link.rs` (two Cores link, both send and receive with a third; revoke removes the device; bad or
missing confirmation, unknown request, replay, expiry, refusal; a messaging device cannot offer; the ceremony key is on no
relay; a third device receives the notice) and `tests/device_link_no_notice.rs` (an approval without the notice is
refused by the new device, which adopts no account and publishes nothing). The 16,200-case ACS differential stays green.

## Not exercised

The TypeScript adapter changes (`open_queue`, polling and acknowledging with a capability) type-check and the existing
adapter and wasm tests pass, but they were not run against a real Home or the Cloudflare worker. The wasm artifact
(`make core-wasm`) was not rebuilt.

## Open questions (design is silent; nothing here was decided beyond the operator's D24 ruling)

1. Transcript MAC construction for `OM-DEVICE-LINK-CEREMONY-MAC-v1` (reserved, no construction). The derivation used is provisional.
2. Signer role and device-class numbering of `DeviceEnrollmentNotice` (controller and 1 assumed).
3. How a new device learns its own label (it is "New device" until the person renames it) and how the approving device's account label reaches it (sent in the approval).
4. Quarantined control devices get no notice (they cannot receive); should a quarantined device block an approval?
5. A long ACS chain does not fit one queue message (the approval carries the whole chain; refused with `quota` above about 100 KB).
6. Conversations that did not complete within the five-minute window stay unhanded; there is no retry channel until history transfer exists.
7. Sibling mesh: a third device meets the new device only through contacts' updates, not directly (T to N sessions and intros).
8. Intro service and fetch-capability fields in an introduction are unsigned (a wrong one only makes a fetch fail).
9. Session reset policy for an established-session conflict on re-initiation stays `SessionConflict`.
10. Stale queued `KIND_ACS_UPDATE`s are not coalesced; held wrappers are bounded to 8 and 7 days (ADR-0144 gives no number).
11. A queue-delete endpoint, second-device Home admission (D13) and the UI direction reversal are in D24.
