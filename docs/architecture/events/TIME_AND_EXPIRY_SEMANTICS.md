# Time and Expiry Semantics

- **Status:** Canonical cross-protocol rule; per-object constants remain profile/policy work
- **Checkpoint:** 2026-09-28 Run 5A / ADR-0147

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

Preferred for local elapsed-time behavior where wall-clock changes must not decide the result. For ordinary in-runtime UI timers this may be ephemeral. For a **security-critical durable verifier-local duration** (for example recovery-alone activation), ADR-0147 requires a conservative persisted elapsed-time lower bound across runtime segments: only durably measured monotonic runtime is credited, unmeasured reboot/power-off time does not count, wall-clock jumps never accelerate the timer (except the bounded ADR-0154 web-verifier gap credit below), and missing/rolled-back historical timer state restarts or otherwise lengthens the interval rather than shortening it. ADR-0154 defines the runtime segment: one OS boot session measured by a sleep-inclusive, non-settable monotonic clock paired with a boot-session identifier (`durable_elapsed_clock = boot_session`), so app-process death does not end it. A `process_only` verifier (every browser/PWA) may add wall-assisted credit for the gap between runtime segments only under its Recommended profile, only forward of a durable wall-clock high-water mark, and only with a final authenticated veto sweep before acceptance; Hardened/Maximum verifiers stay strictly monotonic.

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

The carriers are fixed (ADR-0126 rule 9, ADR-0128 rule 16; details in [`EVENT_MODEL.md`](EVENT_MODEL.md) "Expiry"):

- per event, `ConversationEvent.expiry` (wall clock, absolute) is the canonical timer, and `MessageCreate.expires_after`
  (monotonic local duration from receipt) is only an additional local bound: delete at whichever comes first, never
  after `expiry`;
- in W0 1:1 conversations the per-message `expiry` is the only carrier; there is no shared 1:1 timer state;
- in groups, `GroupControlState.policy.disappearing_timer` is the shared setting.

Backup/restore must not intentionally resurrect content already known to have expired. A restored device reconciles authoritative lifecycle/tombstone state rather than trusting an old backup's wall clock.

## 5. Security state

AccountControlState generations, MLS epochs, ratchet steps, key generations, revocation state and replay windows use cryptographic/state-machine sequencing, not wall-clock winner selection. Security-critical verifier-local delays use the ADR-0147 crash-safe monotonic lower-bound rule and cannot be accelerated by backup restore or provider/custodian timestamps.

## 6. Freeze requirement

Every canonical protocol object containing time/TTL/expiry fields must map those fields to one of the clock classes in this document before wire freeze.

Moderation and safety objects follow the same rule. `ModerationDecision.starts_at`/`expires_at` and
`SafetyAdvisory.issued_at`/`expires_at` are display or wall-clock-advisory time with no authority effect: a Room ban,
timeout or restriction ends only through a committed lift transition (ADR-0122 rule 10, ADR-0128 rule 12).

## 2026-09-23 time resolution

OM distinguishes HLC ordering, monotonic local duration, wall-clock human expiry and provider-retention time. No timestamp authorizes constitutional state. Every canonical time field declares skew/bad-clock/restore behavior before wire freeze.
