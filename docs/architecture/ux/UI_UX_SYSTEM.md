# OM Shared UI/UX System

- **Checkpoint:** 2026-09-22 13:42 PHT
- **Status:** canonical product/client design direction
- **Scope:** reference client information architecture, responsive shell, user journeys, interaction semantics, settings model, and cross-platform consistency

## Product intent

OM should feel like an ordinary, polished messenger even though its infrastructure, identity and trust architecture are unusual.

> **Reveal capability, not complexity.**

The default experience is ergonomics and functionality first, aesthetics carefully integrated rather than layered on afterward: simple, uncluttered, peaceful, fast, and immediately legible.

Security should normally be silent. Users should see security UI only when they must understand or act on something.

## Shared client stack

```text
OM Protocol
    ↓
OM Core
    ↓
OM UI
    ↓
PlatformAdapter
    ↓
PWA / iOS / Android / Desktop
```

`om-ui` is one shared implementation of the application experience. Native shells expose OS capabilities and system surfaces; they do not duplicate conversation/settings/product semantics.

### What belongs in `om-ui`

- navigation and adaptive shell;
- chats, calls and people screens;
- conversation rendering and composer;
- discovery, invitations and message requests;
- contact sharing;
- group UI;
- in-app call UI;
- settings, privacy profiles, recovery and devices;
- OM Home health/settings;
- theming and design system;
- accessibility behavior that can be shared;
- local view state and navigation state.

### What stays native/platform-owned

- Keychain/Keystore/Secure Enclave/StrongBox access;
- APNs/FCM/Web Push transport implementation;
- CallKit/Telecom lock-screen/system call surfaces;
- OS share sheet and file picker;
- permission prompts;
- biometric prompts;
- camera/microphone integration where native APIs are required;
- native background execution;
- SMS/MMS/RCS adapters later.

## Primary information architecture

Only three persistent primary destinations:

1. **Chats** — 1:1 conversations, groups, message requests.
2. **Calls** — recent/missed calls and call initiation.
3. **People** — accepted contacts, requests, share/discovery actions.

Settings opens from the user avatar/profile. Groups are conversations, not a separate social/community product. Files/media remain conversation-scoped.

## Adaptive window model

Layout responds to available application window, not platform/device labels.

Initial implementation classes:

| Class | Width | Default structure |
|---|---:|---|
| Compact | `<600 CSS px` | single pane + bottom navigation |
| Medium | `600–839` | navigation rail/compact shell, usually one main pane |
| Expanded | `840–1199` | navigation rail + list/detail |
| Wide | `≥1200` | navigation rail + list + detail + optional inspector |

These are prototyping values, not protocol constants. Validate with actual content, localization, text scaling and foldable/split-screen use.

Selected conversation, draft, scroll position and modal context survive resize/reorientation whenever possible.

## Navigation behavior

### Compact

- bottom bar: Chats / Calls / People;
- app bar contains conversation/person title and the few highest-value actions;
- system back gesture/button navigates naturally;
- details/settings open as full-page or sheet based on context.

### Expanded / Wide

- navigation rail at left;
- list/detail layout for Chats, Calls and People;
- optional inspector/details pane in Wide;
- keyboard shortcuts and command palette become first-class.

Recommended shortcuts:

- `Cmd/Ctrl+K`: universal search / command palette;
- `Cmd/Ctrl+N`: new chat;
- `Esc`: close/back;
- standard platform conventions for copy/paste/search/navigation.

## Onboarding

The onboarding sequence should not expose cryptographic or Cloudflare terminology.

### 1. Welcome

Primary action: **Create my OM**. Secondary: **Restore an identity**.

### 2. Profile

Friendly name and optional avatar. AccountID and cryptographic material are generated silently.

### 3. Set up your OM home

