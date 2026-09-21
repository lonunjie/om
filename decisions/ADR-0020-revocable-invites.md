# ADR-0020 — Revocable, Scoped Invitations

**Status:** Accepted

QR/link invitations are scoped introduction capabilities with random IDs/secrets, expiry, optional use limits, and revocation. Revoking an invite does not affect accepted relationships. Offline/self-contained invites cannot provide instantaneous remote revocation and therefore require explicit short-lived semantics.
