# R13C System-Completeness Evidence

**Date:** 2026-09-29  
**Purpose:** Public design/standards evidence used to constrain R13C. These sources inform principles; they do not prove OM's exact layouts, wording, thresholds, or token values are optimal.

## Localization, RTL, and adaptive layout

- Apple HIG — Layout: adaptable layout should remain familiar across display sizes, orientations, resizable windows, text-size changes, and locale/RTL differences. https://developer.apple.com/design/human-interface-guidelines/layout
- Apple HIG — Right to left: system UI frameworks reverse ordinary interface flow for RTL and designers must treat icons/text appropriately. https://developer.apple.com/design/human-interface-guidelines/right-to-left
- Apple localization testing documents pseudo-languages including double-length, RTL, accented, bounded, and tall variants. https://developer.apple.com/documentation/xcode/preparing-your-interface-for-localization
- Android adaptive-layout guidance uses **current window size**, not physical device identity; compact/medium/expanded references include 600 dp and 840 dp width transitions. https://developer.android.com/develop/adaptive-apps/guides/use-window-size-classes
- W3C WCAG Reflow requires non-exempt content to remain usable without two-dimensional reading scroll at the equivalent of 320 CSS px width. https://www.w3.org/WAI/WCAG21/Understanding/reflow
- W3C bidi authoring guidance recommends isolation (`bdi` / `dir=auto`) for injected text whose direction is unknown. https://www.w3.org/TR/i18n-html-tech-bidi/

## Writing and error language

- Apple HIG — Writing: clear, familiar, plain language; match tone to context; avoid unnecessary jargon; make errors specific and actionable; keep settings labels clear. https://developer.apple.com/design/human-interface-guidelines/writing
- Apple HIG — Alerts: use interruptions sparingly; be direct, neutral, specific, and avoid generic `Error` titles when useful information exists. https://developer.apple.com/design/human-interface-guidelines/alerts
- GOV.UK Design System — Error messages: clear/concise, describe what happened and how to fix it, avoid technical jargon and blaming language. https://design-system.service.gov.uk/components/error-message/

## Haptics, sound, and feedback

- Apple HIG — Feedback: match delivery channel to significance, integrate status near the affected object, and provide feedback through multiple accessible channels. https://developer.apple.com/design/human-interface-guidelines/feedback
- Apple HIG — Playing haptics: use documented system patterns consistently, prefer short causal haptics, and make haptics optional. https://developer.apple.com/design/human-interface-guidelines/playing-haptics
- Android haptics design principles: `less is more`; haptics should enrich interaction rather than become distracting, and system/action-oriented feedback APIs are preferred. https://developer.android.com/develop/ui/views/haptics/haptics-principles
- Apple notifications guidance: notification content should be concise, avoid duplicate notifications, avoid exposing sensitive information, and never rely on sound alone. https://developer.apple.com/design/human-interface-guidelines/notifications/

## Performance

- web.dev Interaction to Next Paint: ≤200 ms at p75 is the current `good` Core Web Vital threshold for web responsiveness; OM uses this as an external ceiling/health signal, not as the desired latency for every local action. https://web.dev/articles/inp
- Android rendering guidance: at 60 Hz, frames above ~16 ms risk visible jank; modern higher-refresh devices can require smaller frame budgets. https://developer.android.com/google/play/vitals/render
- Apple launch guidance emphasizes immediate/streamlined launch and getting the first usable screen available quickly. https://developer.apple.com/design/human-interface-guidelines/launching

## Scope implication

R13C therefore chooses:

- capability/window-driven responsive states instead of device-name layouts;
- pseudolocalization and bidirectional-text isolation as pre-release engineering requirements;
- concise object/action-oriented copy;
- short optional semantic haptics rather than pervasive vibration;
- measurable latency budgets with decoration sacrificed before interaction;
- a token schema that protects semantic roles from unsafe theme overrides;
- a v1 scope map that explicitly prevents optional delight/customization from becoming correctness dependencies.
