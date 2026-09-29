# OM Icon Construction, Geometry, and Optical Rules

**Status:** Normative construction guidance for Run 11A.  
**Artwork:** Run 11B produced the first functional SVG candidate set. Artwork remains evidence-gated and not yet frozen as final v1.

## 1. Master grid

Use a **24-unit master grid** for the primary functional family.

Reasons:
- it maps naturally to common mobile icon sizing;
- it is large enough for consistent geometry;
- it remains easy to derive 20-unit dense desktop and larger variants;
- it aligns with established icon-system practice without binding OM to a specific vendor library.

Reference sizes:

| Context | Artwork box | Typical control target |
|---|---:|---:|
| Dense desktop | 20 logical px | >= 40 logical px where platform conventions permit |
| Standard OM UI | 24 logical px | >= 44 logical px; 48 preferred for frequent mobile controls |
| Emphasized/large | 28–32 logical px | component-defined |
| Illustration/empty state | not part of functional icon grid | not applicable |

The control target is not the icon size.

## 2. Live area and overshoot

Default live area: approximately **20 × 20 units inside the 24-unit box**, leaving optical breathing room.

This is a guideline, not a cage.

Circular/diagonal forms may overshoot keylines slightly when needed to appear the same optical size as square forms. Do not mathematically equalize shapes that then look unequal.

## 3. Nominal stroke

Use an initial nominal stroke near **2 units at the 24-unit master**.

Allowed optical correction range for the first set: approximately **1.5–2.25 units**, justified by legibility rather than style variation.

Rules:
- matching functional siblings should feel equally weighted;
- internal detail may require a thinner stroke;
- do not make an icon heavier merely because it is more important; importance belongs to layout/color/container hierarchy;
- test at actual rendered sizes, not only enlarged vector view.

The exact stroke token remains provisional until Run 11B renders the full set side by side.

## 4. Corners and terminals

Default family character:
- clean, restrained geometry;
- modestly softened exterior corners where appropriate;
- clear interior counters;
- consistent terminal character;
- avoid extreme rounded "cute" geometry in the functional base family.

OhMessenger may later add expressive icon skins, but the default functional set should remain calm and precise.

## 5. Fill strategy

The base family is primarily **outline/monochrome**.

Filled variants are reserved for stable roles such as:
- selected/current navigation;
- toggled state where the filled form is materially clearer;
- small status marks that need sufficient visual mass.

Do not create fill variants merely for visual variety.

## 6. Optical balance

Review every icon against at least:
- circle;
- square;
- person;
- message;
- call;
- search;
- more;
- delete.

The goal is equal perceived weight and footprint, not identical path area.

Check:
- center of mass;
- apparent baseline;
- apparent cap height;
- diagonal density;
- negative-space balance;
- similarity to adjacent glyphs.

## 7. Complexity budget

At 24 units, prefer:
- one dominant silhouette;
- no more internal detail than meaning requires;
- counters large enough to survive low-density or blurred viewing;
- no fine decorative texture.

If a concept needs several tiny subparts to become understandable, use a different metaphor or pair the icon with text.

## 8. Small-size survival

Every functional symbol intended for icon-only compact use must remain identifiable at:
- 20 px;
- 24 px;
- common device pixel densities;
- light/dark;
- grayscale;
- increased contrast.

Run 11B must export raster previews at 1× and low-quality/downsampled conditions for review.

## 9. Pixel alignment

Vectors remain resolution-independent, but critical horizontals/verticals should land cleanly at common sizes.

Do not distort a curve merely to force every point to a pixel boundary. Favor overall legibility and optical continuity.

## 10. Containers

The icon artwork itself should not bake in a circle/square container unless containment is part of the symbol's meaning.

Buttons, tabs, selections, and hover/focus backgrounds belong to component styling.

Examples:
- use a bare plus symbol in an Add button rather than permanently drawing a circular plate into the icon;
- use the component's selected container rather than a second duplicate background inside the SVG.

## 11. Slash/negation

Use one consistent negation/slash grammar.

The slash indicates negation of the base concept, not danger by itself.

Examples:
- muted speaker;
- notifications off.

Do not use a slash as a generic error symbol.

## 12. Badges and modifiers

Modifiers must occupy predictable regions and remain subordinate to the base symbol.

Candidate modifiers:
- plus/add;
- check/verified;
- exclamation/warning;
- lock/security;
- count badge rendered by the component, not embedded into most artwork.

Modifier position may adapt for RTL only when its semantic relationship is directional.

## 13. Export/source requirements for Run 11B

Each icon should ultimately have:
- semantic ID;
- source vector;
- 24-unit canonical viewBox;
- accessible-name contract stored outside the SVG;
- directionality metadata;
- variant metadata;
- hash/version;
- monochrome default;
- no raster-only master.

Do not put arbitrary `title` strings inside the SVG as the only accessibility mechanism; accessibility naming belongs to the consuming component.
