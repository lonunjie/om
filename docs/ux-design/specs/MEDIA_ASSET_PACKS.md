# OM Media Asset Packs

**Status:** Normative Run 11B architecture.

## 1. Principle

Media packs are content, not code.

OwnMessenger and OhMessenger need a shared asset mechanism without allowing a theme/sticker/effect pack to become a plugin execution surface.

## 2. Supported classes

The initial architecture recognizes:
- theme tokens;
- wallpapers/backgrounds;
- sticker media;
- reaction media;
- expressive effect frames/media;
- optional sound sets;
- avatar/fallback styles;
- illustrations;
- future icon skins subject to stricter semantic validation.

## 3. Declarative-only security boundary

A pack MUST NOT:
- execute JavaScript or other code;
- contain event handlers;
- request network resources at render time;
- reference external fonts/images/scripts/stylesheets;
- remap OM actions;
- add permissions;
- invoke native APIs;
- weaken security confirmations;
- replace protected security/status semantics with unrelated artwork.

Untrusted SVG is treated as active-capable markup and must be parsed/sanitized into OM's allowed static subset before use. The Web platform's SVG specifications explicitly distinguish secure/static processing from script-capable SVG; OM should adopt the stricter model regardless of host platform.

## 4. Allowed SVG subset

For third-party packs, allow only a reviewed static subset such as:
- `svg`, `g`, `path`, `circle`, `ellipse`, `rect`, `line`, `polyline`, `polygon`;
- same-document reusable definitions only when required and sanitized;
- presentation attributes from an allowlist;
- no `script`, `foreignObject`, embedded HTML, event attributes, external `href`, or CSS imports;
- no network URLs.

Implementations may rasterize third-party SVG at install/import time as an additional isolation measure.

## 5. Manifest/integrity

Every pack has a manifest with:
- stable pack ID;
- semantic version;
- author/creator label;
- license declaration;
- asset paths and MIME types;
- SHA-256 of every asset;
- role metadata;
- optional light/dark pairing;
- static/reduced-motion fallback relationships;
- optional author signature.

The canonical draft schema is `contracts/om-asset-pack-manifest-v0.schema.json`.

## 6. Local-first install

Installing a pack should verify all declared hashes before activation. Missing/changed assets fail closed. Packs can be shared peer-to-peer or hosted anywhere in the future, but rendering does not depend on contacting the original host.

## 7. Theme vs behavior

Themes may alter approved presentation. Packs cannot change navigation, action semantics, confirmation requirements, privacy defaults, Action Registry mappings, or accessibility names.

## 8. Reduced motion and fallbacks

Every expressive animated asset that conveys more than decoration needs a static or reduced-motion rendering. Decorative OhMessenger media may disappear entirely in Focus/Reduced Motion profiles if no information is lost.

## 9. Sound packs

Sounds are optional expressive/presentation assets. Security-critical meaning may not be conveyed by sound alone. Volume/mute and system accessibility preferences remain authoritative.

## 10. Icon skins

Icon skins are the most restricted pack type because they can affect comprehension. They are future-facing and MUST pass the same semantic ID, label, risk, RTL, and comprehension validation as the built-in set. Until that validation pipeline exists, third-party icon skins should remain disabled by default.
