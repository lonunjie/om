# Motion Tokens and Physics

This file operationalizes [`SEMANTIC_MOTION_AND_DELIGHT.md`](SEMANTIC_MOTION_AND_DELIGHT.md).

## 1. Motion classes

Provisional duration classes:

| Token | Range | Use |
|---|---:|---|
| `motion.instant` | 0–80 ms | pressed/focus acknowledgement, tiny state response |
| `motion.micro` | 100–140 ms | icon/state changes, small reveal/hide |
| `motion.standard` | 160–220 ms | ordinary surface transitions |
| `motion.spatial` | 220–280 ms | clear object relocation or shell transformation |
| `motion.expressive` | 280–500 ms | optional OhMessenger flourish |

No ordinary task-critical transition should make a fast user wait for a long animation to finish.

## 2. Direct manipulation

While the user drags/swipes/resizes:

- the manipulated object follows input 1:1 unless resistance itself communicates a boundary;
- action thresholds become perceptible before commitment;
- cancelling reverses naturally;
- release continues from the object's current position rather than restarting a canned animation.

## 3. Easing

Default semantic motion should feel responsive and settled rather than bouncy.

Use ease-out for entering/settling and ease-in for simple exits where appropriate. Spring motion is allowed for direct manipulation and expressive treatments, but excessive overshoot is not a default OM personality.

Exact platform curves remain implementation tokens and may be tuned per renderer while preserving duration class and semantic relationship.

## 4. Composer ↔ Omnibar morph

The compact-shell transition is a semantic transform, not a crossfade between unrelated controls.

The lower input surface should visibly retain continuity while:

- message-target identity withdraws;
- Omni prompt/result affordances appear;
- the message draft is preserved outside the rendered Omni text state;
- exiting Omni restores the prior composer state.

The transition should normally fit `motion.spatial` and remain interruptible.

## 5. Conversation navigation

Entering a conversation should preserve enough source/destination continuity to teach the spatial relationship, but must not make long list jumps visually theatrical.

On powerful devices, shared-element continuity may be used where stable and performant. On constrained clients, a simpler directional/spatial transition is valid.

## 6. Reduced motion

Reduced Motion changes the renderer, not the meaning.

- remove parallax, large travel, bounce, particle systems, and unnecessary zoom;
- preserve state change with short opacity, color/surface, icon, text, or instant geometry transitions;
- expressive motion may be disabled completely;
- semantic motion should reduce to the least motion required for comprehension.

## 7. Performance fallback

When rendering budget is constrained:

1. preserve immediate input feedback;
2. preserve semantic state visibility;
3. reduce expressive effects;
4. simplify shared-element/spatial motion;
5. never delay text input or message delivery just to complete an animation.

## 8. Motion and attention

Motion intensity is bounded by the attention hierarchy. Repeated routine events should not continuously animate in peripheral vision.
