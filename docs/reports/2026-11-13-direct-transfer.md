# Direct transfer of large attachments, Core side (group G)

- **Date:** 2026-11-13. **Scope asked for:** `core_kernel` (`crates/om-core`). **Status:** built and tested against a loopback byte channel; committed 2026-11-13 after the committing session read the six printed domains in full (see below).
- **Result in one line:** `om-core` now has a sans-IO direct-transfer state machine that serves the sealed object (same BlobID, same `ciphertext_digest`) over any ordered byte channel, with resumable 16 KiB chunks, backpressure, digest binding before plaintext and the ruled refuse-then-offer decision. It is not yet wired into the engine, the wasm bridge or the UI.

## Design sources read

Read in full by the committing session (2026-11-13), from `python3 tools/design_gate.py needs crates/om-core/src/direct.rs crates/om-core/tests/direct_transfer.rs crates/om-core/src/lib.rs`: every document and ADR printed for the six domains `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events` (30 documents, among them [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), [`REACHABILITY_AND_OBJECT_REPAIR.md`](../architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md), [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md), [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md); and the 49 listed ADRs, including ADR-0126, 0130, 0140, 0159). Read in full by the authoring session: AGENTS.md "Design gate"; the v1 completion plan section 3 (Attachments row); [`2026-11-04-attachments-core-status.md`](2026-11-04-attachments-core-status.md); ADR-0158; [`spec/cddl/lan.cddl`](../../spec/cddl/lan.cddl); [`spec/protocol/lan-delivery-profile.json`](../../spec/protocol/lan-delivery-profile.json); [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md); ADR-0150 decision 1 and the direct-file subsection (later decisions only skimmed by grep); [`machine/repository_contract.json`](../../machine/repository_contract.json) (`local_delivery`) and the scope table in [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md).

**Not read:** ADR-0156, `V-003`/`V-015`/`V-024` gate texts, the ADRs that amend the printed documents but are not in the printed list (for example ADR-0131, 0135-0138, 0141-0147, 0154, 0161), [`home-api.md`](../../spec/protocol/home-api.md) (untouched here).

## Authorization finding (needs the operator's eye)

`local_delivery` (ADR-0158, the LAN binding) is `authorized: false`; its gates V-003, V-015 and V-024 are open. The completion plan and the Attachments row nevertheless put "direct online or LAN transfer bound to BlobID and ciphertext digest" in WP3, and ADR-0150 and [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) make direct encrypted transfer part of the V1 blob path (`core_kernel` lists blobs). I therefore built only the part that is blob semantics and transport-neutral, and left out everything that is `local_delivery` itself: the `LanFrameV1` canonical CBOR codec, rendezvous tags and seeds, `LanSignalEnvelopeV1`, the WebRTC DataChannel and the `om+lan://` route. **Committing session's ruling (2026-11-13): this is `core_kernel` blob work, and the text does not contradict it.** Reasons: (1) the 2026-11-04 completion plan puts "larger files: direct online or LAN transfer bound to BlobID and ciphertext digest" in the Attachments row; (2) [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) "Zero-payment personal bootstrap" says V1 online/LAN file transfer prefers direct encrypted paths carrying the same BlobID/`ciphertext_digest`-bound object, and local receipt is not StoragePool durability (AT-06, ADR-0150), which is exactly the semantics built; (3) `core_kernel` in [`repository_contract.json`](../../machine/repository_contract.json) lists blobs, while `local_delivery` is described as the "offline-local LAN message/file delivery **binding** frozen by ADR-0158", i.e. the LanFrameV1 codec, rendezvous and DataChannel, none of which is built here; (4) nothing read in the six domains forbids or relocates it. Constraints noted from the reading: a future WAN path exposes peer addresses to each other (METADATA_THREAT_MODEL section 15 analogue, and the Hardened profiles' relay-only preference), so it needs a declared route policy rather than a silent default; the direct object must stay inside the sealed 1:1 path for the message itself (the `AttachmentRef` travels in the ordinary event, never in the clear); and the read capability in the ref is relationship-scoped, as the blob provider needs no AccountID or ConversationID. `Design-Departures: none` holds on that basis.

## What is built

