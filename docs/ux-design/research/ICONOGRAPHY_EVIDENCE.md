# Iconography Evidence and Research Notes

**Purpose:** Record external evidence behind Run 11A decisions. These sources guide constraints; they do not prove a specific OM glyph is optimal.

## 1. Familiarity, complexity, concreteness, distinctiveness

McDougall, Curry, and de Bruijn (1999) established measurable icon/symbol dimensions including concreteness, complexity, meaningfulness, familiarity, and semantic distance.

McDougall et al. (2000) found:
- concreteness particularly affects initial meaning acquisition;
- visual complexity continues to affect search;
- distinctiveness benefits from both visual and semantic contrast.

Later experimental work also reports improved complex-task performance for icons that are simpler, more concrete, or more familiar, with familiarity helping working-memory maintenance.

**OM implication:** start with familiar, simple metaphors; evaluate distinctiveness inside the actual icon family.

References:
- https://pubmed.ncbi.nlm.nih.gov/10502873/
- https://pubmed.ncbi.nlm.nih.gov/11218339/
- https://www.sciencedirect.com/science/article/pii/S0169814119306171

## 2. Labels and age/experience

A 2024 ergonomics study found age-related differences in icon identification and reported that adding text labels resolved comprehension differences associated with age and icon design category.

A 2023 eye-tracking study reported that semantic familiarity strongly affects visual search and that text can materially aid low-familiarity icons.

**OM implication:** text is not a concession. Uncommon, novel, security-critical, or low-familiarity icons should be paired with visible labels.

References:
- https://www.sciencedirect.com/science/article/pii/S0169814124000696
- https://www.sciencedirect.com/science/article/abs/pii/S0169814122001433

## 3. Comprehensibility testing

ISO 9186-1 specifies a method for testing whether graphical symbols communicate their intended message without supplementary text. The 2014 edition remains current while a replacement draft is under development in 2026.

**OM implication:** custom label-free symbols must earn that status through comprehension testing rather than designer intuition.

References:
- https://www.iso.org/standard/59226.html
- https://www.iso.org/standard/92166.html

## 4. Grid, stroke, optical correction, and target separation

Material Design's established icon construction guidance uses:
- 24dp system icons;
- consistent grids/keylines;
- nominal 2dp strokes;
- optical corrections when complexity requires them;
- separation between icon artwork size and touch-target size.

OM is not adopting Material artwork, but these practices provide a tested construction baseline for a coherent custom family.

Reference:
- https://m1.material.io/style/icons.html

## 5. Scalable variants and semantic state

Apple's SF Symbols system demonstrates useful icon-system capabilities:
- vector scaling;
- variants such as fill/slash/enclosed forms;
- multiple rendering modes;
- custom-symbol support;
- layer-based animation and contextual replacement;
- direction-aware/localized variants.

OM should learn from these system-design capabilities while keeping OM-owned concepts visually consistent across PWA, Android, and iOS.

References:
- https://developer.apple.com/sf-symbols/
- https://developer.apple.com/documentation/uikit/configuring-and-displaying-symbol-images-in-your-ui
- https://developer.apple.com/design/human-interface-guidelines/right-to-left

## 6. Accessibility naming

WCAG 2.2 requires non-text controls to have a text alternative/name describing their purpose. Visible labels and programmatic names must also stay aligned where label-in-name applies.

**OM implication:** accessible naming is part of icon semantic metadata, not something added after SVG export.

References:
- https://www.w3.org/WAI/WCAG22/Understanding/non-text-content
- https://www.w3.org/TR/WCAG21/

## 7. App icon identity vs functional symbols

Apple's current app-icon guidance emphasizes simplicity, memorability, small-size legibility, centered primary content, and adaptation to platform masking/appearance variants.

**OM implication:** the future OM identity mark and app icon should share a core identity but remain separate from the functional icon vocabulary. Run 11C will explore brand/app marks after the functional language is grounded.

Reference:
- https://developer.apple.com/design/human-interface-guidelines/app-icons

## 8. What evidence does not establish

These sources do not justify:
- copying Apple or Material artwork;
- declaring 24 units or 2-unit strokes universally optimal;
- assuming every familiar icon is cross-cultural;
- removing labels because an icon resembles a standard one;
- treating a successful laboratory result as proof of OM comprehension.

OM will validate the actual glyphs it ships.
