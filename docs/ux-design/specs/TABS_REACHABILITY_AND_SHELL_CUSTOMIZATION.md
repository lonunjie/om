# Tabs, Reachability, and Shell Customization

**Status:** Normative direction with prototype-level defaults

## 1. Default visible navigation

OM defaults to one top-level view: **Chats**.

If exactly one top-level tab is enabled, the tab strip is hidden. The user already knows the current place, so permanently displaying a single tab adds chrome without adding orientation.

Users can expose additional views through OwnMessenger, including Calls, People, Spaces, Files/Media, and future registry-approved destinations.

## 2. Capacity is derived, not fixed

The shell must calculate how many tabs fit safely from:

- available width;
- safe-area insets;
- text scaling;
- localization;
- minimum target size;
- selected density;
- user preference.

A typical phone may often show roughly four labeled tabs, but `4` is never a product invariant.

## 3. Overflow

If visible tabs exceed safe capacity:

- horizontal scrolling is allowed;
- the selected tab remains easy to re-find;
- partial-next-item, edge fade, position marker, or equivalent must reveal that more tabs exist without relying only on motion;
- semantic movement should preserve spatial ordering during reordering or selection;
- keyboard and screen-reader traversal follow the same stable order.

## 4. Reordering and visibility

Users may:

- show/hide optional tabs;
- reorder them;
- pin one or more preferred tabs;
- reset to the OM default;
- save shell presets later if justified.

Capabilities hidden from tabs remain reachable through Omnibar and canonical actions.

## 5. Handedness and reachability

OM must not equate `leading` with `important` or hard-code top-left account access.

For compact mobile, the default shell should place high-frequency primary controls in the lower reach zone.

Recommended default:

- Omnibar: bottom;
- account/profile: trailing lower edge on root surfaces;
- Send/voice: far trailing edge of composer;
- Omnibar trigger: adjacent to Send/voice;
- attachment: leading edge;
- Back: platform/gesture + visible leading fallback where needed.

OwnMessenger may mirror leading/trailing control groups without changing action meaning.

OM should ask or let the user choose a reachability preference rather than infer handedness from private behavioral telemetry.

## 6. Platform consistency

The compact shell should remain almost identical across iOS, Android, and mobile PWA.

Platform-specific adaptations are limited to system-owned behavior such as:

- safe areas;
- OS Back integration;
- share sheets;
- system pickers;
- permission prompts;
- biometric surfaces;
- keyboard/accessibility APIs.

The location and meaning of OM-owned primary shell controls should not drift merely because platform UI kits differ.
