# OM Invite and QR Media Specification

**Status:** Normative Run 11B specification.

## 1. Purpose

Invite media is a first-contact UX surface. It must be easy to scan/share, clear about invite scope, privacy-preserving, and usable before the recipient has OM installed.

## 2. QR reliability is more important than branding

OM QR rendering MUST:
- preserve a clear four-module quiet zone on every side;
- use high-contrast dark modules on a light backing surface by default;
- keep modules square and crisp;
- avoid decorative logo overlays in the default QR;
- avoid gradients/textures inside code modules;
- avoid theme colors if they materially reduce scan contrast;
- include a link/action fallback outside the QR.

The QR plate may remain light even inside a dark invite card. This is intentional functional color/contrast, not a failure to theme.

QR Code itself is standardized as ISO/IEC 18004. DENSO WAVE's implementation guidance specifies a four-module quiet zone around the symbol. OM uses that as the default minimum.

## 3. Invite semantics are visible, not inferred from color

Invite cards MUST state, when applicable:
- one-time vs reusable;
- expiry/bounded lifetime;
- intended recipient restriction, if any;
- whether approval is required;
- that opening the link does not upload an address book.

One-time/expiring state may use an icon or color, but text remains the authoritative cue.

## 4. Share card hierarchy

Default hierarchy:
1. human purpose — "Join me on OM" / equivalent localized phrase;
2. invite scope — one-time/reusable/expiry;
3. QR;
4. open/install action;
5. concise privacy context;
6. optional inviter display name/avatar when intentionally included.

Do not expose internal AccountIDs, cryptographic fingerprints, provider identifiers, or secret invite material as explanatory text unless the protocol design explicitly requires it.

## 5. Deep-link/install behavior

An invite URI should be versioned and transportable through ordinary apps. Opening it should:
1. open OM when an appropriate client is installed;
2. otherwise open the PWA/install path;
3. preserve the pending invite across install/launch where platform rules permit;
4. ask for explicit acceptance before creating the relationship;
5. clearly report expired/used/invalid invitations.

The visual design cannot define the protocol URI format; that is merged later with architecture.

## 6. Theme behavior

Light and dark invite cards may differ, but the QR plate itself remains scan-optimized. The OM accent may style surrounding controls, never the QR modules by default.

## 7. Accessibility

- A QR image needs an accessible description of its purpose, not a transcription of encoded secret material.
- The same invite must be available through a conventional link/copy/share action.
- Critical expiry/one-time state is text, not color alone.
- Share-card text must meet the normal OM contrast gates.

## 8. Fixture assets

`assets/invite/v0/` contains non-production examples, including light/dark cards and a fixture QR that resolves only to an invalid example domain.
