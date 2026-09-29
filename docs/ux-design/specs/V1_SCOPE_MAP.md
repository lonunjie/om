# OM UI/UX v1 Scope Map

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Reconciled UI/UX scope under ADR-0159. Canonical architecture remains authoritative.

## Purpose

Prevent design completeness from becoming release-scope explosion. A capability can be well-designed without being mandatory for v1.

## V1 required UX foundation

These are required for a coherent OM v1 experience, subject to architecture support:

### Core communication

- chat-first shell and Chats default root;
- one-to-one conversations;
- ordinary group conversations;
- text messaging with reply/reaction/edit/delete/forward semantics already expected by OM architecture;
- attachments/files/media and voice notes;
- durable local drafts;
- message states, offline queueing, retry/recovery;
- local/global search through the Omnibar;
- QR/link invitation flows without contact upload by default;
- notifications with privacy-safe preview defaults;
- one-to-one audio/video calling and live-call state language;
- group-call UI where the core exposes group calling.

### Trust, privacy, and continuity

- device authorization/revocation UX;
- verification state language;
- recovery scopes represented truthfully;
- provider/discovery state separated from trust;
- local personalization learning off/on/reset;
- local-first search/index privacy boundaries;
- no hidden remote processing for personalization/translation/transcription.

### OwnMessenger essentials

- OM Clear light/dark reference pair;
- system appearance support;
- safe text-size/density preferences;
- tab visibility/reordering;
- Focus/Balanced/Playful experience profiles;
- notification/privacy controls;
- accessible keyboard/pointer/touch operation;
- Settings searchable via Omnibar.

### Accessibility/internationalization foundation

- WCAG-compatible PWA semantics/reflow targets;
- VoiceOver/TalkBack-compatible semantic mappings in native adapters;
- Reduced Motion handling;
- screen-reader/action equivalents for gesture accelerators;
- externalized strings, plural/select formatting, RTL-capable layout, locale-aware date/number formatting;
- pseudo-localization tests even if v1 ships a limited language set.


### Architecture-required V1 capability surfaces

These are V1 even when hidden from the default shell until context requires them:

- device-local scheduled send with late-binding send semantics;
- architecture-supported account-private draft sync among eligible linked devices, with conflict preservation;
- Spaces with architecture-valid nested Spaces and linked independent Rooms;
- lightweight threads and persistent Topics;
- signed public Publication/feed/follow provenance UX distinct from private Rooms;
- polls;
- first-class stickers and bounded looping expressive media with static/reduced-motion fallbacks;
- rich-text composition/sanitized paste;
- disappearing/view-once lifecycle UX;
- ContactCapsule + MessageRequest provenance;
- privacy-controlled typing/presence/read-receipt presentation;
- privacy-aware link previews;
- block/report/selective EvidencePackage flows;
- T0–T4 Home onboarding disclosures and sponsored client-origin choice;
- recovery-on-by-default custody, Protection profiles, device suspension/quarantine and Safety Freeze surfaces.

## V1 optional / ship if implementation cost and architecture readiness permit

These may ship in v1 without changing the core mental model:

- additional preset themes beyond OM Clear;
- user-created/imported theme packs after asset validation is production-ready;
- advanced keyboard profiles (e.g. Vim-like) beyond standard shortcuts;
- app-specific language selector in addition to OS language;
- sender-supplied/local voice-note transcripts;
- richer OhMessenger stickers/effects while preserving static fallbacks;

## Experimental / evidence-gated

Do not make these correctness dependencies:

- Bayesian or other advanced adaptive Omnibar rankers beyond transparent baseline scoring;
- AI/LLM natural-language interpretation in Omnibar;
- remote speech recognition/translation convenience paths;
- automatic behavior inference such as handedness;
- novel icon-only promotion for Omnibar/Spaces/security concepts without comprehension evidence;
- advanced dynamic sensory/animation packs;
- third-party/community asset distribution marketplace.

## V2+ candidates

- screen sharing/advanced collaborative call surfaces unless architecture moves them into v1;
- advanced multi-window/workspace workflows;
- gaze/ring/game-controller/novel HID optimized adapters beyond generic accessibility/input hooks;
- richer plugin/bot administration surfaces;
- automatic translation/transcription systems with explicit privacy architecture;
- sophisticated cross-account unified search if identity/account boundaries remain understandable;
- adaptive AI-generated themes/effects.

## Deferred human/evidence gates, not feature backlog

The following are intentionally unresolved and must not be interpreted as missing architecture features:

- final OM logo/wordmark;
- public long-form product-name clearance;
- promotion of risky/novel icons to label-free use;
- final brand-palette freeze beyond OM Clear as reference candidate;
- physical iOS/Android ergonomics and assistive-technology closure;
- blinded usability validation.

## Scope-change rule

Adding a v1 feature requires answering:

1. Does it serve OM's core communication/private-control mission?
2. Does it introduce a new mental model or reuse an existing object/action grammar?
3. Can it work truthfully across PWA-first constraints and native clients?
4. Is it safe/accessibly operable without an onboarding course?
5. What existing v1 work does it displace?

A feature that fails these questions defaults to optional/experimental/v2 rather than silently expanding v1.
