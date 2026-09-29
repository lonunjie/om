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

## Measurement controls (ADR-0170 review of this plan)

These controls make runs reproducible and comparable. A run that violates one is recorded as invalid, not as a pass or
a miss.

1. **Sample sizes.** Cold start: at least 30 launches per device after one discarded warm-up launch. Warm start and
   input-to-next-paint: at least 200 interactions. Send-to-delivered/visible: 500 sends. Local search: 100 queries over a
   fixed query set. A reported p95 needs at least 100 samples; below that, only p50/p75 are reported.
2. **Device state.** Battery at or above 50% and not charging (except CI), screen brightness fixed, OS power saving off
   unless the scenario is a power-saving variant, and a 10-minute cool-down between cold-start series. Runs where the
   device reports thermal throttling are discarded.
3. **Clock alignment.** For two-device timings, each device records its offset to one local reference clock before and
   after the series; a run with any offset above 20 ms, or drift above 10 ms, is invalid. Offsets are stored with the raw
   samples.
4. **Realistic state.** Local search runs over the encrypted-at-rest index in the platform DeviceVault configuration the
   release ships, never a plaintext test index. Conversation fixtures use the same size distribution each run (checked in
   under `conformance/perf-fixtures/`).
5. **CI regression rule.** CI compares each emulated scenario's median with the previous `main` baseline and fails when
   it regresses by more than 10% or the bundle grows by more than 2%. Baselines move only in a commit that states why.
6. **No production telemetry.** Performance instrumentation is compiled only into measurement builds; release builds
   carry no performance telemetry, and lab result files never contain message content, identifiers or keys.
