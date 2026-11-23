# Plugins build plan (scope `plugins`, authorized by ADR-0259)

Date: 2026-11-23. Status: scope **authorized** on the operator's explicit instruction, 2026-11-23 ([`ADR-0259`](../../decisions/ADR-0259-authorize-plugins-engineering.md), Accepted). This is a plan; no product code for the scope exists yet. This is the largest waiver of the five scopes authorized that day: gate V-010 has no evidence recorded, so the plan puts the adversarial suite first and keeps the host unreachable until it passes.

## Design sources read

Read in full: [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), the `plugins` scope contract, the V-010 gate record, [`ADR-0259`](../../decisions/ADR-0259-authorize-plugins-engineering.md). The service-principal document, read for [`the services plan`](2026-11-23-services-bots-build-plan.md), also applies: an autonomous plugin is a `local_component` `ServicePrincipal`.

**Not read, and every slice must read before it builds** (`python3 tools/design_gate.py needs <slice paths>`): [`CLIENT_AND_EXTENSION_ARCHITECTURE.md`](../architecture/client/CLIENT_AND_EXTENSION_ARCHITECTURE.md), [`PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md`](../architecture/client/PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md), [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md), [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md), [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md), [`THEMES_FONTS_AND_PERSONALIZATION.md`](../architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md), [`PROTOCOL_EXTENSIBILITY.md`](../constitution/PROTOCOL_EXTENSIBILITY.md), ADR-0124 and ADR-0129 in full, the V-010 research note, and the UX brief. No V-010 prototype exists to re-run.

## 1. What the scope covers, per the design

"Extension" is six different things; this scope is the **local client plugin** (and the host side of the **mini-app surface**). Protocol extensions are schemas, content bundles are inert data (already the `expressive_media` scope), remote services are `ServicePrincipal`s, and provider adapters stay behind their narrow interfaces.

- **Format:** the WebAssembly Component Model with an OM-defined `om:plugin` WIT world. A plugin imports only granted capabilities, exports declared entrypoints, has no ambient authority, no shared OM memory, no database, key or DOM access. WASI is not inherited; each reviewed interface is granted individually.
- **Host capabilities** are narrow typed interfaces (`ui.*`, `content.read_selected`, `content.read_current_conversation`, `content.search_local_scope`, `compose.*`, `conversation.send_message`, `conversation.set_reaction`, `net.fetch_origin_scoped`, `storage.plugin_private`, `notifications.plugin_local`, `service.invoke`). Plaintext read, network egress, draft and send are separate grants.
- **Send is confirmed.** `conversation.send_message` shows the exact content and destination and needs the user's confirmation each time; only that is authored by the account. A plugin that must act without confirmation runs as a `local_component` `ServicePrincipal` with its endpoint credential sealed in the DeviceVault and never readable by plugin code, and its events are labelled as automation or carry a `RepresentationGrant`.
- **Taint:** any invocation that read conversation plaintext is tainted and gets no network egress without the conversation's `plaintext_egress` policy (in a 1:1, both participants'); the one exception is a one-shot user-confirmed selected-context disclosure; continuous egress is export and needs bridge authorization; taint follows data through `storage.plugin_private`.
- **PWA isolation:** plugins and mini-apps run in a sandboxed frame or worker on a **different registrable domain** than the ClientHost, with the ClientHost cross-origin isolated, every host-to-plugin message on a `MessageChannel` bound to the exact origin, and plugins holding any `content.*` capability **disabled** where the browser cannot give process isolation; never a same-origin fallback. Native hosts use a separate process or equivalent memory isolation.
- **Never given directly:** account or device private keys, MLS secrets, DeviceVault material, recovery secrets, update signing keys, unrestricted identity or governance transitions, filesystem, processes, FFI, raw DOM, or any way to suppress security warnings.
- **Host-owned UI:** declarative host-rendered surfaces (composer action, message action, command palette entry, isolated panel, settings page, returned cards and forms); plugins cannot draw over or hide identity warnings, bridge and plaintext-recipient indicators, automation and provenance labels, roster entries, device-link notices, permission prompts, destructive confirmations or recovery state.
- **Mini-apps:** a remote origin in host chrome that always shows its origin and service identity, receives only an explicit launch payload (no cookies, history, keys, or ambient chat id), cannot imitate OM security prompts, cannot occlude chrome, and treats any plaintext sent into it as egress.
- **Packages:** `PluginID`, `PluginControlState`, immutable `PluginPackageRelease` with a hashed manifest (components, ABI versions, entrypoints, requested capabilities, network-origin requests, resource limits); release authenticated by the current control state; permission upgrades never ride silently with code (existing grants are the ceiling, new ones stay disabled until approved); rollback protection with explicit local pinning.
- **Resources:** per-plugin memory, CPU or fuel, tasks, storage, network budget, background time and event rate; crashed or hung plugins are terminated without touching canonical state, which changes only through transactional host APIs.
- **Anti-fragmentation:** a plugin never defines shared-state meaning, an absent plugin never corrupts baseline state, incompatibility is surfaced rather than approximated, no catalog is mandatory.

