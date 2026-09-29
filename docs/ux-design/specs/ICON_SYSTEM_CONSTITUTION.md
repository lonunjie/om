# OM Icon System Constitution

**Status:** Normative foundation; Run 11B assets now implement this contract.  
**Scope:** Functional iconography only. Final artwork is deferred to Run 11B.

## 1. Purpose

OM icons are part of the interaction language. They are not decorative shorthand.

An icon may help a user:
- recognize an object;
- discover or confirm an action;
- distinguish state;
- orient within the shell;
- understand a relationship;
- recover from failure;
- recognize security consequences.

If an icon does not improve one of those jobs, it should not be added merely to make the interface look richer.

## 2. Meaning before drawing

Every icon begins with a semantic identifier and a plain-language meaning before any path is drawn.

The normative order is:

`concept -> action/state/object semantics -> accessibility name -> visual metaphor -> geometry -> optional motion`

Do not begin with a clever glyph and search for a meaning afterward.

## 3. Familiar before novel

Prefer a familiar, widely understood visual metaphor when one exists and fits OM's meaning.

Examples include search, close, camera, microphone, attachment, call, video, copy, download, and delete.

Novel OM concepts may receive custom symbols only when:
1. no familiar symbol communicates the concept without conflict;
2. text alone would create unacceptable scanning cost;
3. the symbol can be tested for comprehension;
4. a visible label remains available where the concept is uncommon, risky, or ambiguous.

OM must not redesign familiar actions merely to appear distinctive.

## 4. Simple, concrete, distinctive

For initial comprehension and search efficiency, prefer symbols that are:
- visually simple;
- concrete when the concept permits;
- semantically familiar;
- distinct from neighboring icons;
- recognizable from silhouette at small size.

Complexity that adds no semantic information is removed.

Distinctiveness is judged within the actual OM action set, not in isolation. Two individually good icons are a bad pair if they are easily confused in the same menu.

## 5. Labels are a design tool, not a failure

Icon-only presentation is appropriate only when context and familiarity make meaning strong enough.

Visible text is required or strongly preferred for:
- security-critical actions;
- irreversible or hard-to-reverse actions;
- uncommon actions;
- novel OM concepts;
- ambiguous symbol families;
- first-use situations where evidence shows poor comprehension.

Menus, sheets, confirmations, Omnibar results, and settings should normally pair icons with text.

The icon must still have a programmatic accessible name even when visible text is omitted.

## 6. OM concepts vs platform-owned concepts

OM owns the visual language for OM-owned concepts so PWA, Android, iOS, and future clients remain recognizable.

Use platform-native imagery when the operating system owns the interaction, such as:
- biometric system prompts;
- OS share destinations;
- system permission sheets;
- platform window controls;
- system file/photo pickers when rendered by the OS.

Do not force an OM symbol into a surface whose meaning is controlled by the platform.

## 7. One icon, one primary meaning

An icon has one primary semantic role.

Do not reuse the same glyph for unrelated concepts merely because it "looks close."

Variants may communicate related states of the same concept:
- selected/fill;
- on/off;
- muted/unmuted;
- verified/unverified;
- download/downloading/complete;
- pinned/unpinned.

A state variant may not silently become a different action.

## 8. Shape before color

A functional icon must remain understandable in monochrome.

Color may accelerate interpretation by applying OM semantic roles such as:
- action/information;
- focus;
- success/verified;
- warning;
- failure/danger.

Critical meaning must retain a non-color cue such as shape, label, position, badge form, or state variant.

## 9. Directionality is semantic

Mirror icons in RTL only when their direction expresses reading/navigation/spatial flow.

Usually mirror:
- back/forward;
- reply/forward arrows when meaning follows reading direction;
- enter/exit spatial arrows;
- directional navigation chevrons.

Usually do not mirror:
- play triangle;
- clock;
- camera;
- microphone;
- QR;
- check;
- universal media controls;
- slash/negation convention unless the icon system explicitly requires it.

