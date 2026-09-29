# Omnibar Local Indexing and Performance

**Status:** Normative performance/privacy requirements with implementation candidates  
**Scope:** PWA-first local search, native adaptation, indexing lifecycle, latency budgets

## 1. Principle

The Omnibar must feel like part of the local application, not a network service.

**Common local find/do tasks must never wait for network access.**

Network/provider results, AI enrichment, or directory lookup may augment the list only after the local fast path is already useful.

## 2. Search planes

OM should maintain separate logical search planes so a slow corpus does not block a fast one.

### Plane A — hot shell state

Small, always-cheap data:

- Action Registry labels/aliases;
- user pins;
- recent entities;
- recent safe actions;
- current context;
- tab/settings destinations.

This plane should be available immediately when Omni mode opens.

### Plane B — entity metadata

Locally known:

- people/identities;
- conversations;
- groups/Spaces;
- account names;
- local display names/aliases.

### Plane C — full-text local content

Potentially large:

- message text stored locally;
- attachment/file names and metadata;
- link titles where already available locally;
- other explicitly indexed local content.

### Plane D — optional expensive/remote enrichment

Examples:

- provider/directory lookup;
- opt-in semantic search;
- remote history retrieval permitted by architecture;
- optional language-model interpretation.

Plane D must never be required to make the Omnibar responsive or useful.

## 3. PWA execution model

The PWA should keep expensive indexing/search work off the UI thread.

Recommended shape:

```text
UI thread
  │  query + cancellation token
  ▼
Omnibar search worker
  ├─ hot/entity index
  ├─ full-text index
  └─ optional local ranking
       │
       └─ streamed candidate batches → UI
```

IndexedDB is a broadly available browser-side structured store and is usable from Web Workers. It is an acceptable baseline persistence layer.

SQLite WASM backed by OPFS is a strong candidate for a richer local index where profiling justifies it. Current SQLite WASM documentation supports OPFS-backed databases and describes Worker-based configurations and different VFS concurrency/performance trade-offs. This package does **not** freeze SQLite/OPFS as the only implementation; the storage engine remains an engineering choice behind the search interface.

Native Android/iOS clients may use a native SQLite/full-text implementation or another suitable engine while preserving the same search semantics and benchmark contract.

## 4. Full-text retrieval baseline

If SQLite FTS5 or an equivalent engine is used, useful proven primitives include:

- token full-text search;
- prefix indexes for short incremental prefixes;
- BM25-style lexical ranking;
- snippets/highlighting;
- trigram/substring indexes where the size/performance trade-off is justified.

These are implementation tools, not product semantics. OM's UI ranking combines lexical relevance with typed result class, explicit pins, context, recency, and stable action semantics.

## 5. Fuzzy matching policy

Typo tolerance is most valuable for small identity/action vocabularies and should be bounded.

Recommended stages:

1. exact normalized match;
2. word/token prefix match;
3. full-text lexical match;
4. bounded typo/fuzzy match for people, chats, actions, and settings;
5. substring/trigram fallback where appropriate;
6. optional semantic expansion only when explicitly supported.

Do not run expensive edit-distance scans across the entire message corpus on every keystroke.

Short queries need special handling because trigram techniques are ineffective below their token width and broad fuzzy matching can create noisy results.

## 6. International text

OM must not assume English word boundaries or ASCII identifiers.

The index/search abstraction must support:

- Unicode normalization;
- case folding where linguistically appropriate;
- diacritic-aware matching policy;
- scripts without whitespace-delimited words;
- locale-aware tokenization where required;
- mixed-script names;
- emoji in names/content;
- right-to-left text.

Search quality for Filipino languages, CJK, Arabic, Indic scripts, Thai, and other non-English text must be part of validation rather than an afterthought.

## 7. Incremental indexing

The content index should track the locally available message store incrementally.

Events that may update indexes include:

- message received/sent/edited/deleted;
- conversation rename;
- contact/local alias change;
- attachment metadata change;
- local history import/removal;
- account removal;
- retention expiry.

