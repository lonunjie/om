# Settings and OwnMessenger Architecture

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Normative v1 candidate

## Purpose

OwnMessenger should make OM deeply personal without turning Settings into a warehouse of switches. Common changes should be reachable through the Omnibar, contextual controls, and a small, stable settings information architecture.

## Principles

1. **Search is the fastest settings path.** A user may type or speak `larger text`, `show Calls tab`, `left handed`, `hide message previews`, or `turn off playful effects` in the Omnibar and reach the exact setting or safe action.
2. **Settings describe outcomes, not implementation.** Prefer `Message previews` over platform-specific storage or notification terminology unless the platform itself owns that setting.
3. **Common choices stay near the object.** Per-chat mute, per-chat wallpaper, call device selection, and similar contextual choices live where they are used and are also searchable from Settings.
4. **Advanced does not mean unsafe.** Security-critical actions remain in their canonical security/device flows even when discoverable by search.
5. **Themes change presentation; user configuration may change allowed behavior.** Downloaded themes never acquire permission to alter interaction policy.
6. **Searchable does not mean hidden.** Accessibility, privacy, devices, and recovery remain directly navigable even when the Omnibar can jump to them.

## Stable settings groups

### Appearance

- theme pair: System / Light / Dark where appropriate;
- preset theme family, including OM Clear;
- text size and readable width preferences where platform permits;
- density: Compact / Comfortable / Large / Custom safe range;
- conversation background/wallpaper;
- expressive intensity: Focus / Balanced / Playful;
- Reduced Motion mapping follows system preference first, with explicit OM override only where platform guidance permits.

### Interaction

- enabled/reordered top-level tabs;
- Omnibar placement where supported;
- left/right-handed shell preference;
- keyboard shortcut profile and custom bindings within safe remapping rules;
- send/enter behavior on hardware keyboards;
- gesture accelerators;
- local personalization learning: on/off/reset;
- voice-command preferences and privacy mode.

### Notifications

- global notification mode;
- message/group/call distinctions;
- lock-screen preview level: Private / Sender only / Full preview;
- sounds and haptic notification preferences where the OS permits;
- quiet hours/focus integrations where the architecture/platform supports them.

### Privacy & security

- Protection profile: Recommended / Hardened / Maximum Privacy / Custom;
- recovery custody/status and scope entry points;
- local personalization data controls;
- local search/index data controls;
- invitation defaults;
- verification/security explanation entry point;
- app lock and local privacy controls where supported.

Security-critical actions such as device suspension/permanent revocation and recovery execution deep-link into their dedicated flow rather than executing directly from a settings result.

### Devices

- linked/authorized devices;
- current device;
- add/link device;
- device details;
- suspend/quarantine with explicit consequence wording;
- pending/final control transition state;
- Safety Freeze/contested-control state where applicable;
- permanent revoke/remove as a distinct action.

### Data & storage

- media auto-download;
- cache/storage use;
- local history/index management;
- export where architecture permits;
- local cleanup;
- attachment-quality/data preferences.

A cache-clear action must not be worded as history deletion unless it actually deletes message history.

### Accessibility & language

- follow system text size/accessibility by default;
- language/locale selection if OM supports app-specific language choice;
- sensory feedback controls;
- high-contrast or increased-contrast preference where applicable;
- speech/voice accessibility settings;
- shortcut/action discoverability.

### Advanced

Only genuinely specialist options live here, such as diagnostics, provider configuration, developer/export diagnostics, and experimental capabilities explicitly enabled by the user.

**Advanced must not become the home for essential privacy or accessibility controls.**

## Omnibar settings results

A setting candidate exposes:

- canonical setting ID;
- current value when useful;
- destination or reversible action;
- risk class;
- IdentityProfile scope;
- whether OS-owned configuration must be opened instead.

Example:

```text
Message previews                    Setting
Sender only

Show Calls tab                      Setting
Off

Revoke this device                  Security action
Review consequences…
```

Search may rank settings from local use/context, but it must not execute a destructive or security-critical setting merely because the phrase matched.

## Setting scope

Every setting is explicitly one of:

- device-global;
- app-global;
- IdentityProfile-specific;
- conversation-specific;
- Space/channel-specific.

The UI must not make a per-device setting appear synchronized across IdentityProfiles/devices if it is not.

## Reset and portability

OwnMessenger should expose separate resets for:

- visual customization;
- interaction customization;
- personalization learning;
- notification preferences where app-owned;
- keyboard mappings.

`Reset personalization` must not delete message history, contacts, or security state.