Complex symbols must be reviewed component by component rather than blindly flipped.

## 10. Selection, focus, status, and risk are distinct

Icon treatment may vary with state, but these meanings cannot collapse into one tint.

- **selected/current** may use fill/weight/container treatment;
- **focus** uses OM's focus treatment and remains distinguishable from selection;
- **disabled** reduces emphasis without erasing recognizability;
- **pending** uses calm progress treatment;
- **success** may use success color plus shape;
- **warning** may use warning color plus shape;
- **danger/failure** uses protected danger semantics plus label/shape.

A filled icon means selected/current only where that convention is stable for the component family.

## 11. Motion must have semantic or emotional purpose

Functional icon motion is allowed when it communicates:
- state transition;
- action progress;
- causality;
- completion;
- relationship.

Examples:
- mute -> unmute transformation;
- retry -> sending -> delivered state;
- pin settling into pinned state;
- Omnibar trigger morphing between message and Omni modes.

Decorative motion belongs to OhMessenger, not the base functional icon layer.

Reduced Motion must preserve the same state information without requiring the movement.

## 12. Accessibility is intrinsic

Every actionable icon must expose a stable accessible name that describes its action or state.

Rules:
- programmatic names must match visible labels when labels are present;
- decorative icons are hidden from assistive technology;
- status icons expose state when the state matters;
- icon-only controls receive tooltips/help on pointer/keyboard platforms where appropriate;
- critical actions are not icon-only in confirmation surfaces.

Accessibility names are semantic contract data and are not themeable.

## 13. No baked-in words

Do not place ordinary language words inside functional icon artwork.

Exceptions are established symbolic characters whose meaning is itself the symbol, such as:
- `@`;
- `#` where explicitly representing a channel/tag concept;
- `+` / `−`;
- conventional mathematical or media symbols.

Localization must not require redrawing most of the icon family.

## 14. Identity and function remain separate

The OM brand mark may become recognizable enough to act as the Omnibar/OM trigger, but branding must not contaminate unrelated actions.

Functional icons communicate function.
Brand marks communicate identity.
Illustrations communicate context.
OhMessenger media communicates emotion.

Do not use a playful or branded mark where a neutral action glyph is clearer.

## 15. Themes cannot redefine icons

OwnMessenger themes may alter approved presentation attributes such as:
- semantic tint;
- container surface;
- permitted stroke/fill treatment;
- animation personality within the motion contract.

A theme may not:
- replace Delete with a harmless-looking unrelated symbol;
- swap meanings between icons;
- hide protected risk/status markings;
- remove accessibility names;
- make focus/selection indistinguishable;
- introduce animated behavior that changes interaction policy.

Custom icon packs, if supported later, must pass the same semantic/comprehension gates as built-in icons.

## 16. Comprehension is measured

OM will not assume a custom symbol is understandable because its designers understand it.

Before a novel icon becomes a label-free default:
1. test the intended meaning without supplementary text;
2. collect confusion alternatives;
3. test within realistic neighboring icons;
4. measure time/error as well as subjective preference;
5. repeat across relevant age/experience groups;
6. preserve a visible label if comprehension is inadequate.

ISO 9186-style comprehensibility testing is the reference methodology for novel graphical symbols. OM may use a lightweight adapted protocol for product testing while keeping the same principle: test what people understand, not what designers intended.

## 17. Evidence over originality

If a familiar platform/industry metaphor performs better than a novel OM symbol, use the familiar metaphor.

OM's distinctiveness should emerge from the complete interaction language — clarity, color, motion, spacing, tone, and coherence — not from making common actions unnecessarily unfamiliar.

## Brand mark boundary

The OM identity mark is governed by [`OM_IDENTITY_AND_APP_ICON.md`](OM_IDENTITY_AND_APP_ICON.md), not by the functional icon inventory. A brand mark may appear beside or inside a control only when the control has a defined semantic action and accessible name. Branding never substitutes for an action verb.
