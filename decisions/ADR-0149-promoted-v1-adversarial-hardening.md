# ADR-0149 — Adversarial hardening of ADR-0148 promoted V1 foundations

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** Run 7A normative hardening of the five ADR-0148 V1 promotions; executable/conformance evidence deferred to Run 7B
- **Amends:** ADR-0148 V1 product-base promotion
- **Amended by:** ADR-0155 (non-critical registered extensions: object/schema failures ignore the extension instead of rejecting the object; location card create-only, single-point fallback and edit suppression)
- **Does not amend:** OM-Crypto-v1, AccountControlState authority, the frozen W0 ConversationEvent body grammar, MLS/group security, IntroGate, relay authority, or provider trust semantics

## Context

Run 6B proved the first-order ADR-0148 behavior, but an adversarial review of the newly promoted surface found four places where an implementation could still be locally conforming to the prose/tests while disagreeing with another implementation or reviving stale state:

1. `om.voice-note.v1` and `om.location-card.v1` were standardized in prose/vectors but were not registered as schema-checked key-23 extensions in the canonical extension registry. The existing strict-codec path also did not prove that a registered extension appears only on its declared owning object.
2. The location fallback was required only to be non-empty; nothing mechanically tied it to the same coordinates as the extension, allowing old and new clients to show different authenticated semantic content.
3. Account-private sync did not yet freeze causal anti-resurrection rules, remote-future-clock handling, in-flight revocation behavior, or stale-backup republishing. A timestamp-only implementation could pin a scalar preference or revive deleted state.
4. Multi-profile switching specified namespace isolation but not the asynchronous time-of-check/time-of-use boundary. An operation started under profile A could complete after a switch and accidentally consult profile B as the process-global active profile.
5. Release-signature agility described required algorithms/thresholds but did not state strongly enough that acceptance is over authorized **role + key identity + algorithm + root-policy generation + exact metadata bytes**, not merely the presence/count of algorithm tags.

These are bounded semantic gaps. They do not justify a new authority layer, new crypto primitive, or a W0 body change.

## Decisions

### 1. Canonical key-23 extension registration is mandatory and object-scoped

`om.voice-note.v1` and `om.location-card.v1` are registered in [`spec/registries/om-numeric-fields.json`](../spec/registries/om-numeric-fields.json) and mirrored in [`spec/registries/om-cbor-profile.json`](../spec/registries/om-cbor-profile.json). A registered extension is one immutable tuple:

```text
(extension_id, owning_object, criticality, payload_schema, wave)
```

A conforming decoder MUST reject a registered extension when:

- it appears on a top-level object other than its declared `owning_object`;
- its `critical` bit differs from the registry entry;
- its payload violates the registered schema; or
- the same extension ID is duplicated in the extension map (ordinary canonical-map duplicate-key rejection already covers this).

Unknown non-critical extensions remain ignorable/preservable under the existing extensibility rules. Registration does not make either ADR-0148 presentation extension critical.

Because the generic frozen registry grammar intentionally uses unsigned integer field keys/types, the two not-yet-shipped ADR-0148 payloads are hardened to canonical integer-keyed nested structs rather than adding a new signed-integer schema primitive.

`om.voice-note.v1` value (`VoiceNoteV1Extension`):

```text
{
  0: attachment_index uint 0..31,
  ? 1: waveform_u8 bstr 16..256
}
```

`attachment_index` refers to the immutable `message.create` attachment list of the containing event. Later `message.edit` events cannot retarget it. If the referenced attachment is absent, not audio, or lacks `duration_ms`, the extension is unusable presentation metadata and MUST NOT reinterpret or invalidate the base encrypted attachment semantics.

`om.location-card.v1` value (`LocationCardV1Extension`):

```text
{
  0: lat_e7_offset uint 0..1800000000,   # signed lat_e7 = value - 900000000
  1: lon_e7_offset uint 0..3600000000,   # signed lon_e7 = value - 1800000000
  ? 2: accuracy_m uint 1..1000000,
  ? 3: label tstr 1..256 UTF-8 bytes
}
```

This encoding still represents exactly the ADR-0148 fixed-point ranges `[-90, +90]` and `[-180, +180]` degrees at 1e-7 degree precision. It changes only the extension-value encoding; it does not add a W0 ConversationEvent field.

### 2. Static-location fallback is deterministically bound to the extension point

The W0 RichText fallback MUST contain the exact canonical ASCII token derived from the decoded point:

```text
geo:<lat-fixed-7>,<lon-fixed-7>
```

where each coordinate is rendered from signed E7 as base-10 degrees with exactly seven fractional digits, no leading `+`, and `-` only for negative values. Examples:

