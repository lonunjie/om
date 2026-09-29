# Accessibility Platform Mapping

**Status:** normative mapping guidance, Revision 9.

The rule is **same semantic object/action, platform-native accessibility expression**. OM should prefer native controls/semantics first and add custom accessibility metadata only when native semantics are insufficient.

## 1. Web / PWA

### General

- Prefer semantic HTML over ARIA when equivalent semantics exist.
- Maintain visible keyboard focus and logical restoration after overlays/destructive removal.
- `Tab` / `Shift+Tab` move between components; arrows navigate inside composite widgets where the chosen ARIA pattern defines that behavior.

### Omnibar

Use an editable **combobox + listbox popup** pattern when suggestions are presented as one-dimensional selectable candidates.

Recommended model:

- native text input with combobox semantics;
- `aria-expanded`, `aria-controls`, accessible name;
- one `option` per executable candidate;
- `aria-activedescendant` can retain DOM focus in the input while assistive focus moves among results;
- `Down/Up` navigate candidates, `Enter` activates selected candidate, `Escape` dismisses;
- do not place separately focusable buttons *inside* listbox options. Represent `Maria` and `Call Maria` as distinct candidates instead.

This matches OM's stable entity/verb distinction and avoids inaccessible nested controls.

### Conversation log

A current conversation may use `role="log"` / equivalent live-region semantics when testing confirms acceptable AT/browser behavior. The role is specifically intended for sequential updates such as chat logs and has polite live-region behavior. Avoid announcing every local visual state transition redundantly.

### Chat list

Treat the ordinary chat list as navigation/list content, not a listbox, unless the UI truly implements listbox selection semantics. In a multipane shell, expose the current conversation with a native/current-state mechanism (for example an appropriate current/selected semantic) without conflating keyboard focus and selection.

### Menus/dialogs/sheets

Follow APG keyboard/focus conventions and restore focus to the invoking object after dismissal when it still exists.

## 2. iOS / iPadOS

Use standard SwiftUI/UIKit controls when possible so VoiceOver, Voice Control, Switch Control, Dynamic Type, and platform focus behavior inherit native semantics.

For custom OM objects:

- concise accessible label identifying the object;
- value/state where relevant;
- hint only when the result of activation is not obvious;
- correct traits/role;
- custom accessibility actions for gesture-only accelerators such as Reply, React, Retry, or Archive;
- preserve logical reading/navigation order;
- respect system Reduce Motion, text size, contrast, and appearance preferences.

A swipe-to-reply message remains accessible because `Reply` is exposed as a named accessibility action. VoiceOver's actions rotor can surface these custom actions.

## 3. Android

Prefer native/Compose controls and built-in semantics. For custom composables:

- expose semantic role and state;
- use `clickable`/`toggleable` semantics where appropriate;
- expose complex gesture equivalents through `CustomAccessibilityAction`;
- group/merge child semantics when that improves traversal without hiding important content;
- keep logical TalkBack/Switch Access traversal order;
- provide explicit keyboard/D-pad focus behavior for desktop/tablet/TV-like contexts where relevant.

Android's Compose guidance specifically recommends custom actions as accessible alternatives to complex gestures such as swipe-to-dismiss and drag/drop.

## 4. Cross-platform semantic mapping examples

| OM semantic object/action | Web/PWA | iOS/iPadOS | Android |
|---|---|---|---|
| Omnibar | editable combobox + listbox candidates | search/text field + accessible result collection | text field + semantic result collection |
| Reply to message | candidate/menu/button + accessible name | custom accessibility action `Reply` | `CustomAccessibilityAction("Reply")` |
| Retry failed message | visible Retry action; status tied to message | named action/value/state on message | named custom action/state on message |
| Current conversation | current/selected navigation state distinct from focus | selected/current state exposed separately from VoiceOver focus | selected semantic distinct from accessibility focus |
| Incoming messages | tested `log`/polite announcement strategy | platform announcement behavior only when useful/non-duplicative | TalkBack/live-region equivalent only when useful/non-duplicative |
| Modal confirmation | native dialog semantics + focus trap/restore | native alert/sheet semantics | native dialog semantics |

## 5. Announcements are part of the attention budget

Screen-reader announcements are interruptions too. Do not announce routine pending/sync changes repeatedly. Prioritize:

- newly received content when the user is in that conversation and announcements are useful;
- failures that require recovery;
- consequential confirmation results;
- changes the user cannot otherwise discover from current accessibility focus/state.

Do not duplicate spoken content that the assistive technology already announced through normal focus traversal.

## 6. Input-label stability

Accessible action names must remain stable across themes. OwnMessenger can alter presentation and permitted key bindings, but a theme cannot rename `Reply` to an unrelated metaphor or hide a security consequence.

## 7. Required platform test matrix

Before calling a component implementation conformant, exercise at least:

- Web: keyboard + one major screen reader/browser combination on each supported desktop OS family; mobile PWA with screen reader;
- iOS/iPadOS: VoiceOver, Voice Control, Dynamic Type, Reduce Motion, increased contrast where supported;
- Android: TalkBack, Switch Access/Voice Access where relevant, font/display scaling, Remove animations/Reduce Motion equivalents, keyboard navigation on a large-screen device;
- mixed-input tablet: touch + keyboard + trackpad without mode switching.

Support claims must be recorded per actual OS/browser/AT versions. Standards conformance alone is not proof of interoperability.

## 8. Primary references

- W3C ARIA Authoring Practices, keyboard interface: https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/
- W3C APG combobox pattern: https://www.w3.org/WAI/ARIA/apg/patterns/combobox/
- W3C ARIA chat-log technique: https://www.w3.org/WAI/WCAG22/Techniques/aria/ARIA23.html
- W3C structural-role guidance (prefer semantic HTML): https://www.w3.org/WAI/ARIA/apg/practices/structural-roles/
- Apple accessibility fundamentals: https://developer.apple.com/documentation/swiftui/accessibility-fundamentals
- Apple accessible controls/custom actions: https://developer.apple.com/documentation/swiftui/accessible-controls
- UIKit `UIAccessibilityCustomAction`: https://developer.apple.com/documentation/uikit/uiaccessibilitycustomaction
- Android Compose semantics: https://developer.android.com/develop/ui/compose/accessibility/semantics
- Android accessible composables: https://developer.android.com/guide/topics/ui/accessibility/composables
