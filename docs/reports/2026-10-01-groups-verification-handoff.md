# Groups verification: status and handoff (V-004, V-009, V-019)

Written 2026-10-01 at the end of the first groups session. `groups` (W1) is blocked by V-004, V-009 and V-019. This session
did the parts of V-004 that need no wire decisions. Everything below the "Next" headings is a ready plan, not work done.

## Done

| Item | Evidence |
|---|---|
| V-004 formal model, MLS library check, partition convergence (criteria 0, 1, 4) | [`GroupCommit.tla`](../../verification/tla/GroupCommit.tla), [`models-v004.json`](../../verification/tla/models-v004.json), [`v004-mls-ordering`](../../verification/prototypes/v004-mls-ordering/README.md), [ADR-0173](../../decisions/ADR-0173-group-commit-ordering-fork-detection-and-recovery.md) |
| Design gaps found and decided | F-24 no same-parent ordering rule; F-25 a merged MLS epoch cannot be rolled back, so a partition fork is unavoidable; F-26 a removal on a losing branch is not a removal. Rules in ADR-0173, R-393..R-396, T-158..T-160. |
| V-004 now 6/8 pre-engineering criteria | `python3 tools/om_gate.py show V-004` |

The model needed four fixes before it passed, and each is a real rule: a removal merged on a losing branch must be
recoverable; `forked` members must not rejoin through a chain that is itself not canonical; a bounded model must not
let its own bound hide the scenario (the intent commits are unbounded, the extra commits bounded); and the canonical-chain
depth must exceed the longest chain. The main config takes about 4.5 minutes, so `make tlc` got slower by that much.

## Next: V-004 wire freeze and GroupHistoryBundle (criteria 6 and 7)

**Method (V-005's, stronger than V-008's hand-coded validators).** Both strict codecs are registry-driven, so adding the
group structs to the registry makes them validate the group vectors with no per-type code:
[`verification/prototypes/v005-encoding/rust`](../../verification/prototypes/v005-encoding/rust/src/schema.rs) and
[`verification/cbor/node_strict_harness.mjs`](../../verification/cbor/node_strict_harness.mjs). Later waves use an **overlay** registry
(`OM_REG`) that extends the ConversationEvent dispatch table (kinds 6, 8, 9, 10 and the already-typed 7, 11-14), so the frozen W0
corpus is untouched. The Node harness already reads `OM_REG`; the Rust codec embeds the registries with `include_str!` and needs a
runtime override added ([`verification/bounded/cross_wave_feasibility.py`](../../verification/bounded/cross_wave_feasibility.py) builds such an overlay and is the pattern). Then:

1. Write [`spec/cddl/group.cddl`](../../spec/cddl/group.cddl); add structs to [`om-numeric-fields.json`](../../spec/registries/om-numeric-fields.json), types 28-31 to [`om-object-types.json`](../../spec/registries/om-object-types.json), labels to
   [`om-domain-separation.json`](../../spec/registries/om-domain-separation.json). Keep the three registry files byte-stable apart from additions (`json.dumps(indent=2,
   ensure_ascii=False)` round-trips them). New enums need `enum_value_waves` entries.
2. Generate vectors by importing [`gen_vectors.py`](../../verification/prototypes/v005-encoding/gen/gen_vectors.py) as a module and patching its `TYPES`
   (it filters to W0); its `emit_positive`, `body_mutations` and raw emitter work for any type.
3. Run the Rust codec, the Node harness, `gen/compare.py` and `gen/cddl_check.py` (Rust `cddl` 0.10.7 and Ruby `cddl` 0.12.14, installed in
   `v005-encoding/.tools`).
4. For criterion 7 add a semantic bundle-admission check in two languages and vectors that prove a bundle's claimed EventIDs never
   reach live dedupe or any reducer ([`protocol_semantics_adr0161.py`](../../verification/bounded/protocol_semantics_adr0161.py) is the existing model).

**Design already worked out (decide, then freeze; keys are permanent once frozen).**

- Types: 28 `GroupControlState` (signed, role device, hash label), 29 `GroupControlIntent` (signed), 30 `GroupCommitBinding` (signed, hash
  label, binding hash is the ADR-0173 tie-break), 31 `GroupHistoryBundle` (signed by the author device). Labels follow
  `OM-GROUP-...-SIG-v1` / `-HASH-v1`, plus `OM-GROUP-MLS-COMMIT-HASH-v1` for the MLS Commit bytes.
- `GroupControlState`: conversation_id, state_version, previous_state_hash, profile {name, avatar_ref, description}, membership[]
  (principal_ref, **membership_incarnation** (R-279), role_ids, endpoint_policy), role_definitions (reuse `RoleDefinition`),
  governance {principals, threshold, high-risk operations}, policy {join_rule, new_device_policy, history_policy, history window,
  disappearing_timer, security_profile id, sequencing_timeout (30..3600 s, ADR-0173), past_epoch_window (0..8)},
  coordinator_succession[], invites[], mls_epoch_ref, transition_binding_hash. Reserve the Space-binding key for W3. Container limit is
  4096 items ([`om-cbor-profile.json`](../../spec/registries/om-cbor-profile.json)), so membership is capped at 2048 and a bundle at 4096 items.
