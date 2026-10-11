# OM Identity and App-Icon System

**Status:** normative identity architecture; candidate artwork remains provisional in Revision 11C.

OM's identity system must make the product easy to recognize without weakening the UI/UX constitution. The identity mark is not a decorative shortcut around usability: it must remain simple, legible, platform-adaptive, culturally respectful, and recognizable when color, motion, depth, and container shape disappear.

## 1. Identity layers

OM separates four things that are often incorrectly fused:

1. **Core mark** — the compact visual identity that must survive monochrome, masking, tiny sizes, notification treatment, and platform theming.
2. **Wordmark/name lockup** — `OM`, `OpenMessenger`, or other approved product-name presentation. Typography is a separate decision from the mark and is not frozen by Revision 11C.
3. **App-icon presentation** — platform-owned container/background/layer treatment around the same core mark.
4. **Context assets** — favicon, notification mark, share/invite branding, store assets, launch surfaces, and other derived forms.

The same core mark should remain identifiable across all four layers.

## 2. Non-negotiable principles

### 2.1 Silhouette before color

The mark must work in one color before a palette is applied. Color may improve recognition, personality, or brand association, but cannot be necessary to understand which mark it is.

### 2.2 One mark, platform-native container

OM owns the mark. The operating system may own the mask, tint, depth, parallax, clear/tinted appearance, launcher shape, or notification treatment.

Do not pre-bake a rounded-square container into the core mark. Doing so creates double masks and makes the identity feel foreign on platforms that use circles, squircles, themed icons, or system-generated effects.

### 2.3 Stable core features across appearances

Default, dark, tinted, monochrome, clear, or themed variants must preserve the same core visual features. An appearance variant may change color/material treatment; it must not swap the identity for a different drawing.

### 2.4 Simplicity is a constraint, not a style trend

Tiny surfaces punish detail. The core mark should use few large forms with meaningful negative space. Complexity may be used later in illustrations or campaign media, but not in the primary app-identification silhouette unless testing demonstrates that it remains robust.

### 2.5 Identity is not a functional icon

The OM mark must not silently stand for an unrelated action such as Search, Settings, or Send. If the mark is used as the Omnibar trigger, account/home affordance, or another control, that relationship must be explicitly defined and accessible by name; the brand mark alone is not a universal action verb.

### 2.6 Avoid sacred-symbol appropriation or accidental claims

`OM` is an acronym/brand family within this project. The identity must not copy or stylize the sacred `ॐ`/Om symbol, or imply religious meaning not actually intended by the product. This is both a cultural-respect and clarity constraint.

### 2.7 Recognizable without reading

The mark should be distinguishable from neighboring app icons at small size even when the product name is not visible. The name remains important for learning and recall, but the shape should not collapse into an indistinguishable generic circle or chat bubble.

### 2.8 No forced finality

A mark is not final because it looks good on a design sheet. Final acceptance requires real-device, blinded human, similarity/trademark, and platform-adaptation checks.

## 3. Platform realization

### 3.1 PWA / web

Provide at minimum:

- scalable source SVG;
- PNG raster exports for broadly supported manifest sizes;
- a dedicated `maskable` asset whose essential mark stays inside the maskable safe zone;
- a monochrome/mask form where useful;
- favicon-specific exports that are redrawn/optically tuned if a direct downscale fails.

Maskable-icon essential content must remain within the inner 80% safe-zone diameter documented for web-app icons. The outer region is expendable background/container space.

### 3.2 Android adaptive icon

Provide:

- background layer;
- foreground layer;
- monochrome layer for themed icons;
- all layers on a 108 × 108 dp coordinate canvas;
- essential foreground content inside the 66 × 66 dp safe region.

Do not draw the OEM mask into the foreground. Test circle, rounded-square, squircle, and other representative launcher masks.

### 3.3 Apple platforms

Provide square, unmasked source layers and let the system apply the platform mask/effects. The core mark must remain centered and consistent across default, dark, clear, and tinted appearances. Revision 11C uses flattened previews only to compare candidates; final production should use the current Apple icon authoring workflow/layer format available to engineering at implementation time.

### 3.4 Notifications and tiny surfaces

Notification/status variants must be monochrome-first. Remove nonessential internal detail rather than shrinking the full-color artwork blindly. If the smallest form needs optical simplification, it may simplify *detail* but may not change identity.

### 3.5 Favicon

