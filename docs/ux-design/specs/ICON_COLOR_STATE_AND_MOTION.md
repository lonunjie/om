# OM Icon Color, State, and Motion Grammar

## 1. Default rendering

Functional icons default to monochrome semantic foreground roles.

Do not assign permanent literal colors inside source artwork unless color is intrinsic to a branded/expressive asset.

Components supply semantic color.

## 2. Semantic color mapping

| Meaning | Default semantic role |
|---|---|
| ordinary icon/text | `content.primary` or component foreground |
| interactive affordance | `action.primary` when emphasis is needed |
| keyboard focus | focus ring/container, not recoloring every glyph |
| selected/current | selected foreground/container |
| success/verified | protected success role + non-color cue |
| warning | protected warning role + non-color cue |
| failure/destructive | protected danger role + non-color cue |
| disabled | disabled foreground while preserving form |

A blue icon does not universally mean "clickable"; interaction is also indicated by control shape, placement, cursor/focus behavior, label, or component state.

## 3. Protected semantics

Themes may remap literal hues only while preserving:
- contrast;
- distinctness;
- non-color redundancy;
- danger/warning/success separability;
- light/dark pair consistency.

Custom theme packs may not recolor danger into an ordinary neutral in a way that erases consequence.

## 4. State variants

State transitions should favor one or more of:
- outline -> fill;
- base -> slash/negated;
- base -> check/complete modifier;
- base -> progress treatment;
- container/indicator change.

Avoid wholesale icon swaps when a related-state variant can communicate continuity.

## 5. Motion classes

### Transform
A symbol becomes its related state.
Examples: mic -> mic-off; pin -> pinned; omni trigger -> active Omni state.

### Progress
A symbol communicates bounded or ongoing work.
Examples: upload/download; joining call.

### Resolve
A pending/failure symbol resolves to success or returns to rest.
Examples: retry -> sending -> delivered.

### Emphasize
Brief state acknowledgment where useful.
Examples: successful pin, reaction acknowledgment.

### Express
OhMessenger-only emotional treatment. Not part of the functional base grammar.

## 6. Motion constraints

Functional icon motion:
- must begin from the current visual state;
- should preserve object identity;
- should stop when the state completes;
- should not loop indefinitely unless the underlying process is truly ongoing and the loop is not distracting;
- must have a Reduced Motion equivalent;
- may not delay action execution or input responsiveness.

## 7. Reduced Motion

Reduced Motion prefers:
- immediate crossfade;
- fill/shape change;
- short opacity transition;
- static state replacement.

Meaning remains identical.

## 8. Attention budget

Icon animation is a low-level attention channel.

Do not animate:
- every unread change;
- routine sync;
- presence flicker;
- ordinary hover;
- decorative badges.

Reserve animation for meaningful causality, active process, completion, or deliberate delight.

## 9. Notification/status glyphs

Status glyphs should be visually quiet.

Examples:
- `pending` should not look like a warning;
- `sent`/`delivered`/`read` should be subtle and privacy-aware;
- `failed` must be discoverable and recoverable;
- verification marks use protected semantics and precise wording.

## 10. Color + shape redundancy

For success/warning/failure, pair semantic color with shape:
- success: check or completion form;
- warning: exclamation/caution form;
- failure: explicit failure/retry treatment;
- security: lock/shield/check only according to actual state.

This remains readable in grayscale and many color-vision conditions.
