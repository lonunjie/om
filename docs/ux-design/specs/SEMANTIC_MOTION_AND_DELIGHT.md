# Semantic Motion and Delight

## 1. Two motion classes

OM distinguishes **semantic motion** from **expressive motion**.

### Semantic motion

Functional. Communicates:

- origin;
- destination;
- hierarchy;
- causality;
- progress;
- relationship;
- state transition;
- cancellation or reversal.

It remains part of the UX even in restrained profiles, though reduced-motion users receive equivalent non-motion feedback.

### Expressive motion

Emotional. Communicates personality, celebration, warmth, surprise, or playfulness.

It belongs primarily to OhMessenger and may be reduced or disabled by user preference.

## 2. Experience profiles

Recommended presets:

### Focus

- semantic motion preserved but restrained;
- minimal expressive effects;
- quiet or disabled sounds;
- reduced background animation;
- subdued reaction rendering;
- compact attention footprint.

### Balanced

- default OM experience;
- semantic motion fully expressed;
- moderate reaction and sticker animation;
- occasional justified delight;
- no routine full-screen interruption.

### Playful

- richer expressive animation;
- animated emoji/stickers;
- theme personality;
- expressive haptics and sounds where allowed;
- celebratory effects;
- still subject to accessibility, attention, and recipient-control rules.

These are presets, not separate products.

## 3. Granular controls

OwnMessenger should eventually permit independent control of:

- semantic motion intensity;
- expressive motion intensity;
- full-screen celebrations;
- animated backgrounds;
- animated stickers;
- reaction effects;
- sounds;
- haptic strength/personality;
- autoplay behavior;
- power/data-sensitive animation;
- reduced-motion preference.

## 4. Sender intent vs receiver rendering

A sender's expressive payload should carry intent, not a command to seize the receiver's screen.

Example:

```text
Sender: celebration(reaction=🎉, intensity=high)
```

Possible receiver renderings:

- Playful → particles + expressive haptic;
- Balanced → short animated flourish;
- Focus → subtle highlight;
- Reduced Motion → static semantic emphasis;
- Screen reader → meaningful spoken state.

## 5. Earned delight

OM distinguishes:

- **deep delight** — speed, trust, undo, predictability, reliability, low friction;
- **surface delight** — animation, sounds, playful visual treatment.

Deep delight must come first.

## 6. Attention budget

Every effect has an attention cost.

Suggested qualitative scale:

| Effect | Attention cost | Typical use |
|---|---:|---|
| subtle state change | very low | delivery/read state |
| brief semantic motion | low | object transition |
| small reaction animation | low–moderate | social expression |
| sound/haptic | moderate | meaningful confirmation |
| banner | moderate–high | event needing attention |
| modal | high | blocking decision |
| full-screen effect | very high | rare deliberate celebration |

Rules:

- routine events should spend little attention;
- frequency reduces acceptable intensity;
- high-intensity effects must be user-controllable;
- accessibility preferences override expressive intensity;
- safety/security information may justify high attention but must remain clear rather than theatrical.

## 7. Motion timing principles

Motion should:

- begin in response to user intent;
- preserve object continuity;
- complete quickly enough not to obstruct work;
- be interruptible where interaction continues;
- avoid arbitrary bounce, overshoot, or travel unrelated to semantics;
- maintain a stable mapping between direction and destination;
- respect reduced-motion preferences.

## 8. Delight rule

Not every animation needs informational utility; some may have emotional utility.

The rule is:

> **No purposeless motion. Joy is a purpose, but joy must not compete with comprehension, consent, accessibility, or task completion.**
