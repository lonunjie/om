# OM Media Placeholders and Content States

**Status:** Normative Run 11B specification, extended by R13A messaging-completeness rules.

## 1. Objects remain visible through state changes

An attachment should not disappear into a generic spinner. The same media object remains identifiable while it moves through local selection, queued/uploading, sent, downloading, available, failed, or unavailable states.

Examples:

`photo -> queued -> uploading -> sent`

`file -> downloading -> available`

`attachment -> failed -> retrying -> sent`

State changes should preserve object identity and location whenever practical.

## 2. Placeholder hierarchy

Before a real preview is available, show the most specific safe cue already known locally:

1. real local thumbnail/preview when already available and policy permits;
2. media-type icon (`image`, `file`, `audio`, etc.);
3. filename/title when safe and meaningful;
4. size/duration/type metadata;
5. state/recovery control.

Do not fabricate a preview that could be mistaken for content.

## 3. Functional color

- pending/queued is calm neutral, not warning amber;
- progress uses ordinary action/information semantics;
- failure uses danger + explicit failure/retry form;
- unavailable/expired uses neutral/disabled treatment unless the reason is actually an error;
- success normally resolves back to the ordinary resting object instead of staying permanently green.

Color is never the only state cue.

## 4. Progress

Use determinate progress when byte progress is trustworthy. Use restrained indeterminate progress only when work is genuinely ongoing but total progress is unknown.

Progress animation must not block input and has a Reduced Motion equivalent.

## 5. Link/remote previews are not "just UI"

Fetching a remote thumbnail, OpenGraph card, avatar, or link preview can reveal network metadata and should be governed by OM privacy/network policy. The UI layer MUST NOT silently invent its own remote-preview fetch path merely to make a card look richer.

When remote preview is unavailable or intentionally disabled, the link/file remains fully usable through text + functional iconography.

## 6. Sensitive media

Spoilers, hidden media, view-once media, or other protected content require explicit state semantics from the architecture. A placeholder must not leak a thumbnail that the policy says is hidden.

## 7. Accessibility

The attachment's accessible name should identify the object and its important state, for example `Photo, uploading, 45 percent` or `PDF file, failed to send, Retry available`.

Routine progress announcements should obey the attention budget and avoid flooding assistive technology.

## 8. R13A integration

For staging, mixed-message transfer, download availability, drag/paste/picker convergence, and failure recovery, see [`ATTACHMENTS_AND_MEDIA_UX.md`](ATTACHMENTS_AND_MEDIA_UX.md). The R13A contract `contracts/om-messaging-contract-v0.json` is the machine-readable messaging state companion.
