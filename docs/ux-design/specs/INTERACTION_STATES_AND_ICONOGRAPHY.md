# Interaction States and Iconography

## 1. Distinct interaction states

Every actionable component should define, where applicable:

- rest;
- hover;
- focus-visible;
- pressed/active;
- selected/current;
- disabled;
- pending;
- success;
- warning;
- failure.

Do not reuse a single tint for multiple unrelated states.

## 2. Focus is not selection

Keyboard focus means “this will receive my next keyboard action.” Selection means “this item is currently chosen.”

They may coexist and must remain visually distinguishable.

## 3. Hover is supplementary

Hover can reveal accelerators on pointer devices, but essential actions may not depend on hover because touch, keyboard, switch, gaze, and many future inputs do not provide it.

## 4. Press feedback is immediate

Controls should react at pointer/touch down quickly enough to establish causality. Do not delay pressed feedback while waiting for network work or an action result.

## 5. Pending is object-local

Where possible, represent pending state on the object that is pending:

- sending message on the message;
- uploading attachment on the attachment;
- joining call on the call surface.

Avoid generic page spinners for local object state.

## 6. Icon language

Baseline icon rules:

- simple, recognizable silhouettes;
- coherent optical size and stroke character;
- no text baked into icons;
- no decorative detail that disappears at small sizes;
- filled variants may indicate selected/current state;
- animated icon transitions are allowed only when they communicate state or purposeful delight.

## 7. Labels

Icons may stand alone only when their meaning is established and accessible naming is present.

Risky, uncommon, or ambiguous actions should have visible text in menus/sheets/confirmation surfaces.

## 8. Directionality

Directional icons and motion must respect RTL layout where semantic direction should mirror. Media controls or culturally invariant symbols must not be mirrored blindly.

## 9. Badges

Badges communicate bounded state, not engagement pressure.

- Prefer meaningful counts such as unread mentions or pending actions.
- Avoid badges whose only purpose is to pull users back into OM.
- Large counts may collapse into a less precise representation when exactness has no task value.

## 10. Presence and read state

Presence/read indicators should be visually quiet and subject to privacy settings. Their absence must not imply a false social meaning when the peer has disabled or hidden the underlying state.

## 11. Detailed icon-system foundation

Run 11A moves iconography into dedicated normative documents:

- [`ICON_SYSTEM_CONSTITUTION.md`](ICON_SYSTEM_CONSTITUTION.md) — meaning, familiarity, labeling, accessibility, directionality, theming, and comprehension rules.
- [`ICON_CONSTRUCTION_AND_OPTICS.md`](ICON_CONSTRUCTION_AND_OPTICS.md) — 24-unit master grid, nominal stroke, live area, optical corrections, small-size survival.
- [`ICON_TAXONOMY_AND_INVENTORY.md`](ICON_TAXONOMY_AND_INVENTORY.md) — semantic categories, familiarity tiers, label policy, and initial icon inventory.
- [`ICON_COLOR_STATE_AND_MOTION.md`](ICON_COLOR_STATE_AND_MOTION.md) — semantic color/state/variant/motion grammar.
- `../contracts/om-icon-inventory-v0.json` — machine-readable source inventory.

These documents supersede any ambiguity in this shorter overview.
