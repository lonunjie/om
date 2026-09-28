# ADR-0155 — Non-critical extension validity independence and presentation-extension edit closure

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** independent architecture review (Claude Opus 5.5) of drop `20260928-2200`, findings F-02 and F-03; no W0 body change and no engineering authorization
- **Amends:** ADR-0149 §1–§2, ADR-0150 §2, ADR-0151 §1
- **Does not amend:** registered **critical** extensions (`om.acs.fork_ack`, `om.acs.legacy_policy`), canonical/deterministic CBOR rejection, the 64 KiB event ceiling, or any valid encoding of the three V1 presentation extensions

## Context

**F-02 — registry version decides event validity.** ADR-0151 layer 1 rejects the whole ConversationEvent when a *known registered non-critical* extension violates its registered schema or appears on the wrong object. A client that predates the registration treats the same ID as unknown non-critical, ignores it, and accepts the event. The three V1 extensions are registered in W0, so no conforming client disagrees about them today. But the rule applies to every future non-critical registration. From the first such registration, a sender, buggy or hostile, can craft an event that pre-registration clients show and post-registration clients reject. It can even differ between two devices of the same account running different versions. That violates the event model's Gate-B exit criterion: two clients given the same authenticated events converge on the same state, apart from explicitly local presentation choices. Clients already shipped with the reject rule can never be changed retroactively, so the rule has to be settled before W0 engineering.

Resource ceilings do not need the reject rule. Canonical CBOR decoding, the 64 KiB event ceiling and the registered schema's bounds are all enforced before semantic rendering either way. A schema-invalid non-critical payload can simply be discarded.

**F-03 — presentation extensions across edits were unspecified.** `om.location-card.v1` binds its point to a `geo:` token in the *creating* event's RichText, and `om.rich-list.v1` annotates block indices of the event that carries it. Nothing said what happens after an accepted `message.edit` replaces the RichText. One implementation could keep showing the original card after the author edited the token away, while old clients show the edited text. Another could apply a create's list annotations to an edit's unrelated blocks. The fallback also had only to *contain* the canonical token. A fallback carrying two different canonical points would render one card on new clients, while old clients show both points.

## Decisions

### 1. A non-critical-registered extension never decides containing-object validity

For an extension ID whose registry entry says `critical: false`:

1. If the wire `critical` bit is **true**, the containing object is rejected (`criticality_mismatch`). A pre-registration client rejects it too, because it sees an unknown critical extension, so the verdict does not depend on registry version.
2. If the wire `critical` bit is **false**, the containing object's validity is decided **only** by its base grammar and its critical extensions. A wrong owning object, a non-map payload, an unknown or missing field, or any registered numeric, cardinality or enum bound violation makes the extension **unusable**. It is ignored for semantics and its authenticated bytes are preserved wherever the object's forwarding/storage rules preserve unknown non-critical data. The diagnostic class (`wrong_extension_object`, `unknown_field`, `depth_schema`, …) stays as it was; only the verdict changes from `reject_object` to `ignore_extension`.
3. Non-canonical CBOR anywhere in the object, including inside an extension value, still rejects the object under OM-CBOR-v1 for every client.

Extension IDs registered `critical: true` keep the ADR-0141/0149 behavior: any owner, criticality or schema mismatch rejects the containing object. A future critical registration that a sender misuses with the bit cleared is seen as unknown non-critical by pre-registration clients. That is inherent in sender-asserted criticality and is recorded as a residual, not a rule.

ADR-0151 layer 1 therefore becomes "registered schema layer: *ignore the enhancement*", and ADR-0151 layer 2 is unchanged.

### 2. Presentation extensions bind to the RichText they were validated against

1. `om.voice-note.v1` and `om.location-card.v1` are meaningful only on `message.create`. On any other event kind they are unusable (`event_kind`) and ignored.
2. `om.rich-list.v1` annotates only the RichText of the event that carries it: `content` for a create, `replacement` for an edit. Annotations from a create or an earlier edit **never** apply to a later edit's replacement. An edit without its own rich-list extension renders as plain W0 RichText.
3. A location card from a `message.create` is rendered only while the conversation's **currently materialized** RichText for that message (the create's content, or the winning edit's replacement) passes the ADR-0149 token check. If an edit removes or changes the token, the card is suppressed and the edited text is rendered. The card is never re-derived from the edit.
4. Voice-note attachment binding is unchanged: attachments are immutable and an edit cannot retarget them (ADR-0149).

### 3. The location fallback must name exactly one point

The ADR-0149 token check is tightened. The fallback must contain the canonical token for the extension point, and it must contain **no other** substring that parses as a canonical `geo:<lat-fixed-7>,<lon-fixed-7>` token for a different point (same ASCII boundary rules as the required token). Repeating the same token is allowed. A second distinct canonical point makes the extension unusable (`fallback_ambiguous`), and the W0 text renders unchanged. Non-canonical `geo:` URIs (other precisions or parameters) are ordinary text and do not count.

## Evidence

V-005 gains one repo-local criterion. The ADR-0148/0149 extension corpus and the ADR-0150 rich-list corpus are re-expressed under §1 so that owner, schema and bound failures are `ignore_extension` and wire-criticality mismatches stay `reject_object`. New vectors cover conflicting location tokens, repeated identical tokens, voice/location on an edit event, a location card after an edit removes its token, and create-time list annotations evaluated against edit text. The Python and Node implementations must agree on every vector.

## Consequences

Event validity for non-critical data no longer depends on which registry version a client ships, so old and new clients, and a user's own devices on different versions, keep materializing the same conversation. Malformed presentation metadata degrades to the W0 text every client already renders. Edits cannot leave a stale card or list interpretation that disagrees with what older clients show. No wire field, registry ID, criticality value or valid encoding changes. This ADR is Anthropic-authored, so ADR-0117 requires a non-Anthropic reviewer.
