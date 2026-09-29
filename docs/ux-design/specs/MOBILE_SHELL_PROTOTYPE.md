# Mobile Shell Prototype v0

**Status:** Recommended prototype baseline  
**Scope:** compact iOS, Android, and mobile PWA  
**Intent:** turn the UX constitution into concrete shell behavior without freezing final visual branding.

## 1. Default shell principle

OM starts simpler than most contemporary messengers:

> **Chats is the only default tab. If only one tab is enabled, no tab strip is shown.**

Calls, People, Spaces, Files/Media, and future views remain available through the Omnibar and may be exposed as tabs through OwnMessenger.

This preserves familiar chat-list behavior without forcing users to carry permanent navigation they may never use.

## 2. Root shell

Recommended compact root:

```text
┌─────────────────────────────────┐
│ OM                         ⋯     │
│                                 │
│  Maria                    18:42  │
│  Are you still coming?        2 │
│                                 │
│  Family                   17:10  │
│  Dad: Sent a photo              │
│                                 │
│  Workmates                 Mon  │
│  You: I'll send it tonight      │
│                                 │
│                                 │
├──────────────────────────┬──────┤
│  Find or do…         🎙  │  ●   │
└──────────────────────────┴──────┘
                         account
```

The account control is on the **trailing lower edge by default**, not constitutionally top-left. This places a common shell control near the dominant thumb zone for right-handed users while remaining mirrorable through OwnMessenger.

The account control is not required to remain visible inside an active conversation because account switching is not part of normal message composition. It remains reachable through the Omnibar.

## 3. Multiple enabled tabs

When the user enables multiple top-level views, a tab row appears below the Omnibar:

```text
┌─────────────────────────────────┐
│ current root view               │
│                                 │
├──────────────────────────┬──────┤
│  Find or do…         🎙  │  ●   │
├─────────────────────────────────┤
│ Chats      Calls      Spaces    │
└─────────────────────────────────┘
```

The tab strip is therefore **conditional chrome**. It exists only when it adds navigational value.

Four visible items is a common compact-width reference, not a fixed semantic limit. If enabled tabs exceed safe capacity, the row scrolls horizontally with a visible partial-next-item cue or equivalent non-motion affordance.

## 4. Conversation shell

Inside a conversation, the composer owns the lower reach zone:

```text
┌─────────────────────────────────┐
│ ‹  Maria                    ⋯    │
├─────────────────────────────────┤
│                                 │
│  Hey, are you still coming?     │
│                                 │
│                  Yes, on my way │
│                                 │
│                                 │
├─────────────────────────────────┤
│ ＋  Message Maria…   ◉     ➤    │
└─────────────────────────────────┘
                      Omni   Send
```

The dedicated `◉` is only a prototype symbol for the future OM/Omnibar glyph. Final brand iconography is deliberately not frozen.

Recommended trailing control order for a right-handed default:

1. Send / voice-message action at the far trailing edge;
2. Omnibar immediately beside it;
3. message field;
4. attachment entry on the leading edge.

OwnMessenger may mirror this arrangement. OM must not silently infer handedness from behavior.

## 5. Composer → Omni mode

Tapping the Omnibar control transforms the lower surface rather than stacking another field:

```text
┌─────────────────────────────────┐
│ Maria                           │
│ conversation remains visible    │
│ ─────────────────────────────── │
│ Suggested                       │
│  Call Maria                     │
│  Search this chat               │
│  Show shared files              │
│  Mute until tomorrow            │
│                                 │
├─────────────────────────────────┤
│ ×  Find or do…              🎙   │
└─────────────────────────────────┘
```

The user is never asked to decide whether typed text will become a message or a command. Only one semantic text-entry mode exists at a time.

When Omni mode exits, the prior message draft, reply target, text selection, staged attachments, and cursor position are restored exactly.

## 6. Omnibar focused state

With no query, suggestions appear in this precedence order:

1. explicit user pins;
2. context-relevant actions;
3. recent/frequent local targets;
4. local learned predictions, when enabled;
5. stable generic actions such as Scan QR or Invite someone.

Example:

```text
┌─────────────────────────────────┐
│ Pinned                          │
│  Family                    Chat │
│  Scan QR                 Action │
│                                 │
│ Suggested                       │
│  Call Maria               Action│
│  Search: airport           Query│
│                                 │
├─────────────────────────────────┤
│  Find or do…               🎙    │
└─────────────────────────────────┘
```

