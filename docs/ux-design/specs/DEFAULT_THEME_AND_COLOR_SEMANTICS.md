# OM Clear — Default Theme and Functional Color Grammar

**Status:** selected reference theme for prototype validation; palette values are v1 candidates, not immutable brand law.

## 1. Goal

The default OM theme must be simultaneously:

- highly legible for long chat sessions;
- immediately scannable;
- ergonomic on small touch screens;
- calm when nothing needs attention;
- unambiguous when something does;
- attractive enough to feel crafted rather than clinical;
- safe under color-vision variation;
- usable in both light and dark appearance;
- compatible with OwnMessenger customization.

The default theme is named **OM Clear** because its first job is to make meaning clear.

## 2. Functional color grammar

OM Clear reserves color families for stable meanings.

### Neutral family — rest and structure

Used for:

- base surfaces;
- message-list chrome;
- ordinary incoming messages;
- secondary/tertiary text;
- separators and inactive state.

Neutral surfaces intentionally carry most of the UI. This keeps chroma available as a scarce attention channel.

### Cobalt/azure — action, navigation, information

Used for:

- primary interactive emphasis;
- interactive text where color is appropriate;
- current safe navigation/action emphasis;
- informational state;
- outgoing-message tint rather than a fully saturated message block.

Do not use the action hue on static decorative text. Color should retain information scent.

### Violet — keyboard/focus location

A separate violet family is reserved for strong keyboard focus indication in OM Clear. It helps distinguish **where keyboard input will go** from blue action/selection emphasis.

Focus also has geometry; color is never the only cue.

### Green — success / verified completion

Green may support successful or verified state, always paired with an icon, label, or structural cue when the distinction matters.

### Amber — caution / warning

Amber means attention is warranted but the system is not necessarily failing. Ordinary pending/sending is **not** colored amber; otherwise routine latency would feel alarming.

### Red — danger / failure / destructive consequence

Red is protected for failure and destructive risk. Themes cannot casually reuse the danger color for ordinary decorative highlights.

## 3. OM Clear Light candidate palette

| Semantic role | Value | Intent |
|---|---:|---|
| `surface.base` | `#F7F9FC` | high-luminance cool off-white; long-session base |
| `surface.subtle` | `#EFF3F8` | grouped/secondary surface |
| `surface.raised` | `#FFFFFF` | cards/sheets/menus where elevation is meaningful |
| `text.primary` | `#172033` | high-contrast primary text |
| `text.secondary` | `#566176` | readable secondary text |
| `text.tertiary` | `#627085` | readable small metadata; not a disabled color |
| `border.subtle` | `#D7DEE8` | decorative grouping; not sole state cue |
| `border.strong` | `#9BA8B9` | controls/states needing stronger boundary |
| `accent.primary` | `#0067C5` | interactive/action family |
| `accent.hover` | `#0057AA` | pointer hover |
| `accent.pressed` | `#004A90` | active/pressed |
| `accent.onPrimary` | `#FFFFFF` | content on primary accent |
| `accent.soft` | `#DDEEFF` | low-attention accent surface |
| `state.focus` | `#6536D9` | keyboard focus ring |
| `state.selected` | `#DCEBFF` | selection surface, with geometry/check where needed |
| `status.success` | `#18794E` | success/verified |
| `status.warning` | `#8A5A00` | warning/caution |
| `status.danger` | `#C4333B` | failure/destructive |
| `message.incoming.surface` | `#FFFFFF` | quiet incoming bubble |
| `message.outgoing.surface` | `#E7F2FF` | low-chroma action-family tint |
| `message.failed.surface` | `#FFF0F0` | failure backing plus icon/text cue |

Selected WCAG 2.x contrast checks against `surface.base`:

- primary text: ~15.4:1;
- secondary text: ~5.9:1;
- tertiary text: ~4.8:1;
- accent: ~5.3:1;
- focus: ~6.5:1;
- success: ~5.1:1;
- warning: ~5.6:1;
- danger: ~5.1:1.

White on `accent.primary`: ~5.6:1.

## 4. OM Clear Dark candidate palette