```text
geo:0.0000000,0.0000000
geo:10.7202000,122.5621000
geo:-33.8688000,151.2093000
```

A new client MUST verify this token before rendering the location-card interpretation. If it is absent or differs from the extension coordinates, the extension is treated as malformed **non-critical presentation metadata** and ignored; the ordinary W0 fallback remains the rendered/authenticated message. The whole ConversationEvent is not rejected solely for this non-critical semantic mismatch.

`label` is an untrusted sender-authored annotation. It must never substitute for displaying/retaining the coordinate-bearing point, so a misleading label cannot turn the card into different machine-readable location semantics.

### 3. V1 account-private sync gets causal, revocation and restore anti-resurrection rules

The exact transport/codec remains implementation-owned behind the authenticated account-private E2EE interface, but every accepted V1 sync record MUST conceptually authenticate/bind at least:

```text
profile_id
origin_device_id
origin_acs_generation
origin_sequence
record_type + record_version
stable_item_key
operation/value-or-tombstone
causal_context
optional_hlc_for_user_ordering
```

These fields may be encoded differently by implementations, but their semantics are mandatory.

#### Per-origin sequence and causal context

Each DeviceID has a durable, never-reused monotonically increasing `origin_sequence`. Receivers retain enough per-origin dedupe/high-water state to make replay idempotent without assuming in-order transport.

Each mutation carries causal context sufficient to distinguish:

- **descends from** a previously observed mutation;
- **concurrent with** it; and
- duplicate/already-applied state.

A compact version vector, dotted version vector, or independently equivalent representation is acceptable. Provider arrival order is never merge authority.

#### Revocation and generation major-order

Before applying an inbound sync record, a receiver reconciles the best authenticated AccountControlState it has and verifies that `origin_device_id` is eligible at the record's claimed `origin_acs_generation` and is not revoked/ineligible under a newer trusted generation.

Once a receiver knows that a DeviceID is revoked/ineligible, a not-yet-accepted record from that DeviceID MUST NOT become newly effective merely because it claims to have been authored earlier. Records already durably accepted before learning revocation are non-authoritative application state; they need not be retroactively erased, but a later mutation from a currently eligible device under a higher authenticated ACS generation dominates a lower-generation scalar preference.

#### HLC is not authority and cannot pin the future

HLC may support user-visible ordering/tie-breaking but is not a security or authorization clock. A remote HLC physical component more than the frozen default skew (`300 s`) ahead of the receiver's current wall time is quarantined/not merged until it falls within the allowed window or is superseded. A receiver MUST NOT advance a durable preference floor solely from an unbounded remote future timestamp.

For ordinary scalar preferences, causal order and authenticated ACS generation are considered before any HLC/device-id tie-break. A timestamp alone never makes a stale branch causally newer.

#### Tombstones and stale concurrent writes

Deletion/reset is a typed tombstone, not absence. For the same stable item key:

- a stale value concurrent with a tombstone does not resurrect the item; **tombstone wins concurrent stale state**;
- a value may recreate/reset the item only when its causal context proves it descends from the tombstone (an intentional post-delete action); and
- compaction may summarize old payloads but must preserve enough causal/tombstone high-water information to prevent an offline branch from reviving superseded state.

#### Draft conflict and resolution binding

Concurrent draft heads remain multi-value/conflicted. A resolution record that collapses branches MUST explicitly bind the hashes/IDs of every draft head it supersedes. Compaction cannot discard a head merely because a later timestamp exists. A resolved draft may be compacted only after the surviving record proves which heads it represents.

#### Restore

Historical backup restore follows the existing fresh-DeviceID rule. Draft/preference material imported from an old backup is **local tentative state**, not an outbound sync mutation. The restored installation first reconciles current authenticated ACS and reachable account-private sync state. It MUST NOT automatically republish historical records/tombstones under the fresh DeviceID. A user may explicitly re-create local content after reconciliation, producing a fresh causal descendant/current-generation record.

This prevents backup rollback from becoming a preference/draft resurrection channel without turning account-private sync into constitutional authority.

### 4. Profile-sensitive work captures an immutable execution context

Every profile-sensitive user intent or background task captures a `ProfileExecutionContext` at dispatch. At minimum it identifies the originating `profile_id` and the profile-scoped vault/storage/session namespace required by the operation. After dispatch, an asynchronous continuation MUST NOT consult a mutable process-global `active_profile` to decide where its result belongs.

This applies to at least:

- attachment upload/download and thumbnail/media processing;
- composer drafts and send preparation;
- search/index queries;
- file/media picker callbacks;
- OS share-sheet/import intents;
- notification opens and quick replies;
- deep-link resolution;
- background account-private sync;
- clipboard-triggered actions; and
- desktop/multi-window navigation.

