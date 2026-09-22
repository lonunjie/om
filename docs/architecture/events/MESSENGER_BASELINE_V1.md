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
| Reply / quote | Core interoperable | `reply_to` references an event in the **same ConversationID**. Cross-conversation pointers are not replies. |
| Unicode/basic reactions | Core interoperable | `reaction.set` is idempotent actor/target/reaction state. Custom-pack reactions depend on A-001/A-002. |
| Edit own message | Core interoperable | `message.edit` replaces editable presentation fields; replacing binary payloads means send a new message. |
| Delete own message | Core interoperable | `message.delete` creates a shared tombstone; physical erasure after delivery is never promised. |
| Moderator removal/redaction | Core interoperable for moderated rooms | Distinct `moderation.redact`; moderation authority is not impersonated as the author's delete. |
| Files / photos / video / audio | Core interoperable | Encrypted `AttachmentRef`; camera/gallery/file-picker are local acquisition UX. |
| Voice notes | Core interoperable presentation | Audio attachment with `presentation_role = voice_note`; not a new transport or cryptographic class. |
| Mentions | Core interoperable | Semantic RichText mention nodes; notification indexes may be denormalized but must agree with content. |
| Forward / resend | Core interoperable | A forward is a **new** `message.create` by the forwarding account. Private source identity/event IDs are not leaked across conversations by default. |
| Shared room pins | Core interoperable for groups/rooms | `pin.set` is shared authorized state; a personal bookmark remains local-only. |
| Polls | Standard optional capability included in V1 product | `poll.create`, `poll.vote`, `poll.close`; V1 does not claim cryptographic voter anonymity. |
| Typing | Standard optional ephemeral capability | Best-effort short-TTL signal; never durable correctness state. |
| Presence / last-seen | Standard optional ephemeral capability | Privacy-policy controlled and not required for messaging correctness. |
| Delivery/read receipts | Standard optional capability | `receipt.update`; privacy policy may disable or reduce granularity. |
| Disappearing messages | Core interoperable when used | Existing expiry semantics; expiry is deletion/retention policy, not proof that a recipient forgot plaintext. |
| Search | Local/account-private UX | Local index over data the client can lawfully decrypt; no mandatory global search service. |
| Drafts | Local/account-private UX | Local by default; future account-private encrypted sync may synchronize them. |
| Mute | Local/account-private UX | Notification preference, not room authority. |
| Archive / folders / labels | Local/account-private UX | Organizational view state only. |
| Scheduled send | Local/account-private UX | Encrypted local queue until actual send time. If no authorized device is online, nothing is sent. |
| Stickers | Product-baseline capability | First-class UX is required; bounded inert media + portable pack semantics are frozen in `EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`. |
| GIF-like looping animation | Product-baseline capability | Modeled semantically as looping animation; baseline interoperable expressive payload is bounded animated WebP rather than a requirement to use the legacy GIF codec. |
| Contact sharing | Core interoperable | Existing signed/expiring `ContactCapsule` model. |
| Link preview | Standard optional capability | Existing privacy-preserving provider/client boundary; preview failure must not block message delivery. |
| Block | Core user-safety behavior | Local/account policy with delivery/contact effects defined by the abuse architecture. |
| Report / evidence export | Core user-safety behavior where an operator/community exists | Explicit selective evidence package; no automatic universal OM moderator. |
| 1:1 calls | Product baseline; standard optional capability | Existing call/session architecture. Text messaging still works on clients without media capability. |
| Group calls | Product baseline where platform capability exists | Existing MLS-aware call architecture; no need to make call signaling durable conversation content. |
| Small/normal private groups | Core product baseline | One `ConversationID`, AccountID human roster, DeviceIdentity MLS roster. |
| Communities / spaces | Standard V1 capability | Container/control object; independent linked rooms; see `SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`. |
| Forum topics / lightweight threads | Standard V1 capability | Layered room-local organization; see `SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`. |
| Private announcement room | Standard V1 capability | Ordinary private MLS room with `room.post` granted only to allowed publishers. |
| Public broadcast publication | Standard V1 capability | Separate signed `Publication`, not a pretend MLS group. |
| Static location card | Later standardized capability | No wire shape is frozen in this pass; plain text/map links remain possible. |
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

Threads do not need a new event kind: `message.create.thread_root_event_id` points to the thread root. Topics have separate room-organization state/events described in `SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md` because topic lifecycle is not ordinary message content.

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
3. private source `ConversationID`, `EventID`, AccountID, relay/provider metadata and cryptographic transcript are **not** carried by default;
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

Latest authorized vote state per `(actor_principal, target_poll_event_id)` wins. `poll.close` is monotonic; concurrent/late votes are materialized by the deterministic event-order rule and clients must not use wall-clock timestamps as security authority. A creator or authorized room moderator may close a poll according to room policy.

Because ordinary group events are visible to members who can decrypt the room, V1 does **not** label votes “anonymous.” Cryptographic anonymous voting would be a separate protocol with different trust/tally requirements.

## Scheduled messages

The secure default is deliberately boring:

```text
compose -> encrypt/store in local authorized-device queue -> at due time create/send normal message event
```

No relay/provider gets a special “send this plaintext later” privilege. If users later want guaranteed cloud scheduling while all devices are offline, that becomes an explicit optional service with a visible trust model.

## Voice-note semantics

Voice notes are ordinary encrypted attachments plus presentation metadata such as duration/waveform hints where safe. Clients can offer hold-to-record, playback speed and waveform UI without inventing a separate message security model.

The exact baseline audio codec/container belongs with media capability negotiation rather than the conversation event taxonomy.

## Calls remain outside durable chat events

Call invitation/session signaling remains short-lived session state under `CALLS_AND_REALTIME_MEDIA.md`. A timeline may materialize a local “missed call / call ended” notice, but a client does not need a permanent ConversationEvent to make the media security protocol correct.

## Product completeness rule

A first-party/reference V1 release is not considered product-complete if an ordinary user cannot perform the baseline product actions above simply and visibly. Conversely, a feature being visible in the reference client does not give it hidden authority or make SeedLife its required provider.

## Remaining dependencies

This document resolves **A-010 baseline_product**. It deliberately leaves exact details owned by other blocking architecture items:

- A-001/A-002: sticker, animation and portable pack formats/security;
- A-004: bot/LLM principal invocation and disclosure;
- A-009: plugin/app sandbox and capability model;
- A-011: external-network bridge fidelity and trust-state mapping.

Cross-cutting blockers A-012..A-015 must also pressure-test these semantics for export/escape, metadata leakage, decentralized abuse handling and long-term protocol evolution. Those items may extend the product, but they may not weaken the baseline semantic boundaries frozen here.
