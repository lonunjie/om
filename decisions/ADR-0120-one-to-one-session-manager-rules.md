# ADR-0120 — One-to-one session manager rules

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator (delegated 2026-09-24: "draft the design gap rulings ... check it and accept it if good")
- **Source:** V-003 ([`verification/prototypes/v003-sessions/`](../verification/prototypes/v003-sessions/README.md)). **Amends:**
  [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md), [`CRYPTOGRAPHY_AND_AGILITY.md`](../docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) "Downgrade floor",
  [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md).

## Context

The V-003 model shows that the session manager is safe only if eleven rules hold. The docs stated most of them only
as outcomes, or not at all. Turning off any one rule gives a TLC counterexample (key reuse, a message accepted twice or
lost, or a revoked device authorized again).

## Decision

1. **Normative rules.** A conforming session manager MUST:
   - **K1** make ratchet and outbox state durable before an envelope is handed to any transport;
   - **K2** make the dedupe record and the resulting session state durable before acknowledging receipt to a relay;
   - **K4** on an authenticated reset that names a logical message, re-encrypt that message on a fresh session
     (bounded retries, then a visible delivery failure);
   - **K5** re-check recipient eligibility against the current validated AccountControlState at every encryption,
     including fan-out and retries;
   - **K6** take recipient devices only from validated AccountControlState, never from a provider list (already stated);
   - **K7** persist each peer's latest validated control head across crash and restore, and never accept an older one;
   - **K8** never restore ratchet state from a backup (already stated);
   - **K9** reject traffic from a sending device that is revoked in the receiver's validated view of the sender's
     control state;
   - **K10** carry the prekey (handshake) header on every message of a session until the peer's reply confirms it;
   - **K11** keep handshake prekey secrets (signed, one-time and last-resort) out of every backup and archive. A
     restored device publishes fresh prekeys and answers a prekey message addressed to an old prekey with an
     authenticated reset.
   Each rule's V-003 vector is its conformance test.
2. **Downgrade floor survives restore (K3).** Per-peer floor records are not secret, so they are included in backups.
   After a restore, the 1:1 floor is the maximum of the restored `one_to_one` generation and the `security_generation.one_to_one` of the AccountControlState `minimum_crypto_profile` (ADR-0158). A
   receiver that rejects a session below the floor MUST send an authenticated reject naming the message. The sender
   treats it as a reset (K4) and retries at the floor profile, so the rejection is never silent.
3. **Lazy repair is accepted (P1).** A device that lost sessions does not broadcast to its contacts. Doing so would
   cost traffic and expose the contact graph to relays. Delivery is still guaranteed: a message that cannot be
   decrypted triggers an authenticated reset, and K4 re-sends it. The cost is one extra round trip on the first message
   after a loss.

## Consequences

The V-003 gap list is closed as design work. What remains for V-003 is the non-author review (ADR-0117). The
restore-plus-redelivery TLC configuration stays unchecked, and K11 is covered by its gap configuration and the
fuzzer.
