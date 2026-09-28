# ADR-0145 — Bootstrap single-response and completed-retry closure

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** final pre-review correction to MessageRequest bootstrap lifecycle and sealed-delivery retry ordering; no wire change and no engineering authorization
- **Amends:** ADR-0143 completed-replay receive ordering; ADR-0144 bootstrap lifecycle
- **Amended by:** ADR-0146 sibling-terminalization/reply-route/expiry closure; ADR-0147 lifetime/restore closure

## Context

ADR-0144 correctly narrows `MessageRequestPayload.reply_delivery_key` to one pending request and permits bounded retention for an accepted exact-wrapper retry. A post-closure adversarial pass exposed two remaining ambiguities.

First, an implementation that destroys the request-scoped private bootstrap key immediately after successful bootstrap completion, while still performing delivery-key lookup and HPKE open before consulting durable replay state, cannot acknowledge a byte-identical `COMPLETE` wrapper after an acknowledgement is lost. The sender can retry exactly as ADR-0143 requires, but the requester has already removed the key needed to reach the `COMPLETE` replay record. Retaining the bootstrap private key for every completed retry would restore liveness but needlessly extends secret lifetime.

Second, ADR-0144 says the key is for the accepting recipient's first post-acceptance bootstrap/control response, but also says a new logical bootstrap response may use a new wrapper/id while the request remains eligible. Those statements permit incompatible implementations and leave a multi-device acceptance race underspecified.

The repair is local endpoint state. It changes no W0 object, cryptographic suite or provider authority.

## Decisions

### 1. One MessageRequest accepts at most one logical bootstrap response

A pending MessageRequest has an endpoint-local bootstrap lifecycle:

```text
OPEN -> COMPLETE_REPLAY_ONLY -> EXPIRED
  \-> CANCELLED/DECLINED/EXPIRED
```

`OPEN` means no logical bootstrap response has yet committed. Multiple candidate wrappers may be received or temporarily hold ordinary ADR-0143 `RESERVED` replay records, but **only one valid logical bootstrap response may commit for the MessageRequest**.

The winner is the first candidate whose authenticated inner bootstrap response and associated requester-side state transition commit crash-consistently. That commit atomically compare-and-sets the request from `OPEN` to `COMPLETE_REPLAY_ONLY` and binds the winner's:

- `delivery_key_id`;
- `delivery_id`;
- exact canonical wrapper fingerprint;
- accepted recipient `IntroIdentityBinding` / identity result; and
- imported recipient relationship route/delivery-key state needed for the requester to continue the reciprocal binding over established sealed delivery.

A different candidate that loses this commit race is not a second bootstrap. It is rejected/terminally failed for this request even if its cryptography and recipient device credentials would otherwise be individually valid. Therefore simultaneous acceptance attempts from multiple recipient devices cannot create two first responses or two relationship branches.

This rule does **not** mark the relationship `ACTIVE` early. Existing reciprocal-binding rules still keep it `ACCEPTED_UNBOUND` until both stable identity bindings verify.

### 2. New logical bootstrap responses are closed after the winner commits

Once the request reaches `COMPLETE_REPLAY_ONLY`:

- no new `(delivery_id, wrapper)` may become another logical bootstrap response under that request-scoped key;
- no different wrapper may replace or supersede the winner;
- ordinary established relationship delivery keys are used for the requester's reciprocal response; and
- the request-scoped private bootstrap key is no longer an authority-bearing active key.

The recipient retries transport by resending the **exact already-sealed winning wrapper bytes**, not by resealing a semantically equivalent response with fresh HPKE randomness or a new `delivery_id`.

### 3. `COMPLETE` replay lookup precedes delivery-key lookup for an existing exact wrapper

ADR-0143 replay state is indexed by `(delivery_key_id, delivery_id)` and binds the canonical wrapper fingerprint plus route/expiry metadata. On receipt, after bounded canonical outer parsing and local route-context checks sufficient to identify the replay key and compute the fingerprint, an endpoint first consults existing replay state:

