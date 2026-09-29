# ADR-UIUX-0010 — Executable PWA Conformance Harness

**Status:** Accepted  
**Date:** 2026-09-28

## Context

Revision 9 defined framework-independent components and accessibility mappings. The remaining risk was that those documents could still drift from actual web behavior.

## Decision

1. Maintain a fixture-only PWA reference implementation whose purpose is to exercise OM semantics rather than production networking/crypto.
2. Maintain an executable conformance harness for the behaviors that can be tested automatically.
3. Use the W3C editable combobox/listbox pattern for the Omnibar and retain DOM focus in the input while the active candidate is represented with `aria-activedescendant`.
4. Make each selectable Omnibar candidate one semantic option; do not nest independent buttons in options.
5. Treat focus restoration, target sizes, reflow, Reduced Motion, and input equivalence as conformance properties.
6. Record unavailable browser/AT/device evidence as `not_run`, never as implied pass.
7. Keep evidence classes separate: static, browser-automated, AT-observed, physical-device, and human-task.

## Consequences

- Engineers have executable examples of the intended semantics.
- Browser regressions can be detected before protocol integration.
- OM does not confuse standards compliance with assistive-technology interoperability.
- Future React/other web implementations can replace this fixture as long as they satisfy the same observable contracts.