| Piece | Where |
|---|---|
| `DirectFrame` (mirrors `LanFrameV1` kinds `fetch_blob`, `object_begin`, `object_chunk`, `object_end`, `ack`, `error`; no wire codec) | `crates/om-core/src/direct.rs` |
| `plan_size(limits, len)`: `Spool`, `OfferDirect{spool_max}` (refused for the spool, nothing sent, offer "send when they are online"), or `Refused(Empty / TooLarge{max})` | same |
| `DirectSender`: holds the sealed object; checks BlobID, digest and read capability (constant-time, one uniform `unavailable` error, no oracle); `Begin`, then chunks of at most 16,384 bytes after the receiver's first ack; backpressure 262,144 high and 65,536 low; offer states `Waiting`, `Transferring`, `Delivered`, `Withdrawn` | same |
| `DirectFetch`: built from the received `AttachmentRef`; refuses a declared total that is not `plaintext_size + 16`; only contiguous offsets, never allocates the declared total; checks transport SHA-256 then `ciphertext_digest`; wipes on failure; `open` reuses `AttachmentRef::open` (digest before decrypt); resume keeps the held prefix | same |
| `Durability::LocalReceiptOnly` on both ends (AT-06: local receipt is not StoragePool durability) | same |
| 10 integration tests over a loopback channel and 1 unit test | `crates/om-core/tests/direct_transfer.rs`, `direct.rs` |
| Registered both files | [`machine/file_registry.json`](../../machine/file_registry.json) |

Tests pin: a 5 MiB file goes whole and decrypts equal; a drop after 5 chunks resumes from the held offset and sends only the rest; a flipped bit anywhere yields no plaintext and wipes the buffer; wrong capability, BlobID or digest get the same refusal; bad total, offset gap, oversize and empty chunks fail the receiver; backpressure pauses at the high-water and resumes only at the low-water; a withdrawn offer serves nothing; a ref without a read capability, or a sender given the wrong digest, fails closed; a 6 MiB file against a 4 MiB spool is `OfferDirect` and sends nothing.

## Decisions where the design is silent (please review)

| ID | Decision | Basis |
|---|---|---|
| D1 | **Transport for "direct online" (WAN): none chosen.** The module is transport-neutral. Over the LAN the transport is ADR-0158's DataChannel; a WAN path would need a signalling channel and the design names none (no OM-run STUN/TURN or signalling). The host maps any ordered reliable channel to `DirectFrame`. | Constitution (no OM-run service); ADR-0158 section 4. Whether and how WAN direct works is a question for the operator, not an ADR I may write. |
| D2 | **The recipient pulls.** The sender serves; the recipient sends `fetch_blob` using the read capability in the reference (`read_cap`, key 10) and, for a direct-only message, an `attachment-ref` with no `locations` (schema-valid, key 5 optional). | [`lan.cddl`](../../spec/cddl/lan.cddl) `fetch_blob`; decision B of the attachments report. |
| D3 | **Resume handshake.** `fetch_blob` has no offset. After `object_begin` the receiver's first `ack` carries the contiguous bytes it already holds and the sender continues from there; a fresh transfer id per connection. The sender sends nothing before that ack. | ADR-0158 section 8 ("resumes from an acknowledged byte offset") and the frozen frames; wire shape is my reading. |
| D4 | **Two digests.** `object_begin` carries plain SHA-256 of the transported bytes (as the CDDL says); the reference's `ciphertext_digest` is DS-HASH (`OM-BLOB-CIPHERTEXT-HASH-v1`). The receiver checks both; they are different values. | [`lan.cddl`](../../spec/cddl/lan.cddl) field 5; `attach.rs`. |
| D5 | `object_kind` 8 for blob ciphertext (`OBJECT_KIND_BLOB`, the CDDL range 1..8 with no registry in the files I read) and error codes 1 `unavailable`, 2 `protocol`, 3 `integrity` are local choices. Confirm against the V-024 registry when it exists. | [`lan.cddl`](../../spec/cddl/lan.cddl) gives only ranges. |
| D6 | **Same 16 MiB ceiling as the spool path** (`HARD_MAX_PLAINTEXT`): the object is sealed and opened in one piece. "Larger than the spool bound" therefore means roughly 4 to 16 MiB today. Streaming AEAD for bigger files would be new crypto, so not done. | "No new crypto". |
| D7 | **No advertised store still offers direct** (`spool_max` 0). Decision F of the attachments report (no store, no attachments) was about having nowhere to put the file; direct needs no store. Revert in `plan_size` if the operator disagrees. | Operatorless; AT-06. |
| D8 | A `quota` or `storage_full` refusal on a small file keeps its old behaviour; only the size case offers direct. | Ruled behaviour is about "too big". |

