# OM Messenger Baseline V1 — Product Completeness and Semantic Boundaries

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:18 PHT
- **Scope:** Ordinary messenger completeness, canonical event impact, local-vs-network boundaries, and first-release interoperability expectations

## Decision summary

OM V1 must feel like a normal modern messenger before optional ecosystem features are used. Product completeness does **not** mean every visible feature becomes a new protocol primitive.

The baseline uses four classes:

1. **Core interoperable semantics** — every conforming baseline client must preserve/understand the meaning.
2. **Standard optional capability** — OM defines interoperable semantics, but a client may show an explicit unsupported/degraded surface when the capability is unavailable.
3. **Local/account-private UX** — expected product behavior that does not create shared conversation state.
4. **Later capability** — deliberately excluded from the first-release contract so it cannot distort the core event/security model.

A reference client may implement more than the minimum conformance surface, but it may not reinterpret these classifications.

## First-release matrix

| User expectation | V1 class | Canonical semantic decision |
|---|---|---|
| Text and bounded rich text | Core interoperable | `message.create` + `RichTextV1`; executable markup is never authoritative. |
| Ordered/unordered lists | V1 interoperable presentation | Registered non-critical `om.rich-list.v1` annotates existing W0 paragraph fallbacks; a wire-criticality mismatch rejects the event, while registered-schema violations and presentation mismatches ignore the enhancement and fall back to ordinary RichText (ADR-0150/0151/0155). |
| Reply / quote | Core interoperable | `reply_to` references an event in the **same ConversationID**. Cross-conversation pointers are not replies. |
| Unicode/basic reactions | Core interoperable | `reaction.set` is idempotent actor/target/reaction state, an epoch-major register. Custom-pack reactions use `reaction_asset` (reserved for W1) under [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md). |
| Edit own message | Core interoperable | `message.edit` replaces editable presentation fields; replacing binary payloads means send a new message. |
| Delete own message | Core interoperable | `message.delete` creates a shared tombstone; physical erasure after delivery is never promised. |
| Moderator removal/redaction | Core interoperable for moderated rooms | Distinct `moderation.redact`; moderation authority is not impersonated as the author's delete. |
| Files / photos / video / audio | Core interoperable | Encrypted `AttachmentRef`; camera/gallery/file-picker are local acquisition UX. |
| Voice notes | V1 interoperable presentation | Audio attachment (`AttachmentRef` with an audio media type and `duration_ms`) plus optional non-critical `om.voice-note.v1` presentation extension; old/W0 clients safely fall back to ordinary audio. No new transport or cryptographic class (ADR-0148). |
| Mentions | Core interoperable | Semantic RichText mention nodes; notification indexes may be denormalized but must agree with content. |
| Forward / resend | Core interoperable | A forward is a **new** `message.create` by the forwarding account. Private source identity/event IDs are not leaked across conversations by default. |
| Shared room pins | Core interoperable for groups/rooms | `pin.set` is shared authorized state; a personal bookmark remains local-only. |
| Polls | Standard optional capability included in V1 product | `poll.create`, `poll.vote`, `poll.close`; V1 does not claim cryptographic voter anonymity. |
| Typing | Standard optional ephemeral capability | Best-effort short-TTL signal; never durable correctness state. |
| Presence / last-seen | Standard optional ephemeral capability | Privacy-policy controlled and not required for messaging correctness. |
| Delivery/read receipts | Standard optional capability | `receipt.update`; privacy policy may disable or reduce granularity. |
| Disappearing messages | Core interoperable when used | Envelope `expiry` is the canonical timer; `expires_after` is only a local bound; group timer in GroupControlState ([`EVENT_MODEL.md`](EVENT_MODEL.md) "Expiry"). Expiry is deletion/retention policy, not proof that a recipient forgot plaintext. |
| Search | Local/account-private UX | Local index over data the client can lawfully decrypt; no mandatory global search service. |
| Drafts | V1 local/account-private UX | Stored locally and, for linked eligible devices of the same IdentityProfile, synchronizable through the authenticated E2EE account-private channel under ADR-0148 conflict-preservation rules. Never conversation authority. |
| Mute | V1 local/account-private UX | Notification preference, not room authority; may sync account-privately under the ADR-0148 allowlist. |
| Archive / folders / labels | V1 local/account-private UX | Organizational view state only; may sync account-privately under the ADR-0148 allowlist. |
| Scheduled send | Local UX (device-local queue) | Encrypted device-local, profile-bound queue until actual send time. Sent as an ordinary message stamped at actual send time; an overdue item goes out at the next opportunity marked sent-late to the sender; a device that loses eligibility discards it (ADR-0156). |
| Stickers | Product-baseline capability | First-class UX is required; bounded inert media + portable pack semantics are frozen in [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md). |
| GIF-like looping animation | Product-baseline capability | Modeled semantically as looping animation; baseline interoperable expressive payload is bounded animated WebP rather than a requirement to use the legacy GIF codec. |
| Contact sharing | Core interoperable | Existing signed/expiring `ContactCapsule` model. |
| Link preview | Standard optional capability | Existing privacy-preserving provider/client boundary; preview failure must not block message delivery. |
| Block | Core user-safety behavior | Local/account policy with delivery/contact effects defined by the abuse architecture. |
| Report / evidence export | Core user-safety behavior where an operator/community exists | Explicit selective evidence package; no automatic universal OM moderator. |
| 1:1 calls | Product baseline; standard optional capability | Existing call/session architecture. Text messaging still works on clients without media capability. |
| Group calls | Product baseline where platform capability exists | Existing MLS-aware call architecture; no need to make call signaling durable conversation content. |
| Small/normal private groups | Core product baseline | One `ConversationID`, AccountID human roster, DeviceIdentity MLS roster. |
| Communities / spaces | Standard V1 capability | Container/control object; independent linked rooms; see [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md). |
| Forum topics / lightweight threads | Standard V1 capability | Layered room-local organization; see [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md). |
| Private announcement room | Standard V1 capability | Ordinary private MLS room with `room.post` granted only to allowed publishers. |
| Public broadcast publication | Standard V1 capability | Separate signed `Publication`, not a pretend MLS group. |
| Static location card | Standard optional V1 capability | One-shot `om.location-card.v1` non-critical event extension beside a W0-valid textual/`geo:` fallback. No continuous/background disclosure. |
| Live location | Later capability | Requires explicit continuous-disclosure, expiry and background-access threat modeling. |
| Stories/status feed | Later capability | Not required for messenger completeness and should not force social-network semantics into Core. |
| Payments / wallet | Later/external capability | Explicitly outside OM Core and SeedLife operation. |
| Mini-apps | Later/plugin capability | Must wait for the sandbox/capability model in A-009. |
| Server-side scheduled delivery | Later service capability | Would disclose content or encrypted payload/timing to an external scheduler and needs an explicit service model. |
| Massive private subscriber broadcast | Later cryptographic capability | V1 does not invent sender-key/broadcast crypto beside MLS. |

