# OM Action Registry

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

## Purpose

The Action Registry is the canonical vocabulary of what a user can ask OM to do.

It prevents action logic from being scattered independently across touch handlers, menus, shortcuts, voice grammar, accessibility actions, and future HID integrations.

## Minimum schema

Each action should define:

```yaml
id: reply-message
label: Reply
objects:
  - message
parameters:
  - message_id
preconditions:
  - conversation.writable
risk: reversible
confirmation: none
undo: cancel-reply
semantic_feedback:
  - composer.references-source-message
input_bindings:
  touch: swipe-or-action-button
  pointer: action-button-or-context-menu
  keyboard: configurable
  command_palette: true
  voice: true
  accessibility_action: true
presentation_constraints:
  essential_meaning_must_not_depend_on_motion: true
```

The exact serialization format is an engineering decision. The design requirement is that these semantics exist centrally and are queryable.

## Risk classes

Suggested classes:

- **observational** — read/open/view/filter;
- **reversible** — reply, react, draft changes, archive when undo exists;
- **consequential** — send, forward, publish, leave group;
- **destructive** — delete local history, remove content where recovery is limited;
- **security-critical** — revoke device, rotate identity state, recovery operations, key-sensitive actions.

Higher classes impose stronger interaction requirements.

## Security-critical constraints

Security-critical actions should normally require:

- explicit target identity;
- visible consequences;
- strong confirmation;
- accessible confirmation path;
- no hidden gesture-only completion;
- no motion-only explanation;
- no voice-only irreversible completion unless policy explicitly permits it;
- protection from accidental shortcut activation.

## Action aliases

Different platforms may have multiple bindings to one action.

```text
Reply(message)
  ├─ swipe
  ├─ visible button
  ├─ context menu
  ├─ keyboard shortcut
  ├─ command palette
  ├─ voice
  └─ accessibility action
```

Bindings are replaceable. Action identity is stable.

## Discoverability

The registry should support generation of:

- context menus;
- command palette entries;
- shortcut preference UI;
- accessibility action lists;
- voice grammar hints;
- help/search documentation;
- future device bindings.

## Why this matters

The registry converts future input support from “redesign the app” into “teach a new adapter OM's existing action vocabulary.”

## Omnibar integration

Every suitable user-facing action should be discoverable through the Omnibar without creating a second action implementation.

Useful registry metadata may include:

```yaml
omnibar:
  enabled: true
  aliases:
    - call
    - start call
  accepts_free_text_target: true
  rank_class: common
  safe_to_suggest: true
```

Security-critical actions may be searchable but must not become one-step predicted completions merely because a ranking model believes they are likely.

The Omnibar resolves natural typed/spoken phrasing into canonical actions, then normal preconditions, authorization, confirmation, and feedback apply.

## Omnibar policy metadata

To keep ranking separate from execution safety, Omnibar-visible actions should expose policy metadata conceptually equivalent to:

```yaml
omnibar:
  enabled: true
  aliases: [call, ring, phone]
  result_type: action
  zero_query_suggest: allowed
  learned_promotion: target-only
  voice_invoke: allowed
  voice_auto_execute: bounded
  requires_explicit_target: true
  remote_resolution: not_required
```

The exact schema is an engineering decision. The design requirement is that the resolver can ask the registry what is searchable/suggestible and what execution/confirmation policy still applies.

A language model, fuzzy matcher, or ranking model may produce `start-call(target=Maria)` as a candidate. It cannot mutate `start-call` into a less-protected operation or mark an ambiguous target as confirmed.

## R13A messaging actions

The registry should include canonical IDs equivalent to:

| Action | Target | Risk | Notes |
|---|---|---:|---|
| `send-message` | draft | consequential | local compose fast path; domain confirms acceptance |
| `attach-media` | draft | reversible | file/photo/camera/share adapters converge here |
| `remove-staged-attachment` | attachment | reversible | before send |
| `retry-message` | message | reversible | same logical message identity |
| `cancel-queued-message` | message | consequential | only while domain state permits |
| `retry-attachment` | attachment | reversible | same child object |
| `download-attachment` | attachment | reversible | policy/data constraints still apply |
| `record-voice-note` | draft | reversible | just-in-time microphone permission |
| `stop-voice-note` | voice note | reversible | enters review by default |
| `discard-voice-note` | voice note | destructive | recovery/Undo preferred where feasible |
| `send-voice-note` | voice note | consequential | converges on normal send semantics |
| `discard-draft` | draft | destructive | recovery/Undo preferred |
| `schedule-message` | draft | consequential | explicit verb; not equivalent to Send |
| `edit-scheduled-message` | scheduled message | reversible | before release |
| `cancel-scheduled-message` | scheduled message | reversible | before release |
| `notification-reply` | conversation | consequential | adapter into canonical send flow |
| `notification-mark-read` | conversation | reversible | notification surface does not own read state |
| `notification-mute` | conversation | reversible | user-configured notification policy |

