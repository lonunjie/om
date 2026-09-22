# OM Master Planning & Design Snapshot

Generated: **2026-09-22 13:42 PHT (UTC+08:00)**

This snapshot supersedes the 2026-09-22 07:34 PHT checkpoint, preserved under `history/20260922-0734/`. Earlier snapshots remain preserved under their existing `history/` directories.

## Phase

Architecture consolidation now includes a buildable shared product/UI direction. Protocol/security-sensitive wire choices still require concrete crypto selections, canonical encodings, vectors, adversarial testing and independent review. UI implementation still requires framework/bridge prototyping and usability/performance validation.

## Major additions in this checkpoint

1. One shared `om-ui` implementation is canonical for official PWA/iOS/Android/Desktop clients.
2. Native/platform shells expose capabilities/system surfaces rather than reimplementing product/security semantics.
3. Primary information architecture is Chats / Calls / People, with Settings under profile.
4. Layout adapts to available window size through Compact / Medium / Expanded / Wide modes.
5. Added complete onboarding, discovery, Share Contact, message request, call, protection, recovery, device and OM Home UX.
6. Added semantic MotionSystem and perceived-performance rules targeting smooth, immediate, Telegram-class fluidity without decorative motion.
7. Added deep OwnMessenger themes/fonts while treating themes as untrusted declarative data only.
8. Added theme-proof security semantics, strict no-remote-tracking/no-code theme boundary and web CSP/Trusted Types defense-in-depth direction.
9. Added shared accessibility/input release gates and 44×44 primary touch-target product goal where practical.
10. Added explicit Core/UI/PlatformAdapter typed-command/view-state contract.
11. Added responsive static shell prototype under `design/prototype/`.
12. Renamed the everyday privacy preset from Standard to **Recommended**; crypto strength remains identical across Recommended/Hardened/Maximum Privacy.

## New canonical documents

- `UI_UX_SYSTEM.md`
- `MOTION_AND_PERFORMANCE.md`
- `THEMES_FONTS_AND_PERSONALIZATION.md`
- `ACCESSIBILITY_AND_INPUT.md`
- `UI_CORE_PLATFORM_CONTRACT.md`
- `design/SCREEN_MAP_AND_JOURNEYS.md`
- `design/COMPONENT_CATALOG.md`
- `design/theme-bundle.logical.schema.json`
- `design/ui-tokens.example.json`
- `design/MOTION_TOKENS.md`
- `design/prototype/`
- `research/CURRENT_UI_REFERENCES_20260922.md`
- `CHECKPOINT_20260922_1342.md`
- ADR-0051 through ADR-0058

## Highest-priority unresolved gates

- AccountControlState formal transition spec and vectors.
- CryptoProfile v1 algorithms/library boundary and MLS profile.
- Canonical signed-object/event/control encodings and golden vectors.
- Portable OM Core language/FFI/WASM/key-provider boundary.
- Shared UI framework selection after performance/accessibility prototype comparison.
- Native wrapper/WebView↔Core bridge proof on iOS/Android.
- PWA Web Push/VAPID ownership and notification flow for per-user deployments.
- PWA incoming-call/background behavior and native migration path.
- Relay wire protocol and multi-device sync/history bootstrap.
- Blob/backup manifest and restore/repair protocol.
- EvidencePackage construction and deniability semantics.
- Concrete release/update toolchain and key ceremony.
- Theme bundle resource limits/catalog/signature UX and final visual token values.
- Philippine legal classification of project publisher vs user deployment vs app distributor/provider before broad public release.

See `CHECKPOINT_20260922_1342.md` and `machine/open_questions.jsonl`.
