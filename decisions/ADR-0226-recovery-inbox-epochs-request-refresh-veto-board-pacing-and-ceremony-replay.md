# ADR-0226 — Recovery transport: epoch-scoped inbox, request refresh, the veto board, board pacing and ceremony replay

- **Status:** Accepted (by operator delegation)
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (constitution-driven rulings: fail closed, no operator, metadata minimisation, a Home stays a dumb store, additive only; fill silences only, never amend audited text, never weaken a floor).
- **Class:** `additive_noncritical_extension` (ADR-0165). Home API v1 section 1 says additive behavior is advertised through `ProviderCapabilities.features`; every Home behavior below belongs to the existing feature `om.recovery.store.v1`, changes no existing operation, body or error, and a Home that does not implement it degrades to the earlier behavior (a client treats the missing behavior as "not available here" and keeps its earlier fallback). One new domain-separation label is registered by the implementing change.
- **Amends:** nothing. **Does not amend:** ADR-0201 sections 1 to 6 (clarified and extended as stated), ADR-0202 sections 2 and 3, ADR-0203, ADR-0216, [`home-api.md`](../spec/protocol/home-api.md) sections 1 to 7, [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) 10.3.
- **Scope:** `core_kernel` (ADR-0169) and `provider_reference` (ADR-0171) as already authorized for recovery.

## Design sources read, and not read

Read in full: ADR-0199, ADR-0201, ADR-0202, ADR-0203, ADR-0205, ADR-0216, [`home-api.md`](../spec/protocol/home-api.md) sections 3, 5 and 8 (the retry and idempotency rules), and the constitution documents on authority and the operatorless core. Consulted by search: [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) 10.3, ADR-0131, ADR-0147, ADR-0154, ADR-0195. Not read in full: the Home implementation (`infra/cloudflare-home`, `infra/sqlite-home`) and the provider conformance suite; limits below are starting values to be checked against `usage.ts` before deployment, as ADR-0201 section 4 says of its own.

## Decision

### 1. The recovery inbox belongs to a recovery epoch

1. **Identifier.** `inbox_id = DS-HASH(OM-RECOVERY-INBOX-ID-v2, CBOR [account_id, recovery_epoch])`, where `recovery_epoch` is the `RecoveryPolicy` epoch of the head the writer or requester knows (ADR-0205 section 2: replacement and recovery each advance it). The Home sees an opaque 32-byte value exactly as before and needs no change; only the label and input are new. The v1 derivation (`account_id` alone) is not used by a build that implements this ADR.
2. **A new epoch opens a new inbox with a fresh random capability.** Nothing is rotated: the replacement of ADR-0205 and a completed recovery both create the epoch's inbox and capability in the kit and register it. A controller that is excluded holds only an earlier epoch's capability, so it cannot read the current epoch's inbox, and a recovered device that never knew the earlier capability is not blocked by it.
3. **Earlier epochs stay drainable, never refreshed.** A controller keeps the previous epoch's capability only to drain that inbox for alerts (a request posted under an old epoch is stale against the current policy and is refused by every verifier, but the person is told someone tried). It stops refreshing the earlier registration; it lapses by the Home's retention. The capability is forgotten once that inbox is gone or 400 days after the epoch ended.
4. **Renewal.** A controller re-registers its current epoch's inbox, with the same capability (idempotent), at least every 30 days and whenever a drain says the inbox is not found. The last successful registration is a durable local fact; the `registered` flag is derived from it and from a drain that succeeded recently, never persisted as a bare boolean.
5. **Registration refused.** After a `not_found` drain, a registration that is refused for any reason other than a transient one (`rate_limited`, `quota`, `retryable`) is a terminal state for that epoch: the status shows "alerts need attention", with the single action "Make a new safety code" (which, by item 2, opens a new epoch). It is retried daily meanwhile and never retried in a tight loop. Controller alerts are never silently dropped to a `registered` that is no longer true.

### 2. Refreshing the delivery of an open request

The signed request, its digest, its verifier clocks and its veto history never change (ADR-0202 section 2).

1. **Carrier.** A refresh is a fan-out whose body bytes are **identical** to the earlier ones and whose `request_id` is **new** (an exact retry of a lost response keeps its `request_id`; a refresh does not). Home API v1 section 5 already makes an unchanged `request_id` with unchanged bytes return the determined outcome.
2. **Home behavior.** A fan-out whose body hash equals an item already held for that inbox changes nothing except that it renews that item's retention to its full period; it does not add an item, move it, or count as a distinct request toward the rolling per-day limit. A renewal is honored at most once per 24 hours per item; an earlier one is coalesced and answered as a success. If the item has been acknowledged and removed, an identical body is accepted as a new item at most once per 24 hours per digest and counts as the same request toward the distinct-request limit. All answers stay identical whether or not an inbox exists (ADR-0201 section 3).
3. **Core schedule.** While an attempt is open and the app runs, Core refreshes at every one third and two thirds of the credited time (as now) and at least every 4 days, and on the next foreground after an offline period longer than 4 days. The schedule uses a local timer as a hint; it never shortens or extends the verifier's interval and a Home's retention never counts as evidence of the absence of a veto.
4. **Old Homes.** A Home that treats the refresh as a plain duplicate is unaffected; the request then lives its original retention and the status offers the manual "send again" with the same bytes.

