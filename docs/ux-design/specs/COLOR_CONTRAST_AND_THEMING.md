# Color, Contrast, Light/Dark, and Theming

## 1. Semantic color roles

Components consume roles, not literal colors.

Core roles include:

- `surface.base`
- `surface.subtle`
- `surface.raised`
- `surface.overlay`
- `text.primary`
- `text.secondary`
- `text.disabled`
- `border.subtle`
- `accent.primary`
- `accent.onPrimary`
- `state.hover`
- `state.pressed`
- `state.focus`
- `state.selected`
- `status.info`
- `status.success`
- `status.warning`
- `status.danger`
- `message.incoming.surface`
- `message.outgoing.surface`
- `message.failed`
- `message.pending`


## 1A. Default functional color grammar

The selected reference theme is **OM Calm**. It uses neutral surfaces for most UI area, cobalt/azure for ordinary action/information, violet for keyboard focus, green for success, amber for warning, and red for danger/failure.

This assignment is intentionally conservative: action color should be familiar, while status colors remain protected and visually scarce. Exact values and measured contrast ratios are in [`DEFAULT_THEME_AND_COLOR_SEMANTICS.md`](DEFAULT_THEME_AND_COLOR_SEMANTICS.md) and `research/CONTRAST_AUDIT_R7.md`.

The palette is an engineered prototype candidate, not a claim that research proves one exact hue is universally preferred.

## 2. Contrast is a contract

Themes must preserve accessible text and control contrast. Where formal platform/web standards impose stricter requirements, the stricter requirement wins.

OM targets at least WCAG 2.x AA-equivalent contrast for normal content and controls. OM Calm generally exceeds 4.5:1 even for secondary/tertiary meaningful text and key semantic colors. State indicators also require additional non-color cues.

## 3. Color never stands alone

Examples:

- failure = color + icon/state text or other semantic cue;
- selected = color/surface + geometry or mark;
- unread = weight/marker + color where useful;
- destructive = semantic label/icon + danger role;
- presence = text/icon/state, not only a colored dot where meaning matters.

## 4. Light and dark are first-class

Light and dark themes are not mechanical inversions.

Each semantic role has independently tuned light/dark values. Dark mode should avoid:

- pure-black/pure-white everywhere;
- excessive glowing saturated accents;
- low-contrast gray-on-gray hierarchy;
- shadows that have no visible meaning.

System theme following is the default. Per-account/per-profile overrides are allowed through OwnMessenger.

## 5. Accent customization

Users may select an accent family, but OM derives readable foreground, pressed, selected, and focus variants from it.

If a requested accent cannot satisfy required contrast in a role, the theme engine adjusts the rendered role rather than honoring an unsafe literal value.

## 6. Security/status roles are protected

Themes may restyle semantic status roles but cannot make them indistinguishable.

A theme cannot:

- map danger to the same appearance as success;
- hide verification state;
- remove failure distinction;
- make disabled controls look enabled;
- remove visible keyboard focus.

## 7. Conversation color

Message bubbles should use restrained contrast by default. The outgoing message need not become a fully saturated accent block.

This protects long-session readability and allows reactions/media/content to carry visual weight.

Themes can make conversations more expressive within accessibility limits.

## 8. User content and wallpapers

If a wallpaper/background reduces text or bubble contrast, the renderer must compensate with local surfaces, scrims, or bubble treatments. User-selected imagery never overrides readability.

## 9. Privacy-sensitive previews

Color/blur treatments used to conceal sensitive content must be semantic and accessible. Blur alone is not a security boundary and should never be described as one.

## 10. Color-scarcity rule

A semantic color loses value when it is used everywhere. Decorative treatments should prefer neutral or low-chroma theme surfaces. Saturated action/status colors are spent only where they improve recognition, orientation, or urgency.

## 11. Preset pair requirement

Shipped OwnMessenger presets are validated **light/dark pairs**. Each pair must pass the same contrast, focus, CVD-simulation, targetability, high-contrast, and state-separability matrix. See [`PRESET_THEME_PAIRS.md`](PRESET_THEME_PAIRS.md).
