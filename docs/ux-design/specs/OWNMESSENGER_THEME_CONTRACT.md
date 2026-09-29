# OwnMessenger Theme Contract

## 1. Principle

OwnMessenger lets the user make OM feel like *their* messenger without making OM semantically unstable.

> Personalize expression, not truth.

## 2. Two different powers

Distinguish:

### User configuration

The user may deliberately change behavior allowed by OM, such as:

- visible/reordered tabs;
- density;
- left/right shell placement;
- keyboard bindings;
- Focus/Balanced/Playful profile;
- sounds/haptics;
- font and text presentation;
- account-specific appearance.

### Theme package

A theme may style approved presentation tokens but cannot silently change navigation, actions, permissions, or safety behavior.

This separation prevents a downloaded theme from becoming an executable UX policy.

## 3. Themeable token classes

Themes may provide values for approved roles such as:

- semantic colors;
- typography families/weights where safe;
- radius family;
- spacing personality within density constraints;
- icon family mapped by canonical icon identifiers;
- wallpaper/background;
- semantic motion personality within duration/reduced-motion limits;
- OhMessenger expressive motion;
- sound and haptic packs where the platform permits.

## 4. Protected invariants

Themes cannot:

- rename canonical actions;
- swap action meanings;
- bypass Action Registry confirmation/authentication;
- hide focus;
- shrink required hit targets below safety/accessibility limits;
- hide critical status/failure/security states;
- turn disabled controls into enabled-looking controls;
- reorder security choices to trick the user;
- capture input or network data;
- run arbitrary executable code merely to style OM.

## 5. Theme package safety

A portable theme should be declarative data/assets, not trusted application code.

Clients should validate:

- schema version;
- asset types/sizes;
- token ranges;
- contrast requirements;
- supported icon mappings;
- motion caps;
- safe URI/resource rules.

Unsupported or invalid values fall back to OM defaults.

## 6. Compatibility

Themes target a versioned semantic token contract, not DOM/component class names.

New OM versions may add tokens. Missing theme values inherit safe defaults. Existing meanings must not be reassigned to incompatible roles.

## 7. Per-account and per-context profiles

OwnMessenger may allow different themes or experience profiles by account. Conversation-specific appearance is possible where it does not create phishing/identity confusion.

Security and identity surfaces may deliberately use protected styling independent of the current decorative theme.

## 8. Export/import

If theme/profile export is later supported, exported data must clearly distinguish:

- purely visual theme data;
- interaction preferences;
- notification preferences;
- learned local ranking data.

Learned behavioral data must never be silently bundled into a visual theme export.

## 9. Curated presets and custom themes

OM should ship a small set of studied, QA-validated light/dark preset pairs. Presets are examples of safe personalization, not privileged interaction grammars.

Custom themes may select other aesthetics, but the renderer validates protected semantic roles. When a requested value creates insufficient contrast or semantic collision, OM may derive/adjust the rendered token or fall back to a safe value.

## 10. Color role collision

A custom accent may approach a protected success/warning/danger hue, but it does not gain that meaning. OM must preserve sufficient differentiation using lightness/chroma, geometry, icons, labels, and protected critical styling. Security surfaces may override decorative theme values entirely.