## Event vocabulary changes required by ordinary features

The small immutable event model remains. The baseline adds only semantics that genuinely create shared state:

```text
message.create
message.edit
message.delete
reaction.set
receipt.update
pin.set
moderation.redact
poll.create
poll.vote
poll.close
```

Threads do not need a new event kind: `message.create.thread_root_event_id` points to the thread root. Topics have separate room-organization state/events described in [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md) because topic lifecycle is not ordinary message content.

Unknown optional capabilities continue to use the extension criticality rules. A client must not silently reinterpret a poll, topic operation, custom reaction, or other standardized capability as plain text if that would change meaning.

## Reply and quote boundary

A reply is a relationship inside one conversation security context:

```text
reply_to -> EventID in the same ConversationID
```

A client receiving a cross-conversation `reply_to` treats it as malformed/unsupported. This prevents a private message's opaque event identifier from becoming an accidental global tracking handle.

A user can manually quote/copy text from elsewhere, but that is new authored content unless a separately defined public provenance object is used.

## Forwarding and provenance

Private forwarding must favor recipient privacy and source privacy.

Baseline behavior:

1. the forwarding account creates a new event;
2. copied media is re-encrypted for the destination context and SHOULD receive a fresh BlobID/location record where practical;
3. private source `ConversationID`, `EventID`, AccountID, relay/provider metadata and cryptographic transcript are **not** carried by default; forwarding converts Mention nodes to plain Text with their display label and drops the `mentions` set, because a mention carries the mentioned principal's AccountID (ADR-0128 rule 20);
4. a client may visually say “forwarded” as local/user-authored metadata without claiming authenticated original provenance;
5. public signed Publication posts may carry a public source reference because their source is intentionally public and verifiable.

A future consent-aware provenance extension may add stronger attribution for private content, but V1 does not turn private forwarding into durable cross-context tracking.

## Message editing boundary

`message.edit` may update the message's editable semantic presentation: RichText content/caption and the mention set derived from it.

It does not silently replace already-delivered encrypted binary content. Replacing a photo/file/voice note means send a new message and optionally delete the old one. This keeps blob integrity, caching and audit semantics comprehensible.

## Author deletion vs moderation redaction

These are intentionally separate:

```text
message.delete       author/account-authorized shared tombstone
moderation.redact    room-authorized suppression of another principal's content
```

