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