Switching A -> B does not rebind an in-flight A operation to B. Its result is stored/delivered only into A's context, or discarded/cancelled if A is no longer eligible/available. A UI may explicitly navigate back to A to present that result; it may not silently inject it into B.

Each desktop/PWA window or independent navigation surface is bound to an explicit profile context. A process-global selector is only a convenience for opening/switching views, never a security namespace selector for already-running work.

OS clipboard state is inherently cross-profile/global on many platforms. OM therefore treats clipboard as an explicit platform disclosure boundary: it never automatically imports clipboard contents across a profile switch, and privacy-oriented modes may use the existing timed-clear/sensitive-clipboard platform capability where available.

External intents that cannot be unambiguously mapped to a profile require explicit user choice. Notification actions are bound to the profile/conversation/notification instance that created them; a later active-profile change cannot retarget the action.

### 5. Release-signature agility is role/key/policy bound, not algorithm-counted

The ADR-0148 rule “required algorithms/thresholds” is refined as follows. A verifier accepts metadata only when the currently trusted owner-pinned root policy authorizes the complete verification tuple:

```text
(distribution_id,
 metadata_role,
 root_policy_generation/version,
 authorized_key_id,
 signature_algorithm,
 exact_canonical_metadata_bytes)
```

The policy binds each authorized key ID to its public key, algorithm and role. Thresholds count **distinct authorized key IDs satisfying that role's current policy**, not algorithm tags, duplicate signatures, or arbitrary keys that happen to use an allowed algorithm.

Consequences:

- an `ml-dsa-65` tag from an unauthorized key never satisfies an ML-DSA requirement;
- a key authorized for one metadata role cannot satisfy another role unless the trusted root policy explicitly authorizes it there;
- one physical/logical key cannot be double-counted merely because metadata carries duplicate encodings/signatures;
- changing a key's algorithm/public key under the same key ID is a policy/key mismatch and fails closed;
- signatures cover the exact canonical metadata bytes for the declared role/version; substitution between roles or metadata versions fails;
- optional signatures have no authority until the trusted policy counts their key/algorithm/role toward a threshold.

Root-policy transition follows TUF-style continuity: a new root version is accepted only through the previously trusted root threshold and must also satisfy its own new-root threshold. A long-neglected verifier walks the authenticated root-version chain sequentially (subject to the existing bounded fetch/rollback rules); it does not skip directly to an attacker-selected later root merely because the algorithms look acceptable.

This keeps PQ agility independent of OM identity while preventing algorithm-tag substitution from being mistaken for signing authority.

## Run 7B verification obligations

Run 7A changes the normative surface and intentionally reopens the affected repo-local evidence. Run 7B must provide at least:

1. **V-005 / extension registry:** strict-codec/schema evidence that both ADR-0148 presentation extensions are registered, object-scoped, exact-criticality checked and integer-key payload validated; cross-object misuse fails. Location vectors must include a mismatched-but-nonempty fallback and prove the extension is ignored while the base W0 message remains valid.
2. **V-003 / sync:** bounded traces for far-future HLC quarantine, per-origin replay/idempotence, revoked/in-flight delivery, causal tombstone-vs-stale-update delete-wins, explicit post-tombstone recreation, draft-resolution parent binding, and historical-backup non-republication under a fresh DeviceID.
3. **V-021 / profile TOCTOU:** async A->switch-to-B races for attachment/file picker/search/share/deep-link/notification/background-sync paths; every completion remains bound to A or is cancelled, never redirected into B. Include independent-window contexts.
4. **V-023 / update authorization:** mutations for unauthorized-key-with-right-algorithm, wrong-role key, duplicate-key threshold counting, same-key-id algorithm substitution, metadata-role/version substitution, skipped root generation and missing old/new threshold during root rotation.
5. rerun the full portable evidence/integrity suite and sweep current normative prose for the superseded ADR-0148 text-key extension encoding or any timestamp-only/account-switch global-state wording.

Until Run 7B closes those repo-local obligations, the consolidated different-vendor review handoff is paused and engineering remains unauthorized.

## Consequences

The V1 base remains deliberately small. This hardening adds no project-operated service, no new peer authority, no new cryptographic primitive and no W0 body field. It does make the newly promoted features much less implementation-dependent:

- registered extensions cannot drift by object/criticality/schema;
- old/new clients cannot authenticate two different location points from one event;
- private sync cannot use wall-clock/LWW shortcuts to revive stale state;
- profile switches cannot retarget asynchronous work; and
- release verifiers cannot confuse an allowed algorithm with an authorized signing key/role.