| Semantic role | Value | Intent |
|---|---:|---|
| `surface.base` | `#11151C` | dark charcoal, not absolute black |
| `surface.subtle` | `#171D26` | grouped/secondary surface |
| `surface.raised` | `#1D2530` | raised content |
| `surface.overlay` | `#25303D` | menus/sheets |
| `text.primary` | `#F2F5F8` | bright but softened foreground |
| `text.secondary` | `#B9C2CE` | clear secondary foreground |
| `text.tertiary` | `#8F9BAA` | metadata |
| `border.subtle` | `#34404F` | low-attention separator |
| `border.strong` | `#667485` | stronger boundary |
| `accent.primary` | `#66B2FF` | interactive/action family |
| `accent.hover` | `#8AC4FF` | hover |
| `accent.pressed` | `#4AA4FF` | pressed |
| `accent.onPrimary` | `#07182B` | dark foreground on bright accent |
| `accent.soft` | `#183A5B` | low-chroma blue surface |
| `state.focus` | `#B69CFF` | strong keyboard focus |
| `state.selected` | `#223D5E` | selected surface |
| `status.success` | `#5DD6A1` | success/verified |
| `status.warning` | `#F6C65B` | warning/caution |
| `status.danger` | `#FF7B7B` | failure/destructive |
| `message.incoming.surface` | `#1D2530` | incoming bubble |
| `message.outgoing.surface` | `#183A5B` | outgoing tint |
| `message.failed.surface` | `#451D25` | failure backing plus icon/text cue |

Selected WCAG 2.x contrast checks against `surface.base`:

- primary text: ~16.7:1;
- secondary text: ~10.2:1;
- tertiary text: ~6.5:1;
- accent: ~8.2:1;
- focus: ~8.0:1;
- success: ~10.1:1;
- warning: ~11.5:1;
- danger: ~7.3:1.

Dark text on `accent.primary`: ~8.0:1.

## 5. Light/dark relationship

Light and dark are semantic siblings, not inversions.

Preserve across both:

- which family means action;
- which family means danger, warning, success, and focus;
- relative hierarchy of surfaces;
- incoming/outgoing distinction;
- protected critical state visibility.

Allow to differ:

- exact hue/chroma/lightness needed for equivalent perception;
- shadow treatment;
- separator strength;
- saturation needed to prevent glow in dark mode;
- foreground polarity on accent-filled controls.

## 6. Color area is part of the attention budget

The same hue becomes more attention-grabbing when it occupies more area.

Therefore:

- routine actions often use accent text/icon rather than a huge saturated surface;
- full-width danger red is reserved for genuinely dangerous contexts;
- routine outgoing bubbles use an accent tint instead of solid cobalt;
- success should not turn an entire conversation green;
- warnings should not become yellow wallpaper.

## 7. State pairing

Color must pair with another cue:

- **selected:** selected surface + indicator/check/position;
- **focus:** violet ring/outline + focus geometry;
- **failure:** danger color + failure icon/text/retry affordance;
- **warning:** amber + warning icon/label;
- **verified:** success + verification icon/text;
- **unread:** weight/count/marker + optional accent;
- **pending:** clock/progress semantics + neutral/low-accent treatment.

## 8. High-contrast variant

OM Clear must generate or ship an increased-contrast pair. The high-contrast variant may:

- darken light-mode secondary foregrounds;
- brighten dark-mode foregrounds;
- strengthen separators and focus geometry;
- reduce translucent/blur effects;
- increase selected-state boundary contrast.

It may not change action meaning or information hierarchy.

## 9. Ambient and wallpaper behavior

Wallpaper is never allowed to become the contrast reference for text directly. Message bubbles and shell surfaces maintain their own legible backing where needed.

Dynamic/OhMessenger backgrounds may add interest, but:

- motion obeys the attention budget;
- Reduced Motion has a static equivalent;
- local contrast is maintained;
- decorations do not use protected status colors in a way that simulates a status signal.

## 10. Validation before brand freeze

OM Clear is the **strong default candidate**, not a claim of universal color preference.

Before freezing brand v1, test:

- outdoor/bright-light legibility;
- dim-room dark-mode comfort;
- common forms of color-vision deficiency;
- small and large text;
- one-handed use;
- keyboard-only use;
- high-contrast settings;
- OLED/LCD devices across iOS, Android, and PWA;
- long chat sessions, not only static screenshots.

## 11. Revision 8 stability rule

OM Clear now enters **validation stability**. Do not repeatedly adjust its hues merely to make the package feel newer. Change candidate values only when one of the following produces evidence of a problem:

- contrast/static checks;
- real-device rendering;
- large-text/high-contrast behavior;
- color-redundancy/CVD testing;
- structured usability testing;
- cross-platform semantic mismatch.

This prevents subjective palette churn from destroying comparability across validation runs.