## Not built (what remains)

1. **Engine integration:** an intent for "send when they are online" (a message whose refs carry a read capability and no locations, event built at once, offer held in persisted state with expiry), a `NetCall`/host surface for the channel, serving and fetching from `Core`, and the `AttachmentView` states `waiting_for_recipient`, `transferring`, `delivered_locally` (distinct from `stored_for_later`). Persisting the sender's offer and the receiver's partial bytes across restarts (today in memory, as ADR-0158 says resume state is transport-local).
2. **Event ordering question:** with no spool copy, the recipient can only fetch while the sender is online; the message may arrive first. Is the message sent immediately with the offer, or when the recipient comes online? The ruling says "offer to send when the other person is online"; I did not implement either.
3. `LanFrameV1` CBOR codec and the rest of `local_delivery` (see the authorization finding), the wasm bridge commands and TS mirror, UI copy ("too big to send right now", offer button), a V-003 test of LAN receipt followed by provider backfill (AT-06 end to end), and a WAN direct path (D1).

## Verification

- `cargo test -p om-core`: all suites pass (direct_transfer 10, direct unit 1, attachments 21 unchanged).
- `cargo clippy -p om-core --all-targets -- -D warnings`: clean. `cargo fmt -p om-core` applied.
- `python3 tools/om_docs.py fmt` clean (0 unresolved links); `make check` passes after `om_registry.py sync` registered this report.

## Check of the decisions against the text (committing session)

D1 to D8 hold against the read text; no mismatch needed a code change. D2 and D3 are the report's own wire reading of the frozen frames and stay flagged as such. D5 (object kind 8, error codes) stays provisional until the V-024 registry. D7 is consistent with the "zero-payment bootstrap" text (the spool is bounded and optional; direct needs no store). Open and unchanged: event ordering when the message arrives before the offer is served, and any WAN transport.

## For the committing session (historical, done)

```
python3 tools/design_gate.py needs crates/om-core/src crates/om-core/tests
```

Printed domains for these paths: `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events`. Read them, then commit `crates/om-core/src/direct.rs`, `crates/om-core/src/lib.rs`, `crates/om-core/tests/direct_transfer.rs` and [`machine/file_registry.json`](../../machine/file_registry.json) with:

```
Design-Read: crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events
Design-Departures: none
```

`none` holds only if the operator accepts that direct blob transfer (not LAN discovery, rendezvous or framing codec) is `core_kernel` blob work; if not, cite `local_delivery` as unauthorized and hold the commit.

## Wiring status (2026-11-13, engine slice; code built and tested, NOT committed)

### Design sources read

Read in full by this session: AGENTS.md "Design gate", this report (all of it), `crates/om-core/src/direct.rs`, and the engine, state, API and attachment-flow code it touches (`engine.rs`, `state.rs`, `api.rs`, `flows.rs` compose, `flows/blobs.rs`). **Not read (the committing session must read them before committing):** the 30 documents and ADRs of the six domains printed by the command below. This session did not re-read them; the design decisions below rest on the report's own earlier reading and on the committed "Check of the decisions against the text".

### What was built (files, all uncommitted)

- `crates/om-core/src/state.rs`: `AppState.direct_offers: BTreeMap<Hex, DirectOfferRec>` (`#[serde(default)]`; a record without it loads, tested), `DirectStatus` (`waiting`, `delivered`, `withdrawn`, `expired`), `AttachRec.direct` (serde default false). An outgoing direct attachment has no owner capability and is never put to a store, so every upload, quota and delete path ignores it.
- `crates/om-core/src/api.rs`: intents `SendAttachmentsDirect` and `WithdrawDirectOffer`; queries `PlanAttachment {size}` (`AttachmentPlanView`: `spool`, `offer_direct{limit}`, `too_large{limit}`, `empty`) and `GetDirectOffers` (`DirectOfferView`, with `needsFile`); `AttachmentState` gains `remote_direct`, `waiting_for_recipient`, `transferring`, `delivered_locally`, `not_offered`.
- `crates/om-core/src/flows/blobs.rs`, `flows.rs`: `Tx::prepare_direct_attachment` (refuses anything `plan_size` does not call `OfferDirect`; mints a random 32-byte read capability, no new label), the two intents, expiry (7 days, same as the spool default; finished offers forgotten after 30), `compose` holds an event back only for store-bound attachments.
- `crates/om-core/src/engine.rs`: sans-IO host entry points `plan_attachment`, `prepare_direct_attachment`, `arm_direct_offer`, `direct_serve`, `direct_pump`, `direct_fetch_start`, `direct_fetch_handle`, `direct_fetch_progress`, `direct_fetch_open`. Serving state and fetch state are memory only; the only durable effect of serving is `Delivered` (a `Step` with a commit).
- `crates/om-core/tests/direct_engine.rs` (5 tests) and [`machine/file_registry.json`](../../machine/file_registry.json) (registers it).

