# Mobile Shell State Model

**Status:** Selected design direction for prototype validation  
**Scope:** compact iOS, Android, and mobile PWA

## 1. Problem

OM wants the Omnibar in the lower reach zone by default because it is the universal find/do surface. A conversation also needs the message composer in that same high-value region.

Showing two persistent input fields would waste vertical space, weaken focus, and create a dangerous ambiguity: is typed text a message or a global command/search?

The shell therefore needs one clear owner of the bottom input zone at a time.

## 2. Candidate patterns considered

### Variant A — stacked persistent fields

```text
┌───────────────────────────────┐
│ conversation                  │
├───────────────────────────────┤
│ Omnibar: Find or do…          │
├───────────────────────────────┤
│ Message Maria…                │
└───────────────────────────────┘
```

**Strengths:** both are always visible and discoverable.  
**Weaknesses:** consumes scarce height, splits attention, increases mistyping risk, and visually overweights system chrome.

**Decision:** reject as compact-mobile default.

### Variant B — semantic composer/Omnibar morph

```text
Conversation state
┌───────────────────────────────┐
│ conversation                  │
├───────────────────────────────┤
│ ◉   Message Maria…        ➤   │
└───────────────────────────────┘

Omni state
┌───────────────────────────────┐
│ conversation remains context  │
│ ───────────────────────────── │
│ suggested / matched results   │
├───────────────────────────────┤
│ ×   Find or do…           🎙   │
└───────────────────────────────┘
```

A dedicated Omnibar control invokes Omni mode. The composer transforms into the Omnibar; it is not merely restyled while retaining message-send semantics.

**Strengths:** preserves bottom reachability, one field at a time, fast, spatially coherent, draft-safe, works with touch/keyboard/voice, and expresses the state change through semantic motion.  
**Weaknesses:** requires a recognizable Omnibar trigger and careful focus/state handling.

**Decision:** selected default.

### Variant C — floating Omnibar pill/button above composer

**Strengths:** immediate access and strong discoverability.  
**Weaknesses:** persistent obstruction, visual clutter, conflicts with media/reaction surfaces, and can feel like a floating assistant rather than part of OM's shell.

**Decision:** not default; may be explored as an OwnMessenger accessibility/preference option.

### Variant D — top-bar Omnibar access inside conversations

**Strengths:** keeps composer untouched and is conventional.  
**Weaknesses:** violates lower-reach priority on compact phones and is slower for repeated use.

**Decision:** acceptable secondary access point or large-screen adaptation, not compact default.

### Variant E — gesture-only pull-up / edge invocation

**Strengths:** extremely fast for trained users and visually minimal.  
**Weaknesses:** hidden, hard to discover, accessibility-poor if exclusive, and conflicts with system edge gestures.

**Decision:** accelerator only; never the sole path.

## 3. Selected compact-mobile state model

### 3.1 Root/list state

Default root:

```text
┌───────────────────────────────┐
│ conversations                 │
│ newest activity first         │
│                               │
├───────────────────────────────┤
│ Find or do…               🎙   │  ← Omnibar
├───────────────────────────────┤
│ user-composable tabs          │
└───────────────────────────────┘
```

The Omnibar is persistent because there is no competing composition field. Chats is the only default enabled top-level view; when it is the only enabled view, no redundant one-item tab strip is rendered. Additional user-enabled tabs cause the strip to appear.

### 3.2 Conversation state

On compact mobile, entering a conversation gives the composer bottom-edge priority.

The normal bottom tab row is hidden by default while inside a conversation. This preserves message space and prevents a three-layer bottom stack. Navigation remains available through back/context navigation and the Omnibar.

The composer exposes a dedicated Omnibar trigger. Its exact glyph is not frozen yet, but it must:

- be visually distinct from Search-only, Send, Attachment, and Voice-message controls;
- have the accessible label `Omnibar`;
- retain a stable meaning everywhere;
- support user-selectable leading/trailing placement where practical;
- remain reachable without requiring a path gesture.

