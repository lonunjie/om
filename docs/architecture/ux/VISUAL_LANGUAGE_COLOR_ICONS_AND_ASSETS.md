# OM Visual Language — Color, Icons and Media Assets

- **Checkpoint:** 2026-09-29 Run 3
- **Status:** canonical reference-client visual-system design
- **Scope:** functional color, default theme family, icon semantics, media assets and theme-safe customization

## Principle

Visual design is part of interaction semantics. Color, icon shape, typography and motion may make state easier to recognize, but none is allowed to become the only carrier of meaning.

> **Use visual cues to shorten recognition; never make appearance an authority boundary.**

## Functional color model

OM uses semantic roles rather than component-local color literals.

### Neutral roles

- `canvas` — application background;
- `surface` — primary readable surface;
- `surface_subtle` — quiet grouping/hover/list state;
- `surface_elevated` — menus/sheets/popovers;
- `text_primary`;
- `text_secondary`;
- `text_disabled`;
- `border_subtle`;
- `divider`.

### Interaction roles

- `accent` — primary interactive emphasis;
- `accent_on` — content drawn on accent;
- `selection` — selected list/item state;
- `focus` — keyboard focus indicator;
- `link` — navigable text;
- `pressed` / `hover` — transient interaction state.

### Messaging roles

- `message_own_surface` / `message_own_text`;
- `message_other_surface` / `message_other_text`;
- `reply_context`;
- `mention`;
- `unread_marker`.

Message-bubble color never means verified/unverified identity, delivery authenticity or security level.

### Status roles

Built-in UI keeps a stable conceptual mapping:

- `info` — informational/neutral attention;
- `success` — completed/available/affirmative state;
- `warning` — caution/needs review;
- `danger` — destructive/error/high-impact state.

Every status also has text, icon/shape, accessible name or structural context. Green/red alone is never sufficient.

Security-reserved surfaces remain controlled by OM and can override theme color roles with known-good built-in mappings.

## Default theme and curated presets

The reference default is **OM Calm** and the curated built-in presets are **OM Calm, OM Paper, OM Iris and OM Graphite**,
as defined by [`PRESET_THEME_PAIRS.md`](../../ux-design/specs/PRESET_THEME_PAIRS.md) and
[`DEFAULT_THEME_AND_COLOR_SEMANTICS.md`](../../ux-design/specs/DEFAULT_THEME_AND_COLOR_SEMANTICS.md) (ADR-0159). Their values
are authored prototype seeds, not wire/protocol constants; the role semantics above and the contrast constraints below are
normative for every preset.

Curated defaults deliberately avoid red, amber and green brand accents because those hue families carry status meaning.
Each preset ships as an authored light/dark pair and passes the same automated contrast/security-state suite. High
contrast is an accessibility override, not a decorative extra theme. A user-selected accent that crowds a status role
keeps `warning` and `danger` distinguishable through hue/lightness, shape/outline, labeling and contrast rather than
collapsing the roles.

> Consolidation note (ADR-0166): the parallel Run-3 line proposed an "OM Calm" default plus Evergreen, Warm Slate and
> Violet families. Those palettes are not adopted because they conflict with the ADR-0159 preset set and its
> no-status-hue-brand rule; the structural rules in this document were adopted.

## Contrast and focus

Reference acceptance targets:

- normal text: WCAG 2.2 AA `>= 4.5:1` against its rendered background;
- large text: `>= 3:1`;
- meaningful UI component boundaries/icons/state graphics: `>= 3:1` against adjacent colors;
- OM's authored keyboard focus indicator targets the WCAG 2.2 Focus Appearance geometry/change-of-contrast guidance even though that success criterion is AAA: at least a 2 CSS-pixel-equivalent perimeter area and `>= 3:1` change where the indicator is authored by OM.

Where platform high-contrast/forced-colors behavior applies, platform/user accessibility state wins over theme styling.

## Icon system

Icons are semantic components, not interchangeable artwork.

### Geometry

Reference icon family:

- 24×24 logical grid for standard controls;
- primary optical content generally within ~20×20;
- 2 logical-pixel default stroke at 24×24, with optical corrections where necessary;
- rounded caps/joins by default;
- small-size variants are redrawn/optically corrected rather than blindly scaled;
- fill may reinforce selected/toggled state, but never replaces accessible state/labeling.

