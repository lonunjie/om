# Responsive Capability Layouts

**Status:** Normative layout model; numeric widths are validation references, not cross-platform law

## Principle

OM responds to the **available window and input capabilities**, not to a guessed device category.

A foldable in split-screen may be compact. A phone connected to a large external display may be expanded. A desktop window may be narrower than a tablet.

## Four layout capabilities

### Compact

Typical realization:

- one major pane at a time;
- chat list → conversation → details as navigated contexts;
- composer owns the lower edge in a conversation;
- Omnibar uses the composer/Omni semantic morph;
- enabled tabs appear only when more than one top-level destination is exposed.

### Split

Typical realization:

- conversations/navigation + active conversation simultaneously;
- details remain transient or replace the list when space is constrained;
- global Omnibar may remain available without competing with the composer.

### Expanded

Typical realization:

- navigation rail/sidebar;
- conversation list;
- active conversation;
- optional detail/member/media pane when useful.

Extra width reveals **parallel context**, not new mandatory hierarchy.

### Multi-pane / wide workspace

Used only when the current task benefits from additional persistent context. Examples include a Space channel list plus conversations or a desktop media/details inspector.

Do not fill wide screens merely because space exists.

## Reference widths

For Android/Compose validation, OM may use current window-size-class references such as compact below ~600 dp, medium/split around 600–839 dp, and expanded at ~840 dp and above. These values are **adapter references**, not universal OM breakpoints.

PWA and Apple implementations derive capability from available content/window space, safe areas, text size, and current layout pressure. Container-based decisions are preferred where a component can adapt independently.

## Layout invariants

Across all capabilities:

- object/action semantics stay the same;
- Back returns to the previous interaction context;
- selection survives a resize where the selected object still exists;
- drafts survive layout changes;
- focus restoration remains correct;
- current conversation does not silently change;
- tab meaning/order follows user configuration;
- security-critical surfaces do not become less explicit merely because more space is available.

## Foldables, hinges, safe areas, and overlays

Avoid placing primary controls under:

- display cutouts;
- hinge/fold occlusion;
- system gesture areas;
- Dynamic Island/system overlays;
- browser/install UI or virtual keyboard regions.

Use platform safe-area/window-inset APIs rather than model-specific device lists.

## Height matters too

Short landscape windows can require compact vertical behavior even with large width. Composer, call controls, Omnibar, and critical confirmation actions must remain reachable when the software keyboard is visible.

## Pointer and keyboard density

A large window does not automatically imply mouse/keyboard, and a compact device may have both. Hover affordances and shortcut hints activate based on actual capabilities; touch targetability remains adequate on hybrid hardware.

## Reflow

Web/PWA non-exempt content must remain usable at the WCAG reference equivalent of 320 CSS pixels without two-dimensional reading scroll. OM additionally validates 200% text and large system font settings in representative compact/split layouts.
