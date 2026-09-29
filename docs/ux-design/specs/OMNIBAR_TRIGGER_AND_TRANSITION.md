# Omnibar Trigger and Transition

**Status:** Prototype interaction specification

## 1. Why a dedicated trigger

Inside a conversation, OM needs a discoverable way to invoke the universal find/do surface without stacking two persistent input fields.

A dedicated Omnibar trigger is therefore part of the compact composer.

It is not:

- a Search-only button;
- an AI assistant button;
- an attachment button;
- the microphone for voice messages.

Its stable accessible action is `Open Omnibar`.

## 2. Default position

For a right-handed default compact layout, the Omnibar trigger sits immediately before the far-trailing Send/voice control.

```text
[ + ] [ message field................ ] [ Omni ] [ Send ]
```

This keeps the universal action surface in the strong lower-right reach area without displacing Send from the most familiar trailing position.

OwnMessenger may mirror the arrangement.

## 3. Visual identity

The prototype may use `◉` as a placeholder glyph.

The final glyph should communicate **OM-wide capability** rather than `search`, `AI`, or `settings` specifically. It may derive from the OM mark if the brand system produces an unmistakable small-size form.

The control should gain a short label/tooltip in contexts where discoverability requires it, then rely on learned recognition.

## 4. Transition grammar

Opening Omni mode uses the semantic **Transform** primitive:

1. the composer stops accepting message text;
2. its container geometry becomes the Omnibar container;
3. message-only controls depart;
4. close/back and Omni input affordances enter;
5. suggestion/results surface grows upward from the lower shell;
6. focus moves to Omni input;
7. message draft remains preserved but visually inactive.

The transition communicates **change of purpose**, not navigation to a different app section.

## 5. Timing

Use short, interruption-safe motion. Prototype range:

- approximately 160–220 ms for the primary state morph;
- result rows may follow immediately without choreographed delays;
- no sequential decorative staging that slows task entry.

If the user types during the transition, input must be accepted without waiting for animation completion.

## 6. Exit

Exit may occur through:

- explicit close control;
- system Back;
- hardware `Esc`;
- selected action completion when that action naturally dismisses Omnibar;
- optional learned accelerator.

The exact previous composer state returns.

## 7. Voice distinction

Voice-message recording and spoken Omnibar input are separate intents.

- composer microphone → record/send voice message;
- Omni microphone → interpret speech as find/do input.

The UI state must make this distinction obvious before capture begins.

## 8. Accessibility

Screen readers must announce the mode change and identify the active field purpose.

The transition cannot require motion to understand. Reduced-motion users receive the same state change with restrained visual treatment.
