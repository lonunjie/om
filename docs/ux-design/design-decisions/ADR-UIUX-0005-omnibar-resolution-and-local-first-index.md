# ADR-UIUX-0005 — Stable Omnibar Semantics and Local-First Resolution

**Status:** Accepted design baseline  
**Date:** 2026-09-28

## Context

The Omnibar is intended to replace much permanent navigation for users who primarily message. It must find people/messages/settings and invoke actions such as calls or QR scanning from one fast surface.

This creates three risks:

1. personalization could make identical input do different things over time;
2. large message histories or network/AI dependencies could make the Omnibar slow;
3. typed/spoken natural language could bypass normal safety rules if treated as a separate command system.

## Decision

1. The Omnibar is a **local-first find/do resolver**, not an AI prompt box.
2. Bare entity queries have stable default semantics. For a known person, `Maria` opens/starts the conversation; `Call` remains explicit unless the user explicitly pins a call action.
3. Personalization may learn likely **targets and ordering**, but learned behavior does not silently substitute one verb/action for another.
4. Explicit action language such as `call Maria` or a pinned `Call Maria` resolves to the corresponding canonical Action Registry action.
5. Candidate generation is typed: entity, action, content, settings/navigation, invite/discovery, and other future registered classes.
6. Common actions/entities resolve from local state before message full-text, remote provider lookup, semantic enrichment, or AI interpretation.
7. Expensive search/index work is asynchronous/off-main-thread where the platform permits it, with progressive result batches and cancellation of stale queries.
8. An optional language model may propose candidates for complex wording but never bypass Action Registry safety, identity disambiguation, privacy policy, or confirmation requirements.
9. Voice transcription feeds the same resolver. PWA voice may not silently use server-based speech recognition without explicit privacy-consistent user choice.
10. Search indexes and preference-learning data are separate local data classes with independent lifecycle controls.

## Consequences

### Positive

- predictable bare-name behavior;
- fast common actions even with large histories;
- usable offline;
- no mandatory cloud AI/search dependency;
- personalization remains helpful without becoming surprising;
- typed, voice, keyboard, and future HID paths share one safety model;
- implementation can evolve from IndexedDB/simple indexes to SQLite/FTS or other engines without changing UX semantics.

### Costs

- local indexing consumes storage/CPU and needs retention/security integration;
- multilingual tokenization requires deliberate engineering and tests;
- natural-language interpretation must stay bounded and may sometimes ask for disambiguation;
- optional remote/AI enrichment must be merged without destabilizing local result order.

## Validation required

- bare-name vs explicit-verb comprehension;
- zero-query suggestion usefulness;
- one-handed mobile selection stability while results stream;
- keyboard/assistive-technology combobox behavior;
- message-history search performance at 1k/100k/1M+ local messages;
- multilingual fuzzy/full-text quality;
- cold/warm PWA performance;
- privacy behavior when learning and/or remote speech recognition is disabled;
- stale-query cancellation and late-result stability.
