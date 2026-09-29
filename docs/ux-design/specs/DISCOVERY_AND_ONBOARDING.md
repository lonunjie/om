# Discovery and Onboarding

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

## 1. Goal

OM should be learnable by using it.

The interface should expose enough structure that users can infer likely actions, try them safely, and receive immediate feedback.

## 2. Implicit onboarding

Prefer interaction that demonstrates itself over detached instruction.

Example:

- user begins dragging/swiping a message;
- the message follows the input;
- a reply affordance gradually appears;
- crossing a threshold communicates commitment;
- reversing direction cancels;
- successful completion links the message to the composer.

The user learns the mechanism through action rather than a tutorial card.

## 3. Safe discovery

Experimentation should remain reversible until intent becomes clear.

Use:

- previews;
- thresholds;
- visible targets;
- undo;
- staged commitment;
- confirmation for consequential actions.

Avoid:

- hidden destructive gestures;
- irreversible actions triggered by exploratory movement;
- critical features discoverable only through undocumented gestures.

## 4. Progressive disclosure

Default surfaces show the actions most likely to matter.

Less common actions appear in:

- long-press/context menus;
- overflow menus;
- command palette;
- search;
- advanced settings;
- expert shortcuts.

The UI should not display every capability merely because OM supports it.

## 5. Wayfinding

Users should continuously understand:

- where they are;
- what object they are acting on;
- where an action leads;
- how to return;
- what changed.

Spatial continuity, stable placement, persistent object identity, breadcrumbs only where necessary, and reversible transitions all contribute to digital wayfinding.

## 6. Instruction policy

Use explicit instruction when:

- the action is security-sensitive;
- the consequence is hard to infer;
- the concept is genuinely unfamiliar;
- accessibility requires explicit wording;
- the user is about to make an irreversible decision;
- a gesture or interaction cannot be made sufficiently discoverable.

The goal is not “no instructions.” It is **no unnecessary instructions**.

## 7. No onboarding slide deck

OM should not use a generic multi-page onboarding carousel to teach routine messaging behavior.

First-use education should occur in context and disappear once learned.

Privacy/security decisions that require comprehension may use explicit explanation, but they are decisions, not marketing slides.

## 8. Zero-contact first run

A new installation may start with no conversations and no contacts because OM does not upload the address book by default.

The empty chat list should therefore provide a clear, friendly primary action such as **Invite someone to chat**, plus direct access to QR scanning and the Omnibar.

The user can share an invite through tools they already use: another messenger, email, copied link/code, or direct QR scan.

One-time per-person invitations and reusable/expiring invitations should be presented according to what the underlying architecture can safely support.

After the user has conversations, onboarding-specific chrome should recede.
## 9. Home tiers and sponsored client acquisition

Invite-first does not erase infrastructure semantics. The canonical onboarding path may use T0 sponsored starter mailbox, T1 hosted Home, T2 own cloud, T3 self-hosted, or T4 local-only. Ordinary users should not configure infrastructure primitives, but any sponsor/co-resident observation, closure/migration dependency, or client-origin choice required by architecture is disclosed at the decision point.

A sponsored invite may offer supported native clients plus independent PWA origins. The sponsor must not silently become the only client distributor merely because it provided the introduction/Home.

This explanation is contextual and bounded; it is not a generic onboarding slide deck.

