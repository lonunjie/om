# OM Icon Taxonomy and Core Inventory

**Status:** Normative semantic inventory for Run 11A.  
**Artwork:** To be produced in Run 11B.

## 1. Taxonomy

OM functional symbols are grouped by semantic job rather than by screen.

### A. Shell and navigation
Navigation, account, search, and structural movement.

### B. Messaging actions
Actions performed on conversations/messages.

### C. Calling and live communication
Voice/video/live-presence actions.

### D. Compose and attachment
Input creation and media/file insertion.

### E. Status and recovery
Pending, success, failure, retry, unread, delivery.

### F. Identity, privacy, and security
Verification, device trust, recovery, keys, account safety.

### G. People and organization
Person, group, Space, invite, QR, membership.

### H. Media/content
File/image/video/audio/location/contact/link/poll.

### I. System/common utilities
Settings, copy, share/export, download/upload, sort/filter.

## 2. Familiarity tiers

**Tier U — Universal/common metaphor**  
Use a familiar metaphor unless testing shows a strong problem.

**Tier O — OM/common-messenger concept**  
May require OM-owned drawing, but should preserve familiar messenger semantics.

**Tier N — Novel/OM-specific concept**  
Requires visible label until comprehension is demonstrated.

**Tier S — Security-sensitive concept**  
Icon is always supplementary to explicit wording at consequence/confirmation points.

## 3. Label policy

- `icon-ok`: may be icon-only in strong context, with accessible name/tooltips as applicable.
- `label-context`: visible label required when context does not make meaning obvious.
- `label-preferred`: use text in normal menus/results; icon-only allowed only in highly familiar compact controls.
- `label-required`: icon never carries the action alone in the relevant surface.

## 4. Initial core inventory

The machine-readable source of truth is `contracts/om-icon-inventory-v0.json`.

High-priority Run 11B artwork should begin with these groups:

### Shell
- `omni`
- `search`
- `back`
- `forward-nav`
- `close`
- `more`
- `account`
- `settings`
- `chats`
- `calls`
- `people`
- `spaces`

### Messaging
- `send`
- `reply`
- `react`
- `edit`
- `forward-message`
- `copy`
- `pin`
- `unpin`
- `archive`
- `delete`
- `mark-unread`
- `mute`
- `unmute`

### Calling
- `call`
- `video-call`
- `call-end`
- `mic`
- `mic-off`
- `camera`
- `camera-off`
- `speaker`

### Compose/content
- `attach`
- `add`
- `image`
- `file`
- `audio`
- `location`
- `contact`
- `link`
- `poll`
- `emoji`

### Status/recovery
- `pending`
- `sent`
- `delivered`
- `read`
- `failed`
- `retry`
- `download`
- `upload`

### Identity/security
- `invite`
- `qr-scan`
- `qr-show`
- `verify`
- `verified`
- `device`
- `devices`
- `lock`
- `security-warning`
- `recovery`

## 5. Selection and navigation

Navigation icons may have outline and filled/current variants when the pair remains recognizable.

Do not require fill to understand the tab; current state also uses component semantics such as text, container, indicator, or accessibility state.

## 6. Security symbols

Security symbols are especially vulnerable to false confidence.

Rules:
- a lock does not automatically mean "verified";
- a check does not automatically mean "cryptographically verified";
- "verified" requires an explicit verification state from the underlying model;
- warning symbols never imply compromise unless OM actually knows compromise occurred;
- security actions are paired with precise text.

The icon vocabulary must not claim stronger security than the protocol state provides.

## 7. Omnibar symbol

`omni` is provisional and special.

It may eventually derive from the OM identity mark if:
- it remains legible at 20–24 px;
- people understand it as the OM-wide find/do surface after normal use;
- it does not get confused with account/home/send;
- it has an accessible name such as "Omnibar" or localized equivalent.

Until the brand mark is frozen, the semantic role exists independently from its final artwork.

## 8. Reply vs forward

Reply and forward are directional relations and must be clearly distinguishable.

In RTL:
- directional treatment follows reading/conversation flow where appropriate;
- do not mirror unrelated media/control glyphs merely because the UI direction changes.

## 9. Destructive actions

`delete` and other destructive icons:
- use familiar metaphors;
- use danger color only when consequence is active/foregrounded;
- receive visible text in menus/confirmations where destructive meaning matters;
- never rely on color alone.

## 10. Future inventory growth

New icon proposals must state:
1. semantic ID;
2. meaning;
3. action/object/state type;
4. familiarity tier;
5. label policy;
6. risk class;
7. RTL behavior;
8. variants;
9. why an existing icon cannot serve the meaning;
10. comprehension-test requirement.

This prevents uncontrolled icon proliferation.
