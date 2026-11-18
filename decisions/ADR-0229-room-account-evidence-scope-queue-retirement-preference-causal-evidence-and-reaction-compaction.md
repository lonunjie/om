# ADR-0229 — Room account evidence scope, retirement of dedicated queues, causal evidence for synchronized preferences and reaction compaction at the bound

- **Status:** Accepted (by operator delegation)
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (constitution-driven rulings: fail closed, scoped and non-composable authority, metadata minimisation, no operator; fill silences only, never amend audited text, never weaken a floor).
- **Class:** `additive_noncritical_extension` (ADR-0165) for the Home retirement operation (advertised as a feature, absent on a Home that does not implement it); client and Core behavior for the rest. No frozen W0 object, label or registry value changes meaning, and no group frame field changes.
- **Answers:** Q-ROOM-ACS-1 (Room-only refresh), Q-ROOM-ACS-2 (evidence that does not fit or crosses recovery).
- **Amends:** nothing. **Does not amend:** ADR-0191, ADR-0212 (its rules stand; section 1 below is the reading of "never replace existing pins" that this ADR fixes), ADR-0214, ADR-0215, ADR-0198 (rules 5 and 7 stand; section 4 states how the bound is met), [`GROUP_CHAT_ARCHITECTURE.md`](../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md), [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md), [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md).
- **Scope:** `groups` (ADR-0177), `provider_reference` (ADR-0171) and `core_kernel` (ADR-0169).

## Design sources read, and not read

Read in full: ADR-0191, ADR-0198, ADR-0212, ADR-0214, ADR-0215, [`home-api.md`](../spec/protocol/home-api.md) sections 3 and 5, and the constitution documents on authority, metadata and the operatorless core. Consulted by search: ADR-0121, ADR-0189, ADR-0207, [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md), [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md). Not read in full: the Home implementations' storage code. If an unread text fixes a point differently, that text wins and this ADR is corrected by a follow-up.

## Decision

### 1. A Room's account pin belongs to that Room and may advance along verified descendants (Q-ROOM-ACS-1)

1. **Scope.** The account pin a member holds because a Room proved it (ADR-0191 item 3, ADR-0212 item 3) is stored per **(Room, AccountID)** and is consulted only for that Room. A pin from one Room never answers for another Room. A pin from a direct relationship keeps precedence in every Room, as before, and a Room pin never creates or changes a contact binding. A carried proof for an account in a Room where this member holds no pin establishes that Room's initial pin only, on the authenticated inviter's or committer's word as the existing rule says.
2. **Lookup.** For an account named in a Room frame the receiver uses, in order: the direct relationship pin, then this Room's pin, else the account is pending.
3. **Advance along a verified descendant.** A member whose pin for the account is **Room-only** accepts a later carried proof for the same account **in the same Room** and advances that Room's pin when, and only when, the carried chain contains a signed state equal to the pinned generation and digest, every later transition verifies under the ordinary account verifier, and the new tip is verified as the current rule requires. This is a verified extension of the exact pin, not a replacement of it. The advance is monotonic: a different state at the pinned generation, a chain that does not contain the pin, or a lower tip changes nothing, and a different state at the pinned generation is recorded as a conflict and leaves the account pending. A pin from a direct relationship is advanced only by the relationship path.
4. **Any current member may carry it**, as the authenticated Room frames already allow, because the receiver verifies every signature itself and the carrier supplies no authority.
5. **Recovery links.** A chain that crosses a recovery link does not advance the pin by Room carriage (see section 2). The Room pin keeps its last value and the account is pending where fresher evidence is required.
6. **Evidence required.** Two Rooms with the same account: a pin in one does not admit in the other; a same-ID unrelated genesis in a second Room does not disturb the first Room's pin; a later device revocation carried in the Room advances only that Room's pin; a different state at the pinned generation leaves the account pending; restart.

### 2. Evidence that does not fit, or that crosses a recovery, gets no new carrier (Q-ROOM-ACS-2)

1. **Over the bucket.** The single-frame bound of ADR-0212 and the existing sealed bucket stay. A proof that does not fit is not split, no directory or retrieval service is added, and the account stays pending. The inviter sees that the person "needs to connect directly first" and the existing relationship path (first contact, K7 refresh) is the way to supply the missing states.
2. **Across a recovery.** Verifier-local recovery evidence belongs to the verifier that observed the waiting period. A Room member cannot supply it for another verifier, and a chain that crosses a recovery link is accepted by a receiver only with that receiver's own applicable evidence. Until it has it, the account is pending, and the owner's way back into the Room is the ordinary re-invitation of ADR-0207.
3. **Never.** A missing or unverifiable proof cannot admit the account; no time or timer of another verifier is accepted.

### 3. A dedicated queue can be retired, and only then is its storage released

Each receiving device uses a dedicated queue per Room and a replacement uses a fresh one (ADR-0214). A queue that no route and no grant refers to must stop being a permanent resource.