- existing `COMPLETE` + exact fingerprint + same route/context + still within the recorded replay/acceptance window => emit only the idempotent transport acknowledgement/materialization behavior; **do not** require delivery-key lookup or repeat HPKE/inner processing;
- existing `REJECTED` + exact fingerprint => reject terminally;
- existing state + different fingerprint => reject as collision;
- existing `RESERVED` + exact fingerprint => resume normal processing, which may require the still-retained private delivery key to repeat HPKE open;
- no replay record => perform ordinary delivery-key lookup, route/key/expiry validation and HPKE open before creating `RESERVED`.

The `COMPLETE` fast path is safe because it can only acknowledge the exact canonical bytes previously authenticated and durably committed; it creates no second inner effect and grants no new authority.

### 4. Bootstrap key retirement distinguishes `RESERVED` from `COMPLETE`

For the request-scoped bootstrap private key:

- while any winning candidate is only `RESERVED`, the key material required to resume that exact wrapper remains available through the bounded acceptance window;
- when the winning wrapper and its requester-side imported relationship state commit `COMPLETE`, the request becomes `COMPLETE_REPLAY_ONLY` and the private bootstrap key may be destroyed immediately;
- the durable replay/request tombstone retains only the minimum non-secret metadata required to recognize and acknowledge the exact completed wrapper through the bounded replay/acceptance overlap; ADR-0147 pins the exact wrapper and request-tombstone deadlines and `COMPLETE_REPLAY_ONLY -> EXPIRED`;
- decline, cancellation, terminal invalidation or expiry destroys active bootstrap key material and closes the request according to existing rules.

Thus lost-ack retry liveness does not require extending completed-bootstrap private-key lifetime.

### 5. Bootstrap completion and established-state import are one crash-consistent transition

A requester MUST NOT expose a split state in which imported established recipient delivery state is durable but the MessageRequest remains open to a second logical bootstrap, or in which the request is closed while the winning authenticated binding/delivery state needed for continuation was not durably installed.

The following become durable atomically (transaction or equivalent write-ahead protocol):

1. winning authenticated bootstrap-response acceptance;
2. imported recipient identity-binding result and established route/delivery-key state needed for the reciprocal response;
3. inner/session/dedupe state required by the accepted control response;
4. winning wrapper replay `COMPLETE`; and
5. request lifecycle `OPEN -> COMPLETE_REPLAY_ONLY` bound to that exact wrapper.

After crash, recovery observes either the pre-commit `OPEN`/resumable state or the post-commit replay-only winner state, never a mixture that authorizes a second bootstrap.

### 6. V-003 must exercise the closure directly

The ADR-0144 composition evidence is extended to cover at least:

- completed bootstrap + lost acknowledgement + private bootstrap-key destruction + exact-wrapper retry => idempotent acknowledgement without re-running inner effects;
- a different/new logical wrapper after `COMPLETE_REPLAY_ONLY` => rejected;
- two valid recipient-device candidates racing => exactly one atomic winner; the loser cannot create a second bootstrap branch;
- crash atomicity between accepted bootstrap/imported established state and request closure; and
- negative controls that (a) require the deleted private key before recognizing `COMPLETE`, (b) allow a second logical response after completion, or (c) split the completion/request-closure commit.

## Consequences

The ambiguity between “first bootstrap response” and “new logical bootstrap response” is removed: there is exactly one logical response per MessageRequest, plus byte-identical transport retries of its winning wrapper.

Completed retry handling retains less secret material than a key-retention-only repair. Durable replay metadata, not a live private bootstrap key, is sufficient for exact `COMPLETE` acknowledgement. `RESERVED` remains resumable and therefore retains required key material until completion or expiry.

The relationship activation rules, sealed-delivery wire format, provider model and cryptographic profile are unchanged. Engineering remains unauthorized; this ADR is a final pre-review semantic closure and must be represented in the external-review handoff.
