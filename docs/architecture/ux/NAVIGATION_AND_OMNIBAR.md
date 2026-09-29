# OM Conversation-First Navigation and Omnibar

- **Checkpoint:** 2026-09-29 (parallel Run-3 line; consolidated by ADR-0166)
- **Status:** normative Omnibar/navigation safety boundaries (ADR-0162), layered on the ADR-0159 shell
- **Scope:** global search/action entry, local adaptation, profile isolation and navigation safety

> The shell's presentation model is ADR-0159's: Chats is the only default compact destination and Calls, People,
> Spaces, Publications and Files are user-composable destinations (see [`UI_UX_SYSTEM.md`](UI_UX_SYSTEM.md) and
> [`OMNIBAR_AND_NAVIGATION.md`](../../ux-design/specs/OMNIBAR_AND_NAVIGATION.md)). "Conversation-first" below means the
> same thing: no mandatory multi-destination tab bar. Where wording differs, ADR-0159 governs presentation and this
> document governs Omnibar privacy/safety.

## Principle

OM is primarily a messaging workspace. The default shell therefore keeps the user's conversations and current conversation as the persistent product surface rather than forcing routine messaging through a set of destination tabs.

> **Conversation first; everything else is reachable without becoming permanent chrome.**

Calls and people remain first-class capabilities. They are not required to occupy persistent navigation merely because they are important.

## Default shell

The reference client has one default persistent workspace:

- conversation list / message requests;
- selected conversation when window width permits;
- **Omnibar** entry for people, conversations, local content, settings and actions;
- new-conversation action;
- profile/avatar entry for IdentityProfile switching and Settings.

There is **no required persistent Chats / Calls / People tab bar** in the default configuration.

### Calls

- Start voice/video calls from the conversation/person context or from an explicit Omnibar action.
- Call events and missed calls appear in the relevant conversation timeline/activity state.
- A complete call-history surface remains available through the Omnibar, search, keyboard command and an optional user-pinned shortcut.
- Active-call UI remains a first-class surface and may be system-owned when the platform requires it.

### People

- Start a new relationship/message from New conversation or the Omnibar.
- Contact/request management remains a complete surface reachable from those entry points and Settings.
- People/contact management does not need permanent navigation chrome after the user has established conversations.

### Optional pins

Users may pin secondary destinations such as Calls, People, Requests or Files to an optional rail/toolbar. Pins are explicit presentation preferences. They do not change product/security semantics and are never required for feature discoverability.

On compact layouts, the reference default favors the Omnibar + new-conversation action over a three-destination bottom bar. If user research later shows a discoverability regression, an ADR may adjust the visible shortcut treatment without changing the conversation-first information architecture.

## Omnibar

The Omnibar unifies recognition-oriented navigation and expert shortcuts. It is not a text-only command line.

At rest it can expose a small, recognizable set of useful suggestions such as:

- recent conversations;
- pinned people/conversations;
- New message / Scan QR / Share my OM;
- recent missed call;
- settings/help shortcuts appropriate to current context.

As the user types it can match:

- conversations and accepted people;
- locally indexed messages/files, subject to local privacy settings;
- settings/help;
- typed commands/actions;
- explicit discovery inputs (`om:` invite, QR result, FQDN/organization identity);
- later organization-directory discovery where the configured provider permits it.

The result presentation distinguishes **local/known relationship**, **local history**, **command**, **setting**, and **external/unverified discovery** classes. Ranking must never be shown as identity/security confidence.

## Local adaptive ranking

OM may learn the user's navigation habits, similarly in spirit to history-aware omniboxes, but the learning surface is deliberately narrower and more private than browser history.

### Allowed signals

Within one IdentityProfile, ranking may use:

1. explicit user pin/favorite;
2. query match quality;
3. successful prior selection frequency;
4. recency with decay;
5. current local context (for example, the open conversation or current settings section);
6. local unread/missed state only as a clearly labeled state signal, not engagement optimization.

