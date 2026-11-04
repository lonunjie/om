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
capability) are in D24. The adapter side (`open_queue`, polling and acknowledging with a capability) was committed by the UI agent (`15287f1`).
Every ceremony message is wrapped as a profile-1 sealed envelope the Home accepts (D24 note 9).

Evidence: `tests/device_link.rs` (two Cores link, both send and receive with a third; revoke removes the device; bad or
missing confirmation, unknown request, replay, expiry, refusal; a messaging device cannot offer; the ceremony key is on no
relay; a third device receives the notice) and `tests/device_link_no_notice.rs` (an approval without the notice is
refused by the new device, which adopts no account and publishes nothing). The 16,200-case ACS differential stays green.

## Not exercised

The two-context link runs against the local reference Home in Chromium (`apps/web/e2e/device-link.spec.ts`); the Cloudflare
worker was not exercised.
