# Omnibar Behavior and Resolution

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Normative interaction design baseline  
**Scope:** Typed, spoken, keyboard, accessibility, and future-input use of the OM Omnibar

## 1. Purpose

The Omnibar is OM's universal **find/do** surface. It is not an AI chat box, a conventional search field with commands bolted on, or a replacement for direct manipulation.

It provides one fast place to:

- find people, conversations, messages, files, media, settings, accounts, and locally known resources;
- invoke canonical Action Registry actions;
- create or scan invites;
- start common tasks such as messaging or calling a person;
- reach capabilities that the user has chosen not to expose as permanent tabs.

The Omnibar must remain useful offline and with learning disabled.

## 2. Core semantic rule: learn targets, not verbs

Personalization may learn that **Maria** is a frequent or contextually likely target. It must not silently change what the bare target `Maria` *does*.

A bare entity query has stable semantics:

```text
Maria
  → primary: open/start conversation with Maria
  → explicit secondary actions: Call, Profile, other safe actions
```

An explicit verb changes the intent:

```text
call Maria
  → Call Maria

message Maria
  → Open/start conversation with Maria ready to compose

search Maria airport
  → Search relevant content scoped by the resolved intent
```

A user-created pin may intentionally bind an action and target, for example `Call Maria`, but learned behavior alone must not convert a bare person result into a different side effect.

**Rationale:** personalization should reduce target-selection effort without making the interface semantically unstable or surprising.

## 3. Candidate classes

Every result has a typed class. Candidate generation may be parallel, but the UI exposes one coherent list.

### 3.1 Entity

Examples:

- person/identity;
- conversation;
- group or Space;
- account;
- device when relevant to an explicit device/security query.

An entity result has one stable default action and may expose explicit secondary actions.

### 3.2 Action

Canonical Action Registry operations such as:

- start call;
- create invite;
- scan QR;
- mute conversation;
- switch account;
- open settings;
- show my QR.

Actions always resolve through Action Registry preconditions and safety metadata.

### 3.3 Content

Examples:

- message hit;
- file/media hit;
- locally indexed link or attachment metadata.

Activating a content result should take the user to the exact object and preserve enough context to understand the match.

### 3.4 Navigation/settings

A setting or view that is useful to reach directly, for example:

```text
privacy
  → Privacy settings

notifications
  → Notification settings
```

### 3.5 Invite/discovery

Explicit invite operations, local QR actions, and optional provider-directory lookup. Remote/provider results must be clearly distinguishable from local known identities and may not silently upload the query to a provider unless policy and user configuration permit it.

## 4. Zero-query state

Tapping/focusing an empty Omnibar should be useful immediately.

Candidate order is approximately:

1. explicit user pins;
2. contextually relevant safe actions;
3. recent/frequent local entities;
4. recent safe actions;
5. deterministic entry points such as Scan QR or Invite someone when useful.

Zero-query suggestions are intentionally small. The Omnibar is not a dashboard.

Security-critical, destructive, or unusual maintenance actions are not promoted solely because they were recently used.

## 5. Query interpretation

OM should begin with a deterministic, inspectable resolver rather than requiring an LLM.

A query passes through layered interpretation:

```text
raw input
  ↓
normalization
  ↓
explicit action/alias detection
  ↓
entity + content + settings candidate generation
  ↓
local lexical/fuzzy matching
  ↓
context + explicit pin + local preference ranking
  ↓
Action Registry validation / result rendering
```

Natural phrasing is supported through aliases and bounded parsing. Examples:

```text
ring maria
call maria
phone maria
  → start-call(target=Maria)

invite
invite someone
make invite
  → create-invite
```

Advanced prefix syntax may be added for power users, but ordinary use must never depend on memorizing prefixes.

A future local or remote language model may be an **optional candidate generator** for complex phrasing. It may not bypass the Action Registry, authorization, target disambiguation, confirmation requirements, or privacy policy.

## 6. Result presentation

Each row must answer three questions without requiring guesswork:

1. **What is this?**
2. **What will happen if I activate it?**
3. **Why is it here when that is not obvious?**

A result therefore has:

