# Accessibility and Input Design

## 1. Principle

Accessibility is part of the interaction architecture, not a compatibility layer added later.

Essential functionality must be reachable through more than one modality wherever reasonably possible.

## 2. Supported modality classes

OM's action model should be compatible with:

- touch;
- mouse/trackpad;
- keyboard;
- stylus/pen;
- voice;
- screen readers;
- switch control;
- gaze/eye control;
- assistive pointer devices;
- external HID;
- future modalities.

Not every client must expose every modality, but no core semantic action should be designed in a way that prevents alternative bindings.

## 3. Reduced motion

Essential state must never be communicated only through motion.

For every semantic motion, define an equivalent lower-motion representation such as:

- direct state transition;
- opacity or emphasis change;
- icon/state marker;
- haptic cue;
- accessible announcement;
- persistent relationship indicator.

Reduced Motion affects rendering, not semantic meaning.

## 4. Color, sound, and haptics

Essential state must not rely on a single channel.

Examples:

- errors need more than red;
- unread state needs more than color;
- success needs more than a sound;
- destructive confirmation needs more than a haptic pattern.

## 5. Pointer and gesture independence

Common actions should not require a complex path gesture when a simpler activation is practical.

Gesture shortcuts may accelerate interaction, but visible/action-menu/keyboard/accessibility equivalents should exist for important functions.

## 6. Touch and pointer ergonomics

Touch targets need coarse-input tolerance. Desktop pointer targets may become visually denser while retaining discoverability and keyboard access.

Hover may enrich a desktop experience but must not contain essential information that is unavailable without hover.

## 7. Keyboard-first support

Keyboard interaction should support:

- complete focus traversal;
- direct message/thread navigation;
- common actions;
- escape/cancel semantics;
- command palette;
- shortcut discovery;
- remappable or configurable shortcuts where feasible;
- focus restoration after overlays and dialogs.

An optional expert keymap may exist, but platform-standard shortcuts should remain supported.

## 8. Voice

Voice commands are alternate invocations of registry actions.

Requirements:

- deterministic target resolution when possible;
- visible or audible confirmation for ambiguous target selection;
- stronger confirmation for consequential operations;
- graceful fallback where platform speech APIs are unavailable;
- no assumption that voice is private, quiet, or accessible to every user.

## 9. Future HID

Hardware-specific events must map through an adapter into canonical OM actions.

The core should not contain device-specific business logic.

## 10. Multimodal coexistence

Do not force users into one active input mode.

Touch, pointer, keyboard, and accessibility focus should coexist without destroying each other's context.

An iPad/tablet with keyboard and trackpad is a baseline scenario, not an edge case.
