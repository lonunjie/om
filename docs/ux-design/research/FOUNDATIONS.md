# Research and Design Foundations

This file records the established ideas informing OM's UI/UX language. It is a design reference, not a claim that OM follows one academic school exclusively.

## Affordances and signifiers

**Relevance:** users should perceive what actions are possible and what controls mean without having to remember hidden rules.

Key reference:

- Don Norman, discussion of affordances and signifiers: https://jnd.org/affordances-and-design/

## Direct manipulation

**Relevance:** users act on visible objects rather than navigating abstract command syntax for common tasks. Immediate incremental feedback and reversibility improve learnability.

Classic tradition:

- Ben Shneiderman's direct-manipulation work and subsequent HCI research.

## Recognition rather than recall

**Relevance:** visible choices, familiar patterns, and progressive disclosure reduce memory burden.

Reference:

- Nielsen Norman Group, usability heuristics: https://www.nngroup.com/articles/ten-usability-heuristics/

## Progressive disclosure

**Relevance:** preserve a simple default surface while keeping advanced capability available when requested.

Reference:

- Nielsen Norman Group: https://www.nngroup.com/articles/progressive-disclosure/

## Semantic / functional motion

**Relevance:** motion conveys object relationships, hierarchy, state, feedback, and navigation rather than merely decorating the interface.

References:

- Apple Human Interface Guidelines, Motion: https://developer.apple.com/design/human-interface-guidelines/motion
- Microsoft Fluent 2, Motion: https://fluent2.microsoft.design/motion
- Material Design, motion: https://m1.material.io/motion/material-motion.html
- A List Apart, “Motion with Meaning: Semantic Animation in Interface Design”: https://alistapart.com/article/motion-with-meaning-semantic-animation-in-interface-design/

## Calm technology

**Relevance:** technology should demand as little attention as necessary and move information between periphery and center according to importance.

References:

- Mark Weiser and John Seely Brown, “Designing Calm Technology”: https://calmtech.com/papers/designing-calm-technology
- Calm Technology principles: https://www.calmtech.institute/calm-tech-principles

## Wayfinding

**Relevance:** interface structure can work like environmental navigation: maintain orientation, reveal paths, reduce uncertainty, and provide information when it is needed.

OM applies this as **digital wayfinding**, not as a literal copy of airport signage.

## Implicit tutorials and game onboarding

**Relevance:** good games often teach mechanics by arranging the environment so the player safely discovers them through action, practice, and feedback.

OM should borrow the principle but not the risk tolerance: security and destructive operations require explicit clarity.

## Hedonic and pragmatic UX quality

**Relevance:** a useful interface can also support stimulation, identity, expression, and enjoyment. This supports separating OM's pragmatic interaction grammar from OhMessenger's emotional layer rather than treating them as opposites.

Research tradition:

- Marc Hassenzahl and related work on pragmatic and hedonic qualities of interactive products.

## User delight

**Relevance:** deep delight comes from reliable, effective, low-friction interaction; surface delight from aesthetics and playful effects should build on that foundation.

Reference:

- Nielsen Norman Group, “User Delight”: https://www.nngroup.com/articles/theory-user-delight/

## Accessibility and multimodal input

**Relevance:** functionality should not depend on a single input method or sensory channel.

References:

- WCAG 2.1 target size: https://www.w3.org/WAI/WCAG21/Understanding/target-size
- WCAG animation from interactions: https://www.w3.org/WAI/WCAG21/Understanding/animation-from-interactions
- W3C multimodal interaction requirements: https://www.w3.org/TR/mmi-reqs/
- MDN Pointer Events: https://developer.mozilla.org/en-US/docs/Web/API/Pointer_events

## Platform design references

These are useful constraints, not authorities over OM's design identity:

- Apple Human Interface Guidelines: https://developer.apple.com/design/human-interface-guidelines/
- Microsoft Fluent 2: https://fluent2.microsoft.design/
- Material Design: https://m3.material.io/

## Messenger references worth studying

### Telegram

Study for:

- gesture fluency;
- rich reactions;
- stickers and emoji expression;
- theme/personality systems;
- animation power-saving controls;
- distinction between routine and deliberate expressive reactions.

References:

- https://telegram.org/blog/power-saving
- https://telegram.org/blog/chat-themes-interactive-emoji-read-receipts
- https://telegram.org/blog/video-stickers-better-reactions

### Signal, WhatsApp, iMessage

Study comparatively for:

- restrained interaction;
- familiarity and learned conventions;
- platform-native integration;
- direct manipulation;
- accessibility and system behavior.

OM should learn from interaction patterns without inheriting another messenger's product identity wholesale.

## Omnibar accessibility and local-search implementation references

**Relevance:** OM's universal find/do surface behaves like an editable suggestion control on the web, but its local indexing/search engine should stay behind a portable interface.

References:

- W3C ARIA Authoring Practices, Combobox Pattern: https://www.w3.org/WAI/ARIA/apg/patterns/combobox/
- MDN IndexedDB API: https://developer.mozilla.org/en-US/docs/Web/API/IndexedDB_API
- SQLite FTS5: https://www.sqlite.org/fts5.html
- SQLite WASM persistent storage / OPFS: https://www.sqlite.org/wasm/doc/trunk/persistence.md
- MDN SpeechRecognition: https://developer.mozilla.org/en-US/docs/Web/API/SpeechRecognition

