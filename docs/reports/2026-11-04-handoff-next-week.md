# Hand-off for next week (written 2026-11-04, weekly quota nearly spent)

Read first: [`2026-11-04-v1-completion-plan.md`](2026-11-04-v1-completion-plan.md) (state, rulings, work packages), then this file. Rules for every agent: [`AGENTS.md`](../../AGENTS.md) "Design gate" (budget the domain reading in your brief or you will block at the commit hook; a human-read domain can be claimed only by an agent that read it), and `bin/lonunjie-usage gate <class>` in the lonunjie root before a large dispatch (numbers from the app's `get_usage` tool; one 5-hour window is about 16% of the week; a large package costs 10 to 55% of a window).

## 1. What the operator found testing the real flow (2026-11-04)

Opening "Set up my own, free" opens Cloudflare's deploy page, then GitHub, then stops:

1. **The setup-value box is called `OM_ADMISSION`** on Cloudflare's form and our step only said "paste your setup value", so the two were not connected. Fixed in the app copy (steps now name the box). The template's own `cloudflare.bindings.OM_ADMISSION.description` (in [`infra/cloudflare-home/release/template/package.json`](../../infra/cloudflare-home/release/template/package.json)) did not show as help text for the operator; whether Cloudflare shows it at all is **unverified**. Next release of the template should keep the description short and begin with "OM setup value".
2. **GitHub step stalls.** The operator's Cloudflare GitHub app was approved only for selected repositories, and the deploy flow must create a NEW repository (it clones the template into the person's account), which a selected-repositories installation cannot see. Cloudflare's docs do not say what happens next or whether it returns to Cloudflare. The app now tells the person to allow all repositories for now and to reopen the Cloudflare setup link from OM if the page stops. **Unverified guess; test it.** If the "all repositories" advice does not unblock it, the alternatives are: (a) a documented manual path (create an empty repository first, select it in GitHub, then deploy), (b) Cloudflare's OAuth/API provisioning (see `memory` topic on the Cloudflare OAuth installer research: browser CORS blocks the token exchange, so it needs a tiny non-OM helper and is a design question), (c) a prebuilt-Worker upload path that needs no GitHub at all (design check: PROVISIONING_AND_BOOTSTRAP section 10 prefers the provider-native template; D14 records that the button needs a Git account).
   **Update 2026-11-14 (operator question; D30 in the relay status):** the flow now has three pages. (1) **Before you start**: a Cloudflare account and a GitHub or GitLab account, each its own step with "I have it"; Continue stays off until both are confirmed. The Git step carries the advice for a person who does not want to share all their repositories: make a **new free GitHub account just for this** and let Cloudflare see all of its repositories (so "selected repositories" never comes up). GitLab is named but not claimed to work (never tried). (2) **Set up your delivery service**: the OM setup value with a copy button, the steps (the box is called **OM setup value**; Cloudflare may show it as OM_ADMISSION) and the link. (3) The typed address. Still **unverified**: that a new account or "all repositories" unblocks the flow, whether Cloudflare shows the box description at all, and GitLab. Run [`2026-11-14-relay-setup-walkthrough.md`](2026-11-14-relay-setup-walkthrough.md) and record what each screen says; then tighten or drop the pre-steps from the evidence.
3. **No way back from GitHub to Cloudflare is built by us.** We cannot control that redirect; the page can only say what to do when it stalls (done) and keep the "Open Cloudflare setup" link usable repeatedly (done).

Agent group A (one Sonnet agent, reads PROVISIONING_AND_BOOTSTRAP and the Cloudflare OAuth research topic): run the real flow end to end on a fresh Cloudflare + GitHub account with "selected repositories" and with "all repositories", record exactly what each screen says, and pick the least-friction path; update the in-app steps and the template description from what is observed (screenshots in the session scratchpad only). Needs the operator at the keyboard for the sign-ins.

## 2. Work left, grouped one group per coding agent

| Group | Work | Model | Notes |
|---|---|---|---|
| A | Real deploy flow test and copy (section 1); then upgrade the live Home 0.1.1 to 0.2.0 and check migration v2 and the wake channel | Sonnet | operator present; untested assumptions in [`2026-11-03-relay-deploy-template.md`](2026-11-03-relay-deploy-template.md) |
| B | Deferred review items: D25 (search index in memory) and D26 (microphone and geolocation permission) need the operator's ruling; open a short ADR only if the operator rules a change to audited text | operator, then Haiku | no code until ruled |
| C | Multi-device phase 5 (DeviceHistorySnapshot to the new device), the open questions in [`2026-11-04-multi-device-core-status.md`](2026-11-04-multi-device-core-status.md) (MAC construction, notice signer role, quarantined device blocks approval?), device-link e2e on Firefox/WebKit | Sonnet high | Core, security-critical; non-Anthropic review owed (ADR-0167) |
| D | Personal limits profile and `ProviderUsageState` (PC-C03, D12); second reference Home (static binary + SQLite) for PR-013 and V-020; WAF rules (D8 of the beta checklist, needs a cf-mint token with WAF permission) | Sonnet | after A |
| E | Recovery flows, signed-prekey rotation, PQ epoch phase C | Sonnet high | Core |
| F | Groups (wave W1, ADR-0177): start from the six documents listed in the completion plan, section 3 | Sonnet high | a separate release after the beta |
| G | Direct/LAN large-file transfer, search hit opens the exact message, spool-full test against the real Home, voice-note recording on Firefox/WebKit (no pre-grantable microphone in the harness) | Sonnet | after A |
| H | Haiku-sized chores: release-log commits (needs an agent that read the five infra domains), memory reindex, doc sync after each release | Haiku | give exact files and the check command |

