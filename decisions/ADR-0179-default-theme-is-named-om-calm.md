# ADR-0179 — The default reference theme is named OM Calm

- **Status:** Accepted (operator direction, 2026-10-11)
- **Date:** 2026-10-11
- **Amends:** ADR-0159 and ADR-0166 (the default preset name only); ADR-0162 §4 and ADR-UIUX-0007 (name only)
- **Does not amend:** the palette values, semantic color roles, contrast requirements, preset family structure, or any protocol value

## Context

ADR-0159 named the default preset **OM Clear**, and ADR-0166 kept that name when it replaced Run 3's OM Calm/Evergreen/Warm Slate/Violet set. The reference web client (`packages/om-design`) was built with the name **OM Calm**, and the operator chose to keep it.

## Decision

1. The default reference theme, and the first of the curated presets, is named **OM Calm**. The other presets keep their ADR-0159 names: **OM Paper**, **OM Iris**, **OM Graphite**.
2. This is a display-name change only. The palette, roles and contrast gates are those of the former OM Clear. Stable identifiers do not change: theme id `om.clear`, `data-theme="clear"` in the prototypes, and the file [`docs/ux-design/tokens/om-theme-clear-v1.json`](../docs/ux-design/tokens/om-theme-clear-v1.json) keep their values so that no stored preference, token reference or evidence file is invalidated.
3. Normative docs, specs, token display names and prototype labels now say OM Calm. Accepted ADR text (ADR-0159, ADR-0162, ADR-0166), changelog and snapshot history, and generated evidence under `verification/` are left as written; they are historical records and this ADR is the pointer from the old name to the new.
4. The name is a display string in the client's own catalogue. It is not a protocol value, a wire field or a conformance-bearing identifier.

## Consequences

- A reader who finds "OM Clear" in an older ADR or evidence file should read it as OM Calm.
- The client's built-in theme and the design docs now agree, so the `reference_client_ui` theme work needs no rename.
- A later change to the stable id (`om.clear` → `om.calm`) would need a migration of any stored preference and is not part of this decision.
