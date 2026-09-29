# Omnibar Benchmark and Learning Evaluation Contract

**Status:** normative evaluation contract, Revision 9.

## 1. Why benchmark separately

The Omnibar has two performance problems that must not be conflated:

1. **hot intent resolution** — people, conversations, actions, pins, settings;
2. **deep local content search** — potentially hundreds of thousands or millions of messages.

Hot intent must remain effectively immediate even when deep history is huge. Therefore deep search runs behind a progressive local pipeline and never blocks typing or the first useful suggestions.

## 2. Required benchmark planes

Measure separately:

- local input echo/paint (real client only);
- hot candidate generation;
- local people/conversation matching;
- full-text first batch;
- result merge/reorder stability;
- index build/update cost;
- storage bytes per message;
- memory pressure;
- cold start vs warm cache;
- low-end-device behavior.

The included Python harness measures candidate generation and SQLite FTS5 behavior in the container. It cannot measure browser rendering, OPFS/IndexedDB overhead, mobile flash behavior, or real PWA worker scheduling.

## 3. Dataset gates

Minimum synthetic stress sets:

- 100k messages;
- 1M messages.

Later real-device validation should include representative message lengths, scripts/languages, attachments metadata, edits/deletions, and account partitions. Synthetic English-only data is insufficient for release validation.

## 4. Ranking strategies under evaluation

### Baseline

Transparent recency/frequency/context scoring with explicit query and pins dominant.

### Bayesian-smoothed candidate

A small contextual posterior using decayed local counts plus a bounded prior. Its purpose is smoothing sparse local evidence, not hidden semantic interpretation.

Both strategies must obey:

- explicit verb/query > prediction;
- pins > inference;
- no stochastic movement of security-critical actions;
- target learning does not redefine verbs;
- local-only default state;
- resettable/disableable learning.

## 5. Metrics beyond top-1 accuracy

A more accurate model can still be worse UX if it constantly changes. Measure:

- top-1 target accuracy;
- keystrokes/taps saved;
- correction/backtrack rate;
- **prediction switches** under similar contexts;
- adaptation after a preference shift;
- sparse-context cold-start behavior;
- latency and memory/storage cost;
- behavior when learning is disabled.

This reflects HCI research showing that adaptive interfaces carry surprise/relearning costs; conservatism is a feature, not a failure to personalize.

## 6. Promotion rule

Do not ship the Bayesian model merely because it wins one synthetic score.

Promote a more complex ranker only if it shows a meaningful benefit across:

1. deterministic replay/simulation;
2. real-device latency/storage;
3. structured human tasks;
4. preference-shift scenarios;
5. predictability/stability measures.

If gains are small, keep the simpler model.

## 7. Current Revision 9 evidence

See `research/OMNIBAR_BENCHMARK_R9.md` and the raw JSON. The current container run indicates:

- hot local candidates are far below the UI latency budget in this environment;
- naive FTS5/BM25 over 100k messages is comfortably within the provisional first-batch target;
- the same naive path at 1M messages exceeds the provisional 150 ms p95 target in this environment;
- Bayesian smoothing improved stability and sparse-context synthetic accuracy modestly, but did not improve stable-pattern accuracy and adapted slightly worse after a synthetic preference shift.

Therefore Revision 9 does **not** select Bayesian ranking as the default. The transparent baseline remains the implementation baseline; Bayesian smoothing remains an experimental candidate.