## 3. State on 2026-11-04 evening

- om `main` is pushed through the copy fix; reference Home source is AGPL-3.0-or-later.
- Relay template `lonunjie/om-relay`: `main` now holds release 0.2.0 (merge `c4587ea`; branch `release/0.2.0` kept).
- Web: see the last section of the completion plan for the 0.0.12 release and whether it was pushed to the site's `main`.
- Haiku 5.5 verdict: reliable for copy edits, docs, memory, read-only design lookups and runbook execution with explicit checks; it stops honestly at the design gate. Not used for Core, crypto or multi-module code.

## 4. Update 2026-11-13 (one session, Sonnet; budget at start: 1% of the 5-hour window, 13% of the week)

- **Done:** search hit opens the chat at the exact message and marks it (`0b6a2d9`; unit test in `conversations.test.tsx`, Chromium e2e in `attachments.spec.ts`). Closes that item of group G.
- **Design sources read (all four `apps/web` domains, in full):** `client_endpoint_distribution`, `discovery_first_contact`, `relay_reachability`, `ux_accessibility_personalization`. Not read: crypto, identity, storage, groups, time/ordering domains (the next Core work needs them).
- **Cost of the design gate:** reading the four `apps/web` domains is about 70k tokens. Batch UI changes in one session so the reading is paid once; a one-line UI change does not justify a fresh session.
- **Rule found while reading:** search/index results are bound to the originating `ProfileExecutionContext` (UI-10, R-332); any new search or notification entry point must carry the context and drop on profile change.
- **Gotcha:** `pnpm exec vitest run` in `packages/om-ui` also picks up stale `dist/**/*.test.js` after `tsc -b`; run `vitest run src` instead.
- **Still blocked on the operator, so not started:** group A (real deploy flow), group B (D25/D26 rulings), the non-Anthropic review of multi-device Core (ADR-0167). The product cannot honestly be called "ready for mass use" before these and the open items in groups C to G are done.

### 4a. State at the end of the 2026-11-13 thread (for the next session)

- **In flight:** session `local_c40b6d0e-feb1-4717-82c0-eab067865d79` (multi-device phase 5, `DeviceHistorySnapshot`) was implementing with about 16 uncommitted files in `crates/om-core` and elsewhere; `origin/main` was at `7cd5a0a`. Do not touch those files while it runs. When it finishes, check its diff, run the `om-core` tests and the two-context link e2e, and read its report before building on it. If it died, its working tree is the starting point; `git status` shows what it left.
- **Done today:** `0b6a2d9` (search hit opens the exact message), `7cd5a0a` (device-link copy says the new device starts with new messages only; update it when phase 5 lands).
- **Needs the operator:** `border.strong` contrast conflict (audited 2.3:1 versus the 3:1 component-boundary rule, VS-01; an amendment needs a short ADR), group A (real deploy flow), D25/D26 rulings, non-Anthropic review of multi-device Core (ADR-0167).
- **Next after phase 5, in order:** recovery flows (group E), signed-prekey rotation, then groups (group F), then direct/LAN large-file transfer (group G). Each needs its Core design domains read first; budget the reading (about 70k tokens per domain set).
- **Budget on 2026-11-13:** about 6% of the 5-hour window and 14% of the week at the last check; budget was never the constraint.

## 5. Update 2026-11-13 (second session, Sonnet; multi-device phase 5)

- **Done:** history snapshot to a newly linked device (Core, wasm query, TS types, copy, Chromium two-context e2e). Report:
  [`2026-11-13-multi-device-phase5-status.md`](2026-11-13-multi-device-phase5-status.md); deviation D27 in the relay status. Group C is done except the items below.
- **Still open in group C:** transcript MAC construction (design silent, needs the design owner); device-link e2e on Firefox and WebKit (the new history test
  was run on Chromium only); drafts and ADR-0148 preferences sync; a Devices-screen line reading `GetHistoryTransfer` ("copying older chats", and that the
  oldest were left out when over the bound); a live check of the queue quota on the Cloudflare Home; the non-Anthropic review of multi-device Core (ADR-0167).
- **Answered from the design:** notice signer role is `controller` (registry), a quarantined device does not block an approval (section 12 says "active devices").
- **Gotchas:** a long text message (8 KB) is not accepted by `SendMessage` in the loopback tests, use about 1.5 KB; from inside a chat on a phone-sized screen
  there is no Settings button, e2e reaches Devices with `page.goto("/settings/devices")`; the design gate for `crates/` plus `apps/web` is about 14 domains, so
  batch Core and web work in one session.


## 6. Update 2026-11-13 (third session, Sonnet; recovery flows, group E)

- **No code.** Design reading and mapping only; plan, gaps G1 to G3 (the recovery signing-key derivation is unspecified and needs the
  operator's yes) and the slice list are in [`2026-11-13-recovery-flows-plan.md`](2026-11-13-recovery-flows-plan.md). Group E needs a fresh context for the gate reading.
