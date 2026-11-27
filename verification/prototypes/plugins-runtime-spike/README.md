# plugins-runtime-spike: V-010 feasibility spike (non-production)

Not product code and not for production. Written for `plugins` slice 1 (ADR-0259, build plan 2026-11-23). It answers whether
a plugin can run under a time or fuel budget and a memory cap, with no ambient authority, on a native host and in each
browser engine. Results below were measured on 2026-11-24 on a Linux build host.

## What was run

- `native/`: wasmtime 36 (Component Model, fuel, a resource limiter) running a component built from a hostile core module
  (`ok`, an endless loop, an unbounded `memory.grow`) plus a component that imports a function the host never provides.
- `browser/spike.mjs`: Playwright 1.63 (Chromium 153, Firefox 155, WebKit 26.6). The same hostile core module runs in a
  Worker with a host-provided `WebAssembly.Memory` (initial 1 page, maximum 64) and a watchdog that terminates the Worker.

## Results

| Property | Native (wasmtime, Component Model) | Chromium | Firefox | WebKit |
|---|---|---|---|---|
| Runs a plugin function | yes, 2 fuel units | yes, 42 in 9 ms | yes, 14 ms | yes, 7 ms |
| Instantiate cost | 6 to 24 microseconds, compile 2 ms | under 1 ms | 1 ms | 0 to 2 ms |
| Endless loop is stopped | yes, deterministic: traps when 5,000,000 fuel is used (2.3 ms) | yes, Worker terminated by watchdog at 800 ms | same | same |
| Memory cap | yes, resource limiter denied growth past 4 MiB | yes, growth stops at the host-set maximum (49 of 64 pages after 16-page steps) | same | same |
| Unprovided import | links fail | n/a (a module cannot name host APIs it was not given) | n/a | n/a |
| Component Model in the engine | yes | `WebAssembly.Component` is undefined | undefined | undefined |
| Page stays responsive after the loop | n/a (separate store) | yes | yes | yes |

Findings, recorded honestly:

1. **Native is feasible as specified.** Fuel gives a deterministic CPU budget and the limiter gives a memory cap; a plugin
   with no host imports has no ambient authority.
2. **No browser engine runs the Component Model natively.** A PWA host needs a component-to-core transpile step (for example
   a jco-style transpiler) in the build, which this spike did not run. This is the main open item for the PWA runtime:
   size, start-up and parity of the transpiled plugin are unmeasured.
3. **Browsers have no fuel.** The only CPU bound found is wall-clock termination of the Worker. That is a time limit, not
   a deterministic budget, so behavior differs between native and PWA hosts.
4. **A Worker is not an authority boundary by itself.** A Blob Worker has `fetch`, `XMLHttpRequest`, `WebSocket` and
   `indexedDB` in all three engines. Deleting those globals inside the Worker is not enough: `fetch` stays reachable through
   the prototype chain. The boundary that holds is that the plugin is a core module given only the imports the host
   chooses, with the host glue itself running without network access (for example a Worker or frame under a CSP that
   forbids connections). That configuration was not measured here and must be before the PWA runtime is accepted.
5. Memory caps for a browser plugin require the host to supply the memory as an import, as done here.

## Verdict

Feasible natively; feasible in the browser only for core modules behind a transpile step with a CSP-confined host and a
time-based limit. The V-010 feasibility criterion is therefore **partially met**: not fully closed until the transpiled
path and the CSP-confined host are measured. No independent review has been done.

## Reproduce

Run on a build host with Rust and Node: `cd native && cargo run --release` (writes `hostile-browser.wasm`), then
`cd ../browser && pnpm install --ignore-workspace && node spike.mjs ../native/hostile-browser.wasm`.

## PWA runtime measurements

