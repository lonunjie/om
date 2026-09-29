# Voice Notes UX

**Status:** normative R13A design; final gesture preferences remain validation candidates.

## 1. Voice note is not Omni voice

The composer microphone records a **message attachment**. Omnibar voice interprets a command/query. Their icons, accessible names, active states, permission prompts, and results must remain distinct.

## 2. Default safe recording flow

Recommended default:

```text
idle -> recording -> review -> send
               \-> discard
```

OM should favor a tap-to-start/tap-to-stop flow that works with touch, mouse, keyboard, switch control, and accessibility actions. Hold-to-record may exist as an accelerator or user preference, but it cannot be the only path.

Stopping recording enters **review** by default so an accidental recording is not immediately transmitted. A future explicit OwnMessenger preference may allow a faster send flow if validation supports it.

## 3. Recording state

While recording, show without ambiguity:

- `Recording` state text/icon/geometry;
- elapsed duration;
- Stop;
- Cancel/Discard;
- microphone-in-use state;
- optional waveform as secondary visual feedback only.

A waveform is not the sole recording indicator. Reduced Motion can render it static/minimal.

## 4. Permissions and privacy

Microphone permission is requested only when recording is invoked. The UI must not imply recording before the platform has actually granted capture, and must stop/release capture when recording ends/cancels or the recording surface closes.

For the PWA, capture requires a secure context and browser permission. Browser/platform recording indicators remain authoritative and OM must not try to imitate or obscure them.

## 5. Review

Before send, allow:

- playback/pause;
- duration/seek where feasible;
- discard;
- send;
- optional rename/caption only if the product supports it.

Review survives temporary Omnibar invocation and ordinary transient shell changes just like other staged attachments.

## 6. Playback

Received/sent voice notes expose:

- Play/Pause;
- current time / duration;
- seek where supported;
- accessible playback state;
- optional speed control as a contextual preference/action rather than mandatory permanent chrome.

## 7. Transcript/accessibility

Audio remains the canonical message content. OM may expose a transcript if:

- the sender supplied one;
- an architecture-approved local/on-device transcription capability exists; or
- the user explicitly enables an approved remote transcription service with clear privacy consequences.

OM does not silently send private voice notes to a remote transcription service merely to improve UI convenience.

Where transcript exists, it should be selectable/searchable according to the user's privacy/search policy and visibly identified as generated when appropriate.

## 8. Failure

If recording fails, preserve any successfully captured local audio when technically possible and offer recovery/export/retry rather than discarding silently. If nothing was captured, state that plainly.
