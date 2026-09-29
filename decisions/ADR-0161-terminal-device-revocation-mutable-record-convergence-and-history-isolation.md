# ADR-0161 — Terminal DeviceID revocation, mutable-record convergence and HistoryBundle state isolation

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** Run 2 protocol/security semantic audit; design/specification only, no product engineering authorization
- **Amends:** ADR-0126, ADR-0127, ADR-0128, ADR-0131, ADR-0137, ADR-0157
- **Does not close:** V-002, V-004, V-005, any independent-review obligation, or any engineering scope

## Context

A cross-document authority audit found four security-relevant semantic gaps and one stale residual statement.

1. [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) said removal of a messaging device was reversible by linking the same device again, while ADR-0131 already required a revoked `DeviceID` never to be reused. The bounded ACS model rejected re-promotion of a revoked controller but did not reject re-linking a revoked messaging `DeviceID`. This is an ABA/resurrection path.
2. Frozen W0 `RelaySet` and `BlobLocationRecord` carry versions plus previous-record hashes, but the architecture did not define how two valid devices concurrently extend the same parent. Delivery/location records are not authority, yet order-dependent "latest record" selection can make peers retain different route/location state indefinitely.
3. `HistoryBundle` provenance was defined, but its relationship to live Room state was not. A conforming implementation could accidentally feed bundled historical items into the normal `ConversationEvent`/GroupControlState materializer, allowing an unauthenticated claimed historical EventID or control-like payload to collide with or influence live state.
4. `AccountControlState.minimum_crypto_profile` was described as a generic CryptoProfile floor even though the only defined/account-control use is the 1:1 session downgrade floor from ADR-0120/V-003. CryptoProfile `security_generation` is deliberately per conversation type and group profiles can be incomparable with 1:1 profiles. Without an explicit domain, a future profile carrying a group generation could make the same ACS transition look like a raise to one implementation and a non-raise/lower to another.
5. T-133 still described the cleared-critical-bit gap as inherent after ADR-0157 had closed it with the `!` critical namespace.

These gaps can be closed without changing a frozen W0 object shape.

## Decisions

### 1. A revoked DeviceID is terminal for the account lineage

`permanently_revoked_devices` is a monotone tombstone set.

- Once a `DeviceID` appears in `permanently_revoked_devices`, no descendant AccountControlState may place that same ID in `active_control_devices` or `messaging_devices`.
- A revoked ID is never "un-revoked", re-linked, re-promoted or recycled, including when the same physical hardware returns or a backup is restored.
- Returning/recovered hardware enrolls with a **fresh DeviceID and fresh DeviceCredential** through the ordinary link/recovery path.
- Historical credential material for a revoked ID may remain available where needed to verify old signatures; its presence is not active eligibility.
- The invariant is checked after every ACS transition, independently of which transition row authorized the change. A transition that otherwise has enough signatures still fails if its child resurrects a revoked ID.

This makes ADR-0131's existing no-reuse rule explicit in the canonical ACS transition contract and closes messaging-device resurrection as well as controller resurrection.

### 2. RelaySet and BlobLocationRecord use branch-tolerant deterministic convergence

`RelaySet` and `BlobLocationRecord` remain **availability metadata, not identity, membership, conversation or account authority**. Their existing hash-chain fields now have exact semantics.

For each logical lineage — one accepted relationship's receiving `RelaySet`, or one `BlobID`'s `BlobLocationRecord` knowledge — a verifier maintains accepted record hashes and versions independently from the currently effective payload.

#### 2.1 Structural continuity

