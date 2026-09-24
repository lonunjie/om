# v009-materializer

Feasibility prototype for V-009 (`baseline_event_materialization_convergence`). Not product code. Not for production.

This is **evidence, not gate passage**. The criterion status is `partial`, with details below.

## What is here

| File | Role |
|---|---|
| `materializer/` | Independent **Rust** ConversationEvent materializer (`src/materialize.rs`), written from the public docs. It is incremental: events apply in arrival order, and events whose target has not arrived are buffered. |
| [`gen_corpus.py`](gen_corpus.py) | Deterministic corpus generator (stdlib only): 55 hand-written cases plus 60 seeded random cases. Output goes to `work/corpus.json` (gitignored and regenerated on every run). |
| `compare.py` | Imports the reference [`verification/bounded/conversation_convergence.py`](../../bounded/conversation_convergence.py) **unmodified**, adapts the corpus to it, checks that the reference converges across delivery orders, diffs it against the Rust views, and attributes every disagreement. |
| `run.sh` | Builds and runs everything, then writes [`verification/results/v009-materializer-latest.json`](../../results/v009-materializer-latest.json). It exits non-zero on any non-convergence or unattributed disagreement. |

```sh
verification/prototypes/v009-materializer/run.sh     # ~10 s warm, ~30 s cold build
```

Dependencies: Rust 1.98.1 (pinned in `materializer/rust-toolchain.toml`), `serde_json =1.0.145` (Cargo.lock), and Python 3 (stdlib).

## Corpus coverage (115 cases)

`eventclock` (physical/counter/endpoint tiebreak, identical HLC tuple, backward clock, future-dated pin and close), `edits`, `deletes`, `reactions`, `receipts`, `replies`/threads, `pins`, `redaction`, `polls` (single/multiple, two closes, unauthorized close, retraction), `topics` (lifecycle, duplicate create, unauthorized update, create-time placement, move/clear), `malformed` (bad refs, wrong target kinds, wrong JSON types, wrong clock arity, clock-endpoint mismatch, unknown epoch, wrong conversation, >64 KiB, missing EventID, equivocating EventID), `extensions` (unknown critical capability on content, on a state operation, on an unknown kind, and unknown optional kind), `authorization_race` (pin vs. revocation, redaction vs. demotion, grant vs. action, vote vs. restriction, topic management vs. demotion, close vs. demotion), `duplication`, and 60 `random` cases.

Authority is modeled as an externally ordered chain of control epochs (grants and restrictions per principal). Each event names the epoch it was sent under. This follows [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../../../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) §1/§4 and [`TIME_AND_EXPIRY_SEMANTICS.md`](../../../docs/architecture/events/TIME_AND_EXPIRY_SEMANTICS.md) §1: authority never comes from the HLC.

## Results (run 2026-09-24)

**Update after ADR-0121 (same day).** The reference was corrected for REF-1 and REF-3, and it now implements the
accepted rulings DA-1..DA-3. Re-run: **115 of 115 cases agree, 0 disagreements**, and the reference converges on every
case. The criterion stays `partial` for the other reasons below: the shared authorization adapter (G-V9-5), the
reference covering only part of the kinds, and the missing non-author review.

The original run, before the fixes:


- **Rust materializer:** 115 cases and 64,280 delivery orders. The orders are every permutation for n≤6, otherwise 600 seeded permutations, plus two duplication variants for every third order. **All converge**.
- **Python reference on the extended corpus:** 41,113 orders. It is **non-convergent on 1 case** (`equivocation-duplicate-eventid`; see REF-1).
- **Diff:** 90 cases agree. The other 25 disagree, and **every one is attributed**: 0 materializer bugs, 2 reference bugs, 3 doc ambiguities.

