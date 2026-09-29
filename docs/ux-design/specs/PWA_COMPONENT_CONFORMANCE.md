# PWA Component Conformance

**Status:** normative implementation-boundary guidance, Revision 10.

This specification turns OM's semantic UI contracts into observable web behavior. It is not a claim that standards-conforming markup is automatically interoperable with every browser/screen-reader pair; actual assistive-technology testing remains mandatory.

## 1. Principle

The PWA must implement the same OM object/action semantics as native clients while using web-native controls and accessibility patterns. Browser implementation details must not leak upward into OM actions.

> **Web semantics are a platform dialect of OM's interaction language.**

## 2. Omnibar

When suggestions are visible, the editable Omnibar uses a combobox with a one-dimensional listbox popup.

Required behavior:

- the text field remains the DOM-focus owner while results are navigated;
- `aria-expanded` reflects popup visibility;
- `aria-controls` points to the result list;
- `aria-activedescendant` identifies the current result while a current result exists;
- each executable result is one `option`;
- an option contains no independently tabbable action; entity and verb results are separate candidates;
- Down/Up move the active result;
- Enter activates the explicit active candidate;
- Escape collapses without changing user text or executing a candidate;
- normal platform text-editing keys are not intercepted unnecessarily.

This deliberately follows the W3C APG editable-combobox model rather than inventing an OM-specific keyboard model.

## 3. Conversation log

Dynamic conversation additions may use `role=log` with polite live semantics when interoperability testing confirms good behavior. The implementation must avoid duplicate announcements. Sending a message appends the message to the conversation object; presentation and announcement are consequences of that state change, not separate application actions.

## 4. Focus restoration

A temporary contextual surface restores focus to the logical invoking object when dismissed and that object still exists. In particular:

- conversation Omnibar → composer;
- settings/control surface → settings invoker;
- modal/sheet → invoking action or the next logical safe target.

Focus restoration is part of state-machine correctness, not polish.

## 5. Pointer/touch equivalence

A path-based gesture is never the only route to an action. High-frequency semantic actions must have a simple pointer/tap path and keyboard/accessibility path. The conformance fixture currently exercises opening a conversation using both pointer click and single-pointer touch emulation. Gesture accelerators are tested separately.

## 6. Target size

OM's normal internal target floor remains **44 CSS px** for high-frequency compact controls even though WCAG 2.2's AA minimum target-size criterion is less demanding. The test harness verifies the prototype's primary shell controls against the OM floor. Exceptions must be explicit and justified by the component contract.

## 7. Text enlargement and reflow

The fixture must remain usable with OM's 200% text setting at a 320 CSS-pixel viewport without page-level two-dimensional scrolling. This is a design regression check, not a substitute for browser-zoom testing. Browser zoom to WCAG reflow conditions remains part of manual platform validation.

## 8. Reduced motion

When Reduce Motion is active, expressive/OhMessenger animation is removed or reduced to effectively instantaneous state feedback. Semantic state must remain perceivable through non-motion cues.

## 9. Focus visibility

Keyboard focus must have a visually obvious treatment independent from selection. The current OM Clear fixture requires at least a 3 CSS-pixel outline for tested primary controls; production focus tokens remain governed by the visual token contract and contrast validation.

## 10. Test evidence classes

Record evidence using these labels:

- **static** — schema/token/markup inspection;
- **browser-automated** — observed behavior in a named browser engine/version;
- **AT-observed** — named screen reader/voice/switch technology plus browser/OS version;
- **physical-device** — touch/reachability/layout observation on named hardware;
- **human-task** — structured usability task results.

Never upgrade one class into another by inference.

## 11. Current Revision 10 result

The Revision 10 fixture passes all 17 browser-automated checks that could be executed in the available Chromium runtime. Firefox and WebKit were not runnable in the execution environment and are therefore explicitly **not run**. See `research/PWA_CONFORMANCE_R10.md`.

## 12. Primary standards references

- W3C APG combobox pattern: https://www.w3.org/WAI/ARIA/apg/patterns/combobox/
- W3C WCAG 2.2 / target size: https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/
- W3C Resize Text: https://www.w3.org/WAI/WCAG21/Understanding/resize-text
- W3C Reflow: https://www.w3.org/WAI/WCAG21/Understanding/reflow
- W3C Pointer Gestures: https://www.w3.org/WAI/WCAG21/Understanding/pointer-gestures
