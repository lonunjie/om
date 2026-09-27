# ADR-0143 — Crash-safe sealed-delivery replay reservation and exact retry completion

- **Status:** Accepted
- **Date:** 2026-09-27
- **Scope:** verification-derived correction to ADR-0142 local receive state; no new wire object and no engineering authorization
- **Amends:** ADR-0142 §6 replay/retry semantics and the accepted-relationship session-state profile

## Context

ADR-0142 correctly requires `(delivery_key_id, delivery_id)` to be durably reserved before inner plaintext release or acknowledgement and permits an exact same-route wrapper retry. A V-003 crash-state pass exposed one ambiguity in the original wording: if the process crashes **after** the durable outer replay reservation but **before** the authenticated inner envelope is durably accepted, a receiver that treats every existing reservation as a terminal replay can drop the exact retry forever. The wrapper has then been neither accepted nor safely retryable, creating a crash-induced message-loss path.

The repair must preserve all ADR-0142 security properties: reservation still precedes plaintext/event release, a repeated completed wrapper must never repeat inner application effects, a different wrapper cannot reuse the same key/id pair, changing routes still requires a fresh seal/id, and successful outer HPKE open still does not authenticate the sender.

## Decisions

### 1. Replay persistence is a small durable state machine, not a boolean set

For every accepted outer key `(delivery_key_id, delivery_id)`, the recipient keeps a durable record containing at least:

- a collision-resistant fingerprint of the complete canonical `SealedDeliveryEnvelope` bytes;
- state `RESERVED`, `COMPLETE`, or `REJECTED`; and
- enough local expiry/garbage-collection metadata to retain the record through the wrapper's acceptance lifetime.

These states are endpoint-local persistence. They do **not** change the W0 CBOR/CDDL wire format.

### 2. `RESERVED` is resumable only by the exact same wrapper

After route/key/expiry checks and successful outer authentication/structural parsing, the endpoint durably creates `RESERVED` before inner plaintext/event release or acknowledgement.

If `(delivery_key_id, delivery_id)` already exists:

- a different wrapper fingerprint is rejected as replay/collision;
- `REJECTED` is terminal for that exact wrapper;
- `COMPLETE` is an idempotent transport replay: inner authentication/application effects are not re-run, although an idempotent transport acknowledgement may be emitted; and
- `RESERVED` permits **only the byte-identical/canonically identical same-wrapper retry** to resume processing. The receiver may need to repeat outer HPKE open after a crash because plaintext is intentionally not durable merely to support transport retry.

Thus the durable reservation remains a replay barrier without turning an interrupted first attempt into permanent loss.

### 3. Inner acceptance and `COMPLETE` form one crash-consistent commit

The transition `RESERVED -> COMPLETE` occurs in the same crash-consistent commit as the inner 1:1 session/dedupe state required by K2, using one transaction or an equivalent write-ahead/commit protocol. In particular:

1. outer authentication and structural checks succeed;
2. `RESERVED` is durable;
3. the unchanged inner envelope passes the normal trusted `RelationshipContext`, ACS/DeviceCredential, PQXDH-or-ratchet and inner replay checks;
4. the inner session/dedupe acceptance state and replay state `COMPLETE` become durable together; and only then
5. application-visible plaintext/event release and transport acknowledgement may occur.

If step 2 cannot persist, nothing is released or acknowledged. If step 4 cannot commit, the record remains `RESERVED`; an exact retry may resume. If a crash happens after step 4 but before presentation/ack, the durable accepted event is recoverable through normal local materialization and a transport retry is idempotently acknowledged without a second inner effect.

An inner-authentication failure may durably move the exact wrapper to `REJECTED`; it never becomes authenticated merely because the outer HPKE layer opened.

### 4. Route migration and logical-message dedupe are unchanged

Same-route transport retry reuses only the exact original wrapper. Delivery over another route/provider still uses that route's current delivery key, fresh HPKE randomness and a fresh `delivery_id`. The authenticated inner EventID remains the cross-route/logical-message dedupe identity, so successful redundant-route delivery does not create a second conversation event.

### 5. Verification target

V-003 must exercise at least:

- crash after `RESERVED` followed by exact-wrapper retry;
- crash after inner durable acceptance/`COMPLETE` but before app release/ack;
- persistence failure while creating `RESERVED` and while committing `COMPLETE`;
- exact completed replay without duplicate inner effects;
- same key/id with different wrapper bytes;
- route mismatch and route migration with a fresh id;
- key refresh/revocation/overlap and expiry;
- no clear-sender fallback; and
- the negative control where outer success is incorrectly treated as sender authentication.

V-001 remains responsible for actual pinned HPKE/KAT execution. This ADR does not claim cryptographic execution from a logical state model.

## Consequences

This is a narrow crash-consistency correction discovered during verification, not a new architecture direction. It adds no provider authority, central service, wire field or primitive. The core sealed-delivery design remains ADR-0142; implementations now have an unambiguous recovery rule for the interval between durable replay reservation and durable inner acceptance.

The V-003 sealed-delivery state criterion may be satisfied by executable evidence for these semantics, but V-003 still requires its independent review criterion. V-001 sealed-delivery HPKE execution, V-005 Rust/CDDL execution and V-015 real observer/provider trace evidence remain separate obligations.