### 3. The veto board keeps a path that untrusted junk cannot fill

1. **Two classes, one board.** A veto post may carry the inbox capability as its bearer credential (the same capability a controller already holds in its kit and a Home already stores for the inbox of the request's account). A post with a bearer that matches the inbox of the account that owns the request is stored in the **controller class**, which has its own reserved capacity of 8 per digest; any other post, including one with a wrong or missing bearer, is stored in the **public class** under the existing shape-only admission and the existing capacity of 16. The Home answers a veto post identically in every case (success), so a poster cannot learn which class a post fell into or whether an inbox exists. Reads return both classes, controller class first. Nothing else about admission changes; the Home still verifies no signature and keeps no timestamp.
2. **Residual, stated.** A holder of an earlier kit can fill the controller class of one digest. Safety never depends on a board: the contact notice and the in-session veto of ADR-0190 do not use the Home, and a controller that cannot be reached keeps its pinned head.
3. **Incomplete is not clean.** For the owner-side attempt, a board read is **complete** only when the Home answered and the items returned are below the board's advertised capacity for that class. A board at capacity counts as incomplete evidence, however many items verify. Strict-timer adoption requires, at every known Home, a complete answer (the existing reconciliation rule that an unanswered Home blocks, ADR-0202 section 1) **or** the approval exchange of ADR-0203 section 3 with a surviving controller. A saturated board therefore keeps the device waiting and says so plainly ("we can't confirm that none of your other devices objected; wait, or approve on another device of yours"); it never reads as "no veto". Valid vetoes that were verified before saturation stay terminal.
4. **Capacity is advertised.** The Home's `limits` for the feature list the per-class capacity so a client can tell a full board from a quiet one without guessing; a Home that does not advertise it is treated as always incomplete once it returns 8 or more items.

### 4. Board pacing does not depend on whether a digest exists

1. **Coarse buckets.** Reads of a veto board (`GET .../vetoes`) are paced by one of 256 fixed token buckets chosen by a keyed hash of the digest, under a per-Home secret created with the store and never exposed. Present and absent digests consume the same bucket by the same rule (burst 10, refill 1 per 30 seconds, the get pacing of ADR-0201 section 4) and a denial is `rate_limited` in both cases. State is a constant 256 counters regardless of the number of digests.
2. **Per-digest limits** on stored items (16 public, 8 controller, 8 vetoes per rolling hour) are unchanged and are never visible as `rate_limited`: a full board answers success and drops the post, as ADR-0201 section 3 says.
3. **Cost, stated.** Two unrelated recoveries can share a bucket and delay each other by seconds. This costs availability of a convenience read only.

### 5. Ceremony evidence is replayed by the sender

ADR-0203 section 3 admits chunks only for a pending authenticated manifest and allows an early chunk to be dropped. That rule is unchanged, and nothing is buffered before the manifest. Progress after a lost or reordered piece comes from the **sender** repeating the same objects:

1. The controller sends the manifest and then every chunk, and **repeats the whole sequence** (manifest first, then all chunks, bit-identical, same chunk identities) at one fifth, two fifths and three fifths of the remaining life of the exchange and once more at four fifths, until its own cleanup. The receiver treats an identical repeat of the manifest, or of a held chunk, as a no-op (ADR-0203 section 3) and a conflicting repeat as an abort.
2. The receiver announces nothing new: no acknowledgement, no request for a missing index. Completion and failure remain visible locally ("transfer incomplete, ask again" on expiry, with a fresh ceremony).
3. The enrollment continuation that needs its manifest before chain chunks follows the same rule.

## Where did authority go?

Nowhere new. The Home verifies no signature, learns no account state and never judges a request or a veto. A bearer capability proves only that its holder may pull or post to a mailbox of objects that every consumer verifies itself. A saturated board only delays an adoption; it cannot approve one.

## Evidence required

Home: refresh renews retention once a day and adds no item; a refreshed item reappears after acknowledgement once a day; both answered identically with and without an inbox; controller-class post lands in reserved capacity after 16 public posts; wrong bearer lands in public; read denial identical for present and absent digests at the bucket boundary; 256-bucket state independent of digest count. Core: a new epoch opens a new inbox, an excluded controller cannot read it, a recovered device registers when an earlier epoch's inbox exists, renewal and the terminal refused-registration state across restart, a saturated board blocks the strict timer and a complete board does not, repeated manifest and chunks delivered last-first complete a transfer. Vectors for the new label in both codecs.

## Review requirement

The veto board's two classes and the epoch-scoped inbox identifier are Home API additions; they need the provider conformance cases and an independent review of the board's non-enumeration claim before any production claim.

## Traces

Related: ADR-0190, ADR-0192, ADR-0199, ADR-0201, ADR-0202, ADR-0203, ADR-0205, ADR-0216, ADR-0225.
