# V1 completion plan: state, rulings and work packages (hand-over)

- **Date:** 2026-11-04. **Asked by the operator:** read the whole design and the agent reports of the last days, find drift, say where we are, group the remaining work so each group goes to one coding agent, and answer open agent questions from the design (or from the constitution when the design is silent).
- **Design sources read:** [`AGENTS.md`](../../AGENTS.md), [`ENGINEER_START_HERE.md`](../process/ENGINEER_START_HERE.md) (first 80 lines), ADR-0186, the reports of 2026-11-02 to 2026-11-04 listed in section 1, and the open-beta checklist. Four research passes (Haiku) answered the questions in section 3 from the design with citations. **Not read by me in full:** the architecture domain documents; the coding agents read their own scopes (see their reports).

## 1. Where we are

| Area | State |
|---|---|
| Core (Rust/wasm) | 1:1 sealed-sender messaging, ACS chain, first contact (ADR-0181), cross-relay first contact, signed RelaySet lineage and sender failover (D22), backup file (ADR-0184). Device linking, multi-device fan-out, recovery, signed-prekey rotation: not built (`Rejected(unsupported)`). No attachments, voice notes, location card or local search in Core. |
| Home (Cloudflare Worker + DO) | Queues, key pools, descriptors, IntroGate, wake channel, owner admission (D13/D17). Per-user template published as `lonunjie/om-relay` 0.1.1; **never deployed for real** (assumptions listed in [`2026-11-03-relay-deploy-template.md`](2026-11-03-relay-deploy-template.md)). |
| Web client | Onboarding with own-relay setup (ADR-0186), real wasm Core, backup screen, landing. Live site is om-web 0.0.10 and still carries the old default relay until a new signed release is pushed (operator approval). |
| Drift | The 2026-11-03 audit stands: ADR-0181 2.1, D1, D4, D5, D10 and D22 were drift and are now closed or implemented to the audited text. Remaining drift is UI (conformance review 2026-11-03) and the section-5 features without an audited source. |

## 2. Page 1 of onboarding (operator question)

The wording is right in spirit and matches ADR-0186 and the "operatorless" constitution. Changes made (commit `87e2792`): "Chose" to "Choose"; "infra" to "delivery service" (the voice rule forbids infrastructure words); "Git" to "GitHub or GitLab" (that is what the person actually signs up for); "Easy" and "Advanced" groups kept; the button keeps "Set up my own, free" (ADR-0186, copy test). The old explainer line about the service holding messages locked was dropped from page 1 (it stays under Settings, "What is the delivery service?"). Caution for the study V-018: "Easy" with two new accounts may not feel easy to a first-time user; test it.

## 3. Rulings (design first; constitution where silent)

