# Messaging Completeness v0 — Textual Prototype

This fixture is intentionally framework-independent. It exercises R13A states without selecting a final production layout.

## A. Offline text send

```text
Maria
------------------------------------------------
Are you there?
                         Yes, leaving now   ◷
                         Waiting to send
------------------------------------------------
[ + ] Message Maria…                   [Send]

Connection limited
```

The message exists immediately. `Connection limited` is secondary; the queued message itself carries the useful state.

## B. Failed message

```text
                         Photo from lobby
                         ! Failed to send
                         [Retry]
```

Retry acts on the same message/media object. No global error dialog.

## C. Staged attachment

```text
[image thumbnail] lobby.jpg   2.4 MB   [Remove]
[ + ] Here is the photo…                [Send]
```

Preparation/progress can occur without blocking text editing.

## D. Voice note

```text
Recording  00:18
[Cancel]                  [Stop]
```

After Stop:

```text
Voice note  00:18
[Play] [Discard]          [Send]
```

Waveform may appear but is never the only recording/playback state.

## E. Scheduled message

```text
Tomorrow 08:00
“Please send the report.”
Scheduled on this device
[Edit] [Cancel]
```

The dependency line changes according to real architecture. Never label it `Sent` before release.

## F. Foreground incoming message

If Maria's conversation is already open, the message appears in place. Do not simultaneously show a system banner for the same event.

## G. Lock-screen privacy modes

```text
Private:      New message
Sender-only:  Maria — New message
Full:         Maria — “We arrived.”
```

Full preview is not the privacy-first default.
