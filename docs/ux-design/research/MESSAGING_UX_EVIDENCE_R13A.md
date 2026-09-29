# Messaging UX Evidence — R13A

**Purpose:** record external evidence/standards that constrain the messaging-completeness design. This is not a claim that any one platform guideline is universal OM policy.

## Notification attention and privacy

Apple's current HIG says notifications should be timely/high-value, concise, avoid duplicates, avoid redundant foreground notifications, respect Focus/delivery controls, and avoid exposing sensitive content. It also distinguishes communication notifications from other interruption levels and advises honest urgency classification.

- https://developer.apple.com/design/human-interface-guidelines/notifications/
- https://developer.apple.com/design/human-interface-guidelines/managing-notifications
- https://developer.apple.com/design/human-interface-guidelines/alerts

Android's current notification guidance exposes user-controlled channel importance and lock-screen visibility, including public/private/secret presentation. This supports OM's platform-neutral attention classes plus privacy settings rather than hard-coding one interruption style.

- https://developer.android.com/develop/ui/compose/notifications
- https://developer.android.com/develop/ui/compose/notifications/create-notification

## Microphone/voice capture

MDN documents that `getUserMedia()` requires user permission, secure context, and browser-visible capture indicators. OM therefore treats voice capture as a just-in-time permissioned capability and never hides/imitates the browser's privacy indicator.

`MediaRecorder` is widely available, but specific containers/codecs may vary. UX therefore specifies recording/review semantics without hard-coding one media format.

- https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia
- https://developer.mozilla.org/en-US/docs/Web/API/MediaRecorder
- https://developer.mozilla.org/en-US/docs/Web/API/MediaStream_Recording_API/Using_the_MediaStream_Recording_API

## Media accessibility

WCAG 2.2 requires accessible names for controls and provides time-based-media requirements/techniques, including text alternatives for prerecorded audio-only content in applicable authored-content contexts. For private user-generated messaging, OM does not pretend it can guarantee a transcript for every voice note, but it keeps playback controls accessible and supports transcripts when supplied or generated through a privacy-approved path.

- https://www.w3.org/TR/WCAG22/
- https://www.w3.org/WAI/WCAG21/Understanding/time-based-media
- https://www.w3.org/WAI/WCAG21/Techniques/general/G158

## PWA background work

MDN currently marks Background Sync and Background Fetch as limited-availability APIs. They can improve offline delivery on supporting browsers, but OM PWA correctness must not depend on them. Local durable queue/reconciliation semantics are therefore a design requirement.

- https://developer.mozilla.org/en-US/docs/Web/API/Background_Synchronization_API
- https://developer.mozilla.org/en-US/docs/Web/API/Background_Fetch_API

## Evidence interpretation

These sources justify constraints such as permission timing, privacy, accessible controls, honest attention levels, and progressive/offline resilience. They do **not** prove details such as the ideal voice-note gesture, exact notification defaults for every population, or specific scheduled-message behavior. Those remain validation/architecture decisions.
