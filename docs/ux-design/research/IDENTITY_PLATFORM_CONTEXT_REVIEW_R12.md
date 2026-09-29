# Identity platform-context review — Revision 12

**Evidence class:** synthetic platform-context study + deterministic asset review. Not human findability evidence.

## What was tested

The three R11C shortlist candidates — **One Thread**, **Open Loop**, and **Peer Link** — were placed into synthetic but platform-shaped contexts: iOS-like light/dark/tinted home grids, Android-like adaptive and themed grids, and PWA/browser favicon context. The surrounding icons are synthetic so this package does not depend on third-party brand artwork.

The study intentionally tests **survival in clutter**, mask behavior, monochrome/theme behavior, and relative footprint. It does not claim to measure human recognition or preference.

## Observations

### One Thread
- Retains the most unusual silhouette of the three and remains clearly different in monochrome/themed contexts.
- Its horizontal footprint is the main weakness: within square launchers it can feel optically smaller than circular peers even when its bounds are safe.
- This makes **real app-grid findability timing** especially important. A mild optical scale increase may be justified later, but must not be made before human evidence.

### Open Loop
- Fills square/circular launchers efficiently and stays visually strong in tinted/themed modes.
- Its principal weakness remains semantic/visual crowding: Q-like, search-like, circular-arrow-like, and loop/chat-loop associations are plausible.
- Human open-association testing is more important than another geometry refinement.

### Peer Link
- Strong horizontal peer-to-peer structure and good themed/monochrome survival.
- Interlocking rings/links are a crowded logo motif, so distinctiveness cannot be inferred from rendering quality alone.
- The mark should be tested for unintended chain/link/network-brand associations.

## Platform constraints retained

- Apple: preserve the same core features across default/dark/clear/tinted appearances; let platform material/masking remain platform-owned.
  https://developer.apple.com/design/human-interface-guidelines/app-icons
- Android: keep essential artwork inside the central 66×66 dp never-clipped region of the 108×108 adaptive icon canvas and provide a monochrome themed layer.
  https://developer.android.com/develop/ui/compose/system/icon_design_adaptive
- PWA: preserve maskable/monochrome assets and keep essential artwork inside the mask-safe region.
  https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/How_to/Define_app_icons

## Decision

No shortlist candidate is frozen by this static review. All three remain valid for blinded human testing. The next change to their geometry should be driven by human/device evidence, not aesthetic churn.

## Artifacts

- `assets/identity/r12/previews/platform-context-shortlist-r12.png`
- per-candidate synthetic launcher/browser context PNGs under `assets/identity/r12/<candidate>/`
- `prototypes/identity-validation-v1/` for blinded association + timed synthetic-grid findability
