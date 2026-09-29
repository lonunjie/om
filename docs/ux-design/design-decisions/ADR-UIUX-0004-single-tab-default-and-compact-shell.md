# ADR-UIUX-0004 — Single-Tab Default and Compact Shell Prototype

**Status:** Accepted for prototype validation  
**Date:** 2026-09-28

## Context

Most messenger users spend most of their time in conversations. Permanent Calls/People/other tabs consume space even for users who rarely open them. OM also has a universal Omnibar capable of reaching these functions without permanent navigation.

The compact conversation view additionally has a lower-shell conflict between message composition and global find/do access.

## Decision

1. **Chats is the only default top-level tab.**
2. If only one tab is enabled, the tab strip is hidden.
3. OwnMessenger lets users expose/reorder optional stable views such as Calls, People, Spaces, and Files/Media.
4. The root Omnibar remains bottom-placed by default.
5. The root account control defaults to the trailing lower edge and is mirrorable/customizable.
6. Inside conversations, the composer owns the bottom edge.
7. A dedicated Omnibar trigger sits adjacent to the trailing Send/voice control by default and semantically transforms the composer surface into Omni mode.
8. Structured Discord-like spaces remain optional and progressively revealed rather than becoming the universal direct-message shell.

## Consequences

### Positive

- very low default chrome;
- aligns with users who primarily message;
- retains fast access to all other capabilities;
- improves one-handed reach;
- keeps iOS/Android/mobile-PWA shells almost identical;
- makes navigation complexity user-owned rather than product-imposed.

### Costs

- Omnibar discoverability becomes important;
- hidden optional tabs require excellent customization/search paths;
- account and Omni glyphs need strong small-size identity;
- structured-space users may choose a more complex shell preset.

## Validation required

Prototype testing must verify:

- users discover Omnibar without onboarding slides;
- Composer vs Omni mode is never confused;
- optional tabs remain discoverable through customization;
- one-handed reach works for both leading/trailing preferences;
- a single hidden tab strip does not reduce orientation;
- first-install invite flow is understandable with zero contacts.
