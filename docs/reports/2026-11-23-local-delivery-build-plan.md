# Local delivery build plan (scope `local_delivery`, authorized by ADR-0257)

Date: 2026-11-23. Status: scope **authorized** on the operator's explicit instruction, 2026-11-23 ([`ADR-0257`](../../decisions/ADR-0257-authorize-local-delivery-engineering.md), Accepted). This is a plan; no product code for the scope exists yet beyond the pieces named in section 3.

## Design sources read

Read in full: [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md), [`ADR-0158`](../../decisions/ADR-0158-lan-delivery-profile-v1.md), [`lan.cddl`](../../spec/cddl/lan.cddl), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), the `local_delivery` entry of [`engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json), the V-003, V-015 and V-024 gate records (criteria and progress), and [`ADR-0257`](../../decisions/ADR-0257-authorize-local-delivery-engineering.md).

Read in part: [`direct.rs`](../../crates/om-core/src/direct.rs) (module header and constants) and [`2026-11-13-direct-transfer.md`](2026-11-13-direct-transfer.md) (result and decisions table).

**Not read, and every slice must read before it builds** (run `python3 tools/design_gate.py needs <slice paths>` and read every document and ADR it prints, digest-first for the constitution): [`SEALED_SENDER_DELIVERY_PROFILE.md`](../architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`TRANSPORT_ADAPTERS_AND_INTEROP.md`](../architecture/network/TRANSPORT_ADAPTERS_AND_INTEROP.md), the discovery documents for first contact ([`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md), [`DISCOVERY_FABRIC.md`](../architecture/discovery/DISCOVERY_FABRIC.md)), [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), the crypto documents for session state, the UX brief ([`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md)) and ADR-0150, ADR-0194. The V-024 prototype results ([`verification/prototypes/v024-lan-wire`](../../verification/prototypes/v024-lan-wire), [`verification/bounded/v024_lan_models_r16b.py`](../../verification/bounded/v024_lan_models_r16b.py)) were not re-run for this plan.

## 1. What the scope covers, per the design

Optional offline-local messaging and encrypted file transfer between already-related or QR/invite-linked peers when the WAN is down but the LAN works. It is a transport binding, not a second protocol: canonical events, EventIDs, BlobIDs and security levels are unchanged, and a LAN route is never identity or authority (ADR-0158 sections 5 to 8).

- **Constants:** 900-second rendezvous epochs with adjacent-epoch skew only, K = 8 advertised tags, 16-byte tags, 32-byte route capability, 16 KiB chunks, 262,144 / 65,536 byte backpressure marks.
- **Rendezvous:** a pair seed from the PQXDH secret (`OM-LAN-PAIR-SEED-v1`) or an invite seed (`OM-LAN-INVITE-SEED-v1`); tag by `OM-LAN-RENDEZVOUS-TAG-v1`.
- **Signalling:** seed-encrypted `LanSignalEnvelopeV1`; SDP never in clear; local endpoint, copy/paste or QR.
- **Channel:** ordered, reliable WebRTC `RTCDataChannel` labelled `om-lan-v1`, host candidates only, no project-run STUN, TURN or signalling.
- **Route:** an ephemeral `om+lan://v1/<route-token>` `RelayEntry` reusing the frozen `OM-SEALED-DELIVERY-ROUTE-v1` binding, so 1:1 traffic is an ordinary `SealedDeliveryEnvelope`. Never in a public RelaySet; rotated at epoch or link-layer change.
- **First contact:** a local adapter of the ordinary IntroGate (ticket, `MessageRequest`, reciprocal binding, only then `fetch_async_key`).
- **Privacy:** off by default per contact, off under Hardened and Maximum, retired on block or relationship end, fixed K tags with decoys, presence-inference consequence described in the UI.
- **Capability honesty:** where a runtime cannot establish the channel the capability row reports `local_delivery` unavailable; no weaker fallback.

## 2. Blocking gates and prerequisites

| # | Condition | State |
|---|---|---|
| 1 | Accepted ADR names the scope | Met: ADR-0257 |
| 2 | Gates V-003, V-015 pre-engineering | Passed |
| 3 | Gate V-024 pre-engineering | **Waived by ADR-0257 for engineering only.** Met repo-locally: frozen [`lan.cddl`](../../spec/cddl/lan.cddl) with two-codec vectors, passive-observer and offline-first-contact models. **Open:** real-browser DataChannel feasibility (or an explicit unavailable result) and independent review |
| 4 | `depends_on` | `core_kernel` authorized |
| 5 | Validator | Passes (12 of 12 scopes) |

Pre-release (never an engineering blocker, required before any release offers LAN delivery): two independent implementations interoperating over a real LAN, including a browser client where the capability row claims support.

## 3. What already exists

| Piece | Where | Gap |
|---|---|---|
| Frozen wire (`LanFrameV1` kinds 1 to 8, `LanSignalEnvelopeV1`, invite rendezvous record) | [`spec/cddl/lan.cddl`](../../spec/cddl/lan.cddl), registries | Product Rust codec not yet in `om-codec`; only the receipt vectors test exists |
| Direct transfer state machine (sans-IO, resumable, backpressure, digest binding, receipt) | `crates/om-core/src/direct.rs`, `packages/om-core-wasm/src/direct-driver.ts` | Mirrors the frames but has no wire codec and no channel (its header says so) |
| WebRTC code | `packages/om-web-platform/src/calls/transport.ts` | Built for calls, not for the `om-lan-v1` DataChannel; reuse only after reading its design gate |
| Sealed delivery, `RelayEntry`, route binding, IntroGate and MessageRequest state machine | `om-core`, `om-provider-http` | A `lan` transport kind and the ephemeral route table are new |

## 4. Slices

Each slice reads its design-gate output first, ends with `make check`, the touched crate or package tests and a commit carrying `Design-Read` and `Design-Departures`. Heavy runs go through `bin/lonunjie-heavy` on emeth4.

| Slice | Content | Done when |
|---|---|---|
| 0 | **Confirm the freeze.** Rerun the V-024 vector and model prototypes; add the product Rust codec for every [`lan.cddl`](../../spec/cddl/lan.cddl) object to `om-codec`; replay the positive and negative vectors; check the registered labels and numeric fields. Decide the two open registries (`object_kind` meanings and error codes, see Q2) by ADR by delegation | The Rust codec agrees with both prototype codecs on every vector; ADR-0165 checks pass |
| 1 | **Rendezvous core (sans-IO, `om-core`).** Seed derivation from the session secret and from an invite, epoch and tag functions, accepted-epoch window, fixed K = 8 tag set with decoys and deterministic subset rotation, per-contact opt-in state, retirement on block and relationship end, rate limit before any expensive work | Vectors for tags in epochs e-1, e, e+1; observer mutation tests from the V-024 model run against the product code; retirement test across all devices |
| 2 | **Signalling.** `SignalKey`, signal envelope seal and open with the existing AES-256-GCM-SIV primitive, fresh session id and nonce rule, replay rejection across epochs | Envelope round trip; tamper, wrong epoch and nonce reuse rejected; no new primitive |
| 3 | **Local route and delivery.** Ephemeral `om+lan://v1` `RelayEntry`, the route table with epoch and address-change rotation, `SealedDeliveryKeyRecord` using the frozen binding, a `lan` transport in the delivery adapter, same-EventID dedupe against a later WAN retry (NT-06) | A two-peer loopback test delivers a 1:1 message as an ordinary sealed envelope; a WAN retry of the same EventID materializes once; no clear-sender path exists |
| 4 | **Channel driver and feasibility.** `om-lan-v1` DataChannel with host-local candidates in `om-web-platform`, manual and QR signalling, `LanFrameV1` streaming with backpressure, the capability row. Run on Chromium, Firefox and WebKit on emeth4 and on real devices; record where the channel cannot be established | Per runtime: established and frame and backpressure tests pass, **or** an explicit `local_delivery = unavailable` result is recorded. This closes the open V-024 feasibility criterion |
| 5 | **Offline first contact.** The local IntroGate adapter over the channel; the ordinary request, acceptance and reciprocal binding; async key fetch denied until relationship authority exists | LAN reachability never reaches `ACTIVE`; the V-024 first-contact mutation cases pass against product code |
| 6 | **Files over the channel.** Wire `DirectSender` and `DirectFetch` to the `LanFrameV1` codec and the channel; `fetch_async_key`; local receipt versus StoragePool durability preserved; later WAN backfill keeps EventID and BlobID | A multi-MiB file transfers, resumes from an acknowledged offset and is confirmed only by the authenticated receipt |
| 7 | **UI.** Per-contact "Find each other on the same Wi-Fi" toggle with the presence consequence stated, off by default and unavailable under Hardened and Maximum; offline invite and QR flow with short expiry guidance; local-receipt versus durable status; capability row; reduced-motion and accessibility per the UX brief | UX brief definition of done met; Playwright specs on the three engines |
| 8 | **Evidence.** The V-015 LAN trace and packet-capture plan run against product code, parser fuzzing of every frame and envelope decoder under the agreed CPU-hour budget, the two-implementation interop harness for release | Evidence recorded; nothing claimed as passed that needed a second implementation or a real LAN |

Slices 0 to 3 are pure Core and wire and can be built and tested on emeth4 with no UI. Slice 4 is the feasibility gate for everything the browser can claim; if a runtime fails it, slices 5 to 7 still ship for the runtimes that pass and the capability row reports unavailable for the rest.

## 5. Constitution rules that bind it

C-01 (no project-run signalling, STUN, TURN or discovery service), C-12 (authority never moves to a network location), C-15 (an accepted relationship needs no discovery infrastructure), C-16 (no surveillance mechanism), C-20 (floors unchanged, no new primitive). ADR-0257 restates these as constraints.

## 6. Questions for the operator or an ADR by delegation

| ID | Question | Safest default |
|---|---|---|
| Q1 | Browsers may hide host IPs behind mDNS candidates, and iOS asks for a local-network permission. If a runtime cannot meet the channel, is a QR/manual-only mode acceptable as the supported path there? | Yes, exactly as ADR-0158 section 4 says; otherwise report unavailable |
| Q2 | `object_kind` meanings for values 1 to 8 and the error code registry are only ranges in [`lan.cddl`](../../spec/cddl/lan.cddl); direct transfer used 8 for blobs and 1 to 3 for errors locally | Register them additively by ADR-0165 in slice 0, keeping the direct-transfer choices |
| Q3 | The pair seed needs the PQXDH `SK` from session establishment. Confirm which Core module exposes a purpose-separated export without touching ratchet keys | Add a narrow export in session state, never a ratchet key accessor |
| Q4 | Default value of the per-contact toggle for a brand-new contact | Off, per ADR-0158 section 9 |
| Q5 | K = 8 is fixed by the profile; the V-015 LAN trace criterion names K as a policy constant | Keep 8 |

## 7. Routing and budget

Slices 0 to 2 are well specified with checkable vectors and suit a mid tier worker; slices 3, 5 and 6 touch session, delivery and first-contact state and stay on the session model, each paying the design-gate reading cost (about 70 to 140k tokens). Slice 4 needs emeth4 and real devices. Before dispatching any package, run the usage gate described in the lonunjie [`AGENTS.md`](../../AGENTS.md).

## 8. What this plan did not do

It wrote no code, ran no tests, edited no ledger and changed no ADR. The Q-list is for the operator; nothing in it amends audited text.

## Update 2026-11-25: slice 6 wire and driver, without the real channel

Design sources read for this update: none re-read this session beyond the frozen [`lan.cddl`](../../spec/cddl/lan.cddl) text and the corpus; the code replays the frozen V-024 vectors. Not done: the WebRTC `om-lan-v1` channel in `om-web-platform` (a design-bound path whose documents have not been read), the receipt MAC and digest bindings (host-injected here), `fetch_async_key` and `fetch_blob` frames, first-contact wiring.

Added: `om_core::lan_frame` (typed transfer frames, replayed against the frozen corpus), `lan_frame_encode` and `lan_frame_decode` in om-wasm (`scope-engines`), and `packages/om-core-wasm/src/lan-driver.ts` (a sender and receiver over an abstract ordered channel). Tested with two real module instances over an in-memory channel: a 70,000-byte object in 16 KiB frames, confirmation only on an authenticated receipt, and resume from a durable offset. This is not a WebRTC, browser or real-LAN result.

## Update 2026-11-25 (late): slice 4 adapter, `om-lan-v1` channel without a real network

Design sources read in full this session: [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md) and [`ADR-0158`](../../decisions/ADR-0158-lan-delivery-profile-v1.md). Listed by the design gate for `packages/om-web-platform` and **not read this session**: the client documents (PWA reference client, client and extension architecture, vault, profile isolation, update lifecycle, notifications, plugin architecture, endpoint coercion) and ADRs, the discovery documents, and the UX documents. The adapter adds no UI and no product semantics, but it was written without them, so a reviewer should read it against those documents.

Added: `packages/om-web-platform/src/lan/channel.ts` (export `@om/web-platform/lan`): the ordered `om-lan-v1` DataChannel with `iceServers: []`, text offer and answer for the host to seal in `LanSignalEnvelopeV1`, the profile's 262,144 and 65,536 byte backpressure marks, a `lanChannelCapability` row (`unavailable` when no peer connection exists; no weaker fallback), a 12,288-byte SDP bound. Five unit tests with a fake peer connection pass. Not covered: real browsers (the earlier feasibility spike covers Chromium and Firefox on one machine; WebKit not established), the seed-sealed signal envelope wiring, real phones or networks.
