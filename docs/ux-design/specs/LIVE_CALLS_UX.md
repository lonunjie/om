# Live Calls UX

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Normative R13B design specification  
**Scope:** one-to-one and group audio/video calls across mobile PWA, iOS, Android, desktop, keyboard/accessibility clients, and future adapters

## 1. Principle

A call is a live communication object inside OM, not a separate product mode.

Users should normally start or join a call from the conversation, the Omnibar, a notification/system call surface, or an optional Calls destination. A permanent Calls tab is not required for users who do not want one.

The user-facing object is the **call session**. Signaling attempts, ICE restarts, relay selection, audio routes, media tracks, push delivery, and transport retries are implementation details unless they change what the person can do or understand.

## 2. Canonical call states

A call session may expose states equivalent to:

- `incoming` — an invitation exists and awaits the local user's decision;
- `starting` — the local user deliberately initiated the call;
- `ringing` — one or more intended participants are being alerted when architecture confirms that state;
- `connecting` — OM is establishing usable media;
- `active` — live communication is usable;
- `reconnecting` — the call object remains active but useful media is temporarily impaired;
- `held` — local/system policy has placed the call on hold, if supported;
- `ending` — termination was requested and cleanup is underway;
- `ended` — the call is over;
- `failed` — the call cannot continue and the user may need a recovery action.

OM must not fabricate `ringing`, `connected`, or participant-presence states from local intent alone.

## 3. Object-preserving reconnection

A temporary network break must not visually create a new call.

```text
Active call
    ↓ network/path interruption
Reconnecting…
    ↓ recovered
Active call
```

Participant identity, elapsed call context, mute/camera state, and conversation association remain stable unless the architecture proves they changed.

If recovery fails, the same call object transitions to `failed` or `ended` with a reason suitable for user action.

## 4. Routing is not a security rating

OM may route a call over LAN, direct P2P, TURN/relay, provider infrastructure, or another architecture-approved path. End-to-end security semantics are independent of that route.

Default call UI therefore must **not** use:

- green for “direct” and amber/red for “relayed”;
- a stronger lock icon for P2P than TURN;
- warnings merely because the route changed while encryption did not.

Advanced diagnostics may show `Direct`, `Relay`, or provider/path details when useful, but route labels are technical diagnostics, not trust badges.

## 5. Incoming calls

The incoming surface must communicate:

- who or which group/Space is calling;
- which OM IdentityProfile is receiving the call when multiple profiles are configured;
- audio or video intent when known;
- explicit Answer and Decline actions;
- a non-visual equivalent for every action.

For an incoming video call, OM should provide an explicit privacy-preserving path equivalent to:

- **Answer video** — activates local camera only after the user chooses this action and platform permission permits it;
- **Answer audio** — joins without transmitting local video.

OM must not activate the local camera merely because an incoming video invitation arrived.

## 6. Outgoing calls

The call action belongs naturally to the conversation header/context and Omnibar:

```text
call Maria
video Maria
```

Bare `Maria` remains an entity/open-message intent under the Omnibar rules; prediction must not silently turn it into a call.

Calls started from a group or Space channel retain that context so the call timeline notice and post-call history can return the user to the originating conversation.

## 7. Media controls

The primary in-call control set should remain small:

- mute/unmute microphone;
- camera on/off where video is supported;
- end call;
- audio output/device selection where the platform exposes it;
- participant list for multi-person calls;
- additional actions behind progressive disclosure.

Controls must communicate state with shape/icon/text/accessibility semantics, not color alone.

A muted microphone is a state of the local outgoing audio track/call participation, not a global device-security assertion.

## 8. Permission timing

Microphone and camera access are requested only when the user invokes functionality that needs them.

- audio call or audio answer → microphone request when required;
- video start/answer → camera request when required;
- enabling camera during an audio call → camera request at that moment.

If permission is denied, OM explains the affected capability and any real fallback (`Join without camera`, `Listen only`, `Open Settings`) only if that fallback is actually supported.

Browser/PWA capture requires secure-context and browser permission behavior. OM must not imitate or suppress browser/platform media-use indicators.

## 9. Native platform integration

### iOS/iPadOS

VoIP calls should integrate with CallKit/approved system calling mechanisms when available and architecture permits. The system owns incoming-call presentation, Do Not Disturb coordination, audio-session behavior, and other system-call semantics where required.

### Android

Native Android calling should integrate with Core-Telecom/Telecom and system CallStyle surfaces when appropriate. Audio-route state exposed by Telecom is authoritative; OM should not fight the system with parallel Bluetooth/audio-routing logic.

### PWA

The PWA supports active WebRTC calls where browser capability allows. Background incoming-call reliability, wakeup, ringing, system integration, and media permissions vary by platform/browser. OM must not promise native-equivalent background calling when the host cannot guarantee it.

A capability-limited PWA may truthfully state, for example, that opening/installing the app or enabling notifications improves incoming-call reachability. This is a capability explanation, not a growth prompt.

## 10. Group calls

Group calls are associated with an ordinary group conversation or a Space channel. A group call may have states such as:

- available/joinable;
- joining;
- active;
- leaving;
- ended.

Joining an existing group call is distinct from ringing every member. The architecture may support explicit ringing for small/selected participant sets, but the UI must disclose which behavior is occurring.

Participant presence and speaking/muted indicators must reflect actual media/call evidence, not inferred social status.

## 11. Network-quality feedback

OM should avoid fake precision such as five “signal bars” unless the metric has a defined, tested mapping.

Useful user states are task-based:

- `Reconnecting…`
- `Audio may be delayed`
- `Video paused to keep audio clear`
- `Call ended — connection lost`

Adaptive degradation may reduce video before audio when architecture/policy supports it. The interface explains only changes the user can perceive or act on.

## 12. Handoff and multi-device

Call handoff is a canonical action only if the architecture supports it. A UI must never imply seamless device transfer merely because multiple devices are linked.

If supported, handoff should preserve the same logical call and clearly identify the destination device before transfer.

## 13. Accessibility and multimodal input

Every core call action must be reachable by:

- touch/pointer;
- keyboard where a keyboard exists;
- platform accessibility actions;
- Omnibar/command surfaces when safe;
- voice command adapters where enabled.

An incoming call must not require a swipe-only answer gesture. Call state changes that matter must have non-audio and non-motion equivalents.

## 14. OhMessenger boundary

Expressive ringing, call backgrounds, join/leave flourishes, reactions, and haptics may exist in Balanced/Playful modes, but:

- they may not obscure caller identity or controls;
- remote participants cannot force high-intensity effects;
- Focus/Reduced Motion can suppress them;
- security or network states never use celebratory presentation.