- A genesis record has `version = 1` and omits `previous_set_hash` / `previous_record_hash`.
- A non-genesis record MUST have `version = parent.version + 1` and its previous hash MUST equal the canonical hash of a previously accepted structurally valid parent in the same lineage.
- Concurrent valid writers MAY create multiple children of the same parent, including multiple genesis records during initial partitioned publication. Such records are **branches**, not an authority fork.
- A child received before its parent is `PENDING_PARENT` and has no routing/location effect. It may be reconsidered when that exact parent arrives. An implementation must not permanently reject it merely because delivery order was reversed.
- A record may extend any retained valid branch, not only the branch that happened to be locally effective when the writer was partitioned.
- Version jumps, a non-genesis missing its previous hash, a genesis carrying a previous hash, a previous hash from another relationship/BlobID lineage, or a hash pointing to a parent whose version is not exactly one lower are rejected.

The canonical record hash is the existing domain-separated hash (`OM-RELAYSET-HASH-v1` or `OM-BLOB-LOCATION-HASH-v1`) over canonical OM-CBOR of the unsigned record body.

#### 2.2 Signer eligibility and historical chain nodes

A newly received record still needs the signer eligibility already required by the canonical delivery documents. Later quarantine/revocation/removal can make that record ineligible to be the **effective** record, but does not erase its hash/version as a historical continuity node. Therefore an eligible descendant does not become structurally unverifiable merely because an ancestor's signer was later revoked.

This distinction is important: old signing authority does not stay live, but authenticated continuity history is not rewritten retroactively.

#### 2.3 Deterministic effective record

From all structurally valid records with **at least one authenticating signer currently eligible** for that record type, every peer selects the same effective record by:

1. highest `version`;
2. for equal version, lexicographically smallest canonical record hash.

Receiving the same valid record set in a different order MUST produce the same effective record. A later eligible descendant on either retained branch can become effective by advancing the version. Same-version alternatives remain usable as non-effective fallback knowledge where their capabilities/locators are still valid, but they never change protocol authority.

For `RelaySet`, if no authenticating signer remains eligible after quarantine/revocation/removal, that record ceases to be an effective candidate; the peer recomputes from retained eligible candidates. For `BlobLocationRecord`, the same applies when none of its authenticating signers remains an eligible original-sender/current-conversation device. Ciphertext digest verification remains mandatory, so a location record never authenticates blob contents.

Implementations MAY compact payloads, but while the relationship or referenced BlobID is live they must retain enough `(record_hash, version, lineage)` continuity metadata to validate later descendants and reconsider out-of-order children. Resource limits may refuse/slow additional unauthenticated or excessive updates, but must not choose an order-dependent winner by silently forgetting a competing accepted branch.

### 3. HistoryBundle is recipient-bound inert history, never live Room state

A Room `HistoryBundle` is interoperable, author-authenticated and recipient-specific, but its contents are **archive/display material**.

Before V-004 can freeze/authorize the object, its wire contract MUST bind at least:

- the exact Room `ConversationID`;
- a unique `bundle_id`;
- the bundle author's AccountID and DeviceID;
- the exact recipient AccountID and recipient membership incarnation;
- the GroupControlState hash (and corresponding MLS epoch reference) under which both author and recipient are admitted current members when the bundle is created;
- a creation nonce/sequence sufficient to prevent accidental duplicate import;
- an ordered list of inert history items with bundle-local `history_item_id`s;
- claimed original EventID/author/time only as **untrusted provenance labels**, when present.

Acceptance requires the bundle author device to be active and non-quarantined under the author's current trusted ACS and the bound author/recipient membership incarnations to be valid in the bound Room state. A bundle for another Room, another recipient/membership incarnation, or from a non-member/removed author is rejected.

Most importantly:

- a HistoryBundle item MUST NOT enter the live `ConversationEvent` deduplication namespace;
- a claimed original EventID cannot suppress, replace or authenticate a real live event with that EventID;
- bundled material MUST NOT execute `GroupControlIntent`, GroupControlState transitions, membership/role/grant changes, pin state, poll/vote state, reaction state, edit/delete/redaction state, read/unread state, scheduled-send state or any other live Room reducer;
- references inside a bundle are rendered only inside that bundle's inert history presentation and cannot target live objects for mutation;
- if a client separately possesses a live authenticated event that resembles or matches a provenance label in the bundle, the live event remains independently authoritative; the bundle is still attributed to its bundle author.