- a primary label/object;
- a type/context cue when useful;
- one obvious primary action;
- optional explicit secondary actions;
- match context/snippet for content results;
- local/remote/provider distinction when relevant.

Do not overload an entity row with many permanent action icons. Secondary actions may reveal on selection/hover or appear as a small, predictable set.

## 7. Bare-name behavior

For a resolved known person, the stable primary action is **open/start the conversation**. This satisfies both “I want to message this person” and the common expectation that selecting a person in a messenger opens the chat.

Calling remains one explicit step away:

```text
Maria                         Person
  Message/Open    Call
```

Typing or speaking `call Maria` promotes `Call Maria` as the primary action.

A user's explicit pin may expose `Call Maria` directly in zero-query suggestions without changing bare `Maria` semantics.

## 8. Ambiguity and disambiguation

OM must not guess through material ambiguity.

Examples:

```text
Maria
  Maria Santos — Family
  Maria D. — Work
  Maria — Local invite
```

The list should provide enough identity context to select safely without exposing unnecessary private metadata.

For explicit commands:

```text
call Maria
```

if multiple targets remain plausible, OM shows `Call …` candidates rather than choosing one silently.

Repeated user choice can improve ordering locally, but never remove the identity cues needed to disambiguate.

## 9. Stable list behavior

Fast dynamic ranking must not create a moving target under the user's finger, pointer, or keyboard selection.

Rules:

- explicit text match changes may reorder results as the query changes;
- once a pointer/touch press begins, the targeted row is frozen for that interaction;
- keyboard selection remains attached to result identity, not merely row number, across late merges;
- remote/provider or slow full-text results must not displace a strong exact local result unexpectedly;
- completed query results should settle quickly rather than continuously reshuffle as background evidence arrives.

## 10. Execution policy

The Omnibar distinguishes **suggestion**, **selection**, and **execution**.

- Observational/navigation actions may execute on activation.
- Reversible actions may execute on activation with normal feedback/undo.
- Calls may start on explicit activation, with immediate visible cancellability.
- Sending user-authored content must not occur merely because text was entered into the Omnibar; composition/dictation should create a draft unless the established Action Registry policy explicitly allows a safe send flow.
- Destructive/security-critical actions preserve their required confirmation/authentication.

The ranking engine cannot weaken execution policy.

## 11. Content search

Content results should be progressive:

1. conversation/person/action results appear first;
2. local full-text message/file matches follow as soon as available;
3. optional expensive semantic or remote search appears only when enabled and useful.

A content result opens to the exact match and visually or semantically identifies the matched region without losing conversation context.

Search is constrained by what the client is authorized and able to index locally. OM must not create a server-side plaintext search dependency merely to power the Omnibar.

## 12. Context scoping

The same Omnibar may operate globally or in an explicit scope.

Examples:

```text
Find or do…                  global
Search this chat…            conversation-scoped
Search this Space…           Space-scoped
```

Scope must be visible and reversible. Context may bias global ranking, but implicit context must not silently exclude valid global results.

## 13. Multiple accounts

By default, results are scoped to the active account unless the user has enabled cross-account search.

Cross-account mode must visibly label account provenance before activation. A result from another account may require an explicit account/context switch before an action executes.

No learned profile should merge accounts in a way that erases identity boundaries.

## 14. No-result behavior

A failed query should remain useful without pretending to understand more than OM does.

Examples:

- show close local matches;
- offer `Search messages for …`;
- offer explicit directory/provider lookup if configured;
- offer `Invite someone` when the intended person is absent;
- show matching settings/actions;
- preserve the query while switching scope.

Do not convert arbitrary no-result text into a web search or AI query without explicit user intent.

## 15. Accessibility model

On the PWA, the Omnibar should follow an accessible editable-combobox/search pattern with the input retaining DOM focus while the active result is communicated programmatically. Standard text-editing keys must remain native to the platform.

Screen readers should announce:

- field identity and scope;
- result count changes without excessive chatter;
- active result type and action;
- mode transitions between composer and Omnibar;
- confirmations or safety requirements.

The same semantic result/action model is exposed to native accessibility APIs on Android and iOS.
