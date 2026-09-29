# Attachments and Media UX

**Status:** normative R13A design.

## 1. Stage before send

Selecting, pasting, dropping, capturing, or sharing a file creates a **staged attachment** in the composer. It is not yet a sent message.

The staged object should show, as available:

- safe local preview or media-type icon;
- filename/title when meaningful;
- file size or duration;
- removal control;
- preparation state if OM must transform/hash/encrypt/compress before send;
- any user choice that materially changes the result (for example, send original vs compressed), only where the architecture supports it.

The user can continue editing text while attachments prepare.

## 2. Object-preserving transfer

A sent attachment remains the same recognizable object while its state changes:

```text
staged -> preparing -> queued -> transferring -> available
                         |             |
                         +----failed---+
                                |
                              retry
```

Do not replace the card with an unrelated toast or spinner. When transfer fails, keep thumbnail/name/metadata visible and attach `Failed · Retry` to that object.

## 3. Mixed messages

A message may contain text plus one or more attachments. The visual model allows child attachments to have independent progress/recovery state if the underlying architecture supports independent transfer.

OM must not claim the whole message is fully delivered merely because its text/envelope was accepted while required media is still unavailable.

## 4. Progress

- use determinate percentage only when progress is trustworthy;
- use calm indeterminate activity when total work is unknowable;
- progress never blocks other composition;
- do not announce every percentage change to assistive technology;
- expose meaningful milestones and errors accessibly.

## 5. Download and availability

Received media may be:

- locally available;
- thumbnail/metadata available but full media remote;
- downloading;
- failed;
- intentionally unavailable/expired.

Auto-download policy belongs to user configuration/data policy. The content card itself expresses current availability and offers the appropriate action (`Download`, `Retry`, `Open`, `Save/Export`) without requiring a separate downloads screen.

## 6. Privacy-sensitive previews

Remote link previews, thumbnails, metadata fetches, and media analysis are not decorative implementation details. They can create network/privacy side effects and must be requested only through architecture-approved paths.

A functional text/icon fallback always exists when rich preview is unavailable or disabled.

## 7. Drag, paste, picker, share target

Different input methods feed the same attachment action grammar:

- touch file/photo/camera picker;
- desktop drag-and-drop;
- paste from clipboard where safe;
- OS share target/share sheet;
- accessibility/keyboard invocation.

No attachment capability is available only through drag-and-drop or only through a hidden gesture.

## 8. Oversize/unsupported content

If a file cannot be sent, preserve the staged object and state the specific constraint next to it. Prefer actionable wording:

`Too large for this route · Choose another file`

rather than a generic upload error. Limits come from domain policy, not hard-coded visual assumptions.

## 9. Media viewer

Opening media changes place only when the user genuinely enters a viewer. The viewer returns to the exact originating message/context. Common actions such as Save/Export, Share (where allowed), Copy, or Open externally remain contextual actions rather than permanent chrome.

## 10. Accessibility

Accessible names include the object and important state, for example:

- `Photo, uploading`
- `PDF, 2.4 megabytes, failed to send, Retry available`
- `Voice note, 42 seconds, paused`

Visual thumbnails never replace useful names/state.
