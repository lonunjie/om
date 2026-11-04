# Multi-device and "use OM on my phone and my computer": plan

- **Date:** 2026-11-02
- **Scope:** `core_kernel` (ceremony, session fan-out), `provider_reference` (rendezvous), `reference_client_ui` (screens). All three are authorized.
- **Why this exists:** the operator asked for devices to be onboarded properly on web and phone, and for several devices per person.

## Where things stand

- **A second device cannot join an account today.** `OfferDeviceLink`, `RequestDeviceLink`, `ApproveDeviceLink` and
  `RejectDeviceLink` return `Rejected(unsupported)` in Core ([`flows.rs`](../../crates/om-core/src/flows.rs); status report
  [`2026-10-28-core-kernel-m1-status.md`](2026-10-28-core-kernel-m1-status.md), "What remains" item 2). That is deliberate: linking a device
  that no contact can reach is worse than refusing.
- The UI half exists and is tested but is wired into nothing: `LinkThisDevice`, `LinkDeviceFlow`, `LinkReview`
  (`packages/om-ui/src/devices`). The existing-device side is reachable from Settings > Devices; the new-device side is not shown at first run.
- **The live site has no Home by default** (`VITE_OM_RELAY_URL` is unset on purpose; the Home is deployed but no build points at it; see [`2026-10-28-relay-status.md`](2026-10-28-relay-status.md)).
  The app says "Not connected to a provider", so no message leaves a device, on any device. This blocks everything below for real use.
- Until phase 4 lands, the honest workaround is one identity per device; contacts can invite each. Do not market it as multi-device.

## Phases (each its own session; stop and review between them)

| # | Work | Pass condition |
|---|---|---|
| 0 | **Done 2026-11-02.** First-run page frame, progress cue, skippable passkey prompt, invite-first empty Chats, [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md). | `pnpm typecheck && lint && test`; screenshots at 390x844 and 1280x800 |
| 1 | **Default Home for the signed build.** Operator decision (which Home, disclosure wording, WAF/rate limits on the free tier), then set `VITE_OM_RELAY_URL`, rebuild, stage on a preview branch, approve, push `main`. | Two browsers on the preview exchange a message; live e2e `pnpm --filter @om/web e2e:live` |
| 2 | **Peer ACS-head refresh and `AccountControlHead` fetch** in Core. Prerequisite: a contact must learn that you have a new device. | `crates/om-core/tests` scenario: peer links a device, the other side refreshes, validates (never older, K7) and accepts a first session from it |
| 3 | **Per-device session fan-out** ([`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md), gate V-003): send to every eligible recipient device, self-sync to the sender's other devices, crossed-initiation, dedupe by logical message. | V-003 races/reset/dedupe cases; unit + scenario tests |
| 4 | **Link ceremony in Core + rendezvous in the Home.** Offer (new device) -> request (existing device scans) -> verification code on both -> explicit approve -> signed ACS `link_messaging` transition -> non-suppressible enrollment notice on every other device (R-166). Needs a short-lived ceremony mailbox in the Home API (wire addition: ADR, `om-home-wire` change, conformance case). New device runs `protect` then *join* instead of `create-profile`. | Scenario test: two Cores link, both send and receive with a third; revoke removes the device; a ceremony that finishes without the notice fails |
| 5 | **History and preferences to the new device**: `DeviceHistorySnapshot` after explicit approval, account-private sync for drafts and the ADR-0148 preference list. First cut may ship "future messages only" if the Devices screen says so. | Snapshot round trip on a second profile; no plaintext to the Home |
| 6 | **UI wiring and phone checks**: first-run "I already use OM" beside "Start new" (shown only once phase 4 passes), new-device QR/code screen reached from it, notice banner and Devices list, then the V-006 device checks on a real phone and the installed PWA. Playwright WebKit and Chromium on 390x844 first. | `make web-e2e` with a two-context link test; real-phone checklist [`2026-10-28-v006-device-checks.md`](2026-10-28-v006-device-checks.md) |

**Status (2026-11-04):** phases 2 to 4 and 6 are built. UI is wired per D24 (existing device offers, new device joins): first-run "I already use OM" beside "Start new" (only with a delivery service), `JoinWithLink`, Settings > Devices > Add a device with a device-link QR visibly distinct from an invite QR, verification code on both, explicit Approve/Reject with the Core nonce, a non-suppressible `DeviceNotices` strip, Devices list with revoke. Ceremony messages are profile-1 envelopes (commit 00dfac4), so `apps/web/e2e/device-link.spec.ts` runs the full two-context link against the local Home on Chromium. **Still open:** phase 5 (history snapshot to the new device); the Firefox and WebKit runs of the e2e suite; V-006 real-phone checks; a non-Anthropic independent review of the ceremony and fan-out (ADR-0167).

Phases 2 to 4 are security-critical Core work. Per ADR-0167 the code is reviewed by Claude only so far; a non-Anthropic independent review
of the ceremony and fan-out is still owed before calling them done. Using the top model tier for the design pass needs the operator's go-ahead.

## Decisions needed from the operator

1. Phase 1: which Home becomes the default, and the sponsor/host disclosure wording shown at first run (design rule: disclosed at the decision point).
2. Order: phase 1 first (nothing works end to end without it), then 2 to 4 as one stream.