- `GroupCommitBinding`: conversation_id, **binding_kind** (1 coordinator-sequenced, 2 direct reduction, 3 recovery), **reducing**,
  previous state hash/version, previous and resulting MLS epoch, optional intent_hash (absent for recovery), mls_commit_hash,
  resulting state hash (equal to the previous one for recovery), committer device, optional readded devices (recovery).
- `GroupControlIntent`: conversation_id, previous state hash/version, intent_id (exact-retry identity, ADR-0163), operation
  (enum), reducing, args dispatched on operation. W1 operations: member.admit, member.remove, member.leave, device.add,
  device.remove_revoked, profile.set, disappearing_timer.set, role.define, role.assign, invite_link.issue/revoke/**redeem**,
  **admission.set**, security_policy.set, governance.set, coordinator.replace. Reductions carry the target incarnation.
  **Registry gap found:** [`om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) has no operation for redeeming an invite link
  or for setting the join policy (`room.manage_admission`), although ADR-0128 rule 6 needs both. Add them in the freeze ADR.
- Bodies: `PinSet {target_event_id, present}`, `PollCreate {question RichText, options[2..32] of {option_id, label}, selection_mode,
  max_selections?, results_visibility}`, `PollVote {target_poll_event_id, selected_option_ids set 0..32}` (empty = retract),
  `PollClose {target_poll_event_id}`. Types them in [`events.cddl`](../../spec/cddl/events.cddl) the way ADR-0160 typed the topic kinds.
- `GroupHistoryBundle`: bundle_id, conversation_id, author account and device, recipient account and **membership incarnation**,
  group_control_state_hash, mls_epoch_ref, creation_nonce, items[] of {history_item_id, optional claimed original event id, author
  and time (inert), presentation snapshot of text and attachments only, replies by bundle-local id}.
- Write the freeze as **ADR-0174**; it should also note that this wire needs a `binding_kind`/`reducing` pair for ADR-0173.

Expect this to be the largest remaining item. Budget it as its own session.

## Next: V-009 (second materializer)

[`v009-materializer`](../../verification/prototypes/v009-materializer/README.md) already has an independent Rust materializer that agrees with the Python
reference on 115/115 cases. Four criteria remain, all about independence and coverage:

1. Authorization is still supplied by a shared adapter in `compare.py` (criterion G-V9-5). Derive it inside the Rust materializer from the
   control chain (grants, restrictions, roles per epoch) and compare, so the two sides stop sharing authorization semantics.
2. The Python reference covers pins, redaction, polls and topics only. Extend it (or add a third check) for edits, deletes, reactions,
   receipts, replies.
3. Add corpus cases for R-236 (reaction and vote registers are authorization-epoch-major; a vote after `poll.close` is ignored), R-235
   (expiry deletes at `min(expiry, receipt + expires_after)`, never after the absolute bound) and R-239 (forwarding strips mentions; a
   non-member mention renders inert).
4. Then record the criteria with [`tools/om_gate.py`](../../tools/om_gate.py). The independence field needs an `independent_implementation`; the second
   materializer is a different language but the same author, so say so.

## Next: V-019 (hybrid-PQ ReInit)

OpenMLS 0.9.0 does **not** implement RFC 9420 ReInit (its own source says "`ReInit` is not yet implemented", issue 751). It does
have `MlsGroup::reboot` (new group, same members, behind `fork-resolution`) and a `draft-ietf-mls-pq-ciphersuites` feature. So the
prototype is an OM-level authenticated migration, not a library ReInit:

1. Create a v1 group, then `reboot` it into a hybrid-PQ ciphersuite group whose MLS group id differs and whose `ConversationID` and
   `GroupControlState` do not (the logical `GroupControlTransition` carries both).
2. Vectors: a downgrade (hybrid to classical) is rejected by the monotone `security_generation` floor (ADR-0165); an unauthorized
   reboot (author lacks authority) is rejected; the ConversationID is unchanged; the old group's tail messages stay decryptable inside
   `past_epoch_window`.
3. Check the PQ feature builds on native and wasm32-unknown-unknown the way `v004-mls-ordering/run.sh` does.
4. Record it with [`om_gate.py`](../../tools/om_gate.py); the gate wants an `independent_reviewer` (ADR-0167 single-vendor review).

## After all three

Write the groups review (`verification/reviews/`), pass the three gates under ADR-0167 and propose the `groups` authorization ADR.
Pre-release items stay open: V-004 multi-client partition harness on two implementations, V-009's every-implementation corpus, V-019
interoperable migration once ADR-0107 triggers are met.
