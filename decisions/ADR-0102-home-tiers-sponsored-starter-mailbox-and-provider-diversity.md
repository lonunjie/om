# ADR-0102 — Home tiers, sponsored starter mailboxes and provider diversity

- **Status:** Accepted
- **Date:** 2026-09-24
- **Source:** [`HARDENING_REVIEW_20260924.md`](../docs/reviews/HARDENING_REVIEW_20260924.md)

## Context

The v1 premise required every user to own a cloud (Cloudflare) account before a first message. That fails the Wi-Fi usability test (R-010). It makes a single vendor a practical universal choke point (R-145, 0M). For PWA users it also makes a hosting-account password part of endpoint trust.

## Decision

A Home can be one of five tiers: T0 sponsored starter mailbox from the inviter's Home, T1 hosted Home (household/community/organization/managed), T2 own cloud, T3 self-hosted, T4 local only. All tiers use the same public provider contracts and none is protocol-privileged. T0 is the default first-run path when onboarding starts from an invite. It grants bounded, revocable, non-chainable mailbox capacity paid for by the sponsor's own budget policy, and its metadata exposure to the sponsor is disclosed. Vendor monoculture is a declared 0M risk: a second reference provider with no cloud-vendor dependency must pass provider conformance before public-production claims (V-020, PR-013). For PWA users, hosting-account security is endpoint security (R-164).

## Consequences

Onboarding needs no signup when an invite is available. Provider migration (RelaySet overlap) becomes a daily path rather than a rare one, so its UX must be excellent. The sponsor learns delivery timing for the users it sponsors, and the client says so.
