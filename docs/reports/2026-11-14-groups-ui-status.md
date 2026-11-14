# Groups UI: status (2026-11-14)

## Design sources read

Read in this work: [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md); [`2026-11-13-groups-plan.md`](2026-11-13-groups-plan.md); [`GROUPS_AND_SPACES_UX.md`](../ux-design/specs/GROUPS_AND_SPACES_UX.md); [`CONTENT_AND_MICROCOPY_VOICE.md`](../ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md);
[`ARCHITECTURE_MANDATED_V1_SURFACES.md`](../ux-design/specs/ARCHITECTURE_MANDATED_V1_SURFACES.md); ADR-0177; [`PRODUCT_AND_UX_PRINCIPLES.md`](../overview/PRODUCT_AND_UX_PRINCIPLES.md); [`CURRENT_UI_REFERENCES_20260929.md`](../research/CURRENT_UI_REFERENCES_20260929.md);
[`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md); `om-v1-completion.md` (shared memory); the group intents in `groups.ts` and the harness group
simulation in code.

Read in full at commit review (2026-11-14, a second pass after the builder's context was compacted): every document and ADR that
`python3 tools/design_gate.py needs apps/web packages/om-ui` lists (domains `client_endpoint_distribution`, `discovery_first_contact`,
`relay_reachability`, `ux_accessibility_personalization`: all client, discovery, delivery, ecosystem-plugin and UX architecture
documents, the two overview and research documents, and ADR-0004, 0007, 0012, 0019, 0020, 0021, 0022, 0025, 0033, 0037, 0038,
0040, 0044, 0045, 0051 to 0058, 0059, 0060, 0068, 0075, 0077, 0103, 0115, 0126, 0129, 0130, 0159, 0162), plus
[`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md), ADR-0177 and ADR-0187. [`design_gate.py`](../../tools/design_gate.py) lists no group domain for these paths, so the
group-specific design (groups plan, GROUPS_AND_SPACES_UX) is the builder's list above.

Not read at this review: the `docs/ux-design/specs/` files
other than those in the builder's list (for example [`OMNIBAR_AND_NAVIGATION.md`](../ux-design/specs/OMNIBAR_AND_NAVIGATION.md), [`PRESET_THEME_PAIRS.md`](../ux-design/specs/PRESET_THEME_PAIRS.md)) and [`SCREEN_MAP_AND_JOURNEYS.md`](../ux-design/SCREEN_MAP_AND_JOURNEYS.md) were not re-read.

## What exists

Groups UI against the loopback harness. There is no host driver carrying group wires, so nothing real can be sent: the screens work on
`createGroupSim`, where the harness plays the other people. In the shipped build `groups.available()` is false and no group control shows.

- Create group (optional name, choose active chats, consent line), invitations (join / no thanks), group list rows with a people marker,
  group chat (sender labels, composer, paused state), people sheet (roster, add someone, remove, leave, history and proof notes).
- All steps go through the Action Registry as a new result kind `group`, direct origin only, then `client.groups.do` with the captured
  profile context (UI-10). The Omnibar "New group" entry captures the context and does nothing if it changed.
- Copy is plain words; it says nobody is added without saying yes, that a later joiner sees only later messages, that messages can be
  shown as proof of who wrote them, and that leaving or removing does not erase what others already have. No quantum-safety claim.

## Files changed (all uncommitted)

Modified: `apps/web/src/copy/en.ts`, `apps/web/src/copy/en.test.ts`, `apps/web/src/core/Shell.tsx`, `apps/web/src/core/shell.css`,
`apps/web/src/dev/main.dev.tsx` (dev-only seeded invitation and a test handle), `packages/om-ui/src/actions/registry.ts`,
`packages/om-ui/src/actions/registry.test.ts`, `packages/om-ui/src/conversations/ConversationList.tsx` (optional `marks`),
`packages/om-ui/src/index.ts`, `packages/om-ui/src/shell/primitives/icons.tsx` (`users`), [`machine/file_registry.json`](../../machine/file_registry.json).

New: `packages/om-ui/src/groups/{copy.ts,services.ts,hooks.ts,NewGroup.tsx,GroupInvites.tsx,GroupMembers.tsx,GroupChat.tsx,groups.css,index.ts,groups.test.tsx,__fixtures__/copy.ts}`,
`apps/web/e2e/groups.spec.ts`, and these two reports.