### Semantic rules

- One canonical semantic icon ID per action/state (`send`, `attach`, `call_voice`, `call_video`, `mute`, `camera`, `search`, `new_message`, `reply`, `forward`, `delete`, `warning`, etc.).
- The same semantic ID keeps the same conceptual metaphor across platforms.
- Directional icons mirror in RTL when direction itself is semantic; non-directional objects/logos do not mirror merely because text direction changes.
- Icons with ambiguous consequences receive visible text in menus/destructive confirmations rather than relying on glyph recognition.
- Accessible name comes from the action/state, not the icon asset filename.
- Security/destructive/permission/recovery icons are reserved semantic UI.

### Theme boundary

Untrusted `.omtheme` bundles **cannot replace semantic action/security icons**. Their `icon` assets are decorative media only (wallpaper ornaments, theme preview decoration, optional non-interactive motifs). They may not be bound to a semantic icon ID.

A future alternate semantic icon family must be a reviewed client asset pack shipped/approved with the application, not arbitrary theme content. It must pass recognition, RTL, contrast, accessibility and reserved-surface tests before becoming selectable.

This deliberately narrows the older "icon visual style" theme language: themes may color compatible non-reserved icon roles, but do not redefine the glyph/metaphor.

## Media assets

OM-owned media assets include app/product marks, empty-state illustrations, onboarding illustrations and call/message decorative art. They follow these rules:

- no security-critical instruction is communicated only through an illustration;
- imagery is optional to comprehension and has appropriate accessible treatment;
- decorative assets do not contain remote URLs, trackers or user/account identifiers;
- app/product marks are not reused as verification/trust badges;
- profile/avatar/content media stay separate from OM product chrome so user content cannot convincingly impersonate a reserved control surface;
- animation obeys reduced-motion and performance budgets;
- asset variants support light/dark/high-contrast backgrounds without forcing a theme to alter security semantics.

## Theme-owned sound and motion

A theme may choose a validated sound asset and visual easing family, but:

- notification/call **policy** (whether/how a notification or call is allowed to alert) is a user/privacy/platform setting, not theme state;
- a theme cannot silently turn a required alert into silence or re-enable an alert the user disabled;
- user motion intensity and reduced-motion accessibility preference outrank theme defaults;
- density is a separate user ergonomic preference. Theme spacing/radius tokens are interpreted inside the active density bounds and cannot select Compact/Comfortable on the user's behalf.

## Theme importer resource safety

Before extraction/activation, a reference importer enforces:

- relative ASCII asset paths only; no absolute path, `..` segment, backslash path alias, NUL or post-normalization duplicate;
- unique normalized asset paths;
- declared hash and byte length checked before activation;
- bounded compressed and uncompressed totals plus per-kind limits;
- decompression-ratio limits and streaming extraction into a dedicated sandbox directory;
- allowlisted decoders/media types by asset kind;
- fonts pass the separate hostile-font sanitizer;
- `icon` kind is decorative only and cannot bind semantic icon IDs;
- failure is atomic: invalid bundle does not partially apply tokens/assets.

Reference limits may be tightened by constrained platforms, but no conforming importer has an unbounded extraction path.

## CONFORMANCE-UI-VISUALS

| ID | Case | Pass condition |
|---|---|---|
| VS-01 | default light/dark palette | rendered text/components meet the documented contrast floors; status remains non-color-only |
| VS-02 | theme uses accent similar to warning/danger | compiler/layout preserves distinguishable semantic treatment or rejects the theme |
| VS-03 | theme supplies `icon` named like `delete`/`verified` | asset remains decorative and cannot replace semantic control/security glyph |
| VS-04 | theme asset path `../x`, `/x`, duplicate-normalized path or backslash alias | import rejects before extraction |
| VS-05 | theme attempts silent notification/call sound | user notification/call policy wins; theme cannot change alert semantics |
| VS-06 | theme requests density/motion contrary to user accessibility preference | explicit user density/motion/reduced-motion state wins |
| VS-07 | RTL | directional semantic icons mirror where appropriate; logos/non-directional metaphors remain stable |
| VS-08 | high contrast/forced colors | platform/accessibility override remains operable and reserved security state stays distinguishable |
