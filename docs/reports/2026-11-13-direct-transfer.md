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
