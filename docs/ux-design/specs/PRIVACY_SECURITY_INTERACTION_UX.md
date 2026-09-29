# Privacy and Security Interaction UX

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Normative R13B design specification

## 1. Goal

OM's security architecture should be strong without making ordinary messaging feel like operating a cryptography console.

Security UX therefore follows two complementary rules:

> **Routine protection stays calm. Meaningful security changes appear on the affected object.**

> **When the user must make a security decision, consequences become explicit and deliberate.**

## 2. Never collapse different trust concepts

The interface must distinguish at least these concepts:

### Transport/routing state

How traffic reached the peer: LAN, direct, relay/TURN, provider path, etc.

This is not identity and is not a security score.

### Cryptographic continuity

Whether the contact/AccountID/device cryptographic state is consistent with what OM previously observed or verified according to the architecture.

### User verification

Whether **this user** has explicitly verified a peer/contact using an approved out-of-band method such as in-person QR comparison or another trusted channel.

This is represented as **Verified by you** or equivalent, not as OM claiming universal real-world identity.

### Device authorization

Whether a device currently has AccountID-authorized capabilities.

### Recovery authority

Whether a RecoveryRoot/guardian/policy operation can restore a particular capability.

### Provider/routing authority

Whether a provider may route/discover/store particular metadata under its defined role. Provider selection does not own AccountID identity.

The UI must never compress all of these into one green shield or a single “Security score.”

## 3. No constant meaningless lock

If OM's normal communication is end-to-end protected by design, a permanent lock on every message creates little useful information and can imply guarantees beyond what the icon actually proves.

Default UI should therefore avoid decorative “secure” badges on every conversation/message.

Security affordances become visible when:

- the user opens Security details;
- a relevant state changed;
- verification is available/requested;
- a device/provider/recovery action needs review.

## 4. Verification language

OM must not say `Person verified` merely because a cryptographic check succeeded.

Preferred distinctions include:

- `Encryption check passed` — automatic/protocol check succeeded;
- `Verified by you` — the local user explicitly compared/confirmed through an approved independent method;
- `Verification changed` — previously user-verified cryptographic identity/continuity no longer matches;
- `Not verified by you` — no manual claim has been recorded.

Biometric unlock proves local device authorization, not remote human identity.

## 5. Verification flow

A manual verification flow should support:

1. open the contact Security details;
2. show a human-comparable code and/or QR representation derived from authoritative cryptographic identity data;
3. explain briefly that the comparison should occur in person or over an independently trusted channel;
4. user explicitly confirms the match;
5. local state records `Verified by you` for the verified identity/version.

The QR scanner must not silently mark a peer verified merely because *some* valid QR parsed. The scanned identity must match the intended contact and the user confirms the context.

## 6. Verification change

If a peer that the user **manually verified** changes cryptographic identity/continuity, OM should default to a strong contextual stop before sending new communication to that peer until the user reviews the change.

Suggested wording pattern:

```text
Verification changed
Maria's security identity is different from the one you verified.
Review before sending new messages.

Review security
```

This is not phrased as proof of compromise; legitimate causes can exist.

After review, the user may re-verify or explicitly accept the new state according to architecture/policy.

For contacts never manually verified, continuity changes may use a less disruptive contextual warning unless architecture requires stronger handling.

Group behavior when one recipient has a changed verification state is an architecture merge gate: the UI must accurately represent whether sending is blocked for all, partially deliverable, or requires per-member review.

## 7. Devices

A device-management surface should identify each authorized device using user-comprehensible metadata when available:

- user/device-chosen label;
- platform/device class;
- date added;
- recent activity/last-seen only if architecture can measure it accurately and privacy policy permits;
- capability/status such as active or revoked.

`Suspend device` and permanent revocation are distinct security-critical actions. Suspension/quarantine is the immediate emergency action when canonical state permits; stronger permanent removal may require a later/pending authorization transition. Contested control uses a calm Safety Freeze state rather than last-writer-wins.

