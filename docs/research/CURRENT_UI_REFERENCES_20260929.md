# UI/UX references — Run 3, 2026-09-29

Supporting research for ADR-0162. These references guide the reference product design; they are not protocol dependencies.

## Recognition, adaptive search and navigation

- Chromium Omnibox History Provider: https://www.chromium.org/omnibox-history-provider/
  - Chromium's design uses scored history candidates and seeks immediate suggestions while typing. OM borrows only the broad idea of recency/frequency-informed local ranking; OM deliberately keeps the learned store IdentityProfile-local and does not require durable raw query history.
- Nielsen Norman Group, *Memory Recognition and Recall in User Interfaces* (2024): https://www.nngroup.com/articles/recognition-and-recall/
  - Recognition is easier than recalling commands from scratch. OM therefore makes common Omnibar actions visible/labeled rather than requiring expert command syntax.
- Nielsen Norman Group, *Search Is Not Enough: Synergy Between Navigation and Search*: https://www.nngroup.com/articles/search-not-enough/
  - Search alone increases recall burden. OM keeps obvious new-message/profile/context controls and recognizable suggestions even though the Omnibar unifies cross-app navigation.
- Apple Human Interface Guidelines, Search fields (updated June 8, 2026): https://developer.apple.com/design/human-interface-guidelines/search-fields
  - Search entry placement depends on content/layout and does not intrinsically require a permanent tab. OM uses an inline/toolbar-equivalent Omnibar in a conversation-first shell.

## Accessibility / visual semantics

- W3C WCAG 2.2 overview/new criteria: https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/
- W3C Understanding SC 1.4.11 Non-text Contrast: https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast
  - Meaningful component/state graphics require 3:1 contrast against adjacent colors at AA.
- W3C Understanding SC 2.4.13 Focus Appearance: https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance
  - The AAA criterion specifies a visible focus area comparable to a 2 CSS-pixel perimeter and a 3:1 focused/unfocused change. OM adopts this as a stronger reference-product target even though WCAG 2.2 AA remains the baseline.

## Design consequence

The research supports a hybrid rather than an extreme: a conversation-first minimal shell, an always-reachable recognition-oriented Omnibar, local adaptation for efficiency, visible ordinary controls for novice discoverability, and accessibility semantics that do not depend on color, motion or theme quality.
