# OM Themes, Fonts and Personalization

- **Checkpoint:** 2026-09-22 13:42 PHT
- **Product idea:** OM can also mean **OwnMessenger** — users can make it visually theirs without turning themes into an execution or privacy boundary.

## Design goal

The default OM theme is carefully tuned for ergonomics, clarity, accessibility and calm. Personalization is optional and may be extensive, but it cannot change security semantics or gain message/account authority.

## Theme capability

A theme may customize, within safe bounds:

- semantic color palette;
- light/dark variants;
- typography and bundled/local fonts;
- message-bubble shape;
- corner/radius system;
- spacing/density presets;
- wallpapers/backgrounds;
- icon visual style/accent when compatible with semantic icons;
- notification/call sounds;
- motion intensity/easing preset within safe limits;
- per-conversation appearance where enabled.

## Theme format

Themes are **declarative data-only bundles**. No JavaScript, WebAssembly, native modules, HTML fragments, arbitrary CSS, template expressions, or executable plug-ins.

Conceptual bundle:

```text
my-theme.omtheme/
  manifest.json
  tokens.json
  fonts/
  wallpapers/
  sounds/
  icons/
  integrity.json
```

Theme import validates schema, resource hashes, MIME/type expectations, decompression limits, dimensions and total size before activation.

## Hard security boundaries

Themes MUST NOT:

- execute code;
- fetch arbitrary remote assets;
- embed analytics/tracking beacons;
- access messages, contacts, AccountID, provider configuration or relay endpoints;
- change link destinations or rendered canonical URLs;
- create clickable overlays over security-sensitive controls;
- hide, remove or materially disguise critical security state;
- spoof OS permission/biometric/password prompts;
- redefine the semantic meaning of icons/actions;
- modify crypto, recovery, account-control, relay or protocol policy;
- inject HTML into message rendering;
- register protocol handlers;
- change accessibility labels/roles;
- silently lower contrast below enforced bounds.

## Reserved semantic UI

The following remain under OM semantic control:

- destructive actions;
- account/identity discontinuity warnings;
- device suspension/control-state conflicts;
- unsafe/unsupported transport warnings;
- verified/changed identity state;
- external-link destination presentation;
- focus indicator;
- accessibility/high-contrast override.

A theme may supply stylistic variants only inside enforced semantic and contrast constraints.

## Web/PWA hardening

The reference web client should use a strict Content Security Policy and enforce Trusted Types where supported. Theme data is compiled to known-safe CSS variables/design tokens by trusted code; theme files never directly enter HTML/script injection sinks.

Recommended direction:

- no `eval`/dynamic code generation;
- no theme-provided HTML/CSS selectors;
- no remote font/image URLs in theme manifests by default;
- `object-src 'none'`, strict `script-src`, controlled `style-src` compatible with the implementation;
- Trusted Types enforced for DOM injection sinks in supported runtimes;
- dedicated sanitized rendering path for any rich message content unrelated to theming.

## Fonts

Users may choose:

1. built-in/curated fonts;
2. fonts bundled inside a theme;
3. explicitly imported local font files;
4. platform-local fonts only through explicit user permission where an OS/browser API allows it.

Never silently query or upload the user's installed-font list. Never load a third-party web-font CDN merely because a theme specifies a font name.

Font engine constraints:

- minimum readable size and line-height;
- fallback chains for missing scripts/glyphs;
- locale/script coverage warnings;
- bounded weight/stretch settings;
- user accessibility text-size override wins over theme preference;
- security dialogs can fall back to known-good fonts if a theme font compromises legibility.

## Theme identity and sharing

Themes may have a content hash and optional author signature, but a signature indicates provenance, **not trustworthiness**.

Exported themes must contain no:

- user AccountID;
- conversation/contact identifiers;
- relay/provider endpoints;
- notification subscriptions;
- analytics identifiers;
- history/search data.

## Theme preview

Before apply, preview at minimum:

- conversation list;
- chat bubbles/composer;
- calls;
- settings;
- buttons/focus;
- warning/destructive state;
- light/dark mode;
- high-contrast/accessibility override.

## Density is separate from theme

Offer an explicit density preference such as:

- Comfortable;
- Compact.

Density is an ergonomic choice and should not be hidden inside decorative themes. Minimum hit-target and readability constraints still apply.

## Per-conversation themes

Permitted as presentation-only local state. Do not synchronize a custom theme to another participant unless they explicitly import/accept it. Theme selection must never become observable conversation metadata to relays.

## Future theme ecosystem

A future public theme catalog can exist as a replaceable content provider, not a protocol authority. Users can install themes from files/links/repositories without a mandatory OM marketplace.
