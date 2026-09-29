# Omnibar Voice and Keyboard Interaction

**Status:** Normative modality baseline  
**Scope:** Hardware keyboard, command-style use, speech input, accessibility interaction

## 1. Same intent model

Keyboard and voice do not receive separate action implementations.

```text
keyboard text / shortcut
voice transcript
accessibility input
pointer/touch
        ↓
   Omnibar resolver
        ↓
 canonical entity/result or Action Registry action
```

Modality changes how an intent is expressed, not what that intent means.

## 2. Keyboard opening

Default desktop/hardware-keyboard accelerator candidate:

- `Cmd+K` on Apple platforms;
- `Ctrl+K` elsewhere.

A `/` accelerator may optionally focus the Omnibar when focus is not already in an editable field and the platform/keymap allows it.

OwnMessenger may remap accelerators, but OM should ship with a familiar default and avoid stealing established OS/browser shortcuts without good reason.

## 3. Keyboard result navigation

Baseline:

- typing edits the query using normal platform text-editing behavior;
- `Down Arrow` / `Up Arrow` traverse suggestions;
- `Enter` activates the selected result;
- `Escape` first dismisses the result popup/state and then exits Omni mode according to context;
- `Tab` follows the platform/accessibility pattern and must not be repurposed in a way that traps focus;
- secondary actions are reachable through keyboard focus/action menus without requiring pointer hover;
- selection is anchored to result identity during async updates.

Do not override standard text-editing shortcuts while focus is in the field.

## 4. Web accessibility pattern

For PWA, an editable combobox/searchbox pattern is the appropriate baseline for a text field with a suggestion popup.

DOM focus should normally remain in the text input while the active suggestion is communicated using the platform's accessible active-descendant/list semantics. Escape should allow dismissal without committing a suggestion.

The final implementation must be tested with real browser + assistive-technology combinations; examples from ARIA Authoring Practices are guidance, not a substitute for compatibility testing.

Reference:

- W3C ARIA Authoring Practices, Combobox Pattern: https://www.w3.org/WAI/ARIA/apg/patterns/combobox/

## 5. Power-user behavior

The Omnibar is the keyboard-native command surface for OM.

Examples:

```text
Cmd/Ctrl+K
call maria
Enter

Cmd/Ctrl+K
privacy
Enter

Cmd/Ctrl+K
mute family until tomorrow
Enter
```

Users should not need a separate command-console feature to reach ordinary OM actions.

Optional future expert syntax may provide terse filters or command aliases, but the visible natural-language/action vocabulary remains canonical.

## 6. Voice entry point

The microphone inside Omni mode means **speak a find/do request**.

It is intentionally distinct from the message composer's voice-note microphone.

Examples:

```text
“Call Maria”
“Message Ramon”
“Find the message about Clark airport”
“Scan an invite”
“Open privacy settings”
```

OM should provide immediate state feedback that it is listening for a command rather than recording a message to be sent.

## 7. Speech recognition privacy

PWA speech recognition is not a dependable privacy-preserving foundation across browsers. Current Web Speech `SpeechRecognition` support is not universal, and some browser implementations may use server-based recognition that sends audio to a service.

Therefore:

- OM must not silently transmit microphone audio to a third-party speech service merely because the user tapped the Omni microphone;
- prefer on-device/OS-local recognition where the platform exposes it with acceptable privacy semantics;
- if only remote recognition is available, require clear opt-in/configuration consistent with OM privacy policy;
- if no acceptable recognizer is available, voice command entry may be unavailable while typed Omnibar remains fully functional;
- recognition provider choice must not become a protocol requirement.

Reference:

- MDN SpeechRecognition: https://developer.mozilla.org/en-US/docs/Web/API/SpeechRecognition

## 8. Transcript-before-action model

Speech recognition output becomes Omnibar input, then normal resolution occurs.

For low-risk navigation/read actions, a unique high-confidence result may be activated with minimal friction if the user's selected voice mode allows it.

For actions with meaningful side effects:

- show the resolved target/action clearly;
- preserve Action Registry confirmation rules;
- provide immediate cancellation for interruptive actions such as calls;
- never auto-send dictated message content solely because recognition produced a plausible transcript;
- destructive/security-critical operations cannot become voice-only irreversible completions.

Users may choose a conservative voice profile where every command requires activation after transcription.

## 9. Dictation versus command

OM must distinguish:

- **command speech** in the Omnibar;
- **message dictation** in the composer;
- **voice-note recording** in the composer.

These are separate modes with different privacy and side-effect semantics.

A command such as:

```text
message Maria saying I'll arrive at seven
```

may create a visible draft addressed to Maria. It should not silently send unless a separately designed, explicit voice-send policy is enabled and satisfies Action Registry safety.

## 10. Voice ambiguity

Recognition confidence is not identity confidence.

If the transcript is `Call Maria` and multiple Marias exist, OM shows/speaks the candidate distinctions instead of guessing.

Likewise, an uncertain transcript should be editable rather than repeatedly forcing the user through a voice-only correction loop.

## 11. Screen-reader interaction

Screen-reader users receive the same result model:

- query field and scope;
- currently active result;
- type (`Person`, `Action`, `Message`, etc.) where useful;
- primary action;
- secondary actions through accessible action/navigation patterns;
- result count changes with restrained live announcements.

Do not announce every incremental rank change. Accessibility feedback should be useful, not noisy.

## 12. Future HID and macro input

A future button, switch, dial, eye-control adapter, or macro may invoke `open-omnibar`, enter text, select a result, or directly invoke a registered action.

The adapter never needs to understand OM's internal UI hierarchy; it targets the same action/result semantics.