1. **Operation.** A Home that supports it advertises `om.relay.queue-retire.v1` and offers `POST /v1/relay/queues/{queue}/retire`, authorized by the queue's `relay_admin_cap` (the credential that already authorizes mint and revoke for it), with a random 128-bit `request_id`, idempotent under Home API v1 section 5 (a repeat returns the determined outcome). The queue becomes permanently **non-authoritative**: all grants on it are revoked, deposits answer `revoked`, a drain answers `revoked`, and no operation can undo it.
2. **When the client calls it.** Core retires the predecessor queue after the bounded overlap of ADR-0214 item 3 has ended **and** every grant revocation debt on it is complete or the grants have expired. The retirement debt is durable (the queue, the `request_id`) and is retried exactly after restart until success, `not_found` (success-equivalent) or the Home advertising that it has dropped the queue. Local receive eligibility already ends at the overlap boundary and never waits for HTTP progress.
3. **What the Home keeps and for how long.** On retirement the Home deletes the queue's items, receive and admin hashes, grants and replay rows and releases the storage. It keeps only a **tombstone** of the queue's opaque address (a hash, with no content, owner or time finer than the day) until the later of the longest grant lifetime it ever issued on that queue and its advertised retry horizon, so that a late exact deposit, drain or retire answers `revoked` and not as an unknown address; after that the address is an unknown address, which allocates nothing (the existing rule). The retire request's idempotency record lives for the retry horizon. Replay metadata for individual deposits does not survive retirement, because no device accepts from a retired queue.
4. **Idle expiry.** A dedicated queue with no authenticated receive or admin operation and no accepted deposit for 400 days, or for the longest lifetime a grant on it could carry plus 14 days when that is longer, is retired by the Home itself, with the same tombstone, so an abandoned mint handoff does not leave a file forever.
5. **Bounds.** Tombstones count toward the Home's resource quota and are swept; the number of live and tombstoned queues is bounded by the Home's profile limits and startup cost depends on live queues only.
6. **Metadata.** The retirement is visible to the person's own Home, which already sees leave, rotate and revoke timing; it adds no field to any deposit, drain or wake.
7. **Evidence required.** Repeated create, leave and rotate leave a bounded number of files; a hot journal recovered after a crash mid-retire; a retire retried after restart; a late exact deposit answered `revoked`; an unknown address allocating nothing; the idle sweep; the same behavior on both reference Homes.

### 4. Synchronized preferences count only evidence the receiver can verify

ADR-0215 stores a bounded causal vector with each record. Authentication establishes who sent a record; it does not establish that what the sender says it has seen is true.

1. **Clamp.** A receiver uses a vector entry `(origin, sequence)` only up to the highest sequence it has itself **received and authenticated from that origin**. A claim beyond that is treated as the receiver's own highest observation of that origin, so the record is ordered as concurrent with the origin's records the receiver has not seen. A claim never suppresses an origin's later records and is never copied beyond that clamp into a vector this device sends.
2. **Ordering stays deterministic.** Concurrent records are ordered by authenticated generation, logical time and DeviceID as ADR-0215 says; a sender that really saw the origin's record has a logical time above it, so a genuine dependency still orders correctly once the origin's record arrives.
3. **Entries that name no eligible device** (not listed in the sender's verified ACS at the stated generation) are ignored. Vectors keep their bounds (64 origins per key).
4. **Retained clears.** A candidate must descend from every clear the receiver holds, evaluated with the clamped vector; a clear the receiver has not yet received cannot be waived by a claim of having seen it.
5. **Removal.** Because claims are clamped at ingest, nothing accepted from a later-removed origin keeps suppressing other origins. A record from an origin that was eligible at its stated generation remains valid data until superseded by an ordinary later record.
6. **Evidence required.** A sibling claims far-future sequence for another origin: that origin's next update still applies; the claim is not propagated; restart; the claimant is revoked afterwards; a genuine dependency with the dependency arriving later still resolves to the later value.

### 5. Reaction registers are compacted by order, never by arrival

ADR-0198 rule 7 bounds reactions per member and message. The bound is met this way, in the Room and in the one-to-one chat alike.

1. **Registers.** For each (member account, message) Core keeps at most **64 registers** (a register is the latest event for one reaction key, present or removed, with its order). **At most 16 present reactions are shown** per member and message: the 16 present registers with the highest order. A sender's own client refuses to add a seventeenth present reaction.
2. **Order** is the existing one: `(sending epoch, EventClock, event_id)` in a Room (ADR-0198 rule 5) and the event's own clock and `event_id` in a one-to-one chat; ties cannot occur.
3. **Admission.** An event for a key that has a register replaces it only if its order is higher. An event for a key without one is inserted when fewer than 64 registers are held; otherwise it is inserted, evicting the register of lowest order, only if its order is higher than that lowest order, and it is dropped if lower. Any event older than the lowest retained order is refused, whether it is an add or a removal.
4. **Why this converges.** The retained set is the 64 keys with the highest order of their latest event, a function of the set of valid events and not of their arrival order, and the lowest retained order never decreases, so an event older than something already evicted is refused in every delivery order. A delete of the message keeps its final record as ADR-0198 rule 5 says.
5. **Evidence required.** Cap-boundary tests that deliver the same set of valid events in many permutations (including a removal evicted by churn followed by the delayed earlier add) and require identical visible sets on every replica, in both reducers.

## Where did authority go?

Nowhere new. A Room carries evidence that each member verifies against the account's own signed states; a pin is scoped to the Room that proved it; a Home retires only a resource whose own capability holder asked, or one that nobody has used for longer than anything on it could live; a sibling's claim about what it has seen is believed only as far as the receiver has seen it; a reaction register is ordered by what the event says, never by when it arrived.

## Review requirement

Section 1 item 3 reads "never replaces a held head" (ADR-0191 item 3) and "never replace existing pins" (ADR-0212 item 3) as forbidding a different head and not forbidding a verified extension of the exact pin. The safest alternative, which forbids any advance and relies on the relationship path alone, remains available by keeping this section's item 3 off; a human or specialist reviewer should confirm the reading before the advance is relied on for revocation freshness. Section 3 adds a Home operation and needs the provider conformance cases.

## Traces

Related: ADR-0121, ADR-0176, ADR-0177, ADR-0189, ADR-0191, ADR-0198, ADR-0207, ADR-0212, ADR-0214, ADR-0215.
