# ADR-UIUX-0011 — Functional Icon Language Foundation

**Status:** Accepted  
**Date:** 2026-09-28  
**Run:** 11A

## Context

OM's shell, action grammar, semantic motion, color language, accessibility model, and PWA component contracts are mature enough that functional iconography can now be designed from stable meanings rather than from arbitrary screen mockups.

Icons are a high-frequency information channel and therefore affect discoverability, accessibility, platform continuity, security comprehension, and the perceived personality of OM.

## Decision

OM will maintain an **OM-owned functional symbol language** for OM-owned concepts, while using platform-native imagery where the operating system owns the interaction.

The functional icon language follows these rules:
1. semantics are defined before artwork;
2. familiar metaphors are preferred over novel ones;
3. symbols are simple, distinctive, and concrete where practical;
4. labels remain visible for uncommon, ambiguous, novel, or high-risk actions;
5. icon-only controls always retain accessible names;
6. functional symbols must remain understandable without color;
7. color, fill, and motion communicate state but do not redefine meaning;
8. directional symbols are reviewed for RTL semantically rather than mirrored blindly;
9. themes may style approved icon presentation but cannot change icon meaning;
10. novel label-free icons require comprehension testing.

Construction begins from a 24-unit master grid with a nominal ~2-unit stroke and optical correction, but exact artwork metrics remain provisional until the first complete family is rendered and compared in Run 11B.

## Consequences

### Positive
- PWA, Android, iOS, desktop, and future clients can share one functional visual language.
- Icon meaning can be machine-readable and testable.
- Accessibility naming is part of the contract.
- Security and destructive actions cannot become ambiguous icon-only shortcuts.
- Themes remain expressive without becoming semantic forks.

### Costs
- OM must maintain its own functional icon assets.
- Custom symbols require comprehension/legibility testing.
- Platform mapping work is needed where system icon families differ.

## Deferred

Run 11A does not:
- draw/freeze final SVG paths;
- choose the final OM logo/app mark;
- define expressive sticker/illustration art;
- finalize media-asset pack format.

Those are Run 11B/11C concerns.
