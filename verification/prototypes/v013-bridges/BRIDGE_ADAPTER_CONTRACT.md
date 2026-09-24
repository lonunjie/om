# OM Bridge Adapter Contract (candidate for V-013 review)

- **Status:** candidate specification for gate V-013 ("reviewed bridge adapter contract specification"). It is not
  reviewed. It needs an independent reviewer and an ADR before it is normative.
- **Author:** written by a Claude agent. The design package was authored by a ChatGPT design agent and Claude agents;
  ADR-0117 (proposed) decides whether an AI reviewer counts as independent.
- **Derived from:** `INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md` (§§2–19 and the 08:21 refinement),
  `TRANSPORT_ADAPTERS_AND_INTEROP.md`, `METADATA_THREAT_MODEL.md` (scoped fingerprints),
  `SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md` (`room.add_bridge` is a high-risk capability).
- **Executable model:** `bridgesim/` in this directory. Each rule ID below is a comment at its enforcement point, and
  each MUST rule marked † has a fault-injection switch that the model shows to be necessary.

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119 / RFC 8174.

## 1. Roles and objects

- **Bridge.** A typed `BridgePrincipal` (`BridgeID`, `BridgeControlState`). It runs one or more **adapters**. Each
  adapter connects exactly one foreign network namespace (`foreign_network_namespace`, adapter-defined and opaque to OM).
- **Binding.** The Room's authorization of a bridge. It is a Room control transition gated by `room.add_bridge` and by
  quorum where the Room's policy requires it. It records `bridge_id`, the external network(s), `direction`,
  `mapped_foreign_scope`, `allowed_event_classes`, the fidelity summary, approval evidence and revocation state
  (source §6). A binding has membership intervals `[admitted_epoch, removed_epoch)`.
- **Mapping.** A `BridgeMapping` between one OM scope and one foreign scope (source §9). Mapping state includes the
  **target map**, a bijection between OM EventIDs and foreign event refs that this bridge created or imported.
- **Foreign-side bridges.** A foreign-side bridge is a relay between two foreign networks that OM does not control. It
  is part of the environment, not of this contract. The contract assumes it may preserve interop metadata or strip it.

## 2. Adapter interface (logical)

An adapter implements:

- `capabilities() -> InteropCapabilities` (source §10), including `carries_interop_metadata: bool`.
- `observe() -> ForeignEvent*` with `{namespace, conversation_ref, event_ref, actor_ref, actor_display, kind, target_ref?, content, interop_metadata?}`.
- `emit(ForeignRendering) -> event_ref`.
- `verify_identity_link(namespace, actor_ref) -> ForeignIdentityLink?`.

Adapters are untrusted parsers of foreign input. They MUST apply the bounded-parsing rules of
`RICH_TEXT_AND_CONTENT_SECURITY.md` before producing OM content.

## 3. Security-state rules

- **BAC-SEC-1†.** A bridge MUST become a plaintext recipient only through one Room control transition. That
  transition both admits it (a new epoch) and publishes the Room security state. The security state shows: that
  bridging is active; the external network and scope; the responsible BridgePrincipal; the direction; the exported
  event classes; and **that OM E2EE ends at the bridge for exported content** (source §8). No other path may make a
  bridge a recipient: not a plugin, not a service, not the export path of a Space binding.
- **BAC-SEC-2.** The Room security state MUST NOT show a single end-to-end indicator that spans OM and the foreign leg.
  A foreign network's own E2EE claim, if any, is shown separately as that network's claim (source §15).
- **BAC-SEC-3.** Removing a bridge is a Room control transition that advances the epoch and removes the bridge's
  endpoints (source §6). It MUST NOT be described as deleting content the foreign network already received.

## 4. Outbound rules (OM → foreign)

For each OM event `e` that the bridge can decrypt:

- **BAC-OUT-1†.** Export `e` only if `e`'s epoch lies in one of this bridge's membership intervals *and*
  `allowed_event_classes` and `direction` permit it. Events from before admission are never exported
  retroactively. Events from after removal are never exported, even if they were already in flight.
