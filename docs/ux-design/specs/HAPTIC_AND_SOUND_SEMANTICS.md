# Haptic and Sound Semantics

**Status:** Normative sensory-feedback model; exact platform patterns remain adapter-owned

## Principle

Haptics and sound are feedback channels, not decoration and never the sole carrier of essential meaning.

OM prefers short, causal, platform-appropriate feedback. If removing a vibration/sound makes an action ambiguous, the visual/accessibility feedback is incomplete.

## Semantic haptic vocabulary

OM defines meanings rather than waveform recipes:

| Semantic event | Default intent | Notes |
|---|---|---|
| `selection-step` | subtle orientation | optional; e.g. discrete picker/drag threshold |
| `commit-light` | action accepted | use sparingly for high-value direct manipulation |
| `success-significant` | meaningful completion | device linked, verification completed, important transfer complete |
| `warning` | caution before/at meaningful state | never substitutes for visible warning |
| `failure-actionable` | action failed and needs attention | restrained; not for transient retries |
| `call-connected` | live session begins | one short confirmation if platform permits |
| `call-ended` | live session ended | optional, distinct from connected |
| `record-start-stop` | voice/media capture boundary | platform/OS capture indicators remain authoritative |

Routine message arrival, scrolling, every keystroke, every hover, and every animation should **not** acquire custom haptics.

## Platform mapping

Prefer system-provided semantic haptics when their documented meaning matches. Native adapters may map one OM semantic to different platform patterns. The web/PWA must not pretend to provide equivalent haptic fidelity where browser/platform capability is absent.

## User control

Haptics are optional and independently controllable where the platform allows. Respect system vibration/haptic accessibility preferences.

Reduced Motion does not automatically mean `no haptics`; motion, haptic, and sound sensitivities are separate preferences. OM provides independent sensory reduction where feasible.

## Sound vocabulary

Sounds primarily belong to:

- incoming call/system communication alerting;
- user-enabled notification sounds;
- optional significant completion/failure cues;
- OhMessenger expressive effects when the recipient allows them.

Do not add sounds to routine button presses or successful message sends by default.

Custom sounds, when used, should be short, distinctive, non-startling, and never required to understand what happened.

## Experience profiles

### Focus

- no decorative sounds;
- minimal semantic haptics;
- expressive reaction haptics off;
- system communication alerts follow user/OS notification choices.

### Balanced

- restrained semantic haptics;
- no routine action sounds;
- modest expressive feedback.

### Playful

- richer optional expressive haptics/sounds;
- semantic meanings stay identical;
- recipient intensity controls still win.

## Security and destructive actions

A security warning may use a warning haptic as an additional cue, but the wording, visual state, focus order, and explicit action remain primary. OhMessenger sound/effect packs cannot override protected security sensory roles.
