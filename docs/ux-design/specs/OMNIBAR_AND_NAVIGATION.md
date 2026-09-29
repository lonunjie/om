# Omnibar and Navigation

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Normative shell design direction  
**Scope:** Mobile-first shell, desktop adaptation, global search/action surface, user-composable tabs

## 1. Chat-first, not dashboard-first

OM opens into communication, not a portal or activity dashboard.

The default root surface is the conversation list. Conversations are ordered by most recent activity first because this matches widespread messenger expectations. OwnMessenger may reverse or otherwise customize ordering, but the default must remain immediately familiar.

Calls, people, files, settings, account actions, and other capabilities exist as views or actions around communication; they do not have to occupy permanent top-level navigation.

## 2. The Omnibar is the primary universal action surface

OM should provide a fast global **Omnibar** that can resolve both retrieval and intent.

Examples:

```text
Maria
  → open conversation
  → message Maria
  → call Maria

scan qr
  → open scanner

mute family until tomorrow
  → resolve MuteConversation(...)

privacy
  → open matching settings

"I'll arrive tomorrow"
  → search messages containing the phrase
```

The Omnibar should search or invoke, at minimum:

- conversations;
- people/identities;
- messages;
- files and media where locally indexed;
- settings;
- canonical Action Registry actions;
- recent and pinned actions;
- invite/create/scan operations;
- account switching where safe.

Search and commands are one coherent surface rather than unrelated systems.

## 3. Bottom placement by default

On compact mobile shells, the Omnibar is placed in the lower reach zone by default. This favors one-handed use and avoids assuming that primary controls belong in the top-left corner.

OwnMessenger may allow:

- bottom or top placement;
- left/right mirroring where applicable;
- larger reachability layouts;
- compact or expanded Omnibar form;
- alternate shell presets.

Placement customization may not obscure essential state or create unsafe control collisions.

## 4. Bottom navigation is user-composable

Navigation destinations are **tabs**, but OM does not hard-code that every user must keep the same set visible.

**Default:** Chats is the only enabled top-level view. If exactly one tab is enabled, the compact tab strip is hidden because a one-item navigation control adds chrome without adding orientation. The strip appears when the user enables additional destinations.

Potential tabs include:

- Chats;
- Calls;
- People;
- Files/Media;
- Groups/Spaces if later adopted;
- user-defined or future first-class views.

Users may:

- show/hide allowed tabs;
- reorder them;
- pin preferred destinations;
- choose a shell preset.

The Action Registry and Omnibar remain able to reach hidden destinations.

## 5. Adaptive tab capacity

Four visible tabs is a practical common compact-phone target, but OM must not encode `4` as a semantic limit.

The visible capacity should depend on:

- viewport width;
- font/text scaling;
- minimum accessible target size;
- label length/localization;
- user density preference;
- device posture and orientation.

When more tabs exist than can fit legibly, the tab strip may become horizontally scrollable. Motion should communicate the off-screen continuation and current position rather than merely clipping items.

A scrollable strip must preserve:

- stable tab order;
- clear selected state;
- keyboard/accessibility traversal;
- non-motion indication that more tabs exist;
- rapid return to pinned/high-frequency tabs.

## 6. Tabs are views; Omnibar is capability

Tabs answer: **Where am I?**

The Omnibar answers: **What do I want to find or do?**

This distinction prevents navigation from growing every time OM gains a new capability.

A user who primarily messages may keep only Chats visible and use the Omnibar for everything else. Another user may expose Calls, People, or specialized views.

## 7. Omnibar suggestions

When the Omnibar receives focus, it may show a small locally ranked set of suggestions, for example:

- pinned actions or people;
- recent conversations;
- frequent actions;
- contextually likely actions;
- recent searches;
- scan QR / invite someone when relevant.

Pins are explicit user intent and should outrank learned predictions unless the user chooses otherwise.

Suggestions should be useful before typing and refine immediately as input arrives.

## 8. Omnibar performance requirement

The Omnibar must feel instantaneous for common local tasks.

Design expectations:

- opening it must not wait for network access;
- pinned/recent suggestions come from local state;
- local people/conversation/action results stream first;
- local message search should begin incrementally;
- remote/provider lookup, if explicitly allowed and required, is secondary and visibly distinguished;
- ranking must never block text entry or first results;
- expensive enrichment happens after useful results are already present.

The Omnibar is not acceptable if it becomes a slow AI prompt box.

## 9. Voice is another Omnibar input

Where supported, speaking into the Omnibar should produce the same canonical queries/actions as typing.

Examples:

- “Call Maria.”
- “Message Ramon.”
- “Scan an invite.”
- “Find the message about the airport.”
- “Open notification settings.”

Voice uses the Action Registry and normal safety policy. It is not a parallel command system.