### Decisions

- **G-O1, message-before-offer ordering (the open question 2 above): the conservative option is that the message is sent immediately and only the file waits.** Reasons: (1) the person confirms the offer by sending (two steps: Core refuses and says so, then `SendAttachmentsDirect`), so nothing is sent without an explicit choice; (2) holding the message until the other person is online would need Core to learn presence, which is metadata the design does not give Core and the metadata threat model argues against; (3) the message then rides the ordinary sealed, ordered path with an event built at once (the same as a text message), so it never depends on a transport. Consequence, stated to the person in words: the recipient sees the file as available only while the sender is online (`remote_direct`), and the sender sees `waiting_for_recipient`. This is a reading of silent design text; if the operator prefers "hold the message", it is a change in `send_attachments_direct` only.
- G-O2: a file that fits the spool is not a direct candidate (`prepare_direct_attachment` returns `Invalid`); direct is offered only for "too big" (D8 unchanged).
- G-O3: persisted state holds no secret and no bytes. After a restart the offer shows `needsFile: true` and nothing is served (uniform `unavailable`) until the host hands the original file back (`arm_direct_offer`, which rebuilds the sealed object deterministically and refuses a different file). Expired, withdrawn and delivered offers cannot be armed.
- G-O4: `delivered_locally` means that device's receipt only (AT-06); it is a separate state from `stored_for_later` and never implies another device or an offline recipient can fetch.
- G-O5: receiver resume covers a dropped channel (held prefix kept in memory, the sender continues from it); after a process restart the receiver starts the fetch again, as ADR-0158 treats resume state as transport-local.
- No ADR needed; nothing departs from a read normative text. D1 (no WAN transport) and the `local_delivery` authorization finding are unchanged.

### Tests and checks (this session)

`cargo test -p om-core` all suites pass (new: `direct_engine` 5: refuse-before-send and plan, message-at-once plus 6 MiB transfer with a drop after 5 chunks, resume, `transferring` and `delivered_locally` surviving a restart; offer surviving restart, served only from the same file, uniform refusal before arming; withdraw and lapse; old state without the field loads). `cargo clippy -p om-core --all-targets -- -D warnings` clean, `cargo fmt` applied, `cargo build --workspace` builds (`om-wasm` compiles against the new enums), `python3 tools/om_docs.py fmt` 0 unresolved, `make check` passes. **Not run:** `cargo clippy -p om-wasm --target wasm32-unknown-unknown` fails in this environment before and after the change (the `wasm32-unknown-unknown` std is not installed for the active toolchain: "can't find crate for core"), so wasm conformance was not verified; no TypeScript was touched, so `tsc` was not run.

### What remains

1. **wasm bridge commands** (`crates/om-wasm`, `packages/om-core-wasm`): `plan_attachment`, `prepare_direct_attachment`, `arm_direct_offer`, `direct_serve`, `direct_pump`, `direct_fetch_*`, with a `DirectFrame` JSON shape (bytes as hex like `hexs`; no CBOR codec). Then the TS mirrors in `packages/om-platform-api` (`views.ts`: the five new `AttachmentState` values, `AttachmentPlanView`, `DirectOfferView`; `intents.ts`: the two intents; the query kinds) and the harness. Needs the wasm target installed to verify.
2. **UI copy** (separate slice, per the voice rules): "Too big to send through the delivery service, so it was not sent. Send it straight to them when they are online?"; waiting: "Waiting for them to be online"; sending: "Sending to them now"; done: "They have it. Only their device holds it, nothing is kept for later."; not offered: "No longer offered"; recipient: "Available while they are online".
3. The host driver that supplies the channel and re-arms offers at start (needs a transport: D1, `local_delivery`), a V-003 test of LAN receipt followed by provider backfill, and the `LanFrameV1` codec.
4. Receiver partial bytes across a process restart (host-held prefix handed back); a sender-side check that the channel belongs to the conversation (today the read capability, relationship-scoped, is the gate).