Platform notification, share, picker, drag/drop, microphone, and keyboard affordances are bindings to these actions, not separate implementations.


## R13B live/group/security actions

The registry should include canonical actions equivalent to:

| Action | Target | Risk | Notes |
|---|---|---:|---|
| `start-audio-call` | person/conversation | consequential | explicit call verb/control |
| `start-video-call` | person/conversation | consequential | camera is not activated before intent/permission |
| `answer-call-audio` | call session | consequential | receive without local camera |
| `answer-call-video` | call session | consequential | just-in-time camera permission |
| `decline-call` | call session | consequential | system/native adapters converge here |
| `end-call` | call session | consequential | same logical call object |
| `set-call-muted` | call participation | reversible | stateful toggle; not global device security |
| `set-camera-enabled` | call participation | reversible | permission-gated |
| `choose-audio-endpoint` | call participation | reversible | platform/system routing remains authoritative |
| `join-group-call` | call session | consequential | joining is distinct from ringing every member |
| `open-space` | Space | observational | Spaces remain optional |
| `open-space-channel` | channel | observational | preserves Space context |
| `invite-to-group` | group | consequential | scope/approval/expiry disclosed |
| `invite-to-space` | Space | consequential | scope/approval/expiry disclosed |
| `leave-group` | group | consequential | remote data implications not invented |
| `leave-space` | Space | consequential | scope disclosed |
| `remove-member` | member | destructive | actual capability required |
| `view-security-details` | person/conversation | observational | contextual security surface |
| `compare-security-code` | security relationship | observational | out-of-band comparison path |
| `scan-verification-qr` | security relationship | reversible | QR parse alone does not auto-verify |
| `mark-verified-by-user` | security relationship | consequential | explicit user confirmation |
| `review-verification-change` | security relationship | security-critical | manually verified change |
| `add-device` | device authorization | security-critical | grants account/device capability |
| `revoke-device` | device authorization | security-critical | exact target + consequences required |
| `start-recovery` | recovery operation | security-critical | explicit recovery scope required |
| `reveal-recovery-material` | recovery operation | security-critical | local reauthentication preferred |
| `change-provider` | provider route | consequential | does not change AccountID identity by implication |

Security-critical actions may be found through the Omnibar but must not be promoted into zero-query one-step execution based on learned usage.

## R15 architecture reconciliation additions

The Action Registry is presentation vocabulary only. Canonical/security mutations route through typed Core intents as specified in [`docs/architecture/ux/ACTION_TO_CORE_INTENT_MAP.md`](../../architecture/ux/ACTION_TO_CORE_INTENT_MAP.md).

Additional V1 semantic actions include at least:

| Action | Target | Risk | Notes |
|---|---|---:|---|
| `create-poll` | conversation | consequential | canonical poll event semantics |
| `vote-poll` | poll | consequential | authorization/order Core-owned |
| `close-poll` | poll | consequential | local wall clock is not authority |
| `create-topic` | Room | consequential | persistent Room-local TopicID |
| `move-message-to-topic` | message | consequential | requires actual capability |
| `follow-publication` | Publication | reversible | account-private/provider follow state; not membership |
| `publish-post` | Publication | consequential | signed public Publication event |
| `retract-publication-post` | Publication post | destructive | retraction, not guaranteed Internet erasure |
| `set-disappearing-policy` | conversation | consequential | lifecycle policy, honest erasure wording |
| `share-contact` | PersonRecord | consequential | creates ContactCapsule/provenance |
| `send-message-request` | person/intro target | consequential | ordinary first-contact flow |
| `block-person` | person | consequential | scope/policy explicit |
| `report-content` | selected evidence | consequential | selective EvidencePackage; no universal moderator implied |
| `suspend-device` | device | security-critical | immediate quarantine semantics |
| `finalize-device-revocation` | device | security-critical | distinct from suspension; may be pending/frozen |
| `change-protection-profile` | IdentityProfile | security-critical | Recommended/Hardened/Maximum Privacy/Custom |
| `choose-home-tier` | onboarding | consequential | canonical T0–T4 workflow |
| `choose-client-origin` | sponsored onboarding | consequential | supported native/independent PWA choice |

The older `revoke-device` vocabulary is not sufficient by itself: UI must distinguish suspension/quarantine, pending/final authority transition, Safety Freeze and permanent revocation/removal.
