# OM plugin frame engineering harness

## Design sources read

- [OM design gate](../../AGENTS.md), [agent architecture rules](../../docs/process/AGENT_ARCHITECTURE_RULES.md),
  [engineering entry point](../../docs/process/ENGINEER_START_HERE.md) and
  [constitution digest](../../docs/constitution/CONSTITUTION_DIGEST.md). Running
  `python3 tools/design_gate.py needs packages/om-plugin-frame` reported
  `no mapped code path: the design gate does not apply` and listed no documents. The same command with `--brief` gave
  the same result.
- [ADR-0259](../../decisions/ADR-0259-authorize-plugins-engineering.md),
  [ADR-0266](../../decisions/ADR-0266-plugins-wire-freeze.md),
  [ADR-0129](../../decisions/ADR-0129-extension-service-and-client-trust-boundaries.md),
  [plugin architecture](../../docs/architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) and
  [client and extension architecture](../../docs/architecture/client/CLIENT_AND_EXTENSION_ARCHITECTURE.md).
- The
  [spike's measured results](../../verification/prototypes/plugins-runtime-spike/README.md#measured-results-emeth4-2026-11-26),
  its browser frame, Worker, probes, embedder and server, and observations from the Chromium, Firefox, WebKit and
  preflight JSON.
- [Core PluginHost](../../crates/om-core/src/plugin_host.rs), the
  [TypeScript plugin client](../../packages/om-core-wasm/src/plugins.ts), its scope-engines export and the WASM command
  dispatch for the relayed operations. This checkout calls the factory `createPluginEngines`.

The full constitution originals, design material for other extension scopes and the native spike implementation were
outside this review. The spike results apply to the original harness only.

## Frame and app boundary

The package has no release entry point. The app-facing `mountPluginFrame` returns `null` unless explicitly enabled. Its
iframe uses `sandbox="allow-scripts"` and `referrerpolicy="no-referrer"`. The configuration names
`https://om-plugins.majalmirasol.com` and the exact reference app origin. Rebuilding takes the same configured app
origins for the frame admission check and the Pages `frame-ancestors` response header.

The spike used `allow-scripts allow-same-origin` on its separate-site iframe. Omitting `allow-same-origin` gives the
app-facing frame an opaque origin, so the app cannot establish the exact plugin-origin binding required by ADR-0129 and
ADR-0266. The app path fails closed and transfers no port. The loopback harness supplies a separate iframe with the
spike's measured sandbox attributes to exercise the broker and Worker with synthetic data. The broker uses an exact
`targetOrigin`, checks the browser-reported origin and iframe WindowProxy, and transfers one channel endpoint. The frame
accepts a transfer only from the exact configured parent origin and WindowProxy. The first sequenced port message
confirms that binding. Subsequent messages check the endpoint, binding, direction, sequence and bounded shape.
[HTML sandboxed origin flag](https://html.spec.whatwg.org/multipage/browsers.html#sandboxed-origin-browsing-context-flag),
[HTML cross-document messaging](https://html.spec.whatwg.org/multipage/web-messaging.html#crossDocumentMessages).

The static frame starts a dedicated classic Worker from its own site, with the same enforcing CSP on frame and Worker
responses. Neither response permits ambient connections or remote script or frame sources. The owner frame terminates
the Worker on a wall-clock deadline. The build-installed adapter can report its Wasm linear memory to Core. Core retains
capability, taint, egress, per-send confirmation, UI-tree and resource policy. The app supplies trusted facts and a
confirmation callback for the exact destination and content. The bridge returns verdicts only. It carries no
conversation plaintext, rendering, network execution, Account send or raw host-state API. The local message envelope is
engineering transport for the typed WASM adapter. W4 canonical CBOR remains a separate interface.

## Build and browser evidence

`pnpm build:frame` writes the Pages-format static assets to `dist/site/`. The built-in Worker entry runs an empty Wasm
smoke fixture. The hostile fixture bundle and engineering scope-engine WASM are separate from the Pages output.
[Cloudflare Pages static response headers](https://developers.cloudflare.com/pages/configuration/headers/).

On emeth4, use Node 24, pnpm 12.11.2, the repository's pinned Rust/Wasm toolchain and the browsers already installed for
Playwright 1.63.0. After the workspace lockfile integration in [SHARED_EDITS.md](SHARED_EDITS.md), run this from the
package directory:

```sh
pnpm test:browsers
```

The entry point builds the engineering-only scope WASM and launches Chromium, Firefox and WebKit headlessly. Each engine
overwrites its own `results/*.json` with statuses, raw host observations, engine exceptions, console diagnostics, CSP
events, control and confined sink receipts and the scope-WASM digest. A failure exits nonzero. The harness covers the
opaque-frame refusal path, wrong origin in the expected window, a separate window on another origin, forged bootstrap
and wrong first-port binding, oversized and unknown messages, taint and bilateral consent, an unconfirmed exact send, UI
node overflow, raw host state, runaway Wasm and linear-memory growth. The network cases reuse the spike's fetch,
WebSocket, XHR, remote `importScripts` and `sendBeacon` probes. Each requires a working local control when the API
exists and zero confined sink receipts. The frame also tests the beacon when the Worker API is unavailable.

## Measured on emeth4 (2026-11-26, Playwright 1.63.0)

All 15 hostile and boundary fixtures (opaque frame refusal, wrong and foreign origin, forged bootstrap and port, oversized
and unknown messages, taint, bilateral consent, unconfirmed send, UI overflow, host state, runaway Wasm, memory growth,
ungranted capability) pass on Chromium, Firefox and WebKit, each refused with the named plugin error. The network phase
(fetch, WebSocket, XHR, remote `importScripts`, `sendBeacon`) shows every confined request blocked on all three engines,
with a working control request reaching the local sink and no receipt for any confined request. Two engine differences
are recorded in the raw results: on Firefox, Playwright exposes no script context for the sandboxed probe frame, so the
network phase loads the same document with the same headers as a top-level page (`framed: false`), which is a weaker
stand-in for the framed case measured on Chromium and WebKit; and Firefox sends a Referer on the control Worker's
`importScripts` request despite `Referrer-Policy: no-referrer` (a request to the local sink, not by a confined plugin; the
other requests carry none). Worker `sendBeacon` is absent on all three.

The sections below describe the harness as built; the engine-difference observations quoted in them come from the spike.

This package had no browser measurements before this run. The prior spike observed that Chromium delivered a WebSocket error
event while Firefox and WebKit threw, Firefox returned `false` from the blocked frame beacon while Chromium and WebKit
reported it queued, and WebKit omitted Worker CSP events despite blocked requests. Worker `sendBeacon` was absent in all
three. The new harness preserves those observations as engine-specific outcome fields and compares them with socket and
HTTP receipts.

Local static checks cover source formatting and TypeScript. A passing three-engine run would give engineering evidence
for these synthetic fixtures and the measured sandbox control. Separate-site deployment, process isolation, real
third-party components, independent sandbox review, real grant and consent UI, aggregate memory and fuel enforcement,
and release gate V-010 remain open.

## Design questions

1. Which browser construction will establish the exact plugin origin while preserving the requested opaque iframe? The
   app mount denies the port until this is resolved.
2. What reviewed runtime will enforce the aggregate memory budget across all Wasm modules and binding allocations, and
   what compute budget semantics should replace fuel in the browser? This harness observes fixture linear memory and
   enforces wall-clock termination.
3. What lifecycle guarantees are required if the frame navigates or a port is transferred? The frame closes on page exit
   and the broker closes on navigation, teardown or deadline. Port delivery supplies no fresh document origin.