**Design sources read.** The design gate returned no mapped code path for this prototype. The spike used the
[plugin architecture](../../../docs/architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md),
[client and extension architecture](../../../docs/architecture/client/CLIENT_AND_EXTENSION_ARCHITECTURE.md),
[ADR-0093](../../../decisions/ADR-0093-capability-sandboxed-plugin-extension-classes.md),
[ADR-0129](../../../decisions/ADR-0129-extension-service-and-client-trust-boundaries.md),
[ADR-0259](../../../decisions/ADR-0259-authorize-plugins-engineering.md),
[ADR-0266](../../../decisions/ADR-0266-plugins-wire-freeze.md),
[the slice 3 build-plan text](../../../docs/reports/2026-11-23-plugins-build-plan.md#4-slices), and
[the WIT world](../../../spec/wit/om-plugin.wit) introduced in `32f7b4d`. Repository instructions, the agent architecture
rules, the architecture contract, the constitution digest and the V-010 criteria were also consulted. The original
constitution documents and other architecture domains were outside this reading.

### Earlier local native run (Mac), 2026-11-26 (superseded by the emeth4 results below for the browser path)

Wasmtime 51.0.0 on arm64 macOS ran the component assembled from the WIT export declarations and the spike's core WAT.
`init` succeeded, `on-event` round-tripped empty and nonempty byte lists, and `render` preserved the UTF-8 input and its
error payload in separate calls. In the growth case, three one-page grows succeeded and five failed. Linear memory went
from one to four pages (256 KiB). The loop trapped after consuming the 5,000,000-fuel budget. Raw WAVE output and
process wall times are in [preflight.json](browser/results/preflight.json). These timings include CLI startup, parsing,
compilation, instantiation and the call.

### Measured results, emeth4, 2026-11-26

Linux x64, Node 24.15.0, pnpm 12.11.2, jco 1.40.0, Playwright 1.63.0, Wasmtime 51.0.0. The transpiled fixture (core 0.52 KiB, glue
about 130 KiB uncompressed) matched the native results on every call (init, empty and nonempty event, render, render error, memory
growth); the same component sha256 was used for both. Five fresh Workers per engine, medians:

| Engine | Version | Script load, ms | Compile and instantiate, ms | Parity | CSP `connect-src 'none'` | 500 ms termination | Origin binding |
|---|---|---|---|---|---|---|---|
| Chromium | 153.0.8010.12 | 7.3 | 1.7 | passed | fetch, WebSocket, XHR, importScripts, frame sendBeacon blocked | passed | passed |
| Firefox | 155.0 | 12.7 | 2.4 | passed | same | passed | passed |
| WebKit | 26.6 | 5.3 | 3.5 | passed | same | passed | passed |

`sendBeacon` is unavailable inside a Worker on all three engines (reported as `api-unavailable`); from the sandboxed frame it is blocked. Memory is
bounded only by the fixture's declared linear-memory maximum (one Wasm memory); the browsers give no fuel metering, so wall-clock
termination is the only compute limit. Raw records: [chromium.json](browser/results/chromium.json), [firefox.json](browser/results/firefox.json),
[webkit.json](browser/results/webkit.json), [preflight.json](browser/results/preflight.json).

**Coverage and limits.** One tiny hand-written fixture with no capability imports, not a real plugin: size and start-up numbers say nothing about a
realistic plugin, and jco glue size will grow with the WIT surface. Loopback origins on one host exercise the handshake and CSP, not
separate-registrable-domain deployment or process isolation. Headless Playwright builds only; no real phones. No independent review has been done.

**Feasibility.** On this fixture the transpile step and a CSP-confined Worker or frame behave as the design needs on all three engines, with
wall-clock rather than fuel limits. This closes the two items the earlier spike left unmeasured for a tiny component; it does not select a product runtime
and V-010 stays partially met until a realistic plugin and the production host are measured.

### Unattended measurement

From this spike's `browser/` directory, use Node 22 or newer, a Wasmtime CLI with component `--invoke` support, and
the browsers for Playwright 1.63.0 already installed. The package pins jco 1.40.0, its transpiler 0.21.0, Playwright
1.63.0 and pnpm 12.11.2. The local native measurement used the available pnpm 12.23.0. A transitive lockfile could not
be generated in the offline environment.

```sh
PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 pnpm install --ignore-scripts
pnpm measure
```

`pnpm measure --native-only` exercises the WIT-derived component in Wasmtime without JavaScript dependencies or
browsers. `pnpm measure --no-browser` attempts the transpile and invokes Wasmtime without starting Playwright. A full
run executes every engine, writes each engine's raw JSON even when a launch or check fails, and exits nonzero if any
required observation fails. The build and results use fixed paths. Non-browser runs preserve existing engine records.

- **Transpile and startup.** In a full run, Wasmtime and jco take the exact same component binary. The component uses
  no capability imports. The runner records raw and gzip byte counts for the component, jco files and the equivalent
  browser payload, including an ES module containing the core bytes. It records five fresh Workers per policy, with
  script loading, compilation and instantiation, and elapsed time from worker construction to readiness. The samples
  share a browser context. Parity compares success values, UTF-8/list marshalling and the error payload against the
  native results. [jco instantiation mode](https://bytecodealliance.github.io/jco/transpiling.html#instantiation)
- **Network policy.** Both the plugin frame and classic Worker receive an enforcing HTTP CSP. The confined phase uses
  `connect-src 'none'` for fetch, WebSocket, XMLHttpRequest and `sendBeacon`. The remote `importScripts` probe is governed
  by `script-src`, with the remote source omitted in the confined phase. The probes execute in the JavaScript host
  around the component, exercising CSP even if wrapper code attempts to use ambient APIs. The control policy explicitly
  permits a separate loopback sink origin. A blocked result requires a working control, no confined receipt at the
  sink and either an API failure or matching CSP violation. A beacon can report that it was queued without showing it
  was sent. The sandboxed frame exercises `sendBeacon` explicitly, and the Worker reports API availability separately.
  [Content Security Policy Level 3](https://www.w3.org/TR/CSP3/)
- **Resources.** The Worker invokes the same component's growth and loop cases. The growth case reports successful and
  denied `memory.grow` attempts at the fixture's declared maximum. This measures one Wasm linear memory. The frame
  calls `Worker.terminate()` 500 ms after it receives the loop-start signal, records elapsed time and any return or pong
  received before termination, then services another timer. The measured deadline starts at the signal, and timer
  scheduling varies by engine.
- **Origin binding.** A foreign-origin document in the expected iframe claims the plugin origin in its bootstrap message.
  The host checks the browser's `MessageEvent.origin` and the frame's `WindowProxy`, then withholds the port from the
  foreign origin. The genuine document receives the transferred port via exact `targetOrigin` and completes a sequenced
  exchange. Delivery uses the bound endpoint and sequence. The loopback origins exercise the handshake and CSP but do
  not establish separate-registrable-domain deployment or process isolation. The runner records `crossOriginIsolated`
  as an observation. [HTML cross-document and channel messaging](https://html.spec.whatwg.org/multipage/web-messaging.html)

### Aggregate browser host budgets

From this spike's `browser/` directory, use Node 24 and the pinned dependencies and Playwright browsers described above.

```sh
node aggregate.mjs
```

The runner applies the reference host's reservation code under [ADR-0268 item 4](../../../decisions/ADR-0268-v1-local-delivery-and-plugin-host-rulings.md).
It starts cohorts of one, two and four transpiled fixture components, each in a separate CSP-confined frame with its own Worker.
Four 8 MiB instances isolate the slot refusal on a fifth start. Three 16 MiB instances isolate the aggregate-memory
refusal on a fourth start. A separate 257-page fixture tests the per-instance refusal in an empty host. The fixture declares
the requested pages as both its initial size and its maximum. The reservation gate runs before frame or Worker creation.

Records include actual Wasm pages read from the core instance and wall time from the start request to the component's
`init` reply, including frame load, binding and Worker startup. The four-instance run measures each neighbour's fixture
ping before, during and after a loop. The frame's 500 ms wall-clock guard terminates the spinning Worker. A during-loop
reply counts when it arrives after the loop-start signal and before the frame records termination.

Chromium can expose `performance.memory.usedJSHeapSize` in window contexts. The runner samples it in the embedder
before and after cohort startup and in each initialized frame. The values cover browser-reported JS heaps with
potentially overlapping scopes and cannot be summed. Firefox and WebKit expose no corresponding heap metric through
this probe. Unavailable samples are `null` with a reason. The runner collects no browser process RSS.

The run writes `browser/results/aggregate-<engine>.json` for Chromium, Firefox and WebKit and exits nonzero on failed
checks. These are headless, loopback measurements of the fixture and shared budget code. Separate-domain deployment,
realistic plugins and low-end devices require their own measurements. Register the generated JSON files when adding them
to the repository.

### Open design questions

1. How will a PWA host enforce each plugin's memory budget across every core module emitted by the transpiler, including
   guest-defined memories and allocations in the JavaScript binding? The fixture's one declared Wasm maximum gives no
   measurement of that general enforcement.
2. How will the product host invalidate a bound `MessagePort` when an iframe navigates or a port is transferred? This run
   tests the initial transfer boundary. Port messages do not supply a fresh cross-document origin.

## Native gate on real components (2026-11-26)

`native/tests/gate.rs` compiles real Component Model components with Wasmtime 36, reads the imports Wasmtime itself reports,
and gates them through `om_core::plugin_host` (the product policy): ambient WASI imports (filesystem, sockets, environment)
are refused as unknown capabilities; an ungranted `conversation-send-message` import is refused next to a granted one; a
`content.*` plugin is refused where isolation is unavailable and accepted where it is; and a component whose core module
loops forever consumes its whole fuel allowance, traps, and the host terminates the plugin when the measured use exceeds its
configured budget (5 tests, emeth4, `cargo test --test gate` in `native/`). Coverage: the import gate and the fuel path on
the native runtime with components that declare imports but have no behaviour behind them; capability calls through a real
linker, taint across real calls, and a native process boundary are not exercised here. No independent review.