### For the committing session

```
python3 tools/design_gate.py needs crates/om-core/src crates/om-core/tests
```

Read the printed domains (`crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events`), then commit `crates/om-core/src/{api,engine,flows,state}.rs`, `crates/om-core/src/flows/blobs.rs`, `crates/om-core/tests/direct_engine.rs` and [`machine/file_registry.json`](../../machine/file_registry.json) with `Design-Read: crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events` and `Design-Departures: none` (hold if the operator rejects G-O1).

### Gate progress (2026-11-14, stopped, nothing of the wiring slice committed)

A third session was asked to read the six domains in full (about 360 KB of documents plus 49 ADRs, over 130k tokens) and then commit. It could not finish the read inside its context budget, so it committed no code and claims no `Design-Read`. Read in full by it: AGENTS.md "Design gate", this report, [`TIME_AND_EXPIRY_SEMANTICS.md`](../architecture/events/TIME_AND_EXPIRY_SEMANTICS.md), and the diff stat; read only by grep: [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) (direct transfer carries the same BlobID/digest-bound object; local receipt is not durability, AT-06: consistent with G-O4 and `delivered_locally`). The provider-retention text says provider deletion time is availability behavior, not authority, which does not contradict a 7-day offer expiry. G-O1 not yet checked against the remaining text. The slice stays uncommitted in the working tree for a session that can read the six domains in batches (one domain per session is feasible: commit trailers must still list all six, so read all before committing).

### Gate completed (2026-11-14, fourth session): read in full, checked, committed

**Design sources read in full:** AGENTS.md "Design gate"; this report; all 29 documents and all 47 ADRs that `design_gate.py needs crates/om-core/src crates/om-core/tests` printed for `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events`, each with the Read tool. **Not read:** ADRs that amend those (for example ADR-0131, 0136-0139, 0142-0147, 0150, 0154) beyond what the read documents quote of them; none of them touches blob transfer.

**Check of the wiring decisions against the text (no contradiction found, no ADR needed):**

- **G-O1 (message at once, only the file waits).** No text requires holding a message for a recipient's presence. [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md) makes presence an optional, best-effort ephemeral hint never needed for correctness, and [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md) wants reduced presence under the stricter profiles, so Core learning presence in order to hold a message would be new metadata. [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) says online/LAN transfer prefers direct encrypted paths carrying the same BlobID and `ciphertext_digest` object, and AT-06 says local receipt is not durability: both match G-O1 and G-O4. The message is an ordinary `message.create` with an `AttachmentRef`, ordered and sealed like any other.
- **7-day offer expiry.** A local retention rule, not a canonical time field. [`TIME_AND_EXPIRY_SEMANTICS.md`](../architecture/events/TIME_AND_EXPIRY_SEMANTICS.md): provider and retention time is availability behaviour, never authority; the offer is not signed, carries no authority and lapses by Core's own clock. Same value as the spool default.
- **New API surface.** Typed intents and queries only, Core authoritative, nothing mutates crypto or control state (ADR-0159 rule 3, AGENTS shared UI constraints).
- **Offer persistence.** Persisted state holds no secret and no bytes; the key and read capability live only inside the sealed message. The read capability is random, not derived, so no new domain-separation label is needed. A restored backup shows `needsFile` and serves nothing, consistent with "backups restore history, not authority". Refusals are uniform, so there is no existence oracle.

Residual, unchanged: a message with an `expiry` can outlive its direct offer's use only as far as the 7-day lapse; the offer never extends a message's `expiry`.

Commit carries `Design-Read` for all six domains and `Design-Departures: none`. The earlier "hold if the operator rejects G-O1" condition is cleared: the operator approved G-O1.

## Wasm/TS/UI status (2026-11-14; code built and tested, NOT committed)

### Design sources read