## 10. Conversation view and bottom-edge ownership

Inside a compact-mobile conversation, the message composer has priority over the lower edge because composing is the dominant task.

OM uses one primary text-entry purpose in that region at a time. A dedicated Omnibar trigger changes the lower surface from **Composer** to **Omni mode** using the semantic `Transform` motion primitive. The message draft, cursor/selection, reply target, and staged attachments are preserved and restored when Omni mode closes.

Omni mode has distinct placeholder, iconography, result surface, and submission semantics. Message-send controls disappear while it is active. A global command must never be ambiguous with text that will be sent as a message.

On compact mobile, top-level tabs are hidden by default while a conversation is open so the composer does not compete with a stacked navigation bar. Omnibar, back/context navigation, and optional accelerators still provide fast access. Larger layouts may expose both the composer and global Omnibar simultaneously when they can remain spatially distinct.

See [`MOBILE_SHELL_STATES.md`](MOBILE_SHELL_STATES.md) and ADR-UIUX-0003.

## 11. Account control placement

OM must not constitutionalize “avatar at top-left.” Account/profile switching is a shell function whose placement should respect reachability, handedness, platform constraints, and the user's shell configuration.

**Prototype default:** on root/list surfaces, account access sits at the trailing lower edge beside the Omnibar. This favors common right-handed reach while remaining explicitly mirrorable through OwnMessenger. Inside a conversation it need not remain visible because account switching is not part of normal message composition; it remains available through the Omnibar.

Desktop may place accounts in a left-side rail when that improves multi-account spatial organization.

The invariant is the account model and action semantics, not the corner in which an avatar appears.

## 12. Group and individual conversations share a common grammar

Direct chats and group chats should remain first-class entries in the same conversation model when possible.

A Discord-like left-side organization may be useful for high-structure groups, nested spaces, or channels on larger screens, but OM should not force every direct-message user into a server/channel mental model.

Recommended direction:

- direct chat and ordinary group chat remain simple conversations;
- richer group/space hierarchy may progressively reveal an inner navigation structure;
- desktop can expose that structure in a side rail/pane;
- compact mobile can collapse it behind the current group/space context;
- the outer shell and Omnibar remain consistent.

This allows richer communities later without making one-to-one messaging feel like navigating a workspace product.
## 13. Local ranking is assistance, not authority

Omnibar ranking should begin with explicit pins, strong query match, current context, recency/frequency, and other local evidence. Learning may refine ordering but must not override clear explicit intent.

The ranking engine is replaceable. A transparent weighted baseline should ship before adopting more complex Bayesian/contextual methods. Bayesian smoothing is compatible with the design, but stochastic exploration must not reshuffle security-critical or irreversible actions.

The Omnibar remains fully usable with learning disabled. See [`OMNIBAR_RANKING.md`](OMNIBAR_RANKING.md).

## 14. Adaptive Omnibar hints

The field's accessible identity remains `Omnibar`, while the visual placeholder may optionally offer local/contextual hints such as `Find or do…`, `Call someone…`, `Scan an invite…`, or a locally learned target when privacy-display settings permit it.

Placeholder hints must change only at natural state boundaries, not continuously rotate for attention. Disabling learning falls back to deterministic contextual/generic hints.

## 15. Prototype details

See [`MOBILE_SHELL_PROTOTYPE.md`](MOBILE_SHELL_PROTOTYPE.md), [`TABS_REACHABILITY_AND_SHELL_CUSTOMIZATION.md`](TABS_REACHABILITY_AND_SHELL_CUSTOMIZATION.md), and [`OMNIBAR_TRIGGER_AND_TRANSITION.md`](OMNIBAR_TRIGGER_AND_TRANSITION.md) for the selected low-fidelity shell baseline.

## 16. Stable entity semantics

For a bare known person such as `Maria`, the default entity action is to open/start the conversation. Learned behavior may raise Maria in the results but must not silently turn bare `Maria` into `Call Maria`.

Explicit verbs (`call Maria`), explicit secondary controls, or user-pinned action-target pairs express the alternate action. This keeps personalization useful without making the same query semantically unstable.

## 17. Resolution and index specifications

See:

- [`OMNIBAR_BEHAVIOR.md`](OMNIBAR_BEHAVIOR.md) for candidate classes, ambiguity, execution, and stable result behavior;
- [`OMNIBAR_INDEXING_AND_PERFORMANCE.md`](OMNIBAR_INDEXING_AND_PERFORMANCE.md) for local search planes, PWA/native index options, latency budgets, and index privacy;
- [`OMNIBAR_VOICE_AND_KEYBOARD.md`](OMNIBAR_VOICE_AND_KEYBOARD.md) for hardware-keyboard, speech, and assistive interaction.

