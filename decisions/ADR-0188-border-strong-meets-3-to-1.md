# ADR-0188 — `border.strong` meets 3:1: the OM Calm component boundary is darkened to the nearest passing value

- **Status:** Accepted
- **Date:** 2026-11-14
- **Decided by:** operator, 2026-11-14 ("make the best decisions"; ruling: accessibility wins, component boundaries need 3:1, VS-01, accessibility regressions are release-blocking per AGENTS.md). This ADR amends audited text because the operator decided it; it is not an ADR by delegation, so AGENTS "Design gate" rule 6 does not bar it.
- **Amends:** the two `border.strong` values of the OM Calm palette in [`DEFAULT_THEME_AND_COLOR_SEMANTICS.md`](../docs/ux-design/specs/DEFAULT_THEME_AND_COLOR_SEMANTICS.md) section 3 (light, `#9BA8B9`) and section 4 (dark, `#667485`), and the same two values in [`om-theme-clear-v1.json`](../docs/ux-design/tokens/om-theme-clear-v1.json). Nothing else in the palette changes.
- **Does not amend:** any other role, the functional color grammar, ADR-0159, ADR-0162, ADR-0179, the revision 8 stability rule (it is met, see below), any wire value or protocol constant.

## Audited passage amended

[`DEFAULT_THEME_AND_COLOR_SEMANTICS.md`](../docs/ux-design/specs/DEFAULT_THEME_AND_COLOR_SEMANTICS.md) section 3, row: `` `border.strong` | `#9BA8B9` | controls/states needing stronger boundary ``; section 4, row: `` `border.strong` | `#667485` | stronger boundary ``.

## Context

[`VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md`](../docs/architecture/ux/VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md) "Contrast and focus" requires "meaningful UI component boundaries/icons/state graphics: `>= 3:1` against adjacent colors", and conformance case VS-01 requires the default light and dark palettes to meet the documented floors. [`ACCESSIBILITY_AND_INPUT.md`](../docs/architecture/ux/ACCESSIBILITY_AND_INPUT.md) makes accessibility regressions in shared primitives release-blocking. The audited `border.strong` was authored as the "controls/states needing stronger boundary" token, but its light value measures 2.29:1 on the canvas (2.42:1 on `bg-surface`, 2.17:1 on `bg-sunken`), so it fails VS-01. The 2026-11-03 conformance review reported this ([`2026-11-03-ui-conformance-review.md`](../docs/reports/2026-11-03-ui-conformance-review.md)); the code carried a 2.2:1 floor that only documented the shortfall. In dark, `#667485` passes on the canvas, surface and raised backgrounds (3.83, 3.55, 3.24) but is 2.81:1 on `bg-overlay`, where menus and sheets, and the controls inside them, sit.

## Decision

1. Light `border.strong` becomes **`#7D8EA4`**: the same hue and saturation as `#9BA8B9`, lightness lowered in 0.2% steps until the lowest ratio across `bg-canvas`, `bg-surface` and `bg-sunken` reached 3:1. Measured: 3.17:1 on `#F7F9FC`, 3.35:1 on `#FFFFFF` (`bg-surface`, `bg-raised`, `bg-overlay`), 3.00:1 on `#EFF3F8`.
2. Dark `border.strong` becomes **`#6A798A`**, found the same way, raised until it passed on all four dark surfaces. Measured: 4.11:1 on `#11151C`, 3.80:1 on `#171D26`, 3.47:1 on `#1D2530`, 3.01:1 on `#25303D`.
3. The calm look is kept: both values stay in the neutral cool-grey family, are visibly lighter than `text.secondary` and far from the accent, and `border.subtle` (decorative grouping, "not sole state cue") is unchanged, so the strong/subtle hierarchy holds. Only controls and states that opt into the strong boundary change.
4. The code floor becomes a requirement: `THEME_CONTRAST_PAIRS` asks `border` for 3:1 against canvas, surface, raised, sunken and overlay, so every theme, not only OM Calm, must meet it. A unit test (`packages/om-design/src/design.test.ts`) checks both variants against all five backgrounds.
5. The revision 8 stability rule (section 11) allows a change when "contrast/static checks" produce evidence of a problem; this is that case, and it is limited to the one role.

## Consequences

- Bordered controls (inputs, outlined buttons, cards that use the strong boundary) get a slightly darker outline in light and a slightly lighter one in dark. No layout, role or meaning changes.
- The prototype CSS and standalone files under `docs/ux-design/prototype*` still carry the old value; they are historical prototypes, not the shipped client, and are not edited.
- A custom or preset theme whose border falls under 3:1 on any of the five backgrounds is now rejected by the validator, as VS-01 intends.
- `border.strong` is not claimed to satisfy forced-colors or high-contrast, which remain platform overrides.
