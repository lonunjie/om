# Omnibar Benchmark — Revision 9

**Evidence class:** deterministic synthetic/container benchmark. **Not** human usability evidence and **not** mobile/PWA performance evidence.

Command:

```bash
python3 docs/design/ui-ux/validation/benchmark_omnibar.py \
  --sizes 100000 1000000 \
  --rounds 120 \
  --output docs/design/ui-ux/research/OMNIBAR_BENCHMARK_R9.json
```

Environment values are recorded in the raw JSON.

## Search results

| Messages | FTS5 build | SQLite size | Hot candidates p95 | Full-text/BM25 p95 |
|---:|---:|---:|---:|---:|
| 100,000 | 0.611 s | 13.38 MiB | 0.070 ms | 19.969 ms |
| 1,000,000 | 5.474 s | 136.62 MiB | 0.068 ms | 182.590 ms |

Interpretation:

- the hot people/action plane is not the bottleneck in this synthetic container run;
- a naive local FTS5/BM25 query remains fast at 100k messages;
- at 1M messages, p95 crossed OM's provisional `<=150 ms` first-full-text-batch target;
- this argues for progressive search, worker isolation, warm/hot tiers, query/index tuning, and real-device measurement rather than lowering the UX target prematurely.

Do **not** copy the absolute timings into a product promise. Browser WASM/OPFS, IndexedDB, native SQLite, device CPU, storage, language/tokenization, encrypted-store integration, and thermal state can materially change them.

## Ranking simulation

| Scenario | Ranker | Top-1 accuracy | Prediction switches | Post-shift accuracy |
|---|---|---:|---:|---:|
| stable | baseline | 0.5492 | 0 | — |
| stable | Bayesian | 0.5492 | 0 | — |
| preference shift | baseline | 0.5492 | 6 | 0.5167 |
| preference shift | Bayesian | 0.5466 | 2 | 0.5100 |
| sparse contexts | baseline | 0.3822 | 443 | — |
| sparse contexts | Bayesian | 0.4000 | 313 | — |

Interpretation:

- smoothing helped the deliberately sparse synthetic case and reduced prediction movement;
- it provided no gain in the stable scenario;
- in the preference-shift scenario, its stronger prior made it marginally slower to adapt;
- the differences are small and synthetic, so there is no basis to replace the simpler default ranker yet.

This supports OM's principle: **complexity must earn itself**.

## Research context

Chromium's Omnibox history provider searches an index of significant history, scores matching candidates, and returns a limited set of the most relevant results. OM borrows the general local-history/ranking pattern, not Chromium's browser-specific scoring semantics.

Adaptive-interface research also warns that adaptations can impose surprise/relearning cost. Todi et al. proposed a conservative adaptation policy that changes interfaces only when predicted benefit justifies it; OM applies the same caution to ranking stability rather than allowing aggressive UI rearrangement.

References:

- Chromium Omnibox History Provider: https://chromium.googlesource.com/playground/chromium-org-site/+/refs/heads/main/omnibox-history-provider/index.md
- Todi K, Bailly G, Leiva LA, Oulasvirta A. *Adapting User Interfaces with Model-based Reinforcement Learning* (2021 preprint): https://arxiv.org/abs/2103.06807
