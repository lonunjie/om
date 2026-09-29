# ADR-UIUX-0001 — Develop UI/UX as a Separate Design Track Before Merge

**Date:** 2026-09-28  
**Status:** Accepted for current design work

## Context

The current `om-design` packages are primarily focused on protocol, architecture, verification, identity, recovery, crypto, provider independence, and engineering handoff.

The UI/UX work now has enough independent depth to deserve its own coherent design pass:

- interaction grammar;
- shell strategy;
- semantic motion;
- OhMessenger delight;
- OwnMessenger personalization;
- accessibility;
- keyboard/voice/future input;
- cross-platform consistency.

Merging this material immediately into an arbitrary older `om-design` ZIP would couple new UX decisions to a potentially stale architecture snapshot and make later comparison harder.

## Decision

Maintain the UI/UX work as a **standalone merge-ready package** for now.

The package should:

- avoid duplicating protocol/core architecture;
- reference integration points conceptually;
- keep filenames and paths suitable for later repository insertion;
- contain no history archive of prior versions;
- avoid freezing implementation choices prematurely;
- be merged only against the latest verified OM design state.

## Rationale

Benefits:

- UI/UX can mature without destabilizing architecture verification work;
- the latest architecture package remains the authoritative base;
- merge conflicts are easier to reason about;
- UX design can be reviewed independently by interaction specialists;
- later merge can explicitly reconcile action semantics, shared `om-ui`, and platform adapters.

## Consequence

This package is not a fork of OM architecture. It is a **design module awaiting integration**.

The merge step should be a deliberate reconciliation, not a file-copy operation.
