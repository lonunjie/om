# OM Motion, Responsiveness and Perceived Performance

- **Checkpoint:** 2026-09-22 13:42 PHT
- **Goal:** Telegram-class perceived fluidity without decorative or distracting motion

## Principle

> **Movement should explain, not decorate.**

Smoothness is primarily the result of low input latency, predictable spatial transitions, interruptible gestures, optimistic local updates, preloaded/cached content, and absence of layout shifts. Animation supports that model; it is not a substitute for performance.

## Motion rules

1. Every animation has a semantic purpose: acknowledge input, preserve spatial continuity, communicate hierarchy/state, or explain where content came from/went.
2. Animations never delay core actions.
3. Gestures are direct-manipulation: the surface tracks the pointer/finger and may reverse naturally.
4. Navigation animations are interruptible.
5. Avoid chained ornamental transitions.
6. Avoid unexpected movement after content is already readable.
7. Reduced-motion preference removes large spatial transitions and non-essential animation without removing state feedback.
8. Motion timing should be consistent across PWA/native shells because it lives in `om-ui`.

## Semantic motion tokens

Initial design tokens; tune through prototypes and device profiling.

```text
instant   0–80 ms     press/highlight/state acknowledgement
quick     120–180 ms  menu/sheet micro-transition
standard  180–260 ms  page/pane/element transition
slow      280–420 ms  rare large spatial transition only
```

Use semantic easing/springs rather than per-component magic numbers:

- `enter`: fast deceleration;
- `exit`: slightly faster acceleration;
- `move`: symmetric/physical spring;
- `gesture`: direct mapping while the gesture is active.

No spring should bounce enough to reduce readability.

## Interaction examples

### Send message

1. message appears immediately in local conversation state;
2. composer clears immediately after local acceptance;
3. bubble makes a short spatial/opacity transition from composer area;
4. delivery state changes quietly when transport/core confirms.

Network latency must not gate visible insertion.

### Open conversation

The selected conversation should visually establish continuity between list selection and detail pane/page. On small windows, the conversation may slide/fade from the list context; on wide windows, selection and detail update without unnecessary full-screen motion.

### Image/media viewer

Expand from the tapped thumbnail when possible. On close, return toward its origin. If origin is no longer visible, use a simple fade/scale rather than a fake trajectory.

### Back gesture

On touch platforms where appropriate, back navigation follows the gesture interactively. Cancelling the gesture restores the screen without state loss.

### Calls

Controls appear/disappear smoothly but never hide essential state (mute/camera/end-call) during active manipulation. Network-route changes are not visual events unless quality changes require user information.

## Performance budgets

### Measurable budgets (gate V-022, ADR-0115)

"Fast" is part of OM's mission, so speed is a gate, not a hope. These budgets are checked on **reference device
classes**. **Low-end Android** means a current sub-USD-200 phone with 4 GB RAM and Chrome/WebView. **Mid iOS** means
the oldest iPhone the release supports. **Desktop** means a 4-core laptop. Unless noted, figures are p75 on low-end
Android with a warm network. The same numbers apply to the PWA and to native shells.

| Budget | Target |
|---|---|
| Cold start to interactive conversation list (cached state, offline OK) | ≤ 2.0 s low-end; ≤ 1.0 s mid iOS/desktop |
| Warm start / resume to interactive | ≤ 0.5 s |
| Input to next paint (typing, tap, scroll), web INP | ≤ 100 ms p75, ≤ 200 ms p95 |
| Scroll smoothness in a 10k-message conversation | ≥ 99% frames on time at 60 Hz |
| Send to "delivered" tick, both online, same continent, via Homes | p50 ≤ 0.5 s, p95 ≤ 1.5 s |
| Send to visible on a foregrounded recipient | p50 ≤ 1.0 s |
| Push wake to notification shown (push available) | ≤ 3 s |
| 1:1 message encrypt + decrypt (per message, low-end, wasm) | ≤ 5 ms |
| New 1:1 session (PQXDH-class) establishment compute | ≤ 50 ms |
| Local search across 100k messages | ≤ 300 ms |
| Idle battery drain attributable to OM (native, push on, no traffic) | ≤ 2% per 24 h |
| Steady-state memory, 50 conversations | ≤ 150 MB low-end |
| PWA shell before first paint / all critical JS+wasm (compressed) | ≤ 200 KB / ≤ 1.2 MB |

Rules:

1. Budgets are pre-release criteria for `reference_client_ui` and `provider_reference`. A release that misses one
   needs an ADR waiver naming the budget, the measured value and the fix plan.
2. Pre-engineering, V-001 and V-005 prototypes record crypto and codec costs on native and wasm32. A budget
   that the prototypes show to be unattainable is revised by ADR *before* W0 unlock, not discovered after launch.
3. Budgets are measured in CI on emulated low-end profiles for regressions, and on real reference devices
   before each release.

### General targets

Product targets, not protocol guarantees:

- visible response to direct input within one rendered frame when local;
- maintain 60 fps on ordinary supported hardware; exploit higher refresh rates automatically where available;
- avoid synchronous crypto/storage work on UI thread;
- list virtualization for large histories;
- decode thumbnails and full media at appropriate sizes;
- precompute/layout message measurements when practical;
- keep first meaningful shell paint independent of network availability;
- render cached/local conversation state before synchronization completes;
- avoid blocking startup on provider health checks.

## Optimistic UI boundaries

Optimistic updates are encouraged for operations that can be locally staged safely:

- message creation;
- reactions;
- archive/mute;
- drafts;
- local read state.

Do not falsely present irreversible security/control actions as finalized before `om-core` commits/authorizes them. The UI may show **Pending** or **Waiting for approval**.

## Layout stability

- reserve predictable media dimensions from metadata;
- never move the composer because a delivery state changed;
- prevent late-loading avatars/fonts from shifting message geometry;
- preserve list scroll anchors when older messages are prepended;
- avoid notification bars that unexpectedly push content unless the state is genuinely blocking/actionable.

## Reduced motion

Respect platform/browser reduced-motion preference by default. User may additionally choose motion intensity as an explicit OM accessibility/appearance preference. Theme motion values are defaults only and cannot override that user choice.

Reduced mode should:

- replace large slides/zooms with fades or immediate state changes;
- remove parallax/background animation;
- retain focus/state acknowledgement;
- retain progress indication when needed;
- avoid animated Omnibar/result reordering; preserve the currently armed keyboard target while results update.

## Instrumentation

Performance telemetry should be local-first and opt-in if exported. Development builds should measure:

- input-to-paint latency;
- long tasks;
- animation dropped frames;
- cold/warm startup;
- conversation open latency;
- scroll jank;
- memory pressure;
- media decode latency.

No production analytics dependency is required for OM to work.

## CONFORMANCE-PERFORMANCE

The following are normative pass/fail cases for the registered W0 conformance suite. Passing these cases is evidence; it does not by itself authorize a scope.

| ID | Case | Pass condition |
|---|---|---|
| PF-01 | cold start/reference devices | recorded budget is met or release gate fails |
| PF-02 | open large conversation | latency/memory stay inside published budget |
| PF-03 | send/encrypt fanout | crypto+serialization cost remains within W0 budget on native and wasm32 |
| PF-04 | background idle | battery/network wake budget is not exceeded |
| PF-05 | bundle/install size | shipped reference client stays within declared size budget |
