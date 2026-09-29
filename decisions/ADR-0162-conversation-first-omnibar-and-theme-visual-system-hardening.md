# ADR-0162 — Conversation-first shell, private adaptive Omnibar and hardened visual/theme semantics

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** Run 3 product/UI integration audit; reference-client design only
- **Amends:** ADR-0051, ADR-0053, ADR-0054, ADR-0056, ADR-0057, ADR-0058, ADR-0068, ADR-0148, ADR-0149
- **Does not authorize:** product engineering, `reference_client_ui`, calls, groups or any other scope
- **Consolidation (ADR-0166):** authored on a parallel line that never saw ADR-0159. It is adopted as a hardening layer on the ADR-0159 shell. §1 agrees with ADR-0159 §1 (no mandatory multi-destination tab bar; Chats/conversations are the default workspace; other destinations are user-composable). In §4, OM Clear/Paper/Iris/Graphite from ADR-0159 replace the proposed OM Calm/Evergreen/Warm Slate/Violet palettes.

## Context

The Run 3 audit found that the product/UI layer had strong security boundaries but still contained several internal contradictions and underspecified integration points.

1. The canonical UI document still required three persistent destinations — Chats, Calls and People — while the product direction had become conversation-first and the cross-app search/command surface had become the natural way to reach infrequent capabilities. Leaving both models alive would force implementation to choose between contradictory canonical navigation contracts.
2. Search/command-palette language did not specify how a learned/adaptive ranking model interacts with IdentityProfile isolation, external discovery, destructive commands or identity-verification labels. A browser-like history-aware surface is useful, but a messenger cannot let ranking become a cross-profile correlator, network query leak or pseudo-trust score.
3. Theme bundles allowed arbitrary `icon` assets while only prose prevented semantic icon spoofing. The theme logical schema also accepted path strings that included traversal-capable forms such as `../x` or absolute paths, leaving an avoidable archive-extraction hazard before engineering.
4. Theme capability said themes may set density and motion intensity while other canonical documents made density and reduced-motion explicit user ergonomic/accessibility preferences. Ownership was ambiguous.
5. The visual system named "semantic color" but did not define actual semantic roles, a strong default palette, icon geometry/meaning boundaries or how curated light/dark themes preserve functional color.
6. Call accessibility covered visual state and captions but did not preserve a user-pinned video participant against active-speaker layout changes, an important requirement for visual-language/sign-language use.

These issues can be closed without changing any protocol wire object, cryptographic authority or provider interface.

## Decisions

### 1. The default shell is conversation-first, not destination-tab-first

The reference client's persistent workspace is the conversation list/current conversation plus:

- a visible **Omnibar** entry;
- new-conversation action;
- profile/avatar entry for IdentityProfile switching and Settings.

The earlier requirement for a persistent Chats / Calls / People tab bar is superseded.

Calls and People remain first-class capabilities:

- call buttons live in conversation/person context;
- call events and missed calls remain visible in relevant conversation activity;
- complete call history is reachable through Omnibar/search/shortcut and may be user-pinned;
- contacts/requests/discovery are reachable from New conversation, Omnibar and Settings;
- users MAY pin Calls, People, Requests, Files or other secondary destinations to optional utility chrome.

A feature does not need permanent navigation to be first-class. Conversely, a conversation-first default must not make Calls/People undiscoverable: recognizable Omnibar suggestions and ordinary contextual controls remain visible.

[`NAVIGATION_AND_OMNIBAR.md`](../docs/architecture/ux/NAVIGATION_AND_OMNIBAR.md) is canonical for the shell.

### 2. Omnibar adaptation is local, bounded and separate from trust

The Omnibar combines local search, navigation and typed actions. It may learn within one IdentityProfile from explicit pins, match quality, successful selection frequency, recency decay and local context.

Hard boundaries:

- ranking state and local indexes are IdentityProfile-namespaced;
- by default adaptation needs compact ranking facts, not durable raw query history;
- simply focusing/typing locally does not contact a directory/search provider;
- external/unverified discovery is a visually distinct result class;
- ranking/popularity/mutual-contact signals never become identity confidence, verification or cryptographic continuity;
- profile-sensitive asynchronous Omnibar work captures immutable `ProfileExecutionContext`;
- exact/pinned matches remain reachable even when learned history prefers another candidate;
- users can remove one suggestion, clear/disable adaptation and pin explicit targets.

The Omnibar is recognition-oriented, not command-line-only. Common actions remain labeled/visible for users who do not memorize command names.

### 3. Adaptive ranking never bypasses authority or confirmation

