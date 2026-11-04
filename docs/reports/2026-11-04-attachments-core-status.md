# Attachments, Core side: encrypted send and receive, voice notes, location cards (WP3b)

- **Date:** 2026-11-04. **Scope:** `core_kernel` (`crates/om-core`, `crates/om-wasm`). **Status:** built and tested against a loopback BlobStore; **not wired** to the real Home adapter or the UI (those are other agents' uncommitted paths).
- **Commits:** `d778518` (codec, crypto, extensions), `6b0c2bf` (send, receive, refusals, views, loopback, 21 scenario tests), `99a2d11` (wasm bridge), and this report. `make core-check` equivalents pass natively (fmt, clippy `-D warnings`, all tests); the wasm32 clippy leg could not run here (see "Not verified").
- **Result in one line:** Core seals an attachment on the device under a random per-blob key, persists its identities before any upload, sends the `message.create` only after every blob is stored, refuses over-limit objects with a typed reason before anything leaves, treats a put timeout as `PendingReconciliation`, and exposes plaintext only for bytes that hash to the digest the message committed to.

## Design sources read

Read in full: `design_gate.py needs crates/om-core/src crates/om-codec/src` listed `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events`; their documents were read in full and their ADRs by Decision sections. Also read: `STORAGE_AND_BLOB_ARCHITECTURE`, [`spec/cddl/events.cddl`](../../spec/cddl/events.cddl) (`attachment-ref`, `MessageCreate`), [`spec/protocol/home-api.md`](../../spec/protocol/home-api.md) (blob rows, `blob_cap`), the blob registries (numeric fields, domain separation, home errors), `MESSENGER_BASELINE_V1`, `IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT` to section 8, the v1 completion plan section 3, and the Home agent's report [`2026-11-04-attachments-home-status.md`](2026-11-04-attachments-home-status.md). ADR-0150 (attachments, direct transfer), ADR-0148 and ADR-0149 (V1 promotion, hardening), ADR-0155 (non-critical extension validity), ADR-0118 (item 6), and the existing outbox patterns in `backup.rs`, `relayset.rs`, `flows/ceremony.rs` (read for the patterns only, not end to end).

**Not read:** `SEALED_SENDER_DELIVERY_PROFILE` (the deposit path is unchanged); `IMPLEMENTATION_READINESS` after section 8; the consequence and verification sections of the ADRs; ADR-0118 beyond item 6; `backup.rs`, `relayset.rs` and `flows/ceremony.rs` beyond the NetCall/outbox/classification patterns.

| Source | Effect on the build |
|---|---|
| STORAGE_AND_BLOB, ADR-0150 | BlobID random and E2EE-only; the store sees ciphertext, its length and timing, never a name, type or key; "stored for later" is separate from "delivered"; the UI must not say size is hidden (`AttachmentView.size` is documented as visible to the store). |
| events.cddl, ADR-0155 | `attachment-ref` keys 0 to 10; no inline bytes exist in the schema, so there is no inline fallback to remove. Key 23 non-critical schema failures are ignored; an unknown critical extension or a wrong criticality rejects. |
| ADR-0148/0149/0155 | Voice note: audio plus `duration_ms`, waveform a 16..256 byte untrusted hint. Location: signed E7, exactly seven fraction digits, one distinct canonical point in the fallback, suppressed once the current text fails the check. |
| IMPLEMENTATION_READINESS (transaction contract) | Commit before release; identities persisted before the first effect; a timeout is never a rejection and never becomes a new user action (`PendingReconciliation`, exact retry). |
| home-api, Home report | Owner capability chosen by the sender (delete), read capability derived by the Home goes in `repair_capability`, locator in `locations`; `payload_too_large` and `quota` are different errors. |
| METADATA_THREAT_MODEL 7/14 | No padding in this profile, and Core says so; fetch is never automatic (it would tell the sender's Home host when and from where the recipient looked). |
| OPERATORLESS, completion plan 3 | The BlobStore is whatever the person's own Home advertises; Core depends on no SeedLife service and has no default store. |

## What is built

| Piece | Where |
|---|---|
| Content sealing (AES-256-GCM-SIV; key and nonce from HKDF over the random blob key, salt = BlobID; AD = label, BlobID, plaintext size), `DS-HASH` ciphertext digest, strict `attachment-ref` read/write, field cleaning (file name, https-only locations, media type), typed `AttachError` | `crates/om-core/src/attach.rs` |
| `om.voice-note.v1` / `om.location-card.v1` schemas, canonical `geo:` token, boundary and ambiguity rules | `crates/om-core/src/present.rs` |
| Text-key CBOR maps (key 23 is the only text-keyed map; ordered by encoded length then bytes, strictly ascending on decode) | `crypto/cbor.rs` (`V::TM`) |
| Event with attachments and key 23; `parse` now accepts key 23 (unknown critical still rejects) | `events.rs` |
| Send: `prepare_attachment`, `SendAttachments`, `SendLocation`, persisted `PutBlob` outbox entries, event built when all receipts are in, `RetryDelivery` re-puts with the same ids, abort and best-effort `DeleteBlob` | `flows/blobs.rs`, `flows.rs` (`compose`) |
| Receive: refs stored with the message (also from a sibling device), `attachment_fetch` plan, `open_attachment` (length, digest, then decrypt), `restage_attachment` | `flows/blobs.rs`, `engine.rs` |
| Views: `AttachmentView` (`uploading`, `stored_for_later` with `stored_until`, `remote`), `PresentationView` re-derived on every read from the authenticated extension bytes and the message as it currently reads | `api.rs`, `engine.rs` |
| Host surface: `NetCall::{PutBlob, GetBlob, DeleteBlob}`, `BlobReceipt`, `BlobLimits`, `Core::set_blob_limits` | `host.rs` |
| Loopback BlobStore (limits, idempotent put, quota, corruption and expiry switches) and `Runtime::{prepare_attachment, fetch_attachment}` | `runtime.rs` |
| Scenario tests (21) | `crates/om-core/tests/attachments.rs` |
| Bridge commands `set_blob_limits`, `prepare_attachment`, `restage_attachment`, `attachment_fetch`, `open_attachment` | `crates/om-wasm/src/bridge.rs` |

### Behaviour that the tests pin

- **Key and content confinement.** The BlobStore holds only ciphertext (checked for the plaintext, the key and the file name); no relay deposit contains the key, the plaintext or the name. The key exists only in the persisted profile record and inside the E2EE event.
- **Digest before decrypt.** A flipped bit anywhere, a truncated or extended object, or a wrong key never yields plaintext (`IntegrityFailure`); an expired object is `NotFound`.
- **Typed refusal before sending.** `prepare_attachment` returns `TooLarge{max}` (largest allowed plaintext), `StorageFull` or `NoBlobStore` before sealing; `SendAttachments` repeats the check (limits may have shrunk) and answers `Rejected{too_large}` with nothing queued and no timeline item. A store-side `payload_too_large` is `Rejected{too_large}` and the message never leaves. The reason is not retryable, so the UI cannot offer the same retry. Plain wording is the UI's ("too big to send right now"); Core supplies the code.
- **Stable identity.** BlobID, owner capability, request id and content key are persisted by `prepare_attachment` (before any effect) and reused by every retry, including after a restart. Timeout after effect: `PendingReconciliation`, the same `PutBlob` is still owed byte for byte, and one object (one put) results.
- **Ordering.** The message event is built only after every blob has a receipt, because it carries the receipt's locations and read capability; a refused upload sends nothing.
- **Hostile senders.** Unknown critical extension, a critical registered extension, an empty message and a repeated BlobID reject the event; malformed presentation (3-sample waveform, voice note on an image or on a missing index, token mismatch, two distinct points) is ignored and the message still arrives.

## Decisions where the design is silent (please review)

| ID | Decision | Basis | If the operator disagrees |
|---|---|---|---|
| A | **Content AEAD construction.** `OM-BLOB-CONTENT-AEAD-AD-v1` is only reserved. Core uses it as HKDF info and as the AD prefix: key and 12-byte nonce are HKDF-SHA256(salt = BlobID, ikm = content key), AD = label \|\| BlobID \|\| plaintext size (u64 BE). | A random key per object makes a derived nonce safe; binding BlobID and size stops a blob being replayed under another reference. | Change `attach::content_keys`; the registry entry then moves from reserved to frozen with this construction (a V-001 vector is still owed). |
| B | **Read capability in `repair_capability`** (key 10) and the Home URL in `locations`; owner capability stays local. | Home report decision; it is the only capability slot of the frozen ref. | Needs a registry change (a dedicated key). |
| C | **Adapter to Core receipt.** `PutBlob` answers `Bytes` = CBOR `{0 read cap, 1 [locations], 2 digest the store computed, ? 3 expires_at}` (`host::BlobReceipt`). Core compares the digest with its own; a mismatch or malformed receipt is `Rejected{integrity_failure}` and what reached the store is deleted. | Core must not trust the store; the host contract needed one shape. | Any other encoding is a one-file change. |
| D | **Locations are `https` only**, no credentials; others are dropped on receipt, and a message whose every location is unusable simply cannot be fetched. | A peer-chosen URL is an SSRF and IP-disclosure vector. | Loosen `attach::valid_location` (a LAN profile will need it). |
| E | **Fetch is never automatic.** Core returns a plan; the host decides (tap, or its own policy). | Fetching tells the sender's Home host when and from where; metadata chapter. | None needed; the host owns policy. |
| F | **No advertised store, no attachments.** Until the host calls `set_blob_limits`, Core answers `unsupported`; limits come from the advertised capability, with Core's own bound of 16 MiB. | Fail closed; Core has no default store (operatorless). | Provide defaults at the adapter. |
| G | **`quota` is a definite refusal, retried only by the person's explicit `RetryDelivery`**, never automatically, although Core's `RejectReason::Quota.retryable()` is true and the Home registry says non-retryable. | The object was not stored (definite), the spool frees as blobs expire. | Make `Quota` non-retryable in `api.rs` and drop the retry. |
| H | **A retry makes the retry operation the message's current operation** (the earlier one stays rejected). | The final deposit completes the message's operation. | Track a list of operations per message. |
| I | **One voice note per message** (the extension carries a single index). Two `voice_note` inputs are `invalid`. | The schema has one `attachment_index`. | n/a |
| J | **Geo token boundaries.** A candidate is `geo:` preceded by a non-alphanumeric/non-underscore character; it ends at a non-alphanumeric, not `_ ; ? -`, and not `.` or `,` followed by a digit. The coordinate must re-render identically (no `+`, no leading zero, no `-0.0000000`). Anything else is ordinary text and does not count. | ADR-0149 names the rules without the character classes. Conservative: a doubtful token suppresses the card, never shows a wrong point. | Adjust `present::token_at`. |
| K | **Prepared but unsent attachments are forgotten after one day** and at most 64 are held. | Nothing external has happened to them; bounds state. | Constants in `flows/blobs.rs`. |

`Design-Departures: none` on all commits: C, D, F and the use of the reserved label (A) are implementation choices inside the permissions the design gives; A should still be recorded when the V-001 vector is added.

## What the host, adapter and UI must add

Wasm bridge (`packages/om-core-wasm`, someone else's uncommitted edits; not touched). The Rust bridge has the commands; the TS side needs the same five:

1. `set_blob_limits {limits: {maxBlobBytes, maxSpoolBytes?, maxBlobs?} | null}` from the advertised `ProviderCapabilities.limits` (`max_blob_bytes`, `max_blob_spool_bytes`, `max_blobs`) when `om.blob.spool.v1` is present; call it at start and whenever capabilities change.
2. `prepare_attachment {ctx, plaintext (hex), meta: {mediaType, filename?, width?, height?, durationMs?}}` answering `{prepared: {blobId, ciphertext, ciphertextDigest, ciphertextLen}, commit}` or `{refused: {error: "too_large", max} | "storage_full" | "no_blob_store" | "empty" | "invalid"}` (plus `commit` when present: finish it either way). Stage `ciphertext` under `blobId` only after the commit is durable.
3. `restage_attachment {ctx, blobId, plaintext}` to rebuild the staged ciphertext after a restart.
4. `attachment_fetch {ctx, messageId, index}` answering `{call: get_blob{locations, readCap, digest, length}}`.
5. `open_attachment {ctx, messageId, index, ciphertext}` answering `{opened: {plaintext, mediaType, ...}}` or `{refused: {error: "integrity_failure" ...}}`.

Types: intents `SendAttachments {conversationId, draft, attachments: [{blobId, voiceNote?: {waveform? hex}}]}` and `SendLocation {conversationId, latE7, lonE7, accuracyM?, label?, note?}` (the old `ShareStaticLocation` is still `unsupported`; `SendLocation` replaces it, so the platform API and Action Registry need the new kind); `RejectReason` gains `too_large`; `TimelineItem` gains `attachments` and `presentation`; `NetCall` gains `put_blob`, `get_blob`, `delete_blob`.

Adapter (`packages/om-provider-http`, `homes.ts`):

- `put_blob`: use the NetCall's `ownerCap` and `requestId` as given (Core chose them; do not generate new ones), read the ciphertext from the host staging by `blobId`, send `om-ciphertext-digest = digest`, compare the Home's response digest with `digest`, and answer the `BlobReceipt` CBOR with the Home's own public URL for the object in `locations`. Map `payload_too_large` to `Rejected{too_large}` for blob puts (today it becomes `invalid`) and `quota` to `Rejected{quota}`; map transport timeouts to `Timeout` (never `Rejected`).
- `get_blob`: try `locations` in order with `readCap` as bearer, hash the bytes against `digest` and `length` before returning `Bytes` (Core checks again); `not_found` and `expired` to `NotFound`.
- `delete_blob`: `DELETE` at `location` with `ownerCap`; absent is success.

UI copy and behaviour (ADR-0150, completion plan 3): say "too big to send right now" for `too_large` and offer sending when the other person is online; show "delivered" and "stored for later until {stored_until}" separately; never imply the size or the existence of a file is hidden from the store; do not auto-download on a metered or hidden-IP profile; a voice note renders from `presentation.voiceNote` (waveform may be regenerated), a location card from `presentation.locationCard` only, with the plain text always shown.

## Not verified, not built

- The real Home and adapter path was not exercised; only the loopback store, whose put/get/delete semantics follow the Home report. The 4 MiB body through a Durable Object is unmeasured (Home report).
- `cargo clippy -p om-wasm --target wasm32-unknown-unknown` could not run: this shell's `rustc` is the Homebrew 1.99.1 whose sysroot has no wasm32 target, not the pinned 1.98.1. Native clippy for the whole workspace and the `om-wasm` tests pass; no code in this change is target-specific. Run `make core-check` from an environment with the pinned toolchain.
- Not built: direct online and LAN transfer of the same BlobID and digest (completion plan 3); padding; a way to re-upload an attachment whose spool copy expired after a successful send (the recipient then sees `NotFound` on fetch); deleting a blob when the person deletes the message; attachments in backup export; groups (one peer, one read capability, shared by the peer's devices and our own); a V-001 test vector for the AEAD construction (A); property tests for the geo scanner beyond the hand-written table.
- `DeleteBlob` runs only when a message is refused for a reason that cannot be retried (including an integrity failure); other blobs expire by themselves (7 days on the reference Home).