When a query exists, strong literal matches and explicit intent outrank learned prediction.

## 7. Adaptive placeholder hints

The accessible name is always stable: **Omnibar**.

The visible placeholder may optionally adapt locally, for example:

- `Find or do…`
- `Message someone…`
- `Call someone…`
- `Scan an invite…`
- `Find a message…`

If local learning is enabled, a personalized hint such as `Call Maria…` may be used only when allowed by the user's privacy-display preferences.

Rules:

- placeholder changes must not animate continuously;
- no attention-seeking rotation;
- update only at natural state boundaries such as focus, view change, or app resume;
- contact/message content must not leak through a privacy-sensitive screen state;
- placeholders never change command semantics;
- disabling learning falls back to generic/contextual hints.

## 8. Chat ordering

Default conversation ordering is latest activity first.

OwnMessenger may expose:

- oldest activity first;
- pinned-first variants;
- manual collections/views;
- future alternative sort modes.

The default must remain latest-first because it matches broad learned messenger behavior.

## 9. First-install empty state

No contacts are uploaded by default. A new account may therefore legitimately have no conversations.

Recommended first surface:

```text
┌─────────────────────────────────┐
│                                 │
│              ◉                  │
│                                 │
│       Start a conversation      │
│                                 │
│  Invite a friend, family member,│
│  classmate, or workmate using   │
│  whatever you already use.      │
│                                 │
│       [ Invite someone ]        │
│       [ Scan a QR code ]        │
│       [ Show my QR ]            │
│                                 │
├──────────────────────────┬──────┤
│ Find or do…          🎙  │  ●   │
└──────────────────────────┴──────┘
```

OhMessenger may make this surface playful and warm, but it must not pressure address-book upload, social-graph expansion, or bulk invitations.

The primary CTA opens an invite surface offering:

- one-time invite for one person;
- separately generated one-time invites;
- reusable/expiring invite where architecture permits;
- QR display;
- direct QR scanning;
- share via existing messenger, email, OS share sheet, copy link, or equivalent;
- PWA install/resume flow when OM is not yet installed.

## 10. Structured groups/spaces

Direct chats and ordinary groups remain in Chats.

A richer channel-based community may expose an optional **Spaces** top-level tab. Compact mobile does not permanently display a Discord-style server rail by default.

Recommended behavior:

```text
Chats                     Spaces
  Maria                      Study Group
  Family                     Community
  Workmates                  Project OM
```

Entering a Space may reveal its internal channels through an inner drawer/pane while preserving the outer OM shell grammar.

On tablet/desktop, the same structure may unfold into a persistent left rail + channel pane. OwnMessenger may later offer a compact persistent space rail to users who prefer a Discord-like organization.

## 11. Navigation semantics

- **Tabs change stable top-level place.**
- **Omnibar finds or does.**
- **Composer communicates with the current conversation.**
- **Sheets perform short contextual tasks without pretending the user changed place.**
- **Back returns to the previous interaction context.**

These roles must remain distinct even when visual styling changes.

## 12. Provisional performance budgets

The Omnibar is only successful if it feels immediate. These are prototype budgets, not final conformance numbers:

- touch/focus response begins immediately on the next rendered frame;
- opening Omni mode must never wait for network access;
- pinned/contextual/local suggestions should normally be visible within **100 ms** on supported baseline devices;
- keystroke-to-local-result update should normally remain within **100 ms**;
- deeper local full-text results may stream progressively after the first useful set;
- optional remote/provider results never block local results;
- optional ML/ranking refinement never blocks typing or deterministic matches.

Actual release thresholds must be derived from profiling on representative low/mid/high devices, with mobile PWA included in the baseline.

## 13. Reduced motion

The same shell states remain available with reduced motion.

Normal mode may use a short geometry/role morph to explain the relationship between Composer and Omnibar. Reduced-motion mode uses an immediate replacement or restrained crossfade while preserving:

- draft state;
- focus destination;
- result hierarchy;
- accessible state announcement.

## 14. Prototype questions still open

The following are intentionally not frozen:

- final OM/Omnibar glyph;
- exact control dimensions and typography;
- whether attachment stays always visible or becomes context-dependent at extreme narrow widths;
- exact spring/timing tokens;
- whether the root account control is avatar, identity glyph, or account-stack indicator;
- exact treatment of the optional persistent Spaces rail on compact devices;
- final thresholds for local result latency.

These should be answered by prototype testing rather than aesthetic preference alone.