| Cause | Class | Cases | Description |
|---|---|---|---|
| REF-1 | reference bug | 1 | `by_id = {e.eid: e}` keeps the last-arrived payload of an EventID seen with two different payloads, so the view depends on arrival order. The materializer rejects every version (D-03). |
| REF-3 | reference bug | 4 | `poll.vote` options are not validated. An unknown option, or multi-select on a single-choice poll, is counted and can displace an earlier valid vote. |
| DA-1 | doc ambiguity | 16 | A state operation whose target is missing or of the wrong kind: a pin, redaction, vote or close on a non-message or a never-arriving event, or a topic move to a non-existent topic or of a poll. The reference applies it as register state. The materializer buffers it (`dangling`) or rejects it (`invalid_target`). |
| DA-2 | doc ambiguity | 2 | Duplicate `topic.create` for one TopicID. In the reference the latest create resets title and status; in the materializer the earliest create wins and later ones are rejected. |
| DA-3 | doc ambiguity | 2 | Empty `poll.vote` selection. The reference stores an empty vote; the materializer treats it as retraction. |

**Adapter caveat (limits the comparison).** The reference has no validation layer and takes `authorized` as input. `compare.py` therefore computes that flag from the control chain: schema validity, conversation, size, unknown critical capability, unknown epoch, capability at the sending epoch, close by creator or `room.redact_others`, and topic update by creator or `room.manage_topics`. For malformed and authorization-race cases the two implementations therefore share the adapter's authorization semantics. They are compared only on target handling and conflict resolution. The reference covers only pins, redaction, polls and topics. Edits, deletes, reactions, receipts, replies and placeholders are checked only for Rust self-convergence, not against a second implementation.

## Criterion status

| Gate | Criterion | Status | Evidence | Reason |
|---|---|---|---|---|
| V-009 | second independently written materializer agrees with the reference on the full corpus incl. malformed and authorization-race cases | **partial** | this directory; [`verification/results/v009-materializer-latest.json`](../../results/v009-materializer-latest.json) | Different language, and written before the reference logic was read (commit `4fdfda2` precedes the adapter). It converges everywhere, and every disagreement is attributed. It is not "agrees": 2 reference bugs and 3 ambiguities need fixes or rulings. The reference covers 4 of the 10+ baseline features, and authorization is adapter-supplied. |

**Independence (honest statement):** written by a Claude agent; the design package was authored by a ChatGPT design agent and Claude agents; ADR-0117 (proposed) decides whether an AI reviewer counts as independent. The Rust materializer was written from the docs only. Before its commit, the only part of the reference viewed was the `E` record's field list (lines 1–21). The corpus generator and adapter were written by the same agent as the materializer.

## Doc ambiguities and proposed rulings (implemented in `materialize.rs`)

| ID | Ambiguity | Proposed ruling |
|---|---|---|
| D-01 | HLC tuple ties (same endpoint reusing a tuple) | Total order key `(physical_ms, counter, endpoint, EventID)` |
| D-02 | Which control state authorizes a content event racing a permission change | The state at the event's **sending epoch** (the MLS epoch that authenticated it). Never latest-known state, never HLC. |
| D-03 | Same EventID, different payload | Equivocation: reject all versions (order-independent) and surface a diagnostic |
| D-04 | `endpoint_tiebreak` vs. authenticated endpoint | Must be equal, otherwise the event is malformed |
| D-05 | Cross-conversation `reply_to` is undetectable, because EventIDs are opaque | The message stays valid. An unresolved reference renders as "original unavailable". An event whose own `conversation_id` is foreign is rejected. |
| D-06 | Event names an epoch not in the authenticated chain | Not authority: reject (a real client would hold it pending) |
| D-07 | Clearing a topic placement | `message.topic.set` with `topic_id: null` clears it |
| D-08 | Capability for replies | `room.post`, or `room.reply` when `reply_to` or `thread_root_event_id` is set |
| D-09 | May an author edit after losing post rights? | No: an edit needs `room.post`/`room.reply` at its sending epoch |
| D-10 | Capability for `message.delete` | None beyond authorship: an author can always tombstone their own content |
| D-11 | Capability for `poll.vote` | `room.react`, so members of announcement rooms can vote |
| D-12 | "creator or authorized room moderator may close" | Poll author, or a holder of `room.redact_others` at the sending epoch |
| D-13 | 64 KiB limit measured on what | Canonical encoded event (the prototype uses JSON as a proxy) |
| D-14 | Known state-operation kind with an unknown required capability | Refuse the operation (no effect, no placeholder). Content kinds and unknown kinds get a placeholder. |
| D-15 (DA-2) | Duplicate `topic.create` | The earliest in EventClock order wins; later creates are rejected |
| D-16 | Who may `topic.update`/`topic.status.set` | The topic creator, or `room.manage_topics` |
| D-17 (DA-3) | Empty vote | Retraction |
| D-18 | Pins or reactions on deleted/redacted content | Register state is kept but hidden from the view |
| DA-1 | State operations on missing or wrong-kind targets | Wrong kind: reject. Missing: buffer; never displayed; reported as `dangling`. |