### 3.3 Omni mode

Invoking the Omnibar from a conversation:

1. preserves the message draft, cursor, selection, reply target, and attachment staging;
2. transforms the bottom input surface from **message composition** to **global find/do**;
3. removes message-send affordances;
4. changes placeholder and iconography unambiguously;
5. displays local suggestions/results upward while retaining the conversation as visible context where space allows;
6. routes submitted actions through the Action Registry rather than the message-send path.

There must be no parser ambiguity between a message and an Omnibar command.

### 3.4 Return from Omni mode

Exit paths include:

- explicit close/back control;
- system Back where appropriate;
- `Esc` with a hardware keyboard;
- an optional learned gesture accelerator.

Returning restores the exact composer state that existed before Omni mode.

## 4. Semantic motion

The transition is a **Transform** primitive:

> composer → Omnibar → composer

Motion should communicate that the same valuable lower-reach region has changed purpose. It must not imply that the message draft was erased or sent.

Normal mode may use a short morph of field geometry, icon role, and result surface. Reduced-motion mode should use an immediate state change or restrained crossfade while preserving the same semantics.

OhMessenger themes may alter expressive qualities such as springiness or accent treatment, but may not blur the distinction between Composer and Omnibar.

## 5. Reachability and handedness

OM must not assume all people are right-handed or left-handed.

Defaults should favor common lower-reach ergonomics without placing critical controls permanently in a difficult upper corner. OwnMessenger may offer:

- leading/trailing primary-control preference;
- mirrored accessory placement where safe;
- larger target mode;
- persistent Omnibar trigger mode;
- external-keyboard-first profile.

Explicit user preference outranks inferred handedness. OM should not silently infer handedness from behavioral telemetry unless such inference is separately justified, local, understandable, and user-disableable.

## 6. Keyboard and voice

The state model is modality-independent.

Examples:

```text
Touch:      tap Omnibar trigger
Keyboard:   Cmd/Ctrl+K (default candidate)
Voice:      invoke Omnibar voice entry, then speak intent
Switch/HID: invoke registered `open-omnibar` action
```

A voice-message control in the composer and a voice-command control in Omnibar are semantically different and must remain visually/accessibly distinguishable.

Always-listening wake words are not required and should not be enabled by default.

## 7. Larger screens

The one-field-at-a-time rule is specifically a compact-shell default.

On tablets, foldables, landscape, or desktop widths, OM may keep a global Omnibar available in a rail/header while the composer remains visible because sufficient space exists to preserve distinction.

The underlying actions and state transitions remain the same.

## 8. Prototype acceptance criteria

A candidate implementation should pass all of these before the pattern is treated as final:

- users do not accidentally send Omnibar text as a message or vice versa;
- switching to Omni mode is reachable one-handed on common phone sizes;
- returning from Omni mode restores drafts exactly;
- the state change is understandable with motion disabled;
- screen-reader users receive explicit state/role changes;
- keyboard users can enter and exit without pointer use;
- voice-command and voice-message functions are not confusable;
- no network connection is required to open Omnibar or use local actions/search;
- common Omnibar actions feel no slower than navigating fixed tabs;
- mobile PWA, Android, and iOS preserve the same conceptual shell.

## 8. Concrete prototype baseline

See also [`MOBILE_SHELL_PROTOTYPE.md`](MOBILE_SHELL_PROTOTYPE.md), [`TABS_REACHABILITY_AND_SHELL_CUSTOMIZATION.md`](TABS_REACHABILITY_AND_SHELL_CUSTOMIZATION.md), and [`OMNIBAR_TRIGGER_AND_TRANSITION.md`](OMNIBAR_TRIGGER_AND_TRANSITION.md) for the selected low-fidelity screen hierarchy, trailing-edge account default, Omnibar trigger placement, adaptive placeholder policy, and first-install empty state.