Read in full by this session: AGENTS.md "Design gate"; this report (all of it); [`2026-11-04-attachments-core-status.md`](2026-11-04-attachments-core-status.md); [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md) (sections 7 words, 8 accessibility, 9 definition of done in particular); the code it touches (`crates/om-wasm/src/bridge.rs`, `packages/om-core-wasm/src`, `packages/om-platform-api/src/core`, the harness, `packages/om-ui/src/conversations`, the Action Registry, `apps/web/src/copy/en.ts`). **Not read (the committing session must read them before committing):** every document and ADR that `python3 tools/design_gate.py needs` prints for the paths below (domains `client_endpoint_distribution`, `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events`, `ux_accessibility_personalization`); in particular `CONTENT_AND_MICROCOPY_VOICE`, `UI_CORE_PLATFORM_CONTRACT`, `ACCESSIBILITY_AND_INPUT` and `PRODUCT_AND_UX_PRINCIPLES` were not re-read for the new copy, which rests on the brief's section 7 and the existing copy tests.

### What was built (all uncommitted)

- **Wasm bridge** (`crates/om-wasm/src/bridge.rs`): commands `prepare_direct_attachment`, `arm_direct_offer`, `direct_serve` (answers `{frames, commit}`; the commit is the one durable effect, "delivered"), `direct_pump`, `direct_fetch_start`, `direct_fetch_handle`, `direct_fetch_progress`, `direct_fetch_open`. The two intents and two queries need no command: they pass through `dispatch` and `query`. `DirectFrame` has a JSON shape written in the bridge only (`{kind: fetch_blob|begin|chunk|end|ack|error, ...}`, bytes as hex, camelCase); `om-core` is untouched and there is still no wire codec.
- **TS binding** (`packages/om-core-wasm`): `DirectFrame`, `DirectFetchProgress` types; `attachments.prepareDirect` and `attachments.arm`; a `direct` object (`serve`, `pump`, `fetchStart`, `fetchHandle`, `fetchProgress`, `fetchOpen`) for a future host driver. No channel and no driver.
- **Type mirrors** (`packages/om-platform-api`): five new `AttachmentState` values, `AttachmentPlanView`, `DirectOfferView`, intents `SendAttachmentsDirect` and `WithdrawDirectOffer` (in `INTENT_KINDS`), queries `PlanAttachment` and `GetDirectOffers` (in `QueryResultMap`), optional `AttachmentApi.prepareDirect`. Harness (`packages/om-core-harness`): the same intents, queries and `prepareDirect`, so UI tests run against it.
- **UI** (`packages/om-ui`, copy in `apps/web/src/copy/en.ts`): Action Registry actions `send-attachments-direct` (as strict as `send-attachments`) and `withdraw-direct-offer`; `ConversationServices.directChannel?: boolean`; Composer asks Core (`PlanAttachment`) for a file over the service's ceiling and says it was not sent; file chips show the five states and a "Take it back" button while an offer waits or transfers; `AttachmentCopy.direct` strings.

### Decisions (please review)

