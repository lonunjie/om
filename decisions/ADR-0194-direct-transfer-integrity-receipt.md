# ADR-0194 — Direct large-file transfer: a peer's acknowledgement is a claim; delivery is confirmed by an authenticated integrity receipt

- **Status:** Accepted
- **Date:** 2026-11-17
- **Decided by:** operator delegation 2026-11-16, to fill silence and fail closed. This ADR fills silence only and amends no audited text.
- **Class:** `additive_noncritical_extension` ([`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) section 2, strongest applicable class; ADR-0165). One new frame kind is added to the direct byte channel; the frames of [`lan.cddl`](../spec/cddl/lan.cddl) (ADR-0158) keep their meaning and the frozen W0 corpus is untouched. `local_delivery` is not an authorized scope and no `LanFrameV1` codec exists yet, so nothing of this frame is released; as with ADR-0189, the CDDL is given in the appendix and the implementing session moves it into `spec/cddl/` after reading the printed domains. Fallback for a peer without receipts: the sender stays in the honest "peer claims received" state (section 3); nothing breaks.
- **Amends:** nothing. **Does not amend:** [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../docs/architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) ("Zero-payment personal bootstrap", AT-06), ADR-0150 section 1 ("Direct encrypted file transfer"), ADR-0158 (sections 6 and 8: the `ack` is "only transport progress; it is not message receipt, relationship acceptance or storage durability"), ADR-0119 (frozen W0), [`lan.cddl`](../spec/cddl/lan.cddl).

## Design sources read

Read in full: [`AGENTS.md`](../AGENTS.md) "Design gate"; [`AGENT_ARCHITECTURE_RULES.md`](../docs/process/AGENT_ARCHITECTURE_RULES.md); [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md); [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../docs/architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md); `crates/om-core/src/direct.rs`; ADR-0191 (house format); ADR-0150 decision 1, direct-transfer subsection; the first 120 lines of [`lan.cddl`](../spec/cddl/lan.cddl).

Read in part: ADR-0158 (frame list, backpressure, resume and acknowledgement paragraphs, by search and the lines around them); [`2026-11-13-direct-transfer.md`](../docs/reports/2026-11-13-direct-transfer.md) (decisions D1 to D3, G-O1, G-O3, G-O4 and the UI and wasm summaries, by search and targeted passages, not end to end); [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md) (receipt paragraph); `AttachmentRef` and the `AttachmentState` enum in `attach.rs` and `api.rs`; `apps/web/src/copy/en.ts` direct strings; [`CONTENT_AND_MICROCOPY_VOICE.md`](../docs/ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md) rules 1 to 8.

Not read (stated, not claimed): the rest of what `python3 tools/design_gate.py needs crates/om-core/src/direct.rs` prints for `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events` (about 30 documents and 49 ADRs), notably `REACHABILITY_AND_OBJECT_REPAIR`, `METADATA_THREAT_MODEL`, ADR-0119 ruling 6 and ADR-0161 (only cited by the documents above), `LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE` beyond its cited paragraphs, the V-003 and V-024 gate text; the `direct_engine` and `direct_transfer` tests.

## Context

`DirectSender` moves to `Delivered` when the peer's `Ack` names every byte after `object_end`, wipes the ciphertext, and the UI says "They have it." But the design says the acknowledgement is transport progress only; the sender has no evidence that the bytes were checked against `ciphertext_digest`, only a number the peer chose to send. A faulty or hostile peer (or a transport that acks bytes it later drops) turns into a false "delivered", after which the sender has already discarded the only copy it served. The frozen frames have no field for anything stronger.

## Decision

1. **Two sender states, not one.** `PeerClaimsReceived`: the peer's last `Ack` after `object_end` equals `total_bytes` (the existing "delivered" condition). `Confirmed`: a valid receipt (section 2) for the current transfer id has also arrived. `Delivered` as a name for the first is retired. Only `Confirmed` is shown as "They have it". Neither is storage durability (AT-06, ADR-0150): the copy is the peer device's local copy only, and the StoragePool state is unchanged by either.
2. **The receipt.** After the receiver has (a) received every byte, (b) verified the transport SHA-256 and the `ciphertext_digest`, and (c) durably committed the ciphertext to its local encrypted blob cache (the host reports the commit; a failed or unreported commit sends no receipt), it sends one `object_receipt` frame carrying `receipt_tag`:

```text
K_rcpt  = HKDF-Extract-and-Expand(salt = ciphertext_digest, IKM = content_key,
                                  info = OM-CBOR [ "OM-DIRECT-RECEIPT-KDF-v1", blob_id ], 32)
H_pos   = DS-HASH("OM-DIRECT-RECEIPT-POSSESSION-HASH-v1", OM-CBOR [ transfer_id, ciphertext ])
tag     = HMAC-SHA-256(K_rcpt, OM-CBOR [ "OM-DIRECT-RECEIPT-MAC-v1", transfer_id, blob_id,
                                         total_bytes, ciphertext_digest, H_pos ])
```

   `content_key` is the key in the `AttachmentRef` (the same secret that opens the blob; the peer has it only from the sealed message); `transfer_id` is the sender-chosen fresh 16 bytes of this connection's `object_begin`; `ciphertext` is the whole object. The sender knows every input and compares in constant time. The tag proves, for this transfer id, that the confirmer holds the message's content key and every byte of the object, and the bytes are those the digest commits to. It cannot be replayed from another transfer id, cannot be built from the digest alone and cannot be forged by the channel or a relay.
3. **Sender handling.** After `PeerClaimsReceived` the sender keeps the ciphertext (it no longer wipes at the claim) and waits for the receipt for the life of that channel. A valid receipt gives `Confirmed` and the ciphertext is dropped. An invalid tag is ignored (counted, bounded), the transfer is not failed, the state stays `PeerClaimsReceived`, and the person-visible text says it could not be confirmed. If the channel closes first, the state stays `PeerClaimsReceived` and persisted; a later `fetch_blob` re-runs the exchange (section 5). The offer's existing expiry (7 days) ends an unconfirmed claim as "Not confirmed" (no state upgrade, no durability claim). A peer that never sends a receipt (older build) stays `PeerClaimsReceived` and the UI says so.
4. **Receiver handling.** A receiver sends `Ack` as today (transport progress). It sends the receipt only after the durable commit, and again for any later transfer id on which it holds the verified object. It treats an `error` answering its receipt as "the sender does not support receipts", keeps its verified copy and stops sending receipts; it never discards a verified object because of that.
5. **Resume stays legitimate.** The first `Ack` after `object_begin` may name any contiguous offset from 0 to `total_bytes`, including a nonzero start and `total_bytes` itself (the receiver already holds the verified object, for example after a lost receipt). The sender continues from that offset (no chunks when it equals the total, then `object_end`). The initial offset never moves the sender to `PeerClaimsReceived`; only the `Ack` after `object_end` does. The receipt always covers the whole object, so a resume with a wrong prefix cannot produce a valid tag, and a resumed transfer is confirmed exactly like a fresh one. A receiver already `Complete` MUST answer a new `begin` with an `Ack` at `total_bytes`, recompute `H_pos` over its stored ciphertext for the new transfer id on `object_end`, and send a fresh receipt (this is how a lost receipt recovers). An initial offset above `total_bytes` or below the previously acknowledged offset in the same transfer stays a `protocol` error as today.
6. **Receiver states.** `Complete` (verified in memory, as now) gains the successor `Committed` (the host confirmed durable commit); `Durability::LocalReceiptOnly` stays the receiver's honest label for both. The sender's `Durability` becomes `PeerClaim` or `PeerLocalCopyConfirmed`, both "that device only".
7. **Core API states.** `AttachmentState` gains `PeerSaysReceived` (sender; the claim) next to `DeliveredLocally` (now meaning `Confirmed` only). Persisted `DirectStatus::Delivered` records written before this change are migrated to the claim state on load and are never promoted to confirmed.

## Where did authority go?

Nowhere new. The receipt is a MAC under a key derived from a secret the sender and the message's recipients already share; it grants no capability, changes no event, relationship, route or ACS and is not an event or message receipt (`receipt.update` is untouched). It answers one local question for the sender: did a holder of the message's key actually obtain and verify these bytes. The read capability and the relationship scope of the fetch are unchanged. A recipient's other device with the message can confirm as well; the claim is "a device of the recipient", exactly as a delivered message is.

## Consequences

The sender no longer reports delivery on a bare number, and it keeps the object until delivery is confirmed or the offer ends. One more small frame per transfer. Residual: confirmation is by a device that holds the key (a compromised recipient device can confirm and then leak the file, as for any delivery); a lost receipt leaves "peer claims received" until the next connection; the receipt is not a signature, so it proves nothing to a third party.

## Appendix: frame (to move into `spec/cddl/lan.cddl` by the implementing session)

```cddl
lan-frame-v1 //= lan-object-receipt        ; adds kind 8 to the union
lan-object-receipt = {
  0: 1,                                    ; version
  1: 8,                                    ; kind = object_receipt
  2: bstr .size 16,                        ; transfer_id of the transfer being confirmed
  15: bstr .size 32,                       ; receipt_tag: HMAC-SHA-256 per ADR-0194 section 2
}
```

Registry additions (kind 8 `object_receipt` in `lan_frame_kind`, key 15 `receipt_tag` of `LanFrameV1`; labels `OM-DIRECT-RECEIPT-KDF-v1` (`kdf`), `OM-DIRECT-RECEIPT-MAC-v1` (`mac`), `OM-DIRECT-RECEIPT-POSSESSION-HASH-v1` (`hash`) in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json)) go in with the vectors. If the operator reads the frame set as closed even to an additive kind before `local_delivery` is authorized, the same tag can ride as a sealed same-conversation control payload instead; that needs a registry decision and is not chosen here.

## Work list for a builder

1. **Code** (`direct.rs`): `DirectFrame::Receipt { transfer_id, tag }`; `OfferState::{PeerClaimsReceived, Confirmed}` replacing `Delivered`; the sender keeps `ciphertext` after the claim and drops it at `Confirmed` or `Withdrawn`; `receipt_tag` helper shared by both sides; `DirectFetch::committed()` (host call after durable write) and `take_receipt(transfer_id)`; the `Complete` receiver re-handling a new `begin`, an `object_end` and a repeated fetch as in section 5; unknown or invalid receipt handling as in section 3; `Durability` variants of section 6.
2. **Engine** (`flows/blobs.rs`, `engine.rs`): `DirectStatus::{PeerClaim, Confirmed}`; migration of stored `Delivered` to `PeerClaim`; `AttachmentState::PeerSaysReceived`; persisted offers keep the claim across restart (and the ciphertext is re-armed from the original file as today, G-O3); the host commit call for the receiver; no change to expiry or withdraw rules.
3. **Bridge and TS** (`crates/om-wasm/src/bridge.rs`, `packages/om-core-wasm`, `om-platform-api`, harness): JSON shape `{kind: "receipt", transferId, tag}` (hex); `direct.committed`; the new state value in `AttachmentState`. Codec work for `LanFrameV1` stays with the `local_delivery` session.
4. **Registry and CDDL:** as in the appendix, with the six printed domains read and trailers carried. Class `additive_noncritical_extension`: ADR, registry, schema, fallback (stays "peer claims received"), vectors, trace and threat updates.
5. **Vectors** (`verification/vectors/direct-transfer-receipt-vectors.json.gz`, independent generator in `verification/prototypes/direct-receipt/`): fixed `content_key`, `blob_id`, `ciphertext` (a few sizes including one over 16 KiB), `transfer_id`; expected `ciphertext_digest`, `K_rcpt`, `H_pos`, `tag`. Negatives: another transfer id (replay); one flipped ciphertext bit; truncated ciphertext; wrong digest; wrong `total_bytes`; wrong key; tag from a different blob id; receipt with the right tag for a resumed transfer (same tag as a fresh one for the same id).
6. **Tests:** claim without receipt stays `PeerClaimsReceived` and keeps the ciphertext; valid receipt gives `Confirmed` and drops it; forged tag, replayed tag and a tag for another transfer id are ignored and do not fail the transfer; resume with a nonzero initial offset confirms; initial offset equal to the total (lost receipt) is re-confirmed on reconnect; receiver without a host commit sends no receipt; an ack of `total_bytes` before `object_end` changes nothing; sender restart keeps the claim and never turns it into `Confirmed`; a peer that answers a receipt with `error` keeps its verified copy; migration of a stored `Delivered`; AT-06 case (direct receipt then provider backfill) shows the confirmation as local-only throughout.
7. **UI copy keys** (`apps/web/src/copy/en.ts`, `direct`): `peerSaysReceived` ("They say they have it. Waiting for their device to confirm."), `delivered` changed to ("Their device checked the file and has it. Only their device holds it; nothing is kept for later."), `notConfirmed` ("They said they received it, but OM could not check it. Send it again if they need it."), `sendAgain` ("Send again"). The copy tests and voice rules apply; no jargon (no digest, MAC or receipt).

## UX line

A file shows "They have it" only once their device has checked it; until then it says they say they have it.

## Traces

Requirement R-433, threat T-177, decision D-210. Related: ADR-0150, ADR-0158, STORAGE_AND_BLOB_ARCHITECTURE AT-06, [`2026-11-13-direct-transfer.md`](../docs/reports/2026-11-13-direct-transfer.md).

## 2026-11-18 implementation clarification: bounded receipt recovery

The invalid-tag window is eight comparisons per transfer. The sender computes and caches the expected tag once for
that transfer; eight invalid tags close its comparison window. An authenticated fetch with a fresh sender-generated
transfer ID opens a new comparison window, so a later genuine receipt can confirm the retained object. Fetch alone
performs no receipt hash and cannot confirm anything.

Across the entire offer the sender computes at most eight whole-object receipt hashes, including every recovery
transfer. This counter never resets on fetch. Once exhausted, a transfer with no cached expected tag remains unconfirmed
until the offer ends or the person explicitly creates another offer. A repeated ID does not reopen its comparison
window; a superseded claim cannot confirm a newer transfer. These are local work limits, with unchanged frame, MAC,
expiry and durability meanings. The cached tag is erased on replacement, confirmation, withdrawal and drop.

Authority stays with the existing content-key holder. No transport acknowledgement gains authority. Regression coverage
is in `crates/om-core/tests/direct_receipt.rs` and `crates/om-core/src/direct.rs` (offer-wide work bound).