The existing rules remain: the UI says **"shared by <bundle author>"**, not authenticated original authorship; expired/deleted/redacted content is excluded; `history_policy` is cooperative, not cryptographic. The exact recipient-specific encryption wrapper is frozen with the rest of HistoryBundle under V-004; it must reuse reviewed OM primitives rather than inventing a second group authority path.

A pre-freeze logical contract is recorded in [`spec/protocol/history-bundle.logical.schema.json`](../spec/protocol/history-bundle.logical.schema.json); it is **not** a frozen wire schema and does not satisfy V-004's CDDL/two-codec criterion.

### 4. `minimum_crypto_profile` is specifically the account-wide 1:1 floor

The frozen ACS field name and W0 field number remain unchanged, but its comparison domain is now explicit.

- `AccountControlState.minimum_crypto_profile` MUST reference a CryptoProfile that has a `security_generation.one_to_one` entry and is otherwise admissible for 1:1 use.
- ACS transition authorization compares **only** `security_generation.one_to_one` for this field. A legal `raise minimum_crypto_profile` transition strictly increases that value; equal is not a raise, and a lower value is forbidden. Profile IDs and `security_generation.group` never participate in that comparison.
- ADR-0120 restore/session-floor logic interprets the field in the same 1:1 domain: the post-restore 1:1 floor is `max(backed_up_one_to_one_generation, ACS.minimum_crypto_profile.one_to_one_generation)`.
- A profile that is group-only cannot be placed in `minimum_crypto_profile`. A mixed profile's group generation cannot make an ACS minimum-profile transition stronger or weaker.
- Group downgrade floors remain Room/group state. They compare `security_generation.group` for the active/authenticated group profile and **do not derive a floor from ACS `minimum_crypto_profile`**.
- SecurityAdvisory replacement follows the same domain: replacement of the ACS minimum must have equal-or-higher `one_to_one` generation and otherwise satisfy the advisory/profile rules.

This preserves the frozen ACS wire shape while preventing future cross-domain ordering ambiguity.

### 5. Correct stale critical-extension residual text

ADR-0157 closed the specific T-133 residual in which a future critical extension with `critical=false` could be ignored by old registries. T-133 now points to T-136/R-346 for that case. The remaining non-critical risk is limited to misleading optional presentation/fallback semantics; it is not a cross-version validity split when the extension obeys ADR-0155/0157.

## Verification consequences

- V-002 gains an explicit terminal-DeviceID/no-resurrection criterion and bounded vectors. This repo-local criterion can be evidenced now; the gate remains open for its independent-review obligation.
- V-004's HistoryBundle freeze criterion is strengthened with recipient/Room/membership binding and live-state-isolation negative vectors. V-004 remains open.
- Delivery/storage conformance gains deterministic branch/order convergence cases for RelaySet and BlobLocationRecord. A bounded design model accompanies this ADR, but implementation/interoperability evidence remains pre-release/provider work.
- V-003 additionally pins ACS `minimum_crypto_profile` to the `one_to_one` generation axis and keeps the existing floor model/vectors as executable evidence; group-only profiles are invalid for the field.
- V-005 is unaffected except for the already-open ADR-0157 strict-codec rerun; this ADR adds no W0 fields.

## Consequences

- Device revocation is now genuinely monotone across messaging and control roles.
- Concurrent route/location publications converge independent of network arrival order without turning route records into authority.
- Shared group history cannot become a covert control/event injection path or EventID-collision primitive.
- Frozen W0 CDDL remains unchanged; the new HistoryBundle logical schema is explicitly pre-freeze W1 groups material.
- ACS `minimum_crypto_profile` now has one unambiguous ordering domain (`one_to_one`); group downgrade floors remain independent Room/group state.
- `design_open` stays 0. Engineering remains unauthorized.
