# Research-Constrained Preset Theme Pairs

## 1. Principle

OM ships **curated light/dark pairs**, not a bag of unrelated color swatches.

Every preset inherits the same functional contracts:

- readable foreground contrast;
- protected status meaning;
- visible focus;
- redundant non-color state cues;
- safe hit targets;
- light/dark semantic equivalence;
- Reduced Motion and high-contrast compatibility.

The research supports these constraints. It does **not** establish that any exact decorative hue is universally best. Presets therefore remain testable design hypotheses.

## 2. Preset A — OM Clear (default)

Personality: **clear, calm, precise, quietly friendly**.

- cool-neutral surfaces;
- cobalt/azure action family;
- violet focus;
- restrained blue outgoing-message tint;
- conventional protected green/amber/red statuses.

Purpose: strongest general-purpose default for first-run users and the baseline against which other themes are compared.

See [`DEFAULT_THEME_AND_COLOR_SEMANTICS.md`](DEFAULT_THEME_AND_COLOR_SEMANTICS.md).

## 3. Preset B — OM Paper

Personality: **warm, human, low-glare without becoming low-contrast**.

Changes from Clear:

- light surface shifts toward warm paper neutrals;
- dark surface shifts toward warm charcoal;
- interactive action hue remains blue-family to preserve familiar signification;
- status families remain protected.

Candidate pair:

- Light base: `#FBF8F3`; raised: `#FFFDFC`; primary text: `#241F1A`; accent: `#075FAB`.
- Dark base: `#181512`; raised: `#231E1A`; primary text: `#F6F1EA`; accent: `#75B9F3`.

Use case: people who dislike cool-gray interfaces but still want the default action grammar.

## 4. Preset C — OM Iris

Personality: **expressive but disciplined**.

- subtly plum-neutral surfaces;
- violet action family;
- focus uses a distinct cyan/blue ring so action and keyboard focus remain separable;
- protected green/amber/red status roles remain semantically stable.

Candidate pair:

- Light base: `#FAF8FC`; primary text: `#241C2B`; accent: `#7041C7`; focus: `#006A9C`.
- Dark base: `#161219`; primary text: `#F7F1FA`; accent: `#C5A7FF`; focus: `#6DCBFF`.

Use case: OwnMessenger users who want more character without entering Playful-mode visual noise.

## 5. Preset D — OM Graphite

Personality: **quiet, technical, concentration-first**.

- near-neutral surfaces;
- minimal decorative chroma;
- blue remains reserved for action;
- ideal base for Focus profile and keyboard-heavy use.

Candidate pair:

- Light base: `#F8F8F8`; raised: `#FFFFFF`; primary text: `#1D1D1F`; accent: `#005FBD`.
- Dark base: `#121212`; raised: `#1D1D1F`; primary text: `#F2F2F3`; accent: `#68B4FF`.

Use case: dense work contexts and users who prefer color only when it carries information.

## 6. Why there is no default red, amber, or green brand theme

Those hues already carry strong status conventions. OM can eventually allow advanced user-selected accents from these ranges, but curated defaults should avoid making ordinary actions visually compete with danger/warning/success signals.

If a custom theme selects a semantically crowded accent, OM must maintain protected status differentiation through hue/lightness, iconography, labeling, geometry, and contrast.

## 7. Theme pair QA matrix

Every shipped pair must be tested for:

1. normal and large text contrast;
2. non-text control/focus contrast;
3. common color-vision-deficiency simulations;
4. grayscale distinguishability of critical states;
5. bright and dim ambient conditions;
6. OLED and LCD behavior;
7. iOS, Android, Chromium/WebKit PWA rendering;
8. Comfortable/Compact/Large density;
9. Focus/Balanced/Playful profiles;
10. Reduce Motion and Increased Contrast;
11. long chat-list and conversation scanning;
12. wallpapers/media behind conversation surfaces.

A preset failing functional QA is not shippable regardless of popularity or aesthetics.
