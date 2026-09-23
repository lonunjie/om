# Performance Measurement Plan (V-022)

- **Status:** V-022 pre-engineering criterion 3 (measurement harness plan). Budgets: [`MOTION_AND_PERFORMANCE.md`](../architecture/ux/MOTION_AND_PERFORMANCE.md).
- **Principle:** the same scenario scripts run in CI (emulated, for regressions) and in the lab (real devices, for
  release claims). Only lab numbers support release claims.

## Reference devices (lab)

| Class | Device profile | Why |
|---|---|---|
| Low-end Android | current sub-USD-200 phone, 4 GB RAM, Chrome/WebView stable | most constrained target; the budgets are set here |
| Mid iOS | the oldest iPhone the release supports | Safari/WebKit and iOS background limits |
| Desktop | 4-core laptop, 8 GB | desktop PWA and native shells |

## CI emulation

Chromium through Playwright with CPU throttling ×4 and a "Fast 3G"-class network. It catches regressions and is not
evidence of absolute speed. Bundle size is checked exactly in CI.

## Scenarios and instruments

| Budget | Scenario | Instrument |
|---|---|---|
| Cold / warm start | launch with 50 cached conversations, offline and online | native: platform startup tracing (Perfetto, Instruments); PWA: `performance.mark` from navigation start to list interactive |
| Input to next paint | type 200 characters, open 20 conversations | PWA: Event Timing API (INP); native: frame-timing APIs |
| Scroll smoothness | fling through 10k messages | Perfetto/Instruments frame stats; Chromium tracing |
| Send → delivered, send → visible | two devices, same continent, through each reference Home (Cloudflare T1, static binary) | client timestamps from both devices on one NTP-synced clock; p50/p95 over 500 sends |
| Push wake → notification | 100 wakes with the app backgrounded | OS notification timestamp minus Home push-send log |
| Crypto per message / session setup | V-001 prototype on native + wasm32 now; the real core later | in-process micro-benchmarks, median of 1,000 runs |
| Codec per object | V-005 encoder on native + wasm32 | micro-benchmarks |
| Local search | 100k synthetic messages | query latency p95 |
| Idle battery | 24 h idle with push on, no traffic | Android `batterystats` app attribution; iOS Energy log |
| Memory | 50 conversations, steady state after 10 min | Android `dumpsys meminfo` PSS; Instruments; PWA `performance.measureUserAgentSpecificMemory` |
| Bundle size | production build | brotli-compressed byte counts in CI |

## Reporting

Each run writes `verification/results/perf-<device>-<build>.json` with raw samples, percentiles and the budget
verdict. A release note lists every budget with its measured value. A miss needs an ADR waiver (ADR-0115).

## Pre-engineering status

Crypto and codec costs come from the V-001 and V-005 prototypes (see their result files). The other budgets need
the real client, so this plan is the pre-engineering deliverable for them.