Rebuilding the entire index should be a recovery operation, not normal startup behavior.

## 8. Cancellation and stale work

Every query generation receives an identity/cancellation token.

If the user types:

```text
m
ma
mar
mari
maria
```

stale work for `m`, `ma`, etc. must not block or overwrite results for `maria`.

The worker may reuse prefix/intermediate computation internally, but UI updates are accepted only for the current query generation.

## 9. Provisional latency budgets

These are **design targets to benchmark**, not claims that all hardware can meet them today.

On a representative supported mobile device with a warm application:

| Interaction | Provisional target |
|---|---:|
| Field accepts input / visual echo | next frame; never wait for search |
| Open Omni mode with hot suggestions | ≤ 50 ms p95 after surface is ready |
| Action/entity top matches after keystroke | ≤ 75 ms p95 |
| First local message-search batch | ≤ 150 ms p95 warm |
| First local message-search batch after cold worker/index start | ≤ 300 ms p95 target |
| Background enrichment | progressive; must not block the above |

If a device cannot meet the deeper-search targets, OM should still meet the interaction target by returning hot/entity results first and streaming content matches later.

The UI must never delay a keystroke to wait for a debounce timer. Query scheduling/debouncing is internal and cancellable.

## 10. Result count and virtualization

The Omnibar is a decision surface, not a full database browser.

- return a small best-result batch first;
- paginate/stream additional matches;
- virtualize long result lists;
- avoid rendering hundreds of hidden rows;
- let explicit category/scope expansion request more results.

This improves both speed and accessibility.

## 11. Hot-cache strategy

Keep a bounded in-memory cache for:

- Action Registry aliases;
- pins;
- recent/frequent entity IDs;
- display labels needed for zero-query suggestions;
- current-context actions.

The cache must contain only data the active client is authorized to expose and should be invalidated on account lock/removal as required by the security architecture.

## 12. Search-index privacy

A full-text index is sensitive derived user data because it can reveal message terms even if it is not the canonical message store.

Requirements:

- remain inside the local trust boundary by default;
- be partitioned by account/security context;
- be wiped when the corresponding local message data is securely removed, subject to the main storage architecture;
- never be uploaded as analytics or a ranking artifact by default;
- avoid storing more content than necessary for search;
- respect retention/disappearing-message deletion semantics once reconciled with the main architecture;
- avoid leaking plaintext snippets onto locked-screen or cross-account surfaces.

Encryption-at-rest and key handling must be reconciled with OM's main local-storage/security design before implementation; this UI/UX package does not invent a competing key architecture.

## 13. Learning-data separation

Search indexes and preference-learning state are different stores.

Turning off learned suggestions should be able to clear/disable behavioral ranking state without destroying the user's local message search index.

Conversely, clearing search history/indexes should not silently delete explicit pins or shell customization unless requested.

## 14. Offline behavior

Offline is normal.

The Omnibar should still support:

- local people/chats;
- local message search;
- settings/actions;
- QR creation/showing where locally possible;
- account-local operations whose preconditions are satisfied.

Actions requiring network show their unavailable/pending state only after resolution. The Omnibar itself remains responsive.

## 15. Benchmark corpus

Before freezing implementation, benchmark at least:

- small account: 1k messages;
- ordinary account: 100k messages;
- heavy account: 1M+ locally retained messages if OM supports that scale;
- thousands of people/conversations;
- multiple accounts;
- long multilingual message history;
- low-memory mobile device;
- cold start and warm start;
- installed PWA and browser-tab PWA modes.

Measure p50/p95/p99 latency, index size, memory, battery/CPU during indexing, incremental-update cost, and input responsiveness.

## 16. References

Implementation/background references:

- SQLite FTS5: https://www.sqlite.org/fts5.html
- SQLite WASM persistent storage / OPFS: https://www.sqlite.org/wasm/doc/trunk/persistence.md
- MDN IndexedDB API: https://developer.mozilla.org/en-US/docs/Web/API/IndexedDB_API

These references describe available mechanisms. They do not override OM's privacy, portability, or UX requirements.