## 2. Blocking gates and prerequisites

| # | Condition | State |
|---|---|---|
| 1 | Accepted ADR names the scope | Met: ADR-0259 |
| 2 | Gate V-010 pre-engineering | **Waived by ADR-0259 for engineering only.** All seven criteria are open with no evidence: prototype host on a native Wasm runtime and the PWA runtime with adversarial plugins, plaintext-read versus egress separation (including via plugin private storage), the per-send confirmation rule, PWA separate-domain isolation, the remote-origin mini-app test, the 1:1 consent rule, and the ADR-0156 wire freeze for the manifest, grants and plugin-to-host messages |
| 3 | Feasibility: Wasm sandbox and capability feasibility | **Open** (V-010 `feasibility_status` is open; it does not list the scope in `interface_impact`, so it does not block authorization, but it is the first thing slice 1 must answer) |
| 4 | `depends_on` | `reference_client_ui` authorized |
| 5 | Validator | Passes (12 of 12 scopes) |

Pre-release: external review of the sandbox host. ADR-0259 point 4 also requires all seven criteria met with evidence and the feasibility criteria evidenced before any release offers plugins.

## 3. What already exists

| Piece | Where | Gap |
|---|---|---|
| Content-pack and theme data pipelines (inert) | `crates/om-core/src/expressive`, theme code | Not executable and not part of this scope |
| Capability registry entries relevant to plugins (`room.set_plaintext_egress`, `room.install_service`) | [`spec/registries/om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) | Plugin capabilities not in any registry or WIT |
| ClientHost, DeviceVault, PWA shell | `apps/web`, `packages/om-web-platform`, `packages/om-ui` | No second site, no cross-origin isolation, no plugin host |
| Calls WebRTC code showing the pattern for a host-bound channel | `packages/om-web-platform/src/calls/transport.ts` | Different problem; reuse nothing without reading its gate |

No plugin host, WIT world, manifest or runtime exists in the repository; there is no V-010 prototype directory.

## 4. Slices

Each slice reads its design-gate output first, ends with `make check`, the touched tests and a commit with `Design-Read` and `Design-Departures`. Heavy runs go through `bin/lonunjie-heavy`. **No plugin host code is reachable in any build until slice 3 passes on both runtimes (ADR-0259 control).**

| Slice | Content | Done when |
|---|---|---|
| 0 | **Wire freeze and WIT.** CDDL, registered fields and labels, positive and negative vectors and two independently written codecs for `PluginPackageManifest`, `PluginControlState`, capability grants and the plugin-to-host message formats; the first `om:plugin` WIT world; capability ids registered additively | Both codecs agree on every vector; the WIT world contains no import that is not a named capability |
| 1 | **Feasibility spike (under `verification/`, non-production).** Pick the native runtime and the PWA runtime that can run Component Model plugins with fuel or time limits and memory caps; measure size, start-up and sandbox properties on Chromium, Firefox and WebKit; record failures honestly | A written feasibility report; this closes or fails the open V-010 feasibility criterion |
| 2 | **Adversarial plugin suite (tests only).** Hostile plugins that attempt: ambient imports, host memory reads, egress after plaintext read including through `storage.plugin_private`, sending without confirmation, capability escalation on update, resource exhaustion, security-surface occlusion, spoofed prompts; and the remote-origin mini-app cases | The suite exists and fails against a deliberately permissive stub, proving it can detect each attack |
| 3 | **Host core (sans-IO) and sandbox.** Capability broker, taint tracking, `plaintext_egress` checks including the 1:1 both-participants consent rule, per-send confirmation, resource limits, termination of crashed plugins, transactional host APIs; PWA site isolation and `MessageChannel` origin binding; plugins with `content.*` disabled where isolation is unavailable | The adversarial suite passes on the native and the PWA runtime; no same-origin fallback exists |
| 4 | **Packages and updates.** Manifest verification, control-state continuity, immutable releases, permission ceiling on update, rollback protection and explicit local pin | An update that requests more capabilities leaves them disabled until approved; a rollback is refused unless pinned |
| 5 | **Autonomous plugins as `local_component` services.** Reuse the service engine from the services plan: endpoint credential sealed in the DeviceVault under the plugin's purpose label, never readable by plugin code, automation labelling | A plugin can never produce an account-authored event without confirmation |
| 6 | **Host-owned UI surfaces.** The declarative component and event tree, the reserved security surfaces that plugins cannot cover, install and runtime disclosure for sensitive capabilities, plugin settings page | UX brief definition of done; Playwright on three engines |
| 7 | **Mini-app host.** Separate site, host chrome that always shows origin and service identity, exact-origin message binding, no imitation of host prompts, explicit launch payload | The remote-origin cases of the suite pass; host prompts render outside the frame |
| 8 | **Evidence and release gate.** All seven V-010 pre-engineering criteria recorded with evidence, external review of the sandbox host, fuzzing of manifest and message decoders under the agreed budget | Recorded; nothing claimed as passed that needs independent review |

Slices 0 and 2 are pure wire and test work and can start immediately on emeth4. Slice 1 decides which runtime slices 3 to 7 build on, so nothing past slice 2 starts until it reports.

## 5. Constitution rules that bind it

C-46 (shared meaning is never only in plugin code; an extension that changes interoperable state publishes schema, criticality, encoding and capability semantics independently), C-01 (no project-run plugin store or catalog), C-12, C-20, and the declarative-data, no-spoof, no-suppress rules for themes and extensions in [`AGENTS.md`](../../AGENTS.md). ADR-0259 adds: plugins stay disabled where site isolation is unavailable, never emit an account-authored event without per-send confirmation, and never obtain raw protocol or key state.

## 6. Questions for the operator or an ADR by delegation

| ID | Question | Safest default |
|---|---|---|
| Q1 | The PWA needs a **separate registrable domain** for the plugin and mini-app site. Which domain, and may it be hosted on Cloudflare Pages like the other sites? | This is an operator decision on a domain and a deploy; build against a test domain and do not enable anywhere until you name one |
| Q2 | Which Wasm runtime for the PWA and for native hosts | Whatever slice 1 shows passes the sandbox and budget tests; no preselection |
| Q3 | Do first-release plugins exist at all on the PWA if a browser lacks site isolation? | Disable `content.*` plugins there and say so; UI-only plugins may still run |
| Q4 | Resource limit defaults (memory, fuel, storage, network) | Smallest values that make the adversarial suite meaningful; record them in an additive ADR |
| Q5 | Is a plugin catalog in scope | No (C-01); direct install and share only |

## 7. Routing and budget

Slice 0 is well specified and fits a mid tier worker with a complete spec; slices 1 and 3 decide the sandbox and stay on the session model; slice 2 is a good mid tier task once the attack list above is the spec. The sandbox is security-critical; any change to its boundary needs the design-gate reading and review before commit. Run the usage gate from the lonunjie [`AGENTS.md`](../../AGENTS.md) before dispatching.

## 8. What this plan did not do

It wrote no code, ran no tests, edited no ledger and amended no ADR. Nothing in the Q-list changes audited text. Q1 needs your decision before anything in this scope can be turned on.
