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

Theme import validates schema, resource hashes, MIME/type expectations, decompression limits, dimensions and total size before activation. Font files in a theme are hostile content and also pass the font sanitizer (§Fonts).

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
- safety numbers, verification codes and QR verification text;
- permission, recovery and passkey prompts;
- service/automation authorship labels ("Assistant", "Alice · via Assistant");
- bridge-active and "OM encryption ends at Bridge X" indicators;
- foreign-provenance and `foreign_actor_unverified` labels;
- full-Room service and other plaintext-recipient roster entries, and counterparty notices for representing services;
- device-link and connected-service notices;
- focus indicator;
- accessibility/high-contrast override.

The plugin host's chrome list in [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) is the same list (ADR-0129).

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
- user accessibility text-size override wins over theme preference.

**Security-reserved surfaces always use built-in fonts (ADR-0129).** A font can remap glyphs through `cmap` or GSUB so the rendered text differs from the string underneath, and it stays perfectly legible while doing so. Every reserved surface in §Reserved semantic UI (including safety numbers, codes, URLs and link destinations, principal and bridge labels, identity names inside warnings, and permission and recovery prompts) therefore renders with the client's built-in fonts, whatever the theme says. ~~Security dialogs can fall back to known-good fonts if a theme font compromises legibility.~~

**Imported fonts are hostile content** (ADR-0062). Every font from a theme or an import passes an OTS-class font sanitizer (table whitelist, bounds checks, no hinting bytecode or other executable tables the renderer does not need) before first use, and is rejected if it fails. A theme-compiler test uses a glyph-remapping font and checks that no reserved surface changes.

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

## 14:34 engineering refinement — Auto appearance and per-profile themes

Every built-in theme MUST ship an authored light and dark semantic palette. The normal appearance choice is **Auto**, following the platform's light/dark preference without changing theme identity.

Custom theme bundles must provide valid light+dark variants, or the importer may derive a candidate counterpart and require validation/preview before activation. Contrast, focus, destructive/security states and high-contrast overrides are validated independently in both modes.

OM should ship several calm, broadly appealing curated themes rather than one default plus novelty skins. When a user later creates another `IdentityProfile`, the UI MAY offer a distinct curated theme as an optional visual cue. Theme choice remains local/profile presentation state and never becomes public identity/discovery metadata.

The future profile switcher must not flash/reload the application: preserve per-profile draft, selected conversation and navigation state and switch presentation/state atomically.

## CONFORMANCE-UI-THEMES

The following are normative pass/fail cases for the registered W0 conformance suite. Passing these cases is evidence; it does not by itself authorize a scope.

| ID | Case | Pass condition |
|---|---|---|
| TH-01 | hostile theme values | compiler clamps/rejects unsafe values; no executable content |
| TH-02 | theme targets reserved security UI | override is rejected/ignored |
| TH-03 | custom font on reserved surface | built-in trusted font remains in use |
| TH-04 | light/dark/auto | all required states remain legible and accessible |
| TH-05 | missing optional theme asset | deterministic safe fallback, no network code execution |