- **BAC-OUT-2.** Never export an event that this bridge itself imported.
- **BAC-OUT-3†.** By default, never export an event authored by *another* BridgePrincipal. A mapping MAY opt in with
  `relay_bridged: true`. It then MUST carry the full `InteropTrace` and MUST be refused if any foreign leg of that
  mapping has `carries_interop_metadata = false`. *Rationale:* with a metadata-stripping foreign relay, relaying
  bridged events produces an unbounded echo. This is shown by `mut_relay_bridged_strip`.
- **BAC-OUT-4.** Every emitted foreign event carries an `InteropTrace` where the network supports it
  (`carries_interop_metadata`). The trace is: `origin` (the OM origin, or the origin inherited from the event's
  ForeignProvenance), `traversed_bridge_ids` plus this bridge's ID, `hop_count + 1`, and `hop_limit`. Raw OM
  identifiers MUST NOT appear in it. `origin` is expressed as the scoped fingerprint of BAC-FP-1.
- **BAC-OUT-5.** Record `(om_event_id ↔ foreign_event_ref)` in the target map. Record the foreign ref in the **echo
  table**.

## 5. Inbound rules (foreign → OM)

For each observed foreign event `f`:

- **BAC-IN-0.** If the binding is not currently active (removed, or not yet admitted), import nothing.
- **BAC-IN-1†.** If `f.interop_metadata.traversed` contains this BridgeID, drop it (own reflection).
- **BAC-IN-2†.** If `f.event_ref` is in this bridge's echo table, drop it (own echo). This rule does not depend on
  the foreign network preserving metadata.
- **BAC-IN-3†.** Compute `fp` (BAC-FP-1) from `f.interop_metadata.origin` if present, otherwise from
  `(namespace, event_ref)`. If an event with `fp` is already materialized in this Room, drop `f` as a duplicate
  (source §§13–14).
- **BAC-IN-4.** If `hop_count ≥ hop_limit`, drop it.
- **BAC-IN-5.** Otherwise create a Room event **authored by the BridgePrincipal** (BAC-ID-1) whose
  `ForeignProvenance` has:
  - `foreign_network_namespace`, `foreign_conversation_ref`, `foreign_event_ref` and `foreign_actor_ref`, exactly as
    observed;
  - `foreign_actor_display_hint`, marked as bridge-attested;
  - `bridge_observed_at`, a wall-clock hint that is never ordering authority;
  - `fidelity_flags` (BAC-FID);
  - `origin_fingerprint = fp`;
  - `interop_trace`, extended with this bridge.

  The bridge then records the target map entry.
- **BAC-IN-6.** A drop MUST be recorded in the bridge's diagnostic log with its reason. It MUST NOT be silent in the
  Room if it removes a user-visible action. For example, an unmappable reaction produces a visible fidelity notice.

## 6. Fingerprints

- **BAC-FP-1†.** `origin_fingerprint = H("OM-INTEROP-FP-v1" || 0x00 || room_scope || origin_namespace ||
  origin_event_ref)`, where `room_scope` is a Room-local secret or identifier known only to Room members and bridges.
  The fingerprint is scoped to the **Room and origin**. It MUST NOT include the BridgeID. *Rationale:* a bridge-scoped
  fingerprint cannot deduplicate one foreign event that arrives through two bridges (`mut_fp_scope_bridge`).
  Room-level scoping still prevents cross-Room correlation, which is the concern in `METADATA_THREAT_MODEL.md`.
- **BAC-FP-2.** Deduplication across bridges uses only equal fingerprints, never text, display name or timestamp
  similarity (source §14). If two copies cannot be linked by fingerprint, both are shown, and the later copy is
  flagged `origin_unverified`.

## 7. Identity rules

- **BAC-ID-1†.** An imported event is always authored by the BridgePrincipal. A foreign display name that equals an
  OM account's name MUST NOT change the author (`mut_trust_display`).
- **BAC-ID-2.** The `linked` presentation ("Alice on X — linked to Alice's OM identity") requires a valid,
  unexpired `ForeignIdentityLink` authorized by that AccountPrincipal for exactly `(namespace, actor_ref)`. Otherwise
  the event carries `foreign_actor_unverified`. A link never changes authorship (source §5).