Test at actual 16, 20, 24, and 32 px raster sizes. A favicon is not evidence that a mark works merely because the source is vector.

## 4. Revision 11C candidate directions

All candidate app-icon previews intentionally use the same OM Calm comparison colors so color preference does not decide the silhouette test.

### Open Loop

An open circular communication loop with a small tail.

**Strengths:** compact, excellent mask survival, strong single silhouette, chat/open-loop association.

**Risks:** can read as `Q`, circular-search/loading language, or a generic speech-loop. Requires blinded association testing before further commitment.

### Dialogue Fold

Two equal conversational halves facing each other.

**Strengths:** expresses reciprocity and peer equality rather than a central broadcaster.

**Risks:** small protrusions add noise, and its silhouette is too close to Open Loop in some tiny contexts. Keep as a comparison candidate unless human testing provides a strong reason to continue it.

### Peer Link

Two interlinked peer loops with no central hub.

**Strengths:** strongest direct expression of peer-to-peer/operatorless connection; survives small sizes and masking.

**Risks:** interlocking-ring/link imagery is a crowded identity category and may evoke payments, chain/linking, or partnership before messaging. Similarity clearance is mandatory.

### OM Fold

Direct O + M monogram.

**Strengths:** immediate product-name relation, compact, strong monochrome and mask behavior.

**Risks:** letter-inside-ring monograms are visually crowded across technology/finance/crypto identities. It can feel corporate rather than human and needs especially strong similarity clearance.

### One Thread

A rounded one-stroke `om` ligature.

**Strengths:** clearest direct name relation without relying on a containing circle; distinctive horizontal silhouette; supports the “one continuous interaction language” metaphor.

**Risks:** occupies a rectangular rather than naturally square silhouette, so it can appear smaller inside strict launcher safe zones. It behaves partly like a wordmark and therefore needs multilingual/name-context testing.

## 5. Revision 11C provisional shortlist

For the next *human* evaluation stage, keep these three diverse hypotheses rather than prematurely selecting a winner:

- **One Thread** — name/continuity hypothesis;
- **Open Loop** — simple communication/open hypothesis;
- **Peer Link** — peer/operatorless hypothesis.

Dialogue Fold and OM Fold remain in the package as controls/comparison directions, not accepted finalists.

This shortlist is not a ranking of final quality and does not constitute a trademark or legal availability conclusion.

## 6. Selection gates before freeze

A final mark must pass all of the following:

1. survives 16–32 px rendering with its distinguishing features intact;
2. survives monochrome, grayscale, dark/light, and system tinting;
3. survives circle/squircle/rounded-rectangle masks without clipping essential identity;
4. remains recognizable in a mixed app grid rather than only on a brand board;
5. produces no dominant unintended association in blinded testing;
6. works when paired with `OM` and `OpenMessenger` without requiring one specific proprietary font;
7. does not depend on color or gradient to become unique;
8. receives appropriate visual-similarity/trademark clearance before public adoption;
9. is tested on actual iOS/iPadOS/macOS, Android launchers, and installed-PWA surfaces current at implementation time;
10. does not become a functional control without explicit accessible action semantics.

## 7. Brand color relationship

OM Calm cobalt is used for Revision 11C comparison previews, but the identity mark is not permanently tied to one hue yet. If OM Calm remains the default product theme, brand and action color may intentionally share a family, but functional UI semantics still take priority: a decorative brand accent must never make ordinary static content look actionable.

## 8. Wordmark

Revision 11C intentionally does not freeze a wordmark font. Candidate lockups use a neutral system sans only to test proportion and spacing. Final typography must follow the existing readability, localization, fallback, and no-font-dependency principles in the visual system.

## 9. Revision 12 validation preparation

Revision 12 does not alter the three shortlist geometries. It adds synthetic iOS/Android/PWA context studies, a blinded local association/findability fixture, explicit freeze gates, and preliminary public-web naming/collision reconnaissance.

Important naming consequence: `OpenMessenger` / `Open Messenger` and especially `OMessenger` already have prior/current messaging-software uses in public app/software listings. Internal visual-candidate names (`One Thread`, `Open Loop`, `Peer Link`) also collide with active software/service names. These findings are **warning signals, not legal conclusions**. The final public word mark remains provisional until qualified clearance.

Human identity tests therefore display **Candidate A/B/C** until the reveal stage. Internal candidate names must not prime the participant.
