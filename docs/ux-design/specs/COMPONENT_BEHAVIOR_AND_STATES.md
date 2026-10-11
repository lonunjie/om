# Component Behavior and State Contract

**Status:** normative visual/behavior baseline from Revision 8; complemented by Revision 9 framework-independent state-machine contracts.

The purpose of this specification is to stop OM's visual system from degrading into a collection of locally convenient widgets. Components may look different across OwnMessenger themes, densities, and platform dialects, but their meaning, state model, input coverage, and safety behavior stay stable.

## 1. Global component rule

Every interactive component must make these answers available without guesswork:

1. **What is it?** — object or action identity.
2. **Can I act on it?** — affordance/signifier.
3. **What state is it in?** — selected, focused, pending, failed, disabled, etc.
4. **What will happen?** — action semantics before activation where consequences matter.
5. **What happened?** — immediate feedback after activation.
6. **Can I recover?** — undo/retry/cancel where possible.

Color may accelerate recognition, but cannot be the only answer to any of these questions.

## 2. Chat row

A chat row contains, in priority order:

- identity/group signifier;
- display name;
- latest meaningful preview;
- time/relative time;
- unread/mention/draft/pending state when present.

Default ordering is recency-descending. OwnMessenger may reverse or use other explicit user-selected sorting modes.

### Required states

- ordinary;
- unread;
- mentioned;
- draft present;
- locally queued/pending;
- selected (multipane desktop/tablet);
- keyboard focused;
- pressed/active.

Unread must not be color-only. Use weight/count/marker plus optional accent.

A selected row and a keyboard-focused row are different concepts and must remain visually distinguishable.

## 3. Message bubble

The bubble is an object, not a decoration. It supports direct manipulation while always retaining menu/keyboard/accessibility alternatives.

### Required message states

- local draft (composer, not yet a message object);
- queued/pending;
- sent/accepted by local transport layer;
- delivered/read only where the protocol and privacy policy expose those states;
- failed;
- edited;
- reply-linked;
- selected/focused.

Pending is calm and neutral. Failure is explicit and recoverable:

`Failed · Retry`

with danger color as a redundant cue, never as the sole cue.

## 4. Composer

The composer owns the compact conversation bottom zone except when it semantically transforms into the Omnibar.

Properties:

- minimum comfortable target geometry around adjacent controls;
- draft survives temporary Omnibar invocation;
- attachment state remains visible and recoverable;
- Send becomes available only when the current payload is valid;
- voice-note recording and Omni voice command are visibly/semantically different modes;
- destructive draft loss should normally use recoverability before confirmation dialogs.

## 5. Omnibar result row

Every result exposes:

- primary label;
- result kind or equivalent icon/structure;
- optional contextual detail;
- explicit secondary verb where ambiguity would otherwise exist.

A bare person/entity result **opens/messages** the entity under the stable bare-entity rule. Calling, deleting, revoking, sharing, or other verbs require explicit verb selection.

Late-arriving results must not reorder underneath an active pointer or keyboard selection.

## 6. Buttons and icon controls

Icon-only controls are allowed when their meaning is well-established in context or an accessible label is available. Otherwise use icon + text.

Preferred compact-mobile hit area: **48 logical/CSS px**.
Normal lower bound for OM-owned high-frequency controls: **44 px**, even though WCAG 2.2 AA permits smaller targets in some cases.

States:

- rest;
- hover where pointer exists;
- pressed;
- keyboard focus;
- disabled;
- busy/pending where relevant.

Disabled controls must not be the only way to explain why an action cannot happen; the surrounding state should make the reason discoverable.

## 7. Focus and selection

**Focus** answers: "Where will keyboard/switch input go?"

**Selection** answers: "Which object is currently chosen?"

They may coexist. OM Calm reserves a violet focus geometry to keep focus distinct from cobalt action/selection color.

Reference target for custom focus indicators: at least the visual area of a 2 CSS-pixel perimeter and >=3:1 contrast for the focus change/adjacent boundary, consistent with WCAG 2.2 Focus Appearance guidance.

## 8. Sheets, menus, and contextual surfaces

Use a sheet/menu when the user remains conceptually in the same place. Use navigation when the user changes place.

A contextual surface must:

- preserve the underlying context;
- return focus to the invoking object when dismissed;
- close with Escape/back where appropriate;
- not cover the focused item without a recovery path;
- keep dangerous actions separated and explicitly labeled.

## 9. Invite / zero-contact state

The first-run empty state is an ordinary useful state, not an onboarding carousel.

Primary choices:

- share invite link;
- scan QR;
- show my QR.

No contact upload is requested by default. One-time/bounded invite options are progressively disclosed when the underlying protocol supports them.

## 10. Status language

Protected semantic families:

- cobalt/azure — ordinary action/information;
- violet — keyboard focus location in OM Calm;
- green — success/verified;
- amber — caution/warning;
- red — failure/destructive consequence;
- neutral — routine/pending/structural state.

Themes may remap exact values but not collapse these meanings into indistinguishable states.
