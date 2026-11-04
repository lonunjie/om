# Hand-off for next week (written 2026-11-04, weekly quota nearly spent)

Read first: [`2026-11-04-v1-completion-plan.md`](2026-11-04-v1-completion-plan.md) (state, rulings, work packages), then this file. Rules for every agent: [`AGENTS.md`](../../AGENTS.md) "Design gate" (budget the domain reading in your brief or you will block at the commit hook; a human-read domain can be claimed only by an agent that read it), and `bin/lonunjie-usage gate <class>` in the lonunjie root before a large dispatch (numbers from the app's `get_usage` tool; one 5-hour window is about 16% of the week; a large package costs 10 to 55% of a window).

## 1. What the operator found testing the real flow (2026-11-04)

Opening "Set up my own, free" opens Cloudflare's deploy page, then GitHub, then stops:

1. **The setup-value box is called `OM_ADMISSION`** on Cloudflare's form and our step only said "paste your setup value", so the two were not connected. Fixed in the app copy (steps now name the box). The template's own `cloudflare.bindings.OM_ADMISSION.description` (in [`infra/cloudflare-home/release/template/package.json`](../../infra/cloudflare-home/release/template/package.json)) did not show as help text for the operator; whether Cloudflare shows it at all is **unverified**. Next release of the template should keep the description short and begin with "OM setup value".
2. **GitHub step stalls.** The operator's Cloudflare GitHub app was approved only for selected repositories, and the deploy flow must create a NEW repository (it clones the template into the person's account), which a selected-repositories installation cannot see. Cloudflare's docs do not say what happens next or whether it returns to Cloudflare. The app now tells the person to allow all repositories for now and to reopen the Cloudflare setup link from OM if the page stops. **Unverified guess; test it.** If the "all repositories" advice does not unblock it, the alternatives are: (a) a documented manual path (create an empty repository first, select it in GitHub, then deploy), (b) Cloudflare's OAuth/API provisioning (see `memory` topic on the Cloudflare OAuth installer research: browser CORS blocks the token exchange, so it needs a tiny non-OM helper and is a design question), (c) a prebuilt-Worker upload path that needs no GitHub at all (design check: PROVISIONING_AND_BOOTSTRAP section 10 prefers the provider-native template; D14 records that the button needs a Git account).
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
