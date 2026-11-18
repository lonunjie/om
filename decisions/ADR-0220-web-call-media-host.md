# ADR-0220 — Web call media host and continuity

## Design sources read

[ADR-0217](ADR-0217-authorize-calls-engineering.md), [ADR-0218](ADR-0218-calls-wire-freeze-and-delegated-constructions.md), [ADR-0219](ADR-0219-call-session-lifecycle.md), [ADR-0210](ADR-0210-push-wake-through-the-persons-own-home.md), [CALLS_AND_REALTIME_MEDIA.md](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md), [LIVE_CALLS_UX.md](../docs/ux-design/specs/LIVE_CALLS_UX.md), [SECURITY_PRIVACY_PROFILES.md](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md), the calls build plan and RFC 9605 sections 4 and 9 and Appendix C. The mapped general architecture and ADR sets were reviewed selectively.

- **Status:** Accepted by delegation for engineering under ADR-0217. V-007 release evidence is open.
- **Scope:** The experimental one-to-one web media host. No changes to the frozen CallSignal fields, audited architecture or public release capabilities.

## Decisions

1. **One worker per call.** The web host transfers the two one-shot Core base keys to the same-origin encoded-transform worker. The worker imports them as separate send and receive keys and clears the raw buffers. All outgoing tracks share a counter. The application uses empty SFrame metadata and generation zero (KID 0) in both directions, with direction distinguished by the established call keys. Transforms drop frames until key installation completes. The host closes the peer and terminates the worker on call teardown.
2. **Replay and overlap.** Each receiving key has a 128-counter replay window. Authentication must succeed before the replay window advances. The SFrame module accepts a newer authenticated key with a caller-specified overlap of up to three seconds for its immediate predecessor. A previously used KID cannot be installed again. Live v1 calls have only generation zero and do not activate an overlap or a new rekey mechanism.
3. **Direct-only WebRTC.** Gather up to the ADR-0218 limit and carry the local description only through CallSignal. There are no configured ICE servers. Recommended permits direct candidates with established contacts. Hardened, Maximum Privacy and a Custom policy with no typed route decision available require a private route in this build. Refuse capture and ICE in those profiles while no private route is available. A profile change closes any live direct transport that it disallows.
4. **Recovery on the same call.** The original caller initiates an ICE restart when the browser reports the connection interrupted while online, or when the browser returns online. The callee replies using the existing `ice_update` exchange. Initial and restart descriptions are distinct Core queries. The role determines which SDP type to apply for a restart, so there is no new signal kind or SDP wrapper. A connection or recovery that cannot establish usable media within 20 seconds fails the call. Report active only after installed keys, an authenticated incoming frame and a connected peer. Report an interrupted connection to Core and keep its existing call identity and media keys during recovery.
5. **Engineering opt-in.** The call host and surfaces require `VITE_OM_CALLS=1` and a dev-marked bundle. On browsers without encoded transforms the host refuses before media capture or ICE. The service worker continues to deliver the generic wake of ADR-0210. The page uses only Core's admitted call view and local missed-call notices.

## Where did authority go?

Core still authenticates the contact and owns admission, the local call preference and signal sequencing. The host reports media facts and constrains browser routing under the person's policy. No Home or browser transport receives SFrame key material.
