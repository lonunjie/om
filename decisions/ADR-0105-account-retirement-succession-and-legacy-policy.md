# ADR-0105 — Account retirement is a terminal constitutional transition; legacy policy has minimal power

- **Status:** Accepted
- **Date:** 2026-09-24
- **Source:** `HARDENING_REVIEW_20260924.md`

## Decision

`AccountRetirement` (user_closed | moved | legacy) is a terminal AccountControlState transition authorized by the parent state's constitutional rule, a RecoveryPolicy or, for legacy only, an optional LegacyPolicy. Contacts render Closed/Moved and stop sending. Homes purge queued envelopes on a best-effort basis. Deletion = retirement + local wipe + Home purge. A LegacyPolicy may only retire the account, publish a memorial note and point to a pre-designated successor. It never decrypts history, sends as the user or changes control, and any live controller can cancel it during its contest window (R-171, R-172). V-002 is extended to cover these transitions.