- **U1, conservative wording when no direct channel exists (the case today).** There is no transport (D1) and no host driver, so the web app leaves `directChannel` unset and the UI never offers or promises sending straight to them. A too-big file is refused before it is read or sent, with: "This file is too big to send with your delivery service, so it was not sent. It takes files up to N. Sending a file straight to them is not available on this device yet." An outgoing offer in state `waiting_for_recipient` reads "Not sent. Sending straight to them is not available on this device yet." (never "waiting" for something that cannot happen); an incoming `remote_direct` reads "They offered this file straight from their device. This device cannot receive it yet." and has no download button. With `directChannel: true` (a future host) the same file is staged, the refusal adds "You can send it straight to them while they are online. Nothing is kept for later.", the tray labels it, and the states read "Waiting for them to be online", "Sending to them now", "They have it. Only their device holds it; nothing is kept for later.", "No longer offered", "Available while they are online".
- **U2:** the web copy for "too big" changed from "This file is too big to send right now." to the delivery-service wording above only when Core's plan is `offer_direct`; a file over Core's own 16 MiB ceiling, or a failed plan query, keeps the old words. The Chromium e2e expectation was updated accordingly.
- **U3:** a file sent straight to them goes in a message of its own (Core's `SendAttachmentsDirect` takes only direct-prepared files); mixing is refused with a plain sentence. A voice note is never direct.
- **U4:** "Take it back" is offered whether or not a channel exists (withdrawing is always safe).
- **U5:** the copy says "device", never "channel", "transport", "peer", "LAN" or "WAN"; "delivery service" is the glossed term. The sentence "Only their device holds it; nothing is kept for later" is the plain form of AT-06 / G-O4 (`delivered_locally` is that device only).
- No new crypto, no new label, no ADR needed; nothing departs from a read normative text, but see "Not read" above.

### Tests and checks

- `make core-wasm` (release wasm build and the core guard) passes; `make core-check` passes, including `cargo clippy -p om-wasm --target wasm32-unknown-unknown -D warnings`, run with `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`.
- `packages/om-core-wasm`: 3 new tests through the real wasm Core over a loopback frame channel (6 KB-class file refused for the spool, planned `offer_direct`, message at once, `remote_direct`/`waiting_for_recipient`, transfer equal, `delivered_locally`, nothing stored; a channel that drops after the handshake, restart shows `needsFile`, only the same file re-arms; withdraw serves nothing). The drop test checks the handshake and re-arm, not a byte-exact mid-file resume (the Rust suite `direct_engine` pins that). All 24 tests of the package pass (`pnpm exec vitest run --exclude 'dist/**'`; a bare `src` filter also matches a stale `dist/` copy of `ui.test.js`, which fails to resolve an import: pre-existing, ignored build output).
- `packages/om-ui` (conversations and actions: 67 pass, 11 of them new: direct offer end to end with axe, over-ceiling file, mixing refusal, seven state renderings with axe, registry strictness), `packages/om-platform-api` (9), `packages/om-core-harness` (29), `apps/web` (360 pass, 2 skipped; the copy tests, including the voice rules, pass).
- `pnpm exec tsc -b apps/web` clean. Chromium e2e `attachments` spec passes (oversize now shows the new words). `python3 tools/om_docs.py fmt` (0 unresolved links) and `make check` pass.
- Existing failures not from this change: `pnpm lint` reports one error in `packages/om-provider-http/src/provider.ts:441` and `pnpm typecheck` fails in `infra/cloudflare-home` (`cloudflare:workers` types); both reproduce without these edits.

### What remains

1. A transport and a host driver (D1, `local_delivery`): something that sets `directChannel`, maps a channel to `DirectFrame`, calls `direct.serve`/`pump` on the sender and `fetchStart`/`fetchHandle`/`fetchOpen` on the receiver, re-arms offers with `attachments.arm` at start (the UI does not keep the file, so after a restart an offer shows no "needs the file" prompt yet) and a download button for `remote_direct`.
2. `GetDirectOffers` is typed and in the harness but no screen uses it (no "files I am offering" list, no expiry text, no `needsFile` prompt).
3. The Action Registry and `ACTION_TO_CORE_INTENT_MAP` document: the two new actions are not yet listed there (docs not touched).
4. Brief update: section 7/9 of [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md) is unchanged; add the U1 wording rule there when the committing session has read the voice document.
5. Firefox/WebKit e2e not run.

### For the committing session(s)

Two path groups; the domains differ, so split into two commits if preferred:

```
python3 tools/design_gate.py needs crates/om-wasm packages/om-core-wasm packages/om-platform-api packages/om-core-harness
python3 tools/design_gate.py needs packages/om-ui apps/web
```

The first prints `client_endpoint_distribution`, `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events` (read in full by the fourth session for the Rust side except `client_endpoint_distribution`); the second additionally `ux_accessibility_personalization`. Run each and read what it prints before committing; trailers `Design-Read: <the printed domain ids>` and `Design-Departures: none` (hold if the operator rejects U1). Files: `crates/om-wasm/src/bridge.rs`; `packages/om-core-wasm/src/{attachments.test.ts,index.ts,runtime.ts,types.ts}`; `packages/om-platform-api/src/core/{attachments,client,intents,views}.ts`; `packages/om-core-harness/src/harness.ts`; `packages/om-ui/src/actions/{registry.ts,registry.test.ts}`; `packages/om-ui/src/conversations/{Attachments.tsx,Composer.tsx,copy.ts,services.ts,attachments.test.tsx,__fixtures__/copy.ts}`; `apps/web/src/copy/en.ts`; `apps/web/e2e/attachments.spec.ts`.

### Group 1 commit (2026-11-14): wasm bridge, TS binding, platform-api mirrors, harness

**Design sources read in full (Read tool, no grep-only):** AGENTS.md "Design gate"; this report; all 30 documents and all 47 ADRs that `python3 tools/design_gate.py needs crates/om-wasm packages/om-core-wasm packages/om-platform-api packages/om-core-harness` printed for `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events`. That command printed no `client_endpoint_distribution` domain (the report's earlier list expected it); nothing from it was read for this commit. **Not read:** ADRs that amend the printed ones but are not listed (for example ADR-0131, 0135-0139, 0141-0147, 0150, 0154, 0161); no listed text relies on them for blob transfer.

**Check of group 1 against the text:** no mismatch, no ADR needed, `Design-Departures: none`. The bridge, binding and mirrors add only typed commands, frames, intents, queries and states; the AttachmentRef still travels in the ordinary sealed event ([`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) 3.1), the read capability is relationship-scoped and random ([`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), AT-06, ADR-0126 sender-hiding spirit), no presence is learned by Core ([`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md)), the 7-day lapse is local retention time, never authority ([`TIME_AND_EXPIRY_SEMANTICS.md`](../architecture/events/TIME_AND_EXPIRY_SEMANTICS.md)), and `delivered_locally` is that device only (AT-06, ADR-0150).

**Files:** `crates/om-wasm/src/bridge.rs`; `packages/om-core-wasm/src/{attachments.test.ts,index.ts,runtime.ts,types.ts}`; `packages/om-platform-api/src/core/{attachments,client,intents,views}.ts`; `packages/om-core-harness/src/harness.ts`. No file-registry change was needed. Group 1 imports nothing from `packages/om-ui` or `apps/web` (only the unchanged `ui.test.tsx` of om-core-wasm does), so it is a consistent set on its own.

**Verified:** `make core-wasm` (guard OK), `make core-check`, `vitest run --exclude 'dist/**'` in om-core-wasm (24), om-platform-api (9), om-core-harness (29), `pnpm exec tsc -b apps/web` clean (run with group 2 present in the working tree; group 1 alone was not built in isolation), `om_docs.py fmt`, `make check`. Group 2 (`packages/om-ui`, `apps/web`) stays uncommitted for a later session; it carries `ux_accessibility_personalization` in addition.

### Group 2 commit (2026-11-14, b97b0fb): om-ui and apps/web

**Design sources read in full (Read tool, no grep-only):** AGENTS.md "Design gate"; [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md) (including section 9, definition of done); [`CONTENT_AND_MICROCOPY_VOICE.md`](../ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md); every document and ADR that `design_gate.py needs` printed for the ten files, for `client_endpoint_distribution` (9 documents, ADR-0007, 0038, 0044, 0045, 0059, 0060, 0077, 0103, 0129, 0130, 0159), `discovery_first_contact` (5 documents, ADR-0004, 0019, 0020, 0021, 0025, 0033, 0040, 0126), `relay_reachability` (3 documents, ADR-0012, 0022, 0075, 0126) and `ux_accessibility_personalization` (12 documents, ADR-0037, 0051-0058, 0068, 0115, 0129, 0159, 0162). **Not read:** ADRs that amend those but are not listed (for example ADR-0148, 0149, 0150); [`SCREEN_MAP_AND_JOURNEYS.md`](../ux-design/SCREEN_MAP_AND_JOURNEYS.md) and the other `docs/ux-design/specs/` files except the voice document.

**Result:** no mismatch, no ADR needed, `Design-Departures: none`. U1 wording follows the voice rules (object/state, meaning, next action; plain "delivery service"; no jargon, vendor or protocol terms), the honest-state rule (no "waiting" for something that cannot happen; "Nothing is kept for later" stated), and accessibility (states are text, in the existing `role="status"` note; the take-back control is a quiet `Button`, not colour or gesture only). All side effects go through typed registry intents (`send-attachments-direct`, `withdraw-direct-offer`); nothing in the UI writes Core state; the direct path is offered only when the host sets `directChannel`, which the web app does not. No motion, colour, theme or navigation change.

**Notes for the operator (not departures):** [`ACTION_TO_CORE_INTENT_MAP.md`](../architecture/ux/ACTION_TO_CORE_INTENT_MAP.md) is conceptual and does not list the two new actions (it lists `attach-media`, `download-attachment`); add rows when convenient. The U1 wording rule was not added to [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md) because that brief forbids new decisions without a cited source and U1 awaits operator confirmation.

**Verified:** `vitest run src` in om-ui (261) and apps/web (138), `pnpm exec tsc -b apps/web`, Chromium `attachments.spec.ts` (1 passed, after rebuilding the four e2e bundles), `om_docs.py fmt` (0 rewrites), `make check`. Not run: Firefox/WebKit projects, screenshots at 390x844 and 1280x800 (the visible change is two lines of status text and one quiet button).