Exact/prefix and explicit pins outrank weak learned frequency. A never-used but exact match must not become undiscoverable merely because frequently used items dominate history.

### What is learned

By default the adaptive store keeps compact ranking facts (entry identity, selection count/weight, last-selected bucket and optional pinned state), not a durable raw log of everything the user typed. Raw query history is not required for adaptation.

The user can:

- remove an individual learned suggestion;
- clear learned ranking for the current IdentityProfile;
- disable adaptive ranking while retaining ordinary search;
- pin an item so it is not displaced by learned ranking.

### Hard privacy/security boundaries

- Adaptive state is **IdentityProfile-namespaced** and never pooled across profiles.
- Adaptive state is local/account-private presentation state, not public protocol metadata.
- The ranking store is not uploaded to a directory/search provider merely to improve ranking.
- Provider popularity, mutual-contact count and ranking score never imply verified identity or cryptographic continuity.
- A profile switch cannot reuse another profile's learned candidates, counts, raw local search index or pins.
- Sensitive clipboard/deep-link/search actions capture immutable `ProfileExecutionContext`; completion after a profile switch remains bound to the originating profile or is cancelled.
- External discovery/network queries are visibly distinct from local results; simply focusing or typing in the Omnibar does not silently disclose the query to a network provider.

## Command safety

The Omnibar can surface commands but does not bypass the typed Core-intent boundary.

- Safe navigation may execute immediately.
- Message/send/call actions require the same target/context checks as their ordinary UI controls.
- Destructive, account-control, recovery, provider-migration, permission or plaintext-egress actions never auto-run from ranking/autocomplete and retain their normal confirmation/authorization flow.
- Learned frequency cannot reorder a destructive confirmation so that the destructive choice becomes the default keyboard action.
- Suggestions never imitate OS permission/passkey/biometric prompts.

## Recognition before recall

Keyboard users may type concise commands, but novice use must not require memorizing command names. Common actions remain visible as labeled suggestions and ordinary controls. `Cmd/Ctrl+K` focuses the same Omnibar surface rather than opening a separate expert-only command system.

## Adaptive layouts

- **Compact:** conversation workspace + Omnibar entry + new-conversation action; selected conversation replaces the list and ordinary back returns to it.
- **Medium:** conversation list and/or detail depending available width; Omnibar stays available.
- **Expanded/Wide:** conversation list + detail; optional inspector; optional user-pinned utility rail; Omnibar is keyboard-first but remains visually discoverable.

Resize/orientation changes preserve selection, draft, scroll anchor and Omnibar query state where doing so does not cross IdentityProfile boundaries.

## Accessibility

- Omnibar is a conventional accessible combobox/listbox (or platform-native equivalent), not a custom focus trap.
- Result class, primary label and relevant secondary context are announced without repetitive metadata.
- Learned rank is not conveyed by color alone.
- Keyboard selection remains stable while asynchronous local/network result classes arrive; a late result must not move under the user's imminent Enter/Return action.
- Reduced motion disables animated reordering; list order changes are immediate/stable or announced where needed.

## CONFORMANCE-UI-NAVIGATION

| ID | Case | Pass condition |
|---|---|---|
| NV-01 | fresh install/default shell | messaging/conversation workspace is usable without entering Calls/People tabs; no persistent three-tab dependency |
| NV-02 | adaptive ranking in profile A | only A-local ranking state affects order; profile B's ranking/query history has no effect |
| NV-03 | exact match versus learned frequency | exact/pinned relevant target remains reachable and is not buried by weak frecency |
| NV-04 | external discovery query | local typing does not contact a provider until the user explicitly enters/accepts external discovery behavior |
| NV-05 | destructive command suggestion | command retains ordinary confirmation/Core authorization and is never auto-executed/defaulted by learning |
| NV-06 | asynchronous result arrival | focused/armed result does not silently change before activation |
| NV-07 | clear/disable learning | ranking facts are deleted/ignored while ordinary deterministic search continues |
| NV-08 | call/people reachability | calls, call history, requests and contact management remain directly reachable without permanent destination tabs |