A learned suggestion is only a presentation/ranking decision.

- Safe navigation may execute immediately.
- Send/call actions retain ordinary target/context checks.
- Destructive, recovery, account-control, provider-migration, permission and plaintext-egress actions retain the same typed Core intents and confirmation/authorization as their normal UI surfaces.
- Learning cannot make a destructive confirmation action the default simply because the user selected it frequently.
- Late asynchronous results cannot silently replace the keyboard-armed result just before activation.

### 4. Functional color and semantic icons are explicit visual-system contracts

[`VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md`](../docs/architecture/ux/VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md) defines:

- neutral, interaction, messaging and status color roles;
- stable `info / success / warning / danger` conceptual semantics;
- the reference default and curated presets are ADR-0159's OM Clear, OM Paper, OM Iris and OM Graphite (as consolidated by ADR-0166);
- text/non-text contrast targets and a stronger reference focus-indicator target;
- a 24×24 semantic icon family with consistent metaphors, RTL rules and accessible labels;
- the rule that message color never indicates identity verification/security level;
- media-asset separation between content and product/security chrome.

Color, icon shape, text and structure reinforce one another. Security/action state is never color-only.

### 5. Untrusted themes cannot replace semantic action/security icons

The older permission for theme icon "style" is narrowed.

- Theme `icon` assets are decorative only.
- A theme cannot bind an asset to a semantic icon ID or replace `send`, `delete`, `warning`, identity/security, recovery, permission, bridge/service or other action/state glyphs.
- Themes may color non-reserved semantic icon roles within contrast constraints.
- A future alternate semantic icon family is a reviewed client asset pack, not arbitrary theme content.

This closes a phishing/meaning-reversal path that prose-only semantic constraints did not make mechanically crisp.

### 6. Theme import has an explicit archive/path boundary

[`theme-bundle.logical.schema.json`](../docs/ux-design/theme-bundle.logical.schema.json) now restricts asset paths to normalized relative ASCII segments and records a per-asset declared byte length. The importer additionally rejects duplicate normalized paths.

Reference import semantics reject before extraction:

- absolute paths;
- `.`/`..` traversal segments;
- backslash aliases;
- NULs/invalid path forms;
- duplicate normalized paths;
- hash/declared-length mismatch;
- unbounded/decompression-bomb extraction.

Extraction is streamed into a dedicated sandbox and bundle activation is atomic. Platforms may impose stricter resource limits, but no conforming importer has an unbounded extraction path.

### 7. Density, motion intensity and alert policy are user-owned

Themes may provide spacing/radius styling, easing defaults and validated sound assets, but:

- Comfortable/Compact density is an explicit user ergonomic setting;
- reduced-motion and explicit motion intensity override theme defaults;
- notification/call alert policy is controlled by user/privacy/platform settings;
- a theme cannot silently make a required alert silent or re-enable an alert the user disabled.

This resolves the prior theme-capability contradiction without removing personalization.

### 8. Visual calling preserves user-pinned participants

In shared in-app call UI:

- a participant/video tile explicitly pinned by the user is not displaced solely by active-speaker detection;
- the pin/unpin control is keyboard/screen-reader operable;
- captions/controls avoid covering the primary face/hands region where layout permits;
- active-speaker indication remains available separately and non-audio state remains visible.

This is a UI/accessibility rule only; it does not change call membership or SFrame key semantics.

## Verification consequences

- `conformance:ui_intents` gains Omnibar profile-context and destructive-command cases.
- `conformance:ui_themes` gains traversal, semantic-icon, density/motion and alert-policy cases.
- `conformance:accessibility` gains pinned visual participant and stronger authored-focus cases.
- New bounded evidence validates the reference OM Clear contrast pairs, path rejection, profile-local ranking, stable armed selection and no destructive auto-execution semantics.
- V-021 remains open for its existing independent-review/pre-release obligations; ADR-0162 does not claim to satisfy platform implementation audits.
- V-022 remains the performance gate; Omnibar ranking/search must fit its interaction/search budgets when implemented.

## Consequences

- The default product matches the actual primary use case: messaging remains the persistent workspace.
- Calls/People stay first-class without permanent navigation tax.
- Adaptive navigation can become faster with use without creating central telemetry, cross-profile leakage or trust scoring.
- Theme/icon customization remains deep but loses the ability to redefine action/security glyph meaning.
- Theme extraction has a clear pre-engineering security boundary.
- The default visual language is concrete enough to prototype and test rather than merely saying "calm" or "semantic color".
- `design_open` remains 0. Engineering remains unauthorized.
