# Omnibar Local Ranking and Adaptation

**Status:** Design baseline; algorithm intentionally replaceable

## 1. Goal

The Omnibar should become faster and more relevant with use without creating a surveillance system, a cloud behavioral profile, or an unpredictable AI gate between the user and ordinary commands.

The default ranking engine is therefore:

- local-first;
- deterministic for identical local state where practical;
- bounded;
- explainable enough to debug;
- optional;
- resettable;
- replaceable behind a stable ranking interface.

## 2. Priority order

Explicit user intent dominates learned preference.

Recommended precedence:

1. exact/strong query match;
2. explicit pins and user-defined shortcuts;
3. current-context relevance;
4. recent/frequent local behavior;
5. session continuity;
6. weaker learned preference;
7. optional remote/provider results.

Learning must not cause a weak semantic match to outrank a clearly intended exact match.

## 3. Zero-query suggestions

When the user focuses an empty Omnibar, suggestions may include locally ranked items such as:

- pinned people/chats/actions;
- recent conversations;
- recent actions;
- contextually likely actions;
- unfinished local tasks or draft-related continuations when appropriate;
- QR/invite actions when the account has no contacts/conversations;
- user-defined shortcuts.

These suggestions should remain useful even with learning disabled by falling back to pins, recency, current context, and static safe defaults.

## 4. Recommended v0 ranking model

Start with a transparent weighted baseline rather than a large model.

Conceptually:

```text
score(candidate) =
    query_match
  + explicit_pin
  + context_match
  + decayed_frequency
  + recency
  + session_continuity
  + optional_local_preference_prior
  - safety_or_noise_penalty
```

Weights should be measurable and versioned.

### 4.1 Bayesian use

Bayesian methods are appropriate where they improve stability under sparse local data, for example estimating a smoothed local probability of an action/person given context:

```text
P(intent | context, local_history)
```

A simple Bayesian prior/decayed-count model can be tested before more complex approaches.

### 4.2 Avoid stochastic exploration for ordinary commands

Contextual bandits or Thompson sampling can be useful experimental tools, but OM should not randomly reshuffle important controls merely to learn faster.

If exploration is ever used, it must be bounded to low-risk suggestion ordering and must never introduce uncertainty into destructive, security-sensitive, or identity-critical actions.

## 5. Safety constraints

Security-critical or irreversible actions should not appear as high-confidence zero-query predictions solely because they were used recently.

Examples include:

- revoke device;
- reset/replace identity;
- erase history everywhere;
- destroy recovery material;
- change critical trust policy.

Such actions may be searchable, but ranking and invocation must preserve the Action Registry's confirmation/authentication requirements.

## 6. Privacy model

Local learning data belongs to the user's trust boundary.

By default:

- event history used for ranking stays on-device/local OM storage;
- no behavioral ranking profile is sent to an OM-operated service;
- learning can be disabled without disabling the Omnibar;
- history used for learning can be cleared independently where practical;
- users can pin/override results manually;
- telemetry sharing, if offered, is separate and explicit.

Cross-device personalization, if later supported, must use a user-controlled encrypted synchronization design reconciled with OM's main architecture. It is not implied by enabling local learning.

## 7. Data minimization

The ranker should store the minimum information needed for usefulness.

Prefer aggregates such as decayed counts and timestamps over a permanent high-resolution behavioral event log when detailed history is unnecessary.

Retention and decay should be bounded so ancient behavior does not dominate current preference.

## 8. Fast-path requirement

The Omnibar must not wait for learning, network, or semantic enrichment before becoming usable.

Suggested staged response:

```text
focus/tap
  ↓
instant: pins + recent/contextual local results
  ↓
typed characters: local prefix/fuzzy results
  ↓
optional: local full-text / semantic index
  ↓
optional: remote/provider results
```

Late results merge without displacing an already obvious exact intent unexpectedly.

## 9. User controls

OwnMessenger should eventually expose understandable controls rather than model jargon:

- Learn from my actions: On/Off
- Pinned suggestions
- Clear learned suggestions
- Reset Omnibar ranking
- Sync personalization across my devices: optional/future, if architecture supports it

Advanced users may be allowed to inspect why a result ranked highly, but this is not required in the default UI.

## 10. Evaluation

Compare ranking strategies using local replay/simulation and usability testing.

Measure:

- time to intended result;
- keystrokes/taps to completion;
- top-result acceptance;
- correction/backtrack rate;
- stability from one session to the next;
- cold-start usefulness;
- latency p50/p95/p99;
- memory/storage footprint;
- behavior with learning disabled;
- behavior after preference shifts.

The winner is the method that reduces effort without making OM feel surprising or surveillant.

## 11. Personalization learns target preference without redefining action semantics

A ranking model may learn that `Maria` is more likely than other people when the user types `mar`. It must not learn that bare `Maria` means `Call Maria` merely because calls were frequent.

The verb/action comes from explicit query language, an explicit control, an explicit pinned action-target pair, or a stable entity default defined by [`OMNIBAR_BEHAVIOR.md`](OMNIBAR_BEHAVIOR.md).

This separation keeps local adaptation predictable and makes learned state safe to reset without changing the conceptual grammar of OM.


## 12. Revision 9 decision: baseline remains default

Revision 9 added a deterministic synthetic comparison between the transparent baseline and a Bayesian-smoothed contextual ranker. Bayesian smoothing modestly improved the deliberately sparse-context simulation and reduced prediction switches, but it did not improve the stable scenario and was marginally slower to adapt after a synthetic preference shift.

This evidence is insufficient to justify additional implementation complexity. The transparent recency/frequency/context baseline remains OM's default design baseline. Bayesian smoothing remains an optional experiment behind the same ranking interface.

See `research/OMNIBAR_BENCHMARK_R9.md` and `specs/OMNIBAR_BENCHMARK_AND_LEARNING_EVAL.md`.
