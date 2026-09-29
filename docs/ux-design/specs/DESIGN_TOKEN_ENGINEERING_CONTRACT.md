# Design Token → Engineering Contract

**Status:** Normative token architecture candidate

## Goal

OwnMessenger needs broad visual customization without allowing themes to corrupt semantics, accessibility, or behavior. Engineering therefore consumes a layered semantic token contract rather than raw arbitrary theme values.

## Token layers

### 1. Primitive palette/scale

Theme-authoring values such as raw color ramps, spacing scales, radii, type scales, and motion durations.

**Application components should not directly depend on primitives.**

### 2. Global semantic roles

Examples:

- `surface.base`
- `surface.raised`
- `text.primary`
- `text.secondary`
- `action.primary`
- `focus.ring`
- `status.success`
- `status.warning`
- `status.danger`
- `message.incoming.surface`
- `message.outgoing.surface`
- `motion.semantic.short`

These survive theme changes.

### 3. Component aliases

Examples:

- `chatRow.unread.text` → semantic role;
- `composer.surface` → semantic surface;
- `omnibar.activeOutline` → focus/action role;
- `message.failed.icon` → danger role.

Component aliases can evolve without requiring a theme to know every internal implementation detail.

### 4. State overrides

Hover, pressed, selected, disabled, pending, warning, error, focus-visible, high-contrast, and reduced-motion mappings.

State meaning is protected.

## Protected tokens

A theme compiler/validator may adjust or reject unsafe values for:

- primary/secondary readable text;
- focus-visible indication;
- danger/warning/success semantics;
- selected/current state;
- minimum target size/density safety floors;
- Reduced Motion behavior;
- security/verification protected roles.

A custom theme request is a preference, not authority to produce inaccessible output.

## What tokens cannot contain

Theme/token documents cannot:

- register Action Registry handlers;
- remap Delete/Archive/Reply/etc.;
- request network/filesystem/microphone/contact access;
- weaken confirmation/authentication;
- execute scripts;
- suppress accessibility labels or focus;
- choose which security facts are true.

## Light/dark pairing

A theme family provides independently tuned semantic values for each supported appearance. Dark is not algorithmic inversion. Missing values may inherit only from documented semantic defaults, not from unsafe raw color transforms.

## Versioning

Use semantic versioning for the public token schema:

- patch: compatible clarifications/default corrections;
- minor: additive roles/aliases with fallbacks;
- major: breaking role/removal/meaning changes.

Deprecated tokens remain mapped through a migration table for at least one compatibility window before removal where practical.

## Platform compilation

The same semantic contract may compile to:

- CSS custom properties / typed style objects for PWA;
- SwiftUI/UIKit color/type/spacing resources;
- Compose/Android resource/theme adapters;
- future client equivalents.

Platform adapters may choose native representations but cannot reinterpret semantic meaning.

## User customization

Custom accent, font, density, radius, and expressive personality are resolved through safe ranges and semantic derivation. If a chosen accent cannot satisfy required contrast in a role, OM derives/adjusts the rendered role or explains why the literal value cannot be used there.

## Theme portability

A portable OwnMessenger theme exports presentation data only. Per-account behavioral preferences (tabs, shortcut mappings, learning on/off, privacy choices) are a separate configuration domain and are never smuggled inside a theme.
