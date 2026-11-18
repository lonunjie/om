# ADR-0219 — Local one-to-one call lifecycle

## Design sources read

ADR-0217, ADR-0218, ADR-0210, [`call.cddl`](../spec/cddl/call.cddl), [`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md),
[`LIVE_CALLS_UX.md`](../docs/ux-design/specs/LIVE_CALLS_UX.md), [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md), [`ASYNC_KEY_MATERIAL.md`](../docs/architecture/crypto/ASYNC_KEY_MATERIAL.md),
and [`TIME_AND_EXPIRY_SEMANTICS.md`](../docs/architecture/events/TIME_AND_EXPIRY_SEMANTICS.md) define the affected boundaries. Also consulted
the scoped passages of [`IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md`](../docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md),
[`IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md`](../docs/process/IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md), [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md),
and the calls scope and V-007 entries. Other design-gate material for shared
identity, provider, and presentation code was consulted for relevant boundaries
and was not read in full.

- **Status:** Accepted by operator delegation, 2026-11-18.
- **Scope:** One-to-one call state and its host boundary under ADR-0217.
- **Compatibility:** Uses ADR-0218's frozen payloads, labels, suite and queue lifetime.

## Decision

1. **Simultaneous dial.** While both devices are calling each other and neither
   call has selected a recipient, the numerically lower random CallSessionID
   (lexicographic byte order) wins. The device switching to that incoming call
   cancels its former outgoing call. The device keeping its outgoing call ignores
   the competing offer. Answer is still an explicit local action. A call to
   another contact, or an offer during a selected call, follows the busy rule.
2. **Existing sessions.** Call deposits use the same per-device delivery keys,
   padding and Home operations as messages. The existing sealed envelope
   expiry is the queue's 45-second retention deadline for an offer, capped by
   the route and delivery-key expiry. Other signalling is similarly short-lived.
   The endpoint tries the call ratchet context only in an existing session after
   ordinary-context authentication fails. A repeated initiation prefix that
   names the selected session still follows the session's K10 checks. A call
   context never establishes a session. A device with no usable session and
   route at dial time receives no offer, with no deferred fan-out or bootstrap
   work. No cleartext discriminator is added. The existing expiry value, timing
   and padding bucket remain visible at the Home.
3. **Time and restart.** The receiver rings for up to 45 seconds from first
   sight, provided the authenticated envelope has not expired at receipt. An
   expired but still authenticatable offer can produce a local missed-call
   notice. The sender stops waiting 45 seconds after dial. The injected local
   wall clock is an expiry hint. A backward step observed during a ring ends
   the ring and drops unsent signalling. The existing sealed-envelope skew
   bound is used to limit how far ahead a received expiry may be, never to
   extend the admission deadline. Receipt, expiry, and Home acceptance do not
   attest ringing or playable media. The outgoing view stays `starting` until
   answer or termination. Only a host media fact after key handoff sets
   `active`. Termination immediately clears Core's live data and is visible to
   the host as a terminal state.
4. **Bounds and commit.** Keep one live call per DeviceIdentity, the initial
   remote description and latest accepted restart description, up to 128
   unsent sealed call deposits and 64 local notices. The latest 128 call
   operation IDs participate in normal local deduplication. Up to 512
   authenticated offer tombstones (account, call ID and first-observation time)
   are durable with the receive ratchet and expire after ten minutes plus
   existing envelope skew. When full, refuse new offers until entries expire.
   Only admitted ring attempts and authenticated late offers count toward the
   ten-minute unanswered limit. Acceptance clears that offer's contribution.
   Folding, busy responses and notice retention are local. Ratchet send state
   commits before its transient deposit can be performed. A refused deposit
   fails the affected leg. An uncertain result preserves the existing call
   and waits for authenticated peer signalling or expiry. Restart drops any
   unsent call packets and all live call state, including pending key effects.
5. **Local preference and key handoff.** The switch is bound to the established
   contact's account on this device. Turning it off silences future offers and
   clears an outstanding incoming ring. The Core driver can take the two
   ordered-pair base keys once after selection/acceptance. Key generation is
   zero. The WASM handoff uses raw bytes and wipes the bridge buffer after
   copying. The driver owns the returned buffers until it imports and clears
   them. Call secrets, base keys and descriptions stay outside durable state,
   backup and ordinary queries. The reference client leaves its call driver
   disabled pending the calling release gate.

## Where did authority go?

The account head and established ratchet authenticate the device. Core
validates the contact, local preference and call sequence. The media host
supplies local media facts and has no contact-admission authority. The Home
continues to enforce its existing queue, expiry and capability rules.

## Evidence

`calls_loopback.rs` drives sealed deposits and verifies admission, lifecycle,
restart, compatibility and directional keys. `device_link.rs` exercises the
first answer race across linked devices. The Core KDF test uses an independent
HKDF calculation. The WASM call driver test and contact-sheet switch test cover
the two host surfaces. The downstream SFrame and route verification remains
in V-007.