Both preserve the truth that remote plaintext may already have been copied. A moderator redaction must retain enough local/authenticated metadata to show that moderation occurred without pretending the original author performed the action.

## Shared pin vs personal bookmark

The old event-model wording “pin is local-only” is split into two different features:

- **personal bookmark:** local/account-private state;
- **room pin:** shared `pin.set(target_event_id, present)` authorized by room policy.

This avoids forcing personal organization into the network while supporting the ordinary group-chat expectation of moderator/shared pins.

## Poll semantics

V1 polls are intentionally simple and convergent.

Conceptually:

```text
poll.create
  # the poll is identified by this poll.create EventID
  question
  options[]
  selection_mode: single | multiple
  results_visibility: immediate | after_vote | after_close

poll.vote
  target_poll_event_id
  selected_option_ids[]

poll.close
  target_poll_event_id
```

Vote state per `(actor_principal, target_poll_event_id)` is an epoch-major last-writer-wins register ordered by `(sending epoch, EventClock)`. `poll.close` is monotonic: a vote ordered after the close in that order is ignored, whatever its arrival time, and clients must not use wall-clock timestamps as security authority. Backdating a vote within one epoch is a documented residual risk ([`EVENT_MODEL.md`](EVENT_MODEL.md), ADR-0128 rule 17). A creator or authorized room moderator may close a poll according to room policy.

Because ordinary group events are visible to members who can decrypt the room, V1 does **not** label votes “anonymous.” Cryptographic anonymous voting would be a separate protocol with different trust/tally requirements.

## Scheduled messages

The secure default is deliberately boring:

```text
compose -> encrypt/store in local authorized-device queue -> at due time create/send normal message event
```

ADR-0156 fixes the details:

1. The queue is device-local and profile-bound; it is not in the V1 account-private sync allowlist, and each item captures its `ProfileExecutionContext`.
2. At send time the item becomes an ordinary new `message.create`. EventClock, sending epoch, authorization and `expiry` are taken at actual send time, never backdated to compose or due time.
3. If the device was not running at the due time, the item goes out at the next opportunity and the sender's UI marks it "sent late". Recipients see an ordinary message.
4. If the device or profile loses eligibility first (device revoked, profile removed, left or removed from the room, relationship ended or blocked), the item is discarded with a local notice and never migrated to another device.
5. The queued item is **compose intent, not precomputed protocol output** (ADR-0157). Before actual send it does not persist a signed future ConversationEvent, Double-Ratchet/MLS ciphertext, SealedDeliveryEnvelope, frozen recipient-device fanout/group epoch or provider deposit wrapper. At actual send the client re-resolves current relationship/group state, eligible devices/routes and cryptographic epoch, then constructs/encrypts/fans out normally. Staged attachment bytes may remain locally protected, but authorization/expiry/recipient visibility are rechecked before upload or delivery.

No relay/provider gets a special “send this plaintext later” privilege. If users later want guaranteed cloud scheduling while all devices are offline, that becomes an explicit optional service with a visible trust model.

## Rich-list semantics

V1 standardizes `om.rich-list.v1` as registered **non-critical** presentation metadata over existing W0 paragraph blocks; it does not add a RichText block type or duplicate item text. `RichListV1Extension` contains 1..256 `RichListItemV1` annotations. Each item binds a unique strictly increasing `block_index` (0..4095), `depth` (0..3), `style` (`unordered` or `ordered`) and, only for ordered items, `ordinal` (1..9999). The first item and the first item after any block-index gap have depth 0; within a contiguous run depth may rise by at most one per item.

The extension is meaningful only for `message.create` (against `content.blocks`) and `message.edit` (against `replacement.blocks`); other event kinds ignore it as unusable non-critical presentation metadata. The referenced block must be a W0 paragraph. Its first inline must be an **unmarked** InlineText beginning with the canonical fallback marker: two spaces per depth followed by `- ` for unordered items or the unpadded decimal ordinal plus `. ` for ordered items. Supporting clients first apply the registered ADR-0149/ADR-0151 object/criticality/payload schema: a wire-criticality mismatch rejects the containing ConversationEvent, and any other malformed known payload (wrong object, unknown/missing field, or a registry-declared item/depth/index/ordinal bound) makes the whole list enhancement unusable, leaving the event valid (ADR-0155). After that schema layer passes, any bad block reference/order, nesting transition, ordered/unordered ordinal-presence rule, or marker mismatch causes the entire non-critical list interpretation to be ignored and the authenticated W0 RichText to be rendered unchanged. Unannotated blocks terminate a list run; later runs restart at depth 0. Plain-text copy/export/accessibility fallback comes from the W0 RichText with its canonical markers. Links, mentions, marks and item text therefore retain exactly the semantics old clients receive (ADR-0150). Annotations apply only to the RichText of the event that carries them: a create's list never applies to a later edit's replacement, and an edit without its own `om.rich-list.v1` renders as plain W0 RichText (ADR-0155).

