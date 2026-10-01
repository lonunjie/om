# ADR-0176 — V-004 groups wire freeze: GroupControlState, GroupControlIntent, GroupCommitBinding, GroupHistoryBundle and typed pin/poll bodies (W1)

- **Status:** Accepted for W1 wire compatibility; the operator-model review (ADR-0167) is recorded with the groups review.
- **Date:** 2026-10-01
- **Decided by:** design agent (Claude, sole design agent, ADR-0167)
- **Scope:** V-004 criteria 6 (wire freeze) and 7 (GroupHistoryBundle freeze) only.
- **Source:** [`verification/prototypes/v004-group-wire/`](../verification/prototypes/v004-group-wire/README.md),
  [`verification/results/v004-group-wire-latest.json`](../verification/results/v004-group-wire-latest.json)
- **Amends:** ADR-0156 item 3 (the wire freeze it required), ADR-0161 §3 (bundle wire), ADR-0173 (fixes the `binding_kind`/`reducing` pair),
  [`GROUP_CHAT_ARCHITECTURE.md`](../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md)
- **Does not decide:** the MLS Commit bytes (RFC 9420), the recipient delivery of a bundle beyond "an encrypted blob over the
  recipient's existing 1:1 channel", Space/Publication wire (ADR-0160), service/bridge membership (W4), or the OM-level
  hybrid-PQ migration transition (ADR-0175).

## Decision

1. **Four signed objects**, each a `SignedEnvelope` whose signature role is `device`, with frozen CDDL in
   [`spec/cddl/group.cddl`](../spec/cddl/group.cddl), numeric fields in [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json),
   types in [`om-object-types.json`](../spec/registries/om-object-types.json) and labels in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json):

   | Type | Object | Signature label | Hash label |
   |---:|---|---|---|
   | 28 | `GroupControlState` | `OM-GROUP-CONTROL-SIG-v1` | `OM-GROUP-CONTROL-HASH-v1` (state hash, `previous_state_hash` chain) |
   | 29 | `GroupControlIntent` | `OM-GROUP-INTENT-SIG-v1` | `OM-GROUP-INTENT-HASH-v1` (`GroupCommitBinding.intent_hash`) |
   | 30 | `GroupCommitBinding` | `OM-GROUP-COMMIT-BINDING-SIG-v1` | `OM-GROUP-COMMIT-BINDING-HASH-v1` (the ADR-0173 tie-break) |
   | 31 | `GroupHistoryBundle` | `OM-GROUP-HISTORY-BUNDLE-SIG-v1` | none |

   `OM-GROUP-MLS-COMMIT-HASH-v1` is the hash label for the exact MLS Commit bytes behind `GroupCommitBinding.mls_commit_hash`.
   All keys and enum values are permanent once frozen (registry rule). The three registry files and [`om-room-capabilities.json`](../spec/registries/om-room-capabilities.json) changed
   by **addition only** against the pre-freeze commit ([`additive.py`](../verification/prototypes/v004-group-wire/additive.py) checks it).
2. **GroupControlState** carries: `conversation_id`, `state_version`, optional `previous_state_hash` (absent only at genesis), `profile`,
   `membership` (a set of `{principal_ref, membership_incarnation, role_ids, endpoint_policy}`), `role_definitions` (the registry `RoleDefinition`),
   `governance {principals, approval_threshold, protected_operations}`, `policy {join_rule, new_device_policy, history_policy,
   history_window_seconds, disappearing_timer, security_profile, sequencing_timeout 30..3600, past_epoch_window 0..8}`,
   `coordinator_succession` (an ordered array of device ids), `invites` (a set of `{join_ref, issuer, issuer_incarnation,
   authorization_basis, expiry, max_uses, uses}`) and `mls_epoch_ref` (the MLS epoch of the Commit that installed this version). The
   service policy (W4) and Space authority binding (W3) keys are reserved. Authorization evidence is **not** duplicated in the state:
   it is the signature set of the envelope plus the `GroupCommitBinding`/`GroupControlIntent` chain (the ADR-0160 normalization).
3. **GroupControlIntent** carries `conversation_id`, the parent `previous_state_hash` and `previous_state_version`, `intent_id` (the exact-retry
   identity of ADR-0163), `operation`, `reducing`, `args` (a struct dispatched on `operation`) and `authorization`
   `{kind, acs_head_hash?, space_intent?, join_ref?}`. The sixteen W1 operations are the registry keys `control.<name>`: `member.admit`,
   `member.remove`, `member.leave`, `device.add`, `device.remove_revoked`, `profile.set`, `disappearing_timer.set`, `role.define`, `role.assign`,
   `invite_link.issue`, `invite_link.revoke`, `invite_link.redeem`, `admission.set`, `security_policy.set`, `governance.set`,
   `coordinator.replace`. Reductions carry the immutable target incarnation (R-279).