> **2026-09-24:** when onboarding starts from an invite that carries sponsorship, this step is replaced by "You're in — using Alice's home for now" (T0). Without an invite, the user chooses a community home, sets up their own, or links an existing one. See [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §7. The continuity check (ADR-0106) appears after the first conversation.

Plain-language explanation:

> Your OM home delivers messages when your devices are not online. It belongs to you, and you can move it later.

Provider deployment is automated. Progress language describes outcomes, not vendor products:

- Creating your address
- Setting up message delivery
- Preparing notifications
- Finishing

Provider-specific detail belongs under Advanced/Troubleshooting.

### 4. Ready

Show QR/share link with obvious actions:

- Share my OM
- Scan someone's OM
- Done

Recovery is optional and does not block onboarding.

## Discovery UX

The New Chat surface accepts multiple discovery forms without requiring the user to understand them:

- scan QR;
- tap/paste `om:` invitation;
- paste an OM contact address/link;
- enter an organization/FQDN identity;
- use a shared ContactCapsule.

No uploaded address-book discovery in v1.

## Share Contact

Carol can share Alice to Bob.

- Carol forwards Alice's signed, expiring ContactCapsule.
- Bob sees Alice plus local provenance such as **Shared by Carol**.
- Bob verifies Alice's own signature, not Carol's authority.
- Bob still sends a normal introduction request to Alice.
- Carol never receives Alice↔Bob pairwise delivery capabilities.
- Referral provenance remains local context, never a global trust score.

## Message requests

Unknown senders stay outside the normal relationship until accepted.

The request surface supports:

- Accept;
- Block;
- optional local/report-export actions;
- tightly constrained content prior to acceptance.

Calls, arbitrary attachments and normal relationship capabilities are unavailable until acceptance.

## Conversation UI

Use familiar messaging patterns:

- title/avatar + call actions;
- message bubbles;
- reply/reaction/edit/delete/copy/share;
- attachments and voice messages;
- disappearing and view-once content;
- composer with Attach / Message / Voice / Send.

Do not show routine padlock/encryption badges. Strong encryption is the baseline.

Exceptions: labels that say **who else can read or author** are not routine badges and are always shown (ADR-0129). They are reserved security UI ([`THEMES_FONTS_AND_PERSONALIZATION.md`](THEMES_FONTS_AND_PERSONALIZATION.md)): service/automation authorship ("Assistant", "Alice · via Assistant"), "Bridge active" and "OM encryption ends at Bridge X", foreign-provenance and `foreign_actor_unverified` labels, and the roster entry of any full-Room service or other plaintext recipient.

## Security events

Security attention is rare, calm and actionable.

Examples:

- **Alice's identity changed** — explain possible recovery/device replacement and offer Review.
- **One of your devices has been suspended** — explain consequence and open Devices.
- **Account changes are temporarily frozen** — explain contested control state and resolution path.
- **A new device was linked to your account** — the non-suppressible device-link notice; offer Suspend and open Devices.
- **A service or bridge can now read this conversation** — shown to every member when a full-Room service or bridge is added, or when a counterparty's messages start going to a representing service; name it and offer Review.
- **A connected service was added to your account** — a personal grant with plaintext read plus egress, representation or automation ([`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §2); offer Revoke and open Connected services.

Never use alarmist language for routine background rotations/upgrades.

## People

Contains:

- Requests;
- accepted contacts;
- Your OM / share QR/link;
- local-only contact-name mapping when OS Contacts permission is granted.

Local Contacts data never needs to leave the device.

## Calls

One shared in-app call UI across all official clients. Platform shells may own incoming-call system UI.

Route selection is automatic and invisible: LAN / P2P / TURN / SFU. Advanced call details may expose route and health, but route never changes E2EE semantics.

## Protection presets

User-facing presets:

- **Recommended** — strongest crypto with strong privacy and convenience;
- **Hardened** — less exposed metadata and more conservative routing/push;
- **Maximum Privacy** — minimize metadata even when immediacy/convenience suffers;
- **Custom** — user has changed component policies.

All presets keep the same approved cryptographic floor. They change policy, not cipher strength.

## Notifications

Vendor push sees only opaque wakeup information. Message text/sender previews are rendered locally after fetch/decrypt.

- Recommended: local sender/message preview allowed by user preference.
- Hardened: reduced preview by default.
- Maximum Privacy: generic notification or vendor push disabled with explicit delivery-immediacy tradeoff.

## Recovery UX

Settings → Identity → Recovery.

Default is **On** (ADR-0111). The screen names the custodian in plain words: "Your passkey can restore this identity" (a synced passkey), or "Your recovery key can restore this identity" when no passkey is available and always under Maximum Privacy. It also states the protection: a restore waits (Recommended 72 hours; Hardened and Maximum 7 days) and any of your devices can stop it.

**Turn off recovery** is an explicit, confirmed flow with the ADR-0111 warning: "If you turn recovery off and lose all your devices, this identity is gone for good. Nobody, including OM, can restore it." Turning it back on is one step.

Additional methods may include:

- recovery key (added alongside a passkey, or instead of it);
- trusted people/guardians;
- spare control-capable device;
- future reviewed methods.

Identity recovery and message-history recovery remain separate concepts in the UI.

## Device UX

Show ordinary linked devices and whether they can approve account-control changes.

Primary emergency action is **Suspend this device**, mapping to immediate quarantine. Permanent revocation/control changes follow AccountControlState authorization underneath.

Normal UI should not use internal terms such as constitutional quorum unless Advanced/diagnostic detail is requested.

## OM Home

User-facing infrastructure abstraction.

Basic surface:

- status: Online / Needs attention;
- OM address;
- message delivery health;
- storage/backup summary.

Advanced:

- provider;
- relay set;
- blob/storage provider;
- quota/budget status;
- move/repair/export tools;
- raw diagnostics.

## Offline and LAN

Existing conversations remain readable/searchable offline.

When WAN is unavailable:

> Internet unavailable — local contacts may still work.

Known LAN peers may show a quiet local availability indicator such as **Nearby**. LAN discovery never exposes stable AccountID/usernames to unrelated devices.

## Search and command palette

Search is a first-class global surface, not a feature hidden inside each section. It may search:

- chats and people;
- messages/files locally;
- settings/help;
- commands/actions;
- later, organization discovery.

Search results must preserve privacy policy and never silently upload local indexes.

## Reversible actions

Prefer **Undo** to confirmation dialogs when actions are safely reversible. Reserve blocking dialogs for truly irreversible/high-impact changes such as destructive recovery changes or identity deletion.

## State preservation

Drafts, selection, scroll position and transient navigation state should survive:

- resize/orientation change;
- background/foreground;
- temporary network loss;
- navigation to settings/details and back.

The product should feel continuous rather than page-based.

## Default aesthetic

- quiet and content-first;
- restrained accent;
- subtle surface hierarchy;
- no unnecessary cards/borders;
- no privacy/cybersecurity visual branding;
- dark/light/high-contrast first-class;
- generous hit targets and readable typography;
- aesthetics support comprehension rather than compete with it.

## Multiple identities (architecture now, UI later)

`om-ui` is profile-aware from its first state model, and the V1 reference client exposes switching when two or more profiles exist (ADR-0148). Switching is intentionally simple: avatar/profile chip -> choose Personal/Work/Temporary/etc. -> immediate switch without logout/reload.

Each profile preserves its own drafts, navigation, notification/privacy settings and optional theme. New profiles may be offered a different curated theme, but the user can inherit the global appearance instead.

All themes support Auto light/dark. Appearance transitions should avoid full-screen flashes and respect reduced-motion.

## Rich-text composer

The composer should make Telegram-class formatting effortless: selection toolbar/context commands and keyboard shortcuts for bold, italic, underline, strikethrough, spoiler, inline code, quote/code block and links. It serializes only `RichTextV1`; pasted HTML/Markdown is input to a sanitizer/converter, never rendered/sent as executable markup.
