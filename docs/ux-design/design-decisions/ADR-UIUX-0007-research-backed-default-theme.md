# ADR-UIUX-0007 — Research-backed default theme and functional color grammar

**Status:** Accepted for prototype validation

## Decision

OM will use **OM Clear** as the default reference theme for prototype validation.

OM Clear uses:

- high-luminance positive-polarity light mode as the default reference reading palette;
- independently tuned dark mode;
- neutral surfaces for most of the UI;
- cobalt/azure for ordinary action/information;
- a distinct violet keyboard-focus family;
- protected green/amber/red success-warning-danger roles;
- low-chroma outgoing-message tint rather than a fully saturated bubble;
- redundant icon/shape/text cues for all important color-coded states.

OwnMessenger will also ship curated light/dark theme pairs that pass the same semantic and accessibility contracts.

## Rationale

Research and standards support:

- strong luminance contrast and positive polarity for reading performance;
- independent light/dark tuning rather than inversion;
- redundant state encoding instead of color alone;
- systematic color as a fast visual-search channel;
- large/reachable targets for one-handed mobile use;
- familiar interactive signifiers;
- aesthetics as valuable but subordinate to actual usability.

The evidence does not identify one universally optimal brand hue, so the exact palette remains a prototype-tested engineering/design choice rather than a scientific claim.

## Consequences

- final brand color is no longer completely unconstrained: OM Clear is now the leading v1 candidate;
- the palette is represented through semantic tokens, not component literals;
- theme packages cannot overwrite protected semantics;
- prototype/usability results may still adjust exact hue, chroma, lightness, or surface values without invalidating this ADR's semantic model.
