# ADR-UIUX-0018 — R14 architecture reconciliation audit before merge

**Status:** Accepted for the separate UI/UX design track; does not amend the authoritative OM architecture.

## Context

R13C declared the standalone UI/UX track structurally complete and proposed reconciling it with the latest `om-design` architecture before merging. R14 compared R13C against `om-design-20260929-0634-PHT-reviewed-fixed.zip` (SHA-256 `78ddae29c9ad65972ed6cb346df1cfe16c6316ec30ce3ddb94be4e3d020bbe99`).

## Decision

1. No blind directory merge is allowed.
2. The existing architecture `ux_accessibility_personalization` domain is the primary merge seam.
3. OM Core/protocol architecture remains authoritative where product-scope, recovery, provider, device-control, group/Space or trust semantics differ.
4. R13C may replace stale UX-seed presentation choices when they do not alter Core authority or protocol meaning.
5. Before merge, resolve every R14 item marked `major` and add explicit UX coverage for architecture-designed V1 capabilities missing from R13C.
6. Human security-UX and real-device performance gates remain open; R14 does not self-certify V-018 or V-022.
7. R15 should perform the actual reconciled integration and architecture validation as a distinct checkpoint.

## Consequences

The standalone UI/UX work remains useful and intact, but its candidate V1 scope is no longer treated as independently authoritative. The merge becomes a controlled architecture update rather than a copy operation.