## Voice-note semantics

Voice notes are ordinary encrypted audio attachments plus optional presentation metadata; they never become a separate message security class. V1 standardizes the registered non-critical extension `om.voice-note.v1` beside a W0-valid `message.create` body. Its deterministic-CBOR value is the integer-keyed `VoiceNoteV1Extension` struct: key 0 `attachment_index` (0..31) and optional key 1 `waveform_u8` (16..256 normalized amplitude bytes). The index binds the immutable original `message.create` attachment list; a later edit cannot retarget it. The referenced attachment must be audio and carry `duration_ms`. A client that ignores or rejects only the presentation extension still renders ordinary playable audio, and waveform samples are only presentation hints that may be regenerated locally (ADR-0149).

The reference V1 UX includes record/lock/cancel, pause/resume, waveform where available, scrubbing and playback-speed controls. The exact baseline audio codec/container belongs with media capability negotiation rather than the conversation event taxonomy.

## Static location-card semantics

V1 standardizes `om.location-card.v1` as a registered **non-critical** event extension beside a W0-valid RichText fallback. Its integer-keyed `LocationCardV1Extension` value uses key 0 `lat_e7_offset` (0..1800000000, signed E7 = value - 900000000), key 1 `lon_e7_offset` (0..3600000000, signed E7 = value - 1800000000), optional key 2 `accuracy_m` (1..1000000) and optional key 3 `label` (1..256 UTF-8 bytes).

The extension is meaningful only on `message.create`. The fallback must contain the exact canonical `geo:<lat-fixed-7>,<lon-fixed-7>` token derived from the extension point, and no canonical-form `geo:` token for any other point; repeating the same token is fine (ADR-0155). A canonical-form token is `geo:` followed by `[-]1*DIGIT "." 7DIGIT "," [-]1*DIGIT "." 7DIGIT`, where the character before `geo:` (if any) is not an ASCII letter, digit or one of `_ . + -`, and the character after the last digit (if any) is not a digit or `.`. The extension point's own token renders each coordinate as its signed E7 value with exactly seven fraction digits and no leading zeros or `+` (ADR-0168 F-17). After an accepted edit, the card is shown only while the currently materialized text (the winning edit's replacement) still passes this check; otherwise the card is suppressed and the edited text is rendered. If a new client receives a nonempty but mismatched fallback, it ignores the non-critical card extension and renders the W0 fallback; it does not show a different location than an old client. The label is sender-authored presentation text and cannot replace the coordinate-bearing point. Sharing is one explicit user action. The client does not silently promote EXIF/current-device location, and rendering the card does not grant background location. A third-party map request occurs only under the user's configured map/privacy policy. Live/continuous location remains later work (ADR-0149).

## Calls remain outside durable chat events

Call invitation/session signaling remains short-lived session state under [`CALLS_AND_REALTIME_MEDIA.md`](../calls/CALLS_AND_REALTIME_MEDIA.md). A timeline may materialize a local “missed call / call ended” notice, but a client does not need a permanent ConversationEvent to make the media security protocol correct.

## Product completeness rule

A first-party/reference V1 release is not considered product-complete if an ordinary user cannot perform the baseline product actions above simply and visibly. Conversely, a feature being visible in the reference client does not give it hidden authority or make SeedLife its required provider.

## Remaining dependencies

This document resolves **A-010 baseline_product**. It deliberately leaves exact details owned by other blocking architecture items:

- A-001/A-002: sticker, animation and portable pack formats/security;
- A-004: bot/LLM principal invocation and disclosure;
- A-009: plugin/app sandbox and capability model;
- A-011: external-network bridge fidelity and trust-state mapping — resolved in [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md).
- A-012: import/export/escape — resolved in [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md).

The completed A-013..A-015 passes pressure-tested these semantics for metadata leakage, decentralized abuse handling and long-term protocol evolution. Their resolutions extend the surrounding architecture without weakening the baseline semantic boundaries frozen here.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0121](../../../decisions/ADR-0121-event-ordering-authorization-epoch-and-materialization.md) (polls and topics): vote options are validated, and single-choice polls take one option. An empty selection retracts. The poll author or a holder of `room.redact_others` may close a poll. The earliest `topic.create` for a TopicID by `(sending epoch, EventClock)` wins and initializes both the title and the status registers (ADR-0174 rule 1). `message.topic.set` with `topic_id` absent clears the placement (OM-CBOR never uses `null` as a typed value). An author may always delete their own content, but may edit only while holding post rights.
