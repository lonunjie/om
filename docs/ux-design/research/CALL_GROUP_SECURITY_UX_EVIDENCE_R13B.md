# R13B Calls, Groups/Spaces, and Security UX Evidence

**Date:** 2026-09-29  
**Purpose:** Record external evidence used to bound R13B decisions. These sources support constraints and platform behavior; they do not prove OM-specific usability without human/device testing.

## Native call integration

### Apple CallKit / PushKit

Apple describes CallKit as the system integration point for VoIP calls, coordinating call UI and system behaviors such as Do Not Disturb. Apple also states that VoIP pushes handled with PushKit must be reported through CallKit for apps linked against iOS 13 SDK or later.

Design implication for OM:

- native iOS incoming/outgoing calling should cooperate with system CallKit semantics rather than invent an isolated parallel phone subsystem;
- the PWA cannot be assumed to provide identical background incoming-call behavior;
- call state shown by OM must stay synchronized with the platform call state.

Sources:

- https://developer.apple.com/documentation/callkit
- https://developer.apple.com/documentation/PushKit/responding-to-voip-notifications-from-pushkit

### Android Core-Telecom / CallStyle

Android's Core-Telecom documentation describes registering VoIP calls with Telecom, managing lifecycle and audio endpoints through the platform, and enabling interoperability with wearables/headsets/other call surfaces. Android's `CallStyle` notification is specifically intended for incoming/ongoing call controls.

Design implication for OM:

- use platform call lifecycle/audio-route state rather than maintaining a competing UI truth;
- required answer/decline/hang-up actions map to canonical OM actions;
- remote surfaces remain adapters to the same call object.

Sources:

- https://developer.android.com/develop/connectivity/telecom/voip-app/telecom
- https://developer.android.com/develop/ui/compose/notifications/call-style

## Browser/PWA media privacy

`getUserMedia()` requires a secure context and user permission to access camera/microphone. Browsers expose permission/use indicators. The WebRTC stack supports direct/relay networking but route details can expose network information and are not themselves a user-facing security rating.

Design implication for OM:

- request media permission just in time;
- do not activate a camera just because an invitation arrived;
- do not suppress/imitate browser privacy indicators;
- direct-vs-relay is not rendered as secure-vs-insecure.

Sources:

- https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia
- https://www.w3.org/TR/webrtc/

## Verification language and identity ambiguity

Signal explicitly distinguishes its safety-number / automatic key verification mechanisms from proof of a person's real-world identity. Signal also warns when safety numbers change, supports manual verification, and presents duplicate-name/shared-group context to help users detect impersonation.

Design implication for OM:

- cryptographic continuity checks, user manual verification, display names, and real-world identity must be separate concepts;
- a manually verified identity change deserves stronger review than an ordinary unverified continuity change;
- duplicate names in groups need disambiguation cues.

Sources:

- https://support.signal.org/hc/en-us/articles/360007060632-What-is-a-safety-number-and-why-do-I-see-that-it-changed
- https://support.signal.org/hc/en-us/articles/10223569377562-Automatic-Key-Verification
- https://support.signal.org/hc/en-us/articles/6829998083994-Phone-Number-Privacy-and-Usernames-Deeper-Dive

## Device authorization

Signal exposes linked devices as explicit user-reviewable objects and supports unlinking/revocation. This is useful precedent for treating device authorization as a first-class security surface rather than burying it in diagnostics.

Design implication for OM:

- device lists identify the endpoint being authorized/revoked;
- adding/removing a device is security-sensitive;
- revocation UI must not claim data was remotely erased unless OM architecture actually guarantees it.

Source:

- https://support.signal.org/hc/en-us/articles/360007320551-Linked-Devices

## Group/Space invite patterns

Signal supports group links/QR codes and optional approval for new members. This supports the general interaction pattern of a scoped group invitation whose permissions/approval state are visible before acceptance.

Design implication for OM:

- OM group/Space invites should disclose destination scope and approval/expiry semantics;
- the invite object is not interchangeable with a one-to-one contact invite or device-link QR.

Source:

- https://support.signal.org/hc/en-us/articles/360051086971-Group-Link-or-QR-code

## Privacy permission principle

Apple HIG recommends transparency about privacy-related data/resources and requests camera/microphone access through system permission flows.

Design implication for OM:

- permission prompts must be tied to an understandable immediate action;
- OM should explain why it needs media/device capabilities without front-loading permission requests.

Source:

- https://developer.apple.com/design/human-interface-guidelines/privacy/

## Limits of this evidence

These sources establish platform constraints, security-language precedents, and known privacy requirements. They do **not** establish that OM's exact wording, visual hierarchy, warning intensity, Space navigation, or call control arrangement is optimal. Those remain human/device validation items.

