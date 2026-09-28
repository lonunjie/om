# ADR-0154 — Verifier-local timer runtime segments and web-verifier liveness

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** independent architecture review (Claude Opus 5.5) of drop `20260928-2200`, finding F-01; no wire change and no engineering authorization
- **Amends:** ADR-0147 §5 (crash-safe verifier-local monotonic durations); ADR-0131 §6 by reference
- **Does not amend:** the per-verifier first-observation rule, RecoveryVeto semantics, RecoveryPolicy fields or delays, controller co-signature as the only immediate path, or the rule that Home/custodian/request timestamps are never time authority

## Context

ADR-0131 makes recovery-alone activation verifier-local: every verifier (each device that must accept the recovering account's successor AccountControlState, including every contact's devices) waits the full RecoveryPolicy delay (Recommended 72 h, Hardened/Maximum 7 d) from its own first valid observation. ADR-0147 §5 made that wait crash-safe by crediting only durably measured monotonic runtime and never crediting unmeasured downtime.

ADR-0147 left two things undefined, and together they break recovery liveness for the first V1 client:

1. **What ends a "runtime segment".** ADR-0147 says "within one runtime/boot". Native platforms expose sleep-inclusive monotonic clocks that survive app-process death within one OS boot. An implementation that treats every app-process exit as a segment break credits only the minutes the app is foregrounded, while another credits the whole boot session. Two conforming verifiers would accept the same successor days apart.
2. **Platforms with no cross-process monotonic clock.** A browser/PWA has only a per-page monotonic clock (`performance.now()`), and `performance.timeOrigin` is wall-clock-derived. Under ADR-0147 a PWA verifier can credit only the time a tab is open. A contact who opens OM for ten minutes a day needs about 432 days to credit a 72 h delay. The PWA is the first V1 vertical slice and identity recovery is on by default (ADR-0111), so for most contacts recovery-alone would never complete, and the recovered user could not resume messaging with them.

"False-late over false-early" (ADR-0147) is the right bias. A timer that practically never completes on the primary client is not "late", though: it removes a default-on V1 feature for most users.

The threat model is narrow. The per-verifier interval defeats the ADR-0131 withholding attack because the recovering party cannot backdate a verifier's own first observation, and a wall-clock-assisted gap credit does not change that. It adds exposure only to an adversary who holds the RecoveryRoot **and** can advance a particular verifier's system clock by days **and** whose request draws no controller veto before that verifier's final check.

## Decisions

### 1. A runtime segment is an OS boot session measured by a sleep-inclusive monotonic clock

A verifier's `durable_elapsed_clock` capability is one of:

- `boot_session`: the platform provides a monotonic clock that includes device sleep/suspend, cannot be set by the user or other applications, resets only at OS boot, and can be paired with a boot-session identifier; or
- `process_only`: anything weaker, including every browser/PWA runtime.

For `boot_session` verifiers, a runtime segment is one OS boot session. The verifier checkpoints `(boot_session_id, monotonic_reading, durable_elapsed)`. After the app process is killed and relaunched in the **same** boot session, it credits `monotonic_now - monotonic_reading`. Process death is not a segment break. A new boot session ends the segment, and ADR-0147 rule 3 applies unchanged: powered-off/reboot time is not credited.

Candidate primitives (Android `elapsedRealtime` with boot count, Apple `mach_continuous_time` with boot-session UUID, Linux `CLOCK_BOOTTIME` with `boot_id`, Windows sleep-inclusive interrupt time with a boot identifier) are informative. The PlatformAdapter reports the capability at runtime, and V-021's platform matrix records which OS versions provide it. An adapter that cannot confirm all four properties reports `process_only`.

### 2. `process_only` verifiers may use guarded wall-assisted gap credit

A `process_only` verifier credits monotonic time within each runtime segment exactly as ADR-0147 requires. For the **gap between two runtime segments** it MAY additionally credit wall-clock elapsed time, subject to all of these guards:

1. **Forward-only high-water.** The verifier durably keeps `wall_high_water`, the greatest wall time recorded at any checkpoint or resume of this timer, and raises it after each credit. On resume a gap earns `max(0, wall_now - wall_high_water)`. A backward jump therefore earns zero, and a later forward jump cannot re-credit the wall range already passed: the high-water mark only rises.
2. **Final veto sweep.** If the elapsed lower bound includes any wall-assisted credit, the verifier may accept the recovery-alone successor only after the bound reaches the delay **and**, within the current runtime segment, it completes an authenticated refresh of the recovering account's control state from at least one of its known routes for that account. The refresh must reveal no valid RecoveryVeto and no newer controller-authorized head that supersedes the request. If no refresh can complete, the successor stays pending. This makes the veto window's end an observed fact at the verifier, not just an inference from its clock.
3. **Strict under Hardened/Maximum Privacy.** A verifier whose own IdentityProfile runs the Hardened or Maximum Privacy profile does not use wall-assisted credit. It keeps the strict ADR-0147 behavior and accepts slower recovery. Recommended is the default.
4. **Recorded basis.** The verifier records `timer_basis = monotonic | wall_assisted` with the accepted transition. The continuity detail view may show it, for example "accepted after the recovery wait (device clock-assisted)". It is diagnostic, never alarming, and never peer-visible.
5. **Unchanged ADR-0147 rules.** Wall-assisted credit is never transferred between verifiers, never restored from backup or private sync, and never derived from Home, custodian or request timestamps. Missing, corrupt or rolled-back timer state still restarts the interval, and a restored `wall_high_water` is treated like any other restored timer state.

A `boot_session` verifier MUST NOT use wall-assisted credit across reboots. Where it lacks a boot-session identifier the adapter reports `process_only`, so only rule 2 above applies.

### 3. Scope

This rule governs every field whose time-registry entry says `skew_policy: as RecoveryPolicy.delay_seconds`: recovery-alone activation, the recovery-linked `AccountRetirement.contest_window_seconds`, and (when a later profile activates it) LegacyPolicy `owner_inactivity_wait_seconds`. Ordinary UI timers and `MessageCreate.expires_after` are not security-critical durations and are unaffected.

### 4. Threat and residual risk

T-132 records the residual: an adversary who holds the RecoveryRoot and can advance a Recommended-profile web verifier's system clock can shorten that verifier's wait. The final veto sweep bounds the effect. A controller veto issued at any time before the sweep, and reachable through the account's routes, still blocks acceptance. Hardened/Maximum verifiers are unaffected.

### 5. Evidence

V-002 gains one repo-local criterion: a bounded model covering boot-session process-restart credit, reboot non-credit, web wall-assisted gap credit with backward-jump zero credit and high-water monotonicity, the mandatory final veto sweep, and strict Hardened/Maximum mode. It must catch mutations that (a) treat process death as a boot-session break, or the opposite, a new boot as the same session; (b) credit a backward-then-forward wall jump twice; (c) accept wall-assisted credit without a veto sweep; (d) use wall-assisted credit under Hardened/Maximum; and (e) credit wall time across reboots on a `boot_session` verifier.

## Consequences

Recovery-alone completes on the PWA in roughly the configured delay for ordinary contacts, as ADR-0111 intends. Native verifiers get one precise, interoperable definition of a runtime segment. The residual clock-advance risk is explicit, confined to Recommended-profile web verifiers, and bounded by a mandatory authenticated veto sweep. No wire field, provider authority, time service or cryptographic primitive is added. This ADR is Anthropic-authored, so ADR-0117 requires a non-Anthropic reviewer before V-002's independence record can be satisfied.
