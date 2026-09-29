# OM Accessibility, Input and Inclusive Interaction

- **Checkpoint:** 2026-09-22 13:42 PHT
- **Baseline:** WCAG 2.2 AA minimum for shared web UI, with stronger product targets where practical

## Principle

Accessibility behavior belongs in shared `om-ui` primitives so it cannot diverge by shell. Native platform adapters add platform-specific accessibility integration where required.

## Mandatory baseline

- complete keyboard operability for every interactive feature;
- visible, high-contrast focus indicator; the reference authored indicator targets WCAG 2.2 Focus Appearance geometry/change-of-contrast guidance in addition to the AA baseline;
- semantic accessible names/descriptions;
- predictable focus movement after navigation/dialog actions;
- screen-reader compatible landmarks, lists, buttons, menus, dialogs and live regions;
- text scaling/reflow without loss of operation;
- no color-only state;
- reduced-motion support;
- dark and high-contrast modes;
- RTL/localization-ready layout;
- descriptive non-text media labels where applicable;
- captions/transcription hooks for calling/media;
- deaf-friendly visual call/ringing/mute/speaking-state indicators;
- touch targets larger than bare WCAG minimums for common actions.

## Product target sizes

WCAG 2.2 AA introduces a 24×24 CSS-pixel minimum target-size criterion with exceptions. OM targets **44×44 CSS px or larger for primary touch controls** where layout permits, with compact desktop controls allowed where pointer/keyboard precision is appropriate.

## Keyboard model

Use native HTML controls first. Custom widgets must implement expected ARIA Authoring Practices keyboard interactions and focus rules.

- Tab/Shift+Tab moves among components/actions.
- Arrow keys operate composite widgets where conventional.
- Escape closes dismissible layers and returns focus correctly.
- Focus must never disappear behind overlays or offscreen content.
- Selection and keyboard focus are separate concepts.

## Dialogs/sheets

- labelled purpose/title;
- initial focus chosen deliberately;
- focus trapped only while truly modal;
- return focus to the invoking control or logical successor;
- destructive confirmation clearly identifies the action/subject;
- no theme may alter dialog semantics.

## Conversation accessibility

Messages expose:

- sender;
- timestamp when requested/appropriate;
- edited/deleted/disappearing/view-once state;
- reply context;
- reactions;
- delivery state for the user's own messages;
- attachment type and usable alternative name.

Do not force screen readers to announce decorative bubble metadata repeatedly.

## Calls

Always provide non-audio communication of:

- ringing/connecting/reconnecting;
- microphone muted/unmuted;
- camera state;
- participant joined/left;
- dominant/speaking participant where useful;
- captions/transcription availability and state;
- a user-pinned participant/video tile remains pinned and is not displaced merely by active-speaker detection;
- captions/controls avoid covering the primary face/hands region when layout space permits, supporting sign-language use.

Future caption/transcription implementations should prefer local/on-device processing where feasible. Remote captioning or transcription sends other participants' speech to a third party, so it is a ServicePrincipal invocation under [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md): because it receives audio continuously it joins the call as a visible service participant, shown to **all** call participants in the call roster as able to hear the call, and is never enabled silently by one participant (ADR-0129).

## Themes/accessibility

User personalization never overrides explicit accessibility settings. High contrast, text size, reduced motion and known-good security-state rendering can supersede theme tokens.

## Testing

Every shared primitive should have automated and manual coverage for:

- keyboard interaction;
- focus restoration;
- accessible name/role/state;
- color contrast;
- text zoom/reflow;
- reduced motion;
- screen-reader smoke tests on representative platforms.

Accessibility regressions in shared primitives block release in the same way as core navigation regressions.

## CONFORMANCE-ACCESSIBILITY

The following are normative pass/fail cases for the registered W0 conformance suite. Passing these cases is evidence; it does not by itself authorize a scope.

| ID | Case | Pass condition |
|---|---|---|
| AX-01 | keyboard-only navigation | every interactive core action is reachable with visible focus |
| AX-02 | screen-reader semantics | messages, composer, security notices and dialogs expose meaningful names/roles/state |
| AX-03 | text scaling/reflow | reference layouts remain operable at required scale without hidden security controls |
| AX-04 | reduced motion | semantic state changes remain understandable with non-motion alternative |
| AX-05 | reserved security surface | theme/plugin cannot suppress or relabel required security/accessibility semantics |
| AX-06 | pinned visual participant | active-speaker changes do not steal a user-pinned video tile; keyboard/screen-reader control remains available |
| AX-07 | authored focus indicator | focus remains clearly visible under light/dark/theme/high-contrast surfaces and meets the stronger reference-product target |
