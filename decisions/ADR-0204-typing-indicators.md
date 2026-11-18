# ADR-0204 — Typing indicators: off unless the person turns them on, direct channel only, no new relay-visible signal

- **Status:** Proposed, 2026-11-18. Drafted under the operator's delegation to fill silence and fail closed. **Not accepted and not implemented**: section 5 states why the operator's decision is needed first.
- **Class:** `additive_noncritical_extension` under ADR-0165 (one new ephemeral frame kind on a channel that already exists), if accepted.
- **Amends:** nothing. It fills the silence recorded as Q-CHAT-5: the design names typing as a standard optional ephemeral capability and gives no frame, no lifetime, no rate limit, no delivery class and no meaning for "reduced".

## Design sources read

Read in full: the design gate in [`AGENTS.md`](../AGENTS.md); [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md) (the Typing row); [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md) ("Ephemeral signals"); [`SECURITY_PRIVACY_PROFILES.md`](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md) (the three profiles and the Maximum Privacy list); [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md) (section 8, the three-level table); ADR-0158 and ADR-0194 (the direct channel and its frame list, by search and the lines around them).

Not read, stated and not claimed: the rest of what `python3 tools/design_gate.py needs` prints for `crates/om-core/src/` and `packages/om-ui/src/` ([`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) and the other constitution documents, [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../docs/architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`NOTIFICATIONS_AND_PUSH.md`](../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md), the rest of the UX documents). The constitutional reading below therefore rests on the privacy-profile text and the metadata threat model alone.

## Context

Every message leaves a relay-visible deposit. A typing signal sent the same way adds a deposit for something that is not a message, at the moment a person starts to write, so it adds timing and volume that the relay and anyone watching it would not otherwise see. The threat model already treats "reduced presence/receipts" as a hardening step and Maximum Privacy turns typing off by default. The design offers no way to send a typing hint that adds no relay-visible signal: any frame carried by the relay (including inside the same sealed envelope shape as the MLS application traffic of a Room) is one more deposit.

## Decision (proposed)

1. **Default off, for everyone.** A typing signal is neither sent nor shown until the person turns it on in Settings > Privacy. This answers Q-CHAT-5 the fail-closed way: a capability that exposes behaviour needs a person's yes, in every profile. Maximum Privacy cannot turn it on from the profile screen; it needs the explicit per-person switch, with the cost stated in plain words.
2. **Direct channel only.** The signal travels only on an already open direct channel to that contact (ADR-0158, the LAN or peer channel that carries direct transfers), as a new frame kind `typing` of two states, `start` and `stop`, with no body. It is never deposited at a relay, never sent through a Home, never placed in an MLS application message, and never sent when no direct channel is already open. So it adds no relay-visible object and no new connection; its only observer is the contact the person already talks to on that channel. This is the only reading that satisfies "no extra relay-visible signal beyond existing traffic".
3. **Lifetime and rate.** A `start` shows for at most 8 seconds and then disappears unless refreshed; a sender refreshes at most once every 4 seconds and sends `stop` when the draft is sent or cleared. Nothing is stored, logged or counted, and a received signal is never persisted or forwarded to the person's other devices.
4. **What "reduced" means for Hardened.** At Hardened the switch exists but typing is only received and shown, never sent, unless the person also turns on sending. At Recommended each direction is its own switch. At Maximum Privacy both stay off and need the explicit switch of section 1.
5. **Rooms.** No typing signal in a Room. A Room has no direct channel to every member, and the only other carrier is MLS application traffic, which is relay-visible.

## Why this is not accepted yet

- Section 2 makes typing work only where a direct channel is already open. For most people that is rare, so the feature would be seldom seen. If the operator wants typing over the relay, the cost is a new relay-visible deposit per `start` and a timing signal, which contradicts the stated constraint and needs the operator's own ruling.
- A new frame kind needs wire vectors and a metadata-threat review by someone other than the author; neither exists.
- The constitution documents were not read. An operator who has read them may rule differently on defaults.

## Consequences if accepted

One frame kind in the direct-channel codec, a per-person two-switch setting in Core with the profile rules above, a received-state query and a quiet indicator in the conversation view with a reduced-motion form, vectors, and tests that no deposit, no relay call and no storage write follows a typing signal. Nothing here changes any existing message path.
