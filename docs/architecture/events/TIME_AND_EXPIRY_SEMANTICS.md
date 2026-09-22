# Time and Expiry Semantics

- **Status:** Canonical cross-protocol rule; per-object constants remain profile/policy work
- **Checkpoint:** 2026-09-22 23:02 PHT

## 1. Time is not authority

No unauthenticated wall clock, provider timestamp, HLC value, or numerically larger timestamp can authorize a security/control transition.

Authenticated parent/epoch/hash-chain rules determine authority.

## 2. Clock classes

OM distinguishes:

### Hybrid Logical Clock (HLC)

Used for user-visible/event ordering and deterministic merge aids where documented. HLC is **not** proof of real-world time, freshness of constitutional authority, or permission.

### Wall clock

Used when the protocol intentionally expresses calendar expiry such as invitations/descriptors. Validation must define tolerated skew, bad-clock behavior and whether online trusted time is optional or required.

### Monotonic local duration

Preferred for local elapsed-time behavior where reset/clock changes should not extend a timer, such as certain disappearing-content UI timers during one device runtime.

### Provider retention clock

A provider may enforce bounded queue/object TTL according to its own service contract. Provider deletion time is availability/retention behavior, not message authority.

## 3. Signed expiry

Objects with signed `not_before`/`expires_at` semantics must define:

- which clock validates them;
- maximum accepted skew;
- behavior when local time is obviously invalid;
- whether previously accepted state remains usable during temporary clock uncertainty;
- replay behavior after expiry;
- whether an online time source is merely advisory or part of the trust model.

OM SHOULD avoid making ordinary established messaging fail solely because a consumer device clock is modestly wrong.

## 4. Disappearing content

Disappearing-message policy is authenticated conversation/application state; local deletion execution may use monotonic elapsed time where appropriate.

Backup/restore must not intentionally resurrect content already known to have expired. A restored device reconciles authoritative lifecycle/tombstone state rather than trusting an old backup's wall clock.

## 5. Security state

AccountControlState generations, MLS epochs, ratchet steps, key generations, revocation state and replay windows use cryptographic/state-machine sequencing, not wall-clock winner selection.

## 6. Freeze requirement

Every canonical protocol object containing time/TTL/expiry fields must map those fields to one of the clock classes in this document before wire freeze.