## 8. Target mapping and fidelity

- **BAC-MAP-1.** Replies, edits, reactions, topic moves and poll votes are translated only through the target map.
- **BAC-MAP-2†.**
  - **Outbound:** if the target has no mapping, drop the action and report `reply_target_unavailable`. Never attach it
    to a different foreign event (`mut_attach_unmapped`).
  - **Inbound:** if the target has no mapping, import the event as a new message flagged
    `reply_target_unavailable`.
- **BAC-FID-1.** Each emitted or imported event carries `fidelity_flags`, computed from the target network's
  `InteropCapabilities`: `exact`, `lossy_formatting`, `edit_represented_as_new_message`, `delete_not_propagated`,
  `reaction_not_supported`, `attachment_transcoded`, `reply_target_unavailable`, `foreign_actor_unverified` and
  `origin_unverified` (the last one is new; see §10).
- **BAC-FID-2.** An edit to a network without edits becomes a new message flagged
  `edit_represented_as_new_message`. It never becomes a silent overwrite.
- **BAC-FID-3.** A reaction to a network without reactions is dropped and reported. It is never approximated as text
  that looks like a message from the reactor.
- **BAC-FID-4.** Polls are mirrored only when voter-privacy semantics match. Otherwise the bridge sends a read-only
  summary (source §12).
- **BAC-FID-5.** Deletes propagate only as requests. The Room UI never claims erasure (source §11).

## 9. Conformance (what the model checks)

| Property | Checked where | Holds under this contract |
|---|---|---|
| **Bounded**: every execution reaches quiescence with ≤ 24 events | every state | yes, including with a metadata-stripping foreign relay |
| **TypedAuthor**: imported content is never AccountPrincipal-authored | every state | yes |
| **ForeignProvenance**: namespace, event and actor refs and display hint equal the observed source | every state | yes |
| **IdentityLink**: `linked` iff there is a verified link | every state | yes |
| **E2EEBoundaryDisclosed**: every export comes from an epoch in which the bridge was an admitted, disclosed recipient | every state | yes |
| **Fidelity**: flags equal an independent oracle, and no unsupported semantics are emitted | every state | yes |
| **TargetMapping**: actions attach only to the mapped copy of their target | every state | yes |
| **NoReflection**: each origin materializes once; extra copies only when a foreign relay strips metadata, and then flagged | quiescence | yes, with the residual in §10 |

## 10. Residual risks and proposed design updates

1. **A metadata-stripping foreign relay defeats reflection suppression.** If some foreign-side bridge between two
   mapped foreign scopes drops OM interop metadata and gives no stable origin reference, the OM Room may show a bounded
   number of reflected copies (`foreign_side_bridge_strips_trace`, `foreign_origin_strip`). The copies are
   bridge-authored and flagged `origin_unverified`. The design says reflection is suppressed "based on provenance"; it
   should add that suppression requires provenance to survive the foreign path. *Proposal:* add the
   `origin_unverified` fidelity flag, and let Room policy refuse mappings whose foreign scope is also bridged by a
   relay that does not carry metadata.
2. **Fingerprint scope contradiction.** The 08:21 refinement says fingerprints are "mapping/bridge-domain scoped", but
   §14 needs cross-bridge dedup. The model shows these are incompatible. *Proposal:* scope to (Room, origin)
   (BAC-FP-1).
3. **Relaying bridged content** needs an explicit opt-in (BAC-OUT-3). The source does not forbid bridge-to-bridge
   relay through OM.
4. **Retroactive export.** The source says bridge admission is a visible boundary but does not forbid exporting
   history from before admission. *Proposal:* BAC-OUT-1. History sharing to a bridge would need the separate
   HistoryBundle consent path.
5. **Membership intervals.** A re-admitted bridge must not gain export rights over epochs from while it was removed.
   The model needed interval history, not a single admission epoch.

Not covered by the model, and still open for V-013: real adapters and traces (pre-release); metadata leakage
measurement for fingerprints; identity-link proof methods for each network; poll privacy mapping.
