# Performance UX Budgets

**Status:** Provisional project budgets for engineering validation; not claims about current implementation

## Principle

Performance is part of OM's interaction language. A self-revealing interface stops feeling natural when feedback arrives late enough that people repeat actions, lose context, or question whether the app heard them.

When budgets conflict, preserve:

> input responsiveness → readable/accessible state → semantic feedback → content continuity → decoration

## Reference budgets

These are initial targets to benchmark on representative baseline hardware. They may tighten with evidence; they should not be weakened merely to make tests green.

| Interaction | Initial UX target | Measurement intent |
|---|---:|---|
| keyboard/touch text echo | next paint; target ≤50 ms p95 app work | never wait on network/search |
| common local action feedback | ≤100 ms p95 | pressed/selected/optimistic state visible |
| cached chat open to first local content | ≤100 ms p95 after activation | shell/object continuity first |
| optimistic sent-message insertion | ≤100 ms p95 | transport acknowledgement is separate |
| hot Omnibar suggestions | ≤50 ms p95 | pins/actions/context |
| people/chat candidate batch | ≤75 ms p95 | local index |
| warm deep-search first batch | ≤150 ms p95 | progressive results; 1M history may require tiering |
| theme/profile switch visible update | ≤100 ms p95 | no long blank/repaint state |
| local media placeholder | immediate/next paint | preview may arrive later |
| local cached media preview | target ≤150 ms p95 | placeholder remains if decoding slower |
| warm app restore to usable cached shell | target ≤500 ms p75 | measure by platform |
| cold launch to usable cached shell | target ≤1500 ms p75 | provisional baseline-device target |

For web/PWA field responsiveness, OM should treat Core Web Vitals INP ≤200 ms at p75 as a **ceiling-level external health signal**, while high-frequency OM interactions target faster local feedback.

## Animation/rendering

At 60 Hz, missing the ~16.7 ms frame interval produces visible jank; higher-refresh displays have smaller frame budgets. OM therefore avoids fixed `16 ms` assumptions in architecture and measures against the active display/runtime.

During direct manipulation, prioritize pointer/touch tracking over shadows, blur, animated wallpaper, or other decoration.

## Loading strategy

- render local/cached structure first;
- preserve the object while remote/deep state resolves;
- use progressive search/result batches;
- lazy-load heavy expressive assets;
- defer nonessential analytics/diagnostics work;
- move deep indexing/search off the UI thread where platform permits.

## Graceful degradation

Under thermal/memory/CPU/network pressure, degrade in this order:

1. expressive effects/animated backgrounds;
2. blur/shadow/detail polish;
3. high-cost transitions while preserving semantic state change;
4. media preview quality/resolution where appropriate;

Never intentionally degrade:

- typing responsiveness;
- focus visibility;
- accessibility semantics;
- security consequence wording;
- message/draft continuity;
- basic send/retry controls.

## Evidence classes

Every performance claim records:

- device/runtime/browser build;
- dataset size;
- cold/warm state;
- percentile and sample count;
- battery/thermal caveats where material.

Container benchmarks are engineering evidence, not phone/PWA field evidence.