4. **GroupCommitBinding** carries `conversation_id`, `binding_kind` (1 coordinator-sequenced, 2 direct reduction, 3 recovery), `reducing`, the
   previous state hash/version and MLS epoch, `resulting_mls_epoch`, `intent_hash` (absent only for recovery), `mls_commit_hash`,
   `resulting_state_hash`, `committer_device` and, for recovery, the set `readded_devices`. This is the `binding_kind`/`reducing` pair ADR-0173 needs.
5. **GroupHistoryBundle** carries `bundle_id`, `conversation_id`, `author_account`, `author_device`, `recipient_account`,
   `recipient_membership_incarnation`, `group_control_state_hash`, `mls_epoch_ref`, `creation_nonce` and an ordered array of 1..1024 items
   `{history_item_id, claimed_event_id?, claimed_author?, claimed_time?, text?, attachments?, reply_to_item?}`. An item holds a presentation snapshot
   of text and attachments only; it has no field that can express a state operation. It travels as the plaintext of an encrypted blob that the
   author sends to the recipient over their existing 1:1 channel (an `AttachmentRef`), so it needs no new encryption primitive.
6. **Typed bodies for kinds 6, 8, 9, 10**: `PinSet {target_event_id, present}`, `PollCreate {question RichText, options 2..32 of {option_id 0..255,
   label}, selection_mode, max_selections?, results_visibility}`, `PollVote {target_poll_event_id, selected_option_ids set 0..32}` (empty retracts)
   and `PollClose {target_poll_event_id}`. The ConversationEvent dispatch table is extended **by overlay**: a W1 decoder merges
   [`om-w1-event-bodies.json`](../spec/registries/om-w1-event-bodies.json) (kinds 6-14), a W0 decoder ignores it and keeps these bodies opaque, so the frozen
   W0 corpus and dispatch table are untouched (ADR-0165, T-157). CDDL: `ConversationEventW1` in [`group.cddl`](../spec/cddl/group.cddl).
7. **Registry gap closed.** [`om-room-capabilities.json`](../spec/registries/om-room-capabilities.json) had no operation for redeeming an invite link or setting the join policy, although ADR-0128 rule 6
   needs both. Appended: `control.invite_link.redeem` (basis `invite`, a new basis defined in the registry rules) and `control.admission.set`
   (`room.manage_admission`). Under `admin_approval` a link join request is admitted by `member.admit` carrying the `join_ref`; `invite_link.redeem`
   is valid only when `join_rule` is `open`.

## 3. Cross-field rules (normative; registry and CDDL cannot express them)

A receiver rejects an object that breaks a rule. The first broken rule in the listed order is the verdict. Two independently written checkers
(Python and Node) implement exactly these rules.

**GroupControlState.** (1) `state_version` is 1 exactly when `previous_state_hash` is absent. (2) Membership principals are unique. (3) Membership
incarnations are unique. (4) Role ids in `role_definitions` are unique. (5) Every membership `role_id` is defined. (6) `endpoint_policy` is
`account_devices` exactly for account principals. (7) `coordinator_succession` has no repeated device. (8) `history_window_seconds` is present exactly when
`history_policy` is `recent_window`. (9) Invite `join_ref`s are unique. (10) Each invite has `uses` at most `max_uses`, and its issuer is a member
whose current incarnation equals `issuer_incarnation` (a removed or re-added issuer ends its links, ADR-0128 rule 6). (11) Governance principals are
members. (12) `approval_threshold` does not exceed the number of governance principals.