The Web Speech reference is particularly important to the privacy design: browser speech recognition is not universally available and some implementations may use server-based recognition. OM therefore cannot assume that a PWA microphone implies private on-device recognition.

## Chromium Omnibox as an interaction precedent

Chromium's Omnibox is relevant as a precedent for a single field that reduces choice/cognitive overhead and ranks suggestions using local history signals. Chromium's historical design material emphasizes quick incremental suggestions, stability/predictability, and scored history candidates. OM borrows the interaction lesson, not the browser data model.

OM differs deliberately:

- communication actions have stronger side-effect and identity risks than URL navigation;
- explicit verbs/pins must remain authoritative;
- local usage learning is optional and resettable by design;
- learned target preference must not silently change action semantics;
- security-critical commands remain behind Action Registry safeguards.

References:

- Chromium Omnibox design principles: https://www.chromium.org/user-experience/omnibox/
- Chromium HistoryQuickProvider/history ranking design: https://www.chromium.org/omnibox-history-provider/

## Visual-system foundations

OM's visual-system work should continue to consult platform design systems for accessibility, typography, motion, input, and responsive behavior while preserving OM's own semantic token contract. Apple HIG, Material Design, Fluent, and WCAG are constraints/reference systems rather than interchangeable skins.

## Visual ergonomics, color semantics, and theme evidence

Revision 7 adds a dedicated evidence review and contrast audit:

- [`VISUAL_ERGONOMICS_EVIDENCE.md`](VISUAL_ERGONOMICS_EVIDENCE.md)
- [`CONTRAST_AUDIT_R7.md`](CONTRAST_AUDIT_R7.md)

Key external references include WCAG 2.2, Apple accessibility/dark-mode guidance, controlled studies of display polarity, one-handed smartphone target/reach ergonomics, color-preattentive visual search, color-vision-deficiency prevalence, and experimental work on aesthetics versus usability.

OM uses this evidence to constrain its visual system. It does not claim that research scientifically identifies one universal brand hue; exact theme values remain subject to OM-specific task testing.

## Revision 9 implementation-boundary references

### Web accessibility patterns

W3C's ARIA Authoring Practices emphasizes stable visible focus, conventional keyboard movement, and the distinction between focus and selection. The editable combobox pattern supports retaining DOM focus in the input while moving the active suggestion with `aria-activedescendant`, which maps well to OM's Omnibar. W3C also documents `role="log"` as an appropriate live-region pattern for sequential chat updates, subject to real assistive-technology testing.

References:

- https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/
- https://www.w3.org/WAI/ARIA/apg/patterns/combobox/
- https://www.w3.org/WAI/WCAG22/Techniques/aria/ARIA23.html
- https://www.w3.org/WAI/ARIA/apg/practices/structural-roles/

### Native accessibility actions

Apple exposes named custom accessibility actions in UIKit and SwiftUI, including through VoiceOver's action mechanisms. Android Compose likewise supports `CustomAccessibilityAction` specifically as an accessible alternative for complex touch gestures. These APIs support OM's design rule that gestures accelerate an action but never monopolize it.

References:

- https://developer.apple.com/documentation/uikit/uiaccessibilitycustomaction
- https://developer.apple.com/documentation/swiftui/accessible-controls
- https://developer.android.com/develop/ui/compose/accessibility/semantics
- https://developer.android.com/guide/topics/ui/accessibility/composables

### Conservative adaptation

Todi, Bailly, Leiva, and Oulasvirta's work on model-based reinforcement learning for adaptive UIs explicitly models the cost of surprise/relearning and argues for conservative adaptation. OM does not adopt their RL method as a dependency, but the research supports measuring adaptation cost and stability rather than maximizing prediction accuracy alone.

Reference:

- https://arxiv.org/abs/2103.06807

### Omnibox precedent

Chromium's History Quick Provider searches a local history index, scores candidate matches, and returns a limited set of relevant suggestions. OM borrows the architectural lesson — local indexed candidate generation plus scoring — while preserving a different action/safety grammar appropriate to messaging.

Reference:

- https://chromium.googlesource.com/playground/chromium-org-site/+/refs/heads/main/omnibox-history-provider/index.md

## Revision 10 executable PWA conformance references

Revision 10 turns several standards-derived expectations into browser-observable checks. The conformance harness intentionally distinguishes standards/DOM behavior from assistive-technology interoperability.

References:

- W3C APG editable combobox pattern and keyboard behavior: https://www.w3.org/WAI/ARIA/apg/patterns/combobox/
- WCAG 2.2 Target Size (Minimum): https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/
- WCAG Resize Text: https://www.w3.org/WAI/WCAG21/Understanding/resize-text
- WCAG Reflow: https://www.w3.org/WAI/WCAG21/Understanding/reflow
- WCAG Pointer Gestures: https://www.w3.org/WAI/WCAG21/Understanding/pointer-gestures

The W3C combobox guidance specifically supports DOM focus remaining in the editable field while `aria-activedescendant` communicates the active popup option. Reflow guidance anchors the 320 CSS-pixel no-two-dimensional-page-scroll test. Pointer-gesture guidance reinforces OM's rule that swipe/drag accelerators require simpler alternatives.
