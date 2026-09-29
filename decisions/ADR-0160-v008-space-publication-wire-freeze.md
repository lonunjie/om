# ADR-0160 — V-008 Space/Room and Publication wire freeze

- **Status:** Accepted for W3 wire compatibility; independent review remains required.
- **Date:** 2026-09-29
- **Scope:** V-008 later-wave wire criterion only.
- **Precondition:** R16B established all non-wire V-008 criteria locally (5/6).

## Decision

Freeze the W3 canonical wire forms required by V-008 without changing W0 object identity:

1. `SpaceControlState` = object type 24, signed `SignedEnvelope`, hash `OM-SPACE-CONTROL-HASH-v1`.
2. `SpaceDelegatedIntent` = object type 25, signed `SignedEnvelope`; it carries the validated Space state hash/version, Room ConversationID, registry operation key, reducing flag, evidence kind/payload, and optional target/capability context.
3. `PublicationControlState` = object type 26, signed `SignedEnvelope`, hash `OM-PUBLICATION-CONTROL-HASH-v1`.
4. `PublicationEvent` = object type 27, signed `SignedEnvelope` using signature role `publication_signing_key` (7), with per-key `seq`, optional `previous_post_hash` for non-genesis events, and the applicable PublicationControlState hash.
5. Existing ConversationEvent kinds `moderation.redact` (7), `topic.create` (11), `topic.update` (12), `topic.status.set` (13), and `message.topic.set` (14) are now typed by CDDL instead of remaining opaque maps.

The canonical CDDL is [`spec/cddl/space-publication.cddl`](../spec/cddl/space-publication.cddl); numeric object/field/enumeration assignments are in the frozen registries and are never reused.

## Important normalization

The logical schemas describe authorization evidence and Publication signatures semantically. On the wire, these are represented by the existing `SignedEnvelope` signature set rather than duplicated signature fields inside the body. This avoids two independent signature authorities for one state/event.

Publication signing keys are operational keys subordinate to `PublicationControlState`; they are not AccountID identities. Signature role 7 exists solely to make that distinction explicit and is not valid for W0 objects.

## Canonical validation rules

- `SpaceControlState.previous_state_hash` is absent only for genesis; later versions carry the prior state hash.
- A `SpaceDelegatedIntent` must reference the exact Space state hash/version against which its evidence was authorized; stale heads fail closed under the existing V-008 model.
- A `PublicationControlState` signing-key entry marked retired/revoked carries `last_valid_seq`.
- `PublicationEvent.seq=0` has no `previous_post_hash`; `seq>0` requires it. The chain hash is `DS-HASH` with `OM-PUBLICATION-EVENT-HASH-v1`.
- `moderation.redact` identifies the target EventID; its optional reason is descriptive and is never an authority source.
- `message.topic.set` without `topic_id` clears topic assignment; it does not create a new message or change authorship.
- All sets are canonical bytewise sorted/unique under OM-CBOR-v1.
- Unknown fields remain rejection/fail-closed under the frozen registry rules.

## Evidence

`verification/prototypes/v008-space-wire/` contains independently written Python and Node codecs, shared positive/negative semantic vectors, canonical-byte comparison, and re-encoding checks. The codecs agree on every vector in this checkpoint.

This is repo-local evidence, not independent review. V-008 therefore becomes 6/6 repo-local criteria but remains pre-engineering-open until the required independent review/phase evidence is satisfied.

## Non-goals

This ADR does not freeze GroupControlTransition/MLS wire encoding (V-004), Bridge wire encoding (V-013), archive format/crypto profile (V-014), call signaling, service/bridge event kinds, or W4 plugin representations.
