# ADR-0040 — No uploaded address book in v1

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

OM v1 does not upload or hash users' contact books. Invite/share flows use existing OS share mechanisms; Contacts permission, if granted, is local-only display/matching metadata.