## Verification

- `vitest run` for the whole repo: 87 files passed (om-ui groups tests: create, accept, send and receive, add, leave with confirm,
  refusals, absent without an engine; registry tests for the group actions; `en.test.ts` leaf and jargon checks include the group copy).
- Chromium e2e on the harness build (`apps/web/e2e/groups.spec.ts`): passes at the default phone size and at desktop size, with
  screenshots; axe checks run in the unit tests.
- `eslint packages/om-ui apps/web` clean. `tsc -b` clean for these packages. `pnpm typecheck` still reports errors in
  `infra/cloudflare-home` (missing Cloudflare types) and `pnpm lint` reports errors in `om-core-wasm` and `om-provider-http` files; none
  of those files were touched here.

## Deviations

Checked against the design text above. Numbering continues the relay-status table (D1 to D29); these are D30 and D31.

| ID | Item | What was done | Needs |
|---|---|---|---|
| D30 | **Group steps are not `CoreIntent`s with an `OperationId` and outcome class.** [`UI_CORE_PLATFORM_CONTRACT.md`](../architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) "Core-facing UI model" (every mutation is a typed intent returning `Committed`, `PendingNetwork`, `PendingReconciliation`, ...; UI-12 and UI-13: one logical operation identity across timeouts and restarts) and [`ACTION_TO_CORE_INTENT_MAP.md`](../architecture/ux/ACTION_TO_CORE_INTENT_MAP.md) (rows exist for `remove-member` and `invite-to-room/space` only; none for create, accept, decline, leave or send in a group). | Core's group path is `client.groups.do(GroupIntent)`, not the `CoreIntent` path. The Action Registry therefore returns a new result kind `group` for `group-create`, `group-invite`, `group-accept`, `group-decline`, `group-leave`, `group-remove-member` and `group-send-text` (direct origin only; a suggestion is refused with `suggestion_cannot_confirm`). The result carries no `OperationId`; the step is durable in Core and the screen re-reads state afterwards. The authority boundary is unchanged: typed, parameter-checked, no string RPC, profile context captured (UI-10). | A map row for each group action and a decision whether group steps get `OperationId` and outcome classes (a Core change). The audited map text was not edited. |
| D31 | **Group screens poll every 2.5 s; no change feed.** UI_CORE_PLATFORM_CONTRACT "Rule" (Core produces "view state / event stream", the UI renders it); 1:1 screens use `client.subscribe`. | `GroupApi` has no subscribe, so `useGroups` and the group chat re-read after each step, on tab visibility and every 2.5 s while a group screen is shown. The read is local (Core state in the same page), not a network request, so [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md) "no polling loops when a push/wake path exists" (a relay-connection rule) is not engaged. | A `subscribe` on `GroupApi`; then drop `POLL_MS`. |

Not departures, after reading: the UI-level confirmation for leave and remove. [`ACCESSIBILITY_AND_INPUT.md`](../architecture/ux/ACCESSIBILITY_AND_INPUT.md) "Dialogs/sheets"
("destructive confirmation clearly identifies the action/subject") and [`RECONCILED_INTERACTION_MODEL.md`](../architecture/ux/RECONCILED_INTERACTION_MODEL.md) section 5 ask for exactly that friction,
and UI-11's Core-issued nonce covers a named list (`BlockPerson`, `SuspendDevice`, `RevokeDevice`, `ApproveDeviceLink`, `ChangePrivacyPolicy`, `StartRecovery`,
`VetoRecovery`) that does not include group leave or remove (`remove-member` in the map says "authority checked by current Room state"). A Core nonce for them would be an addition, not a requirement.

## Trailers used when committing

`Design-Read: client_endpoint_distribution, discovery_first_contact, relay_reachability, ux_accessibility_personalization` (all four domains read in full, see above).
`Design-Departures: D30, D31` (relay-status convention: ids from the deviation tables; recorded in this report, operator review pending).

## Update 2026-11-14: ADR-0187 accepted

The operator accepted [`ADR-0187`](../../decisions/ADR-0187-mls-engine-on-wasm-unforked-openmls-with-host-entropy-and-clock.md) on 2026-11-14; read any "proposed" wording above as superseded.