**GroupControlIntent.** (1) `reducing` is true for `member.remove`, `member.leave`, `device.remove_revoked`, `invite_link.revoke` and
`coordinator.replace`, and false for `member.admit`, `device.add`, `profile.set`, `disappearing_timer.set`, `invite_link.issue` and `invite_link.redeem`;
for `role.define`, `role.assign`, `admission.set`, `security_policy.set` and `governance.set` the flag is a claim the receiver recomputes against the parent
state. (2) The authorization kind is allowed for the operation: capability (1) or Space delegation (2) for `member.admit`, `member.remove`,
`role.define`, `role.assign`; membership/authorship basis (3) for `member.leave` and `coordinator.replace`; capability or ACS evidence (1, 4) for
`device.add`; ACS evidence (4) for `device.remove_revoked`; capability (1) or basis (3) for `invite_link.revoke`; invite (5) for `invite_link.redeem`;
capability (1) for the rest. (3) `member.admit` carrying a `join_ref` needs kind 1. (4) `acs_head_hash` is present exactly for kind 4 and for the two
device operations, `space_intent` exactly for kind 2, `join_ref` exactly for kind 5. (5) For `invite_link.redeem` the authorization `join_ref` equals
`args.join_ref`. (6) `security_policy.set` sets at least one field, and carries a window only with `recent_window`. (7) `governance.set` sets at least one
field, and its succession has no repeated device. (8) `coordinator.replace` names two different devices. (9) `invite_link.issue` carries `uses = 0`.

**GroupCommitBinding.** (1) A recovery binding has no `intent_hash`, keeps the state hash, and is not reducing. (2) Any other binding has an `intent_hash`
and changes the state hash; a direct binding is reducing; `readded_devices` appears only on recovery. (3) `resulting_mls_epoch` is
`previous_mls_epoch + 1`.

**GroupHistoryBundle structure.** The author and recipient are account principals and differ; `history_item_id`s are unique; each item has text or
attachments; `reply_to_item` names an earlier item of the same bundle (never a live EventID).

**GroupHistoryBundle admission** (R-363). A receiver applies these gates in order: the envelope signer is `author_device`; `conversation_id` is the Room;
`recipient_account` is the local account; `recipient_membership_incarnation` is the local current incarnation; `group_control_state_hash` is a known
canonical state; `mls_epoch_ref` is the epoch that state was installed at; the author and the recipient (with that incarnation) are members of the bound
state; the author and the recipient still hold the same incarnation in the latest canonical state; the author device is eligible under the author's current
trusted ACS; the bundle structure holds; and the bundle is not a duplicate by `bundle_id` or `creation_nonce` (a duplicate is a no-op). An accepted
bundle writes its items into the archive namespace `(bundle, bundle_id, history_item_id)` with the label "shared by <author>". It never writes the live
ConversationEvent dedupe set or any Room/control reducer, and a claimed EventID never suppresses a live event with that EventID.

**Producer rules (R-347; not machine-checkable on the wire).** A bundle author excludes expired, deleted and redacted content, folds edits into the
item text, strips mentions to plain text, and carries no source ConversationID, relay or provider metadata. A claimed event id, author and time are
labels the author supplies; a receiver renders them as inert provenance and never as authenticated authorship.

## Findings

| ID | Summary | Resolution |
|---|---|---|
| F-27 | The frozen W0 `SignedEnvelope.body` limit (65536 bytes) binds every signed W1 object, so a `max_bytes` above it is not reachable. The earlier draft sizes (2048 members, 4096 items) could never be met: a membership entry is about 90 bytes | Membership is capped at 512 and a bundle at 1024 items, both bounded by the 64 KiB body in practice; the type `max_bytes` stays 131072 to leave room for large post-quantum signature sets. A group beyond that, or a longer history, needs an additive larger-envelope profile before W1 may claim it; a client splits a long history into several independent bundles |
| F-28 | The draft `transition_binding_hash` in `GroupControlState` is circular (the binding carries the resulting state hash) | Dropped. The chain is `GroupCommitBinding.previous_state_hash` / `resulting_state_hash` |
| F-29 | ADR-0128 does not say which intents are reductions, so a direct Commit could not be validated against ADR-0173 rule 1 | The `reducing` classification above; `coordinator.replace` is a reduction because it is committed directly after `sequencing_timeout` |
| F-30 | No registry operation for invite redemption or the join policy | Appended (decision item 7) |

## Consequences

- Requirements R-400 (wire), R-401 (cross-field rules), R-402 (bundle admission and isolation) and R-403 (size bounds); threat T-161. V-004 criteria 6 and 7 are met on this evidence.
- [`spec/protocol/group-control-transition.logical.schema.json`](../spec/protocol/group-control-transition.logical.schema.json) and [`group-history-bundle.logical.schema.json`](../spec/protocol/group-history-bundle.logical.schema.json) stay as the logical contract; the frozen bytes are
  [`group.cddl`](../spec/cddl/group.cddl). The frozen W0 corpus (735 vectors) passes unchanged on both codecs.
- The Rust codec reads the registries from `OM_REG` when set (native only); the Node harness already did.
- V-004 pre-release still needs the multi-client partition harness on two implementations (ADR-0173).