| Question | Answer | Source |
|---|---|---|
| UI gaps S2, S4, S5, V1, V2, C3, V4, L1, F3, T5, F6 | Build to the audited design (rule 4: design wins). OM Calm is canonical; violet focus; tinted outgoing bubble; Omnibar and new-conversation action; chat-row preview; no one-item rail; delivery facts, raw address behind a disclosure. | [`DEFAULT_THEME_AND_COLOR_SEMANTICS.md`](../ux-design/specs/DEFAULT_THEME_AND_COLOR_SEMANTICS.md), [`COMPONENT_BEHAVIOR_AND_STATES.md`](../ux-design/specs/COMPONENT_BEHAVIOR_AND_STATES.md), [`NAVIGATION_AND_OMNIBAR.md`](../architecture/ux/NAVIGATION_AND_OMNIBAR.md), ADR-0179 |
| Section-5 features with no audited source | Keep the functional and security ones (name at setup, web-link invite fragment, Settings search, Lock, Backup, Help, per-device idle lock) as recorded deviations. Remove the visual inventions (golden-section card, brand header, red-dot card, slide-in panes, "round everything"). | [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md) lines 13-14 |
| Device-link transport (design **silent**) | Decided from the constitution (no OM-run service, the person's own relay): the ceremony uses ordinary short-lived queues on the existing device's relay; the scanned code carries the relay address, request deposit capability, response receive capability and a one-time key; no new Home endpoint. Policy parts are from [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) 4.1, 6, 12 and ADR-0181 nonces. | recorded as a deviation by WP2 |
| Attachments without R2 | Small files: the bounded encrypted DO spool (`max_blob_spool_bytes`); larger files: direct online or LAN transfer bound to BlobID and ciphertext digest; R2 stays opt-in. What the sender sees when a file is too big and the recipient is offline is **silent**: decided as "refuse before sending, say so in plain words, offer to send when the other person is online". | [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md), ADR-0150 |
| Local search | Client-side search worker over the local store, FTS-style index, encrypted at rest, per profile, wiped with the messages, never uploaded, never in OS search. | [`OMNIBAR_INDEXING_AND_PERFORMANCE.md`](../ux-design/specs/OMNIBAR_INDEXING_AND_PERFORMANCE.md), [`LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md`](../architecture/storage/LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md) |
| Voice notes, location card | Both are W0 design: ordinary audio attachment plus optional `om.voice-note.v1`; one-shot `om.location-card.v1`. Built after attachments. | [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md), ADR-0148/0155 |
| Groups | W1, not part of `om-open-beta-1` (ADR-0183 lists three scopes). [`V1_SCOPE_MAP.md`](../ux-design/specs/V1_SCOPE_MAP.md) calls ordinary groups V1; read as "V1 product", next wave after the W0 beta. Not started. | [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), ADR-0177 |
| Second Home in Settings | Build as an Advanced item under the delivery-service page ("Add a backup delivery service"); Core and adapter are ready. | MULTI_RELAY 4, D22 residuals |
| Skip on relay page | No skip (a person without a delivery service cannot send; ADR-0186 decision 4). | ADR-0186 |

## 4. Work packages (one coding agent each)

| WP | Scope | Model | Status |
|---|---|---|---|
| WP0 | Page 1 copy; stale status docs | Haiku | page 1 done; docs sync running |
| WP1 | UI conformance fixes (section 3 rulings) | Sonnet | running |
| WP2 | Multi-device Core: peer ACS refresh, per-device fan-out, link ceremony (phases 2 to 4) | Sonnet | running |
| WP3 | Attachments: DO spool in the Home, adapter, Core `AttachmentRef` path, direct transfer, size refusal | Sonnet | queued after WP2 (shares Core) |
| WP4 | Voice note and location card, local search, in `om-ui`/Core | Sonnet/Haiku | after WP3 |
| WP5 | Settings: second delivery service; multi-device UI wiring (first-run "I already use OM", Devices list, notice banner) | Sonnet | after WP1 and WP2 |
| WP6 | Release: first real relay-template deploy on the operator's own Cloudflare account, signed om-web release without the default relay, push to the site repo, GitHub release | Sonnet main | needs the operator's approval and accounts |
| WP7 | Personal limits profile, ProviderUsageState (PC-C03), second reference Home (static binary + SQLite) for PR-013 | Sonnet | after WP6 |
| Later | Recovery, signed-prekey rotation, PQ epoch phase C, groups (W1), portability filters (ADR needed) | Sonnet high | post-beta |

Routing note (2026-11-04): Haiku 5.5 completed a bounded copy change across six files with tests and trailers correctly, and answered design lookups with usable citations; it is used for copy, docs, registry and read-only research. Sonnet is used where code crosses modules or touches security.

## 5. Status at the end of 2026-11-04 (hand-over for the next session)

Budget rule: `bin/lonunjie-usage gate <class>` (hq) before any big dispatch; plan numbers come from the app's `get_usage` tool. At the end of the day the 5-hour window was at 91% (resets 18:30 Manila) and the week at 68%, so the rest waits for the reset.

| WP | Result |
|---|---|
| WP0 | Done: page 1 (`87e2792`), README and UX brief (`e2e36ae`), memory sync (hq `bf452a8`). |
| WP1 | Done: OM Calm palette and violet focus, Omnibar and new-conversation action, no one-item rail, chat-row previews, delivery-service facts page (`915eced` to `34eb535`). Open: Omnibar message search, desktop screenshots, tier and origin facts are shown but not yet choosable, `border.strong` contrast about 2.3:1 on light canvas. |
| WP2 | Done in Core: peer ACS-head refresh, per-device fan-out, link ceremony D24 (`eeeb4d0`, `689c8b5`, `8ec3ec3`, `00dfac4`). Open: history snapshot to the new device (phase 5), independent non-Anthropic review, MAC construction and notice signer role (questions in [`2026-11-04-multi-device-core-status.md`](2026-11-04-multi-device-core-status.md)). |
| WP5a | Done: backup delivery service and "move to a different one" in Settings (`3f9dca0`). |
| WP5b | Done: adapter commit, first-run "I already use OM", Devices wiring, two-context link e2e on Chromium (`15287f1` to `5e810b6`). Open: Firefox and WebKit runs, V-006 real-phone checks. |
| WP3a | Done: Home blob spool `om.blob.spool.v1`, undeployed (needs migration v2 and a new signed template release). Reference Home relicensed to AGPL-3.0-or-later (`2ac4088`, `e8b4bfe`). |
| WP3b | Core attachment, voice-note and location-card paths done against a loopback store. Not wired: adapter, wasm commands, UI. |
| WP6a | Release candidate om-web 0.0.11 signed and staged on the preview branch `release/om-web-0.0.11-preview` of `lonunjie/om.lonunjie.org`, preview `https://0e55a0e1.om-lonunjie.pages.dev`; 16 of 16 files match the manifest. **Not on `main`: the operator approves the production push.** |

**Next, in order (each its own agent, gate first):**
1. WP4b (Sonnet): wire attachments: wasm commands from the Core report, adapter `payload_too_large` to `too_large` plus `BlobReceipt`, UI (attach, voice note, location card), and local search worker (Omnibar message search). Needs the design-gate reading for apps, packages, crates.
2. WP7a (Sonnet): relay template release 0.2.0 with the spool (migration v2) and the AGPL headers; then the first real deploy on the operator's own Cloudflare account to test the untested assumptions in [`2026-11-03-relay-deploy-template.md`](2026-11-03-relay-deploy-template.md).
3. Operator: approve the 0.0.11 preview, then the `main` push; the old default relay disappears from production with it (ADR-0186).
4. Later: personal limits profile and `ProviderUsageState`, second reference Home (PR-013), recovery, signed-prekey rotation, groups (W1).

### Update, later on 2026-11-04 (after the 5-hour reset)

| WP | Result |
|---|---|
| WP4b | Done: attachments, voice notes, location cards and local search wired (wasm, Home adapter, UI), Chromium e2e green (`84cb29c` to `0954eee`). Deviations D25 (search index in memory, rebuilt at unlock) and D26 (Permissions-Policy allows microphone and geolocation for the app's own origin) are not yet operator-reviewed. Open: direct/LAN large-file transfer, opening a search hit goes to the chat not the message, Firefox/WebKit run of the new spec, no spool-full test against the real Home. |
| WP7a | Done: relay template 0.2.0 (spool, migrations v1 and v2, upgrade test) built, signed, verified and staged on branch `release/0.2.0` of `lonunjie/om-relay` (`d1586b9`); `main` of the template repo untouched. |
| WP6a | Superseded by om-web **0.0.12** (includes WP4b): preview `https://146b4002.om-lonunjie.pages.dev`, branch `release/om-web-0.0.12-preview` (`359cf26`), 16 of 16 files match the signed manifest. At go-live re-run `release.mjs --sign-from-vault --append-digest-log` (the manifest rebuilds byte-identical), commit the release-log line, then push the site branch to `main`. |
| Cross-browser | Firefox and WebKit: 24 passed, 2 skipped (live-relay) for the existing cross-browser set. |

**Operator steps to go live, in order:** (1) review the 0.0.12 preview; (2) approve the push of `release/om-web-0.0.12-preview` to `main` of `lonunjie/om.lonunjie.org`; (3) merge `release/0.2.0` into `main` of `lonunjie/om-relay`; (4) upgrade the live Home from 0.1.1 to check migration v2, then run the first real template deploy on your own Cloudflare account (untested assumptions: [`2026-11-03-relay-deploy-template.md`](2026-11-03-relay-deploy-template.md)); (5) rule on D25 and D26.

### Final status, 2026-11-04 night (operator go-ahead given)

- **om-web 0.0.12 is live** on `main` of `lonunjie/om.lonunjie.org` (`af46b9d`): manifest, 16 of 16 files and CSP verified against the signed build at the live origin. It no longer carries a default delivery service (ADR-0186). Its onboarding steps now name the `OM_ADMISSION` box and say what to do when GitHub stalls (see [`2026-11-04-handoff-next-week.md`](2026-11-04-handoff-next-week.md); that GitHub guidance is unverified).
- **Relay template 0.2.0 is on `main`** of `lonunjie/om-relay` (merge `c4587ea`). Not yet deployed to any account; the first real deploy and the live Home upgrade from 0.1.1 are next week's group A.
- Steps (1) to (3) of the go-live list above are done; (4) and (5) remain with the operator.
