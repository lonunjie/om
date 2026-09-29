# Typography, Density, Spacing, and Targetability

## 1. Base grid

OM uses a 4-unit logical spacing grid. Components should normally compose from:

`2, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64`

The 4-unit grid is a construction aid, not a command to force every optical alignment onto an exact mathematical boundary.

## 2. Density profiles

OwnMessenger exposes semantic density profiles rather than arbitrary global scaling:

- **Comfortable** — default compact-mobile profile; generous message/list spacing and touch targets.
- **Compact** — denser desktop/tablet or expert profile; smaller visual spacing while preserving targetability and focus.
- **Large** — larger spacing/targets for accessibility, coarse input, gloves, vehicle/room interfaces, or preference.
- **Custom** — advanced override within safety limits.

Density changes must not reorder information or change action meaning.

## 3. Targetability

Provisional baseline:

- compact-mobile preferred interactive hit region: **48 logical px**;
- compact-mobile minimum normal hit region: **44 logical px** unless a platform/accessibility rule requires more;
- desktop pointer controls may look visually smaller, but keyboard focus and pointer hit regions remain comfortably targetable;
- closely packed controls require sufficient separation to avoid accidental activation.

The visible icon may be smaller than its invisible hit region.

## 4. Typography roles

OM should begin with these semantic roles rather than per-screen font decisions:

| Role | Provisional size | Weight | Typical use |
|---|---:|---:|---|
| `display` | 28–32 | 600–700 | rare major empty-state/identity moments |
| `title` | 20–24 | 600–700 | conversation/surface titles |
| `headline` | 17–20 | 600 | strong list/group labels |
| `body` | 16 | 400 | messages and primary reading text |
| `body.emphasis` | 16 | 600 | emphasized message/list text |
| `label` | 14 | 500–600 | controls, tabs, metadata with importance |
| `caption` | 12–13 | 400–500 | timestamps, secondary metadata |

Values are logical CSS/device-independent sizes and remain subject to platform text scaling.

## 5. Messages are reading surfaces

Message text should not be made artificially small to increase visible message count.

- Default body text should remain comfortable for sustained reading.
- User text scaling propagates through bubble layout rather than clipping.
- Line height should normally be around 1.35–1.5× body size, adjusted by script and font metrics.
- Long messages require comfortable line lengths on wide desktop windows; bubbles should not expand to the full viewport merely because space exists.

## 6. System font first

Until a dedicated OM font demonstrates a real benefit, default to each platform's system UI stack.

Reasons:

- excellent localization/script coverage;
- instant availability;
- strong native rendering;
- accessibility scaling;
- low startup cost;
- reduced visual mismatch in system surfaces.

OwnMessenger may offer alternate fonts. A custom font must not break fallback scripts, emoji, accessibility sizing, or message measurement.

## 7. Optical alignment

Avatar centers, icon optical bounds, text baselines, bubble tails, and badges may require optical—not purely geometric—alignment.

Tokens define intended rhythm; components may make documented optical corrections.

## 8. User scaling outranks layout purity

When font or display scaling increases:

1. controls may become taller;
2. tab labels may truncate or transition to icon+accessible-label modes;
3. panels may become scrollable;
4. content may move from multi-column to single-column;
5. fewer persistent controls may remain visible.

OM must adapt rather than silently cap accessibility scaling.