## Design gaps found

1. **G-V9-1 — HLC last-writer-wins lets a skewed or hostile clock dominate shared state** (`clock-future-dated-pin`). An authorized pinner who stamps `physical_ms` in 2090 wins every later honest unpin until 2090. [`EVENT_MODEL.md`](../../../docs/architecture/events/EVENT_MODEL.md) says a misleading clock "does not grant identity or authorization", but in LWW registers it grants precedence. The same applies to topic title/status, `message.topic.set` and edits. *Proposed:* order shared-state LWW by `(sending control epoch, HLC)`, so a later epoch always supersedes, and/or clamp a received HLC to at most the receiver's local time plus a declared skew. The clamp must not use arrival time as a tiebreak.
2. **G-V9-2 — The stale-epoch acceptance window is local policy, so views can diverge.** Under D-02, a demoted moderator's redaction sent under the old epoch is valid (`race-redact-vs-demotion`). Whether a receiver can still decrypt an old-epoch event depends on its local MLS past-epoch secret retention. Two receivers with different retention materialize different views. *Proposed:* make past-epoch retention a normative GroupProfile parameter. Views are then required to converge only over events every member can decrypt, and undecryptable events render as an explicit gap.
3. **G-V9-3 — EventID equivocation is unspecified** (REF-1). *Proposed:* adopt D-03 normatively, and add an equivocation vector to the conformance corpus.
4. **G-V9-4 — Several capability bindings are not stated:** poll vote, poll close, topic update/status, edit after demotion, and author delete (D-08..D-12, D-16). *Proposed:* add a kind→capability table to [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../../../docs/architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md).
5. **G-V9-5 — The reference model's authorization input is an oracle.** The existing 864-case result therefore cannot show authorization-race behavior. The reference should consume the control-epoch chain itself.

## Proposed design updates

- [`EVENT_MODEL.md`](../../../docs/architecture/events/EVENT_MODEL.md) "EventClock": add the D-01 total-order key and the G-V9-1 epoch-major LWW or clamp rule.
- [`EVENT_MODEL.md`](../../../docs/architecture/events/EVENT_MODEL.md) "Materialized views": add rules for dangling and wrong-kind targets (DA-1) and for equivocation (D-03).
- [`MESSENGER_BASELINE_V1.md`](../../../docs/architecture/events/MESSENGER_BASELINE_V1.md) polls: validate options (REF-3), close authority (D-12), empty-vote retraction (D-17).
- [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../../../docs/architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md): kind→capability table (G-V9-4) and the duplicate-topic rule (D-15).
- [`verification/bounded/conversation_convergence.py`](../../bounded/conversation_convergence.py): fix REF-1 and REF-3, and derive `authorized` from a control chain (G-V9-5). Not edited here, per the task rules.
- [`machine/file_registry.json`](../../../machine/file_registry.json): register this directory's files and [`verification/results/v009-materializer-latest.json`](../../results/v009-materializer-latest.json) (not edited here).