Before confirmation OM must identify the exact device and explain what the selected action actually does. The UI must not promise deletion of history or secrets already stored on that remote device unless the architecture guarantees it.

Unknown/unrecognized device guidance may recommend revocation, but OM does not use fear language or claim an attack without evidence.

## 8. Adding a device

Adding/linking a device is also security-sensitive because it authorizes another endpoint.

The flow should show:

- which IdentityProfile/AccountID is being extended;
- destination device identity/label if available;
- what capabilities/data can synchronize;
- whether history transfer is optional;
- explicit final authorization on an already authorized device when the architecture requires it.

A QR used for device linking is visually and semantically distinct from a contact invite QR.

## 9. Recovery is not one thing

OM design already separates:

- identity/account authority recovery;
- history/data recovery;
- provider/discovery recovery.

The UX must preserve that separation.

Never show a generic `Recover OM` button if the operation can recover only one dimension.

Canonical recovery is on by default when custody can be established. The screen names the actual custodian/status (for example synced passkey, recovery key, pending/unavailable) and preserves opt-out consequences.

Examples:

- `Recover identity`
- `Restore message history`
- `Reconnect provider`

RecoveryRoot or guardian operations are security-critical and require clear target, scope, and consequence descriptions.

## 10. Recovery secrets

Recovery material is revealed only on explicit user action and may require local reauthentication.

OM should assume screen capture and clipboard controls are imperfect across platforms.

Therefore the design must not rely on “screenshots disabled” or “clipboard auto-clears” as the only protection. Those may be defense-in-depth features where supported.

The primary instruction is to store recovery material offline/private according to the architecture's recovery policy.

## 11. Provider changes

Provider selection is an advanced capability, not identity migration.

Default wording should emphasize function:

- `Discovery provider`
- `Relay provider`
- `Message transport provider`

only where those distinctions exist architecturally.

Changing a provider may affect reachability, metadata exposure, performance, or availability, but must not be presented as changing the user's AccountID identity unless it truly does.

OM may offer automatic failover/multiple providers while keeping routine routing quiet.

## 12. Security events use the attention budget

Examples:

### Passive/informational

- a provider route changed with no user-visible consequence;
- a device went offline;
- automatic encryption check succeeded.

### Noticeable/contextual

- a new authorized device was added;
- an unverified contact's continuity changed;
- recovery policy was changed.

### Blocking/security-critical

- manually verified contact changed identity before sending;
- device revocation confirmation;
- RecoveryRoot replacement;
- identity authority transfer.

The channel chosen must match consequence, not merely the word “security.”

## 13. Security warnings belong to the object

A verification change appears in Maria's conversation/security details.

A suspicious/new device appears in Devices.

A failed recovery appears on the recovery operation.

OM should avoid a generic global red banner unless multiple capabilities are genuinely affected.

## 14. Message requests and impersonation resistance

When communication comes from a person with whom the user has no established relationship, OM can expose grounded context such as:

- no prior conversation;
- shared groups/Spaces if locally knowable;
- user-local nickname/contact status;
- duplicate/conflicting names;
- verification status.

OM must not imply that a display name, avatar, username, provider listing, or shared group proves identity.

## 15. Sensitive actions in the Omnibar

Security-critical actions may be searchable:

```text
suspend my old phone
view recovery policy
verify Maria
```

but ranking cannot turn them into zero-query one-tap completions merely because they were recently used.

Execution still flows through Action Registry preconditions, explicit target resolution, confirmation, and local authorization.

## 16. Accessibility

Security states and warnings never depend solely on:

- red/green color;
- lock/check icons;
- motion;
- sound;
- haptic patterns.

Screen-reader labels describe the state and action consequence. Critical confirmation surfaces preserve keyboard/switch access and visible focus.

## 17. OwnMessenger boundary

Themes may style security surfaces within protected semantic tokens, but cannot:

- hide warnings;
- recolor danger into ordinary accent semantics;
- replace verification wording with stronger claims;
- remove confirmations;
- remap security actions;
- suppress required accessibility state.

