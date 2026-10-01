# v009-materializer

Feasibility prototype for V-009 (`baseline_event_materialization_convergence`). Not product code. Not for production.

This is **evidence for the pre-engineering criteria**; gate status is recorded in the ledger, not here.

## What is here

| File | Role |
|---|---|
| `materializer/` | Independent **Rust** ConversationEvent materializer (`src/materialize.rs`). It is incremental: events apply in arrival order, and events whose target has not arrived are buffered. It reads only the **raw control chain** (`role.define`, `role.assign`, `restriction.set`, `member.remove` per epoch) and derives authority itself. |
| [`gen_corpus.py`](gen_corpus.py) | Deterministic corpus generator (stdlib only): hand-written cases (with spec-derived `expect` fields), 60 + 120 seeded random cases in two families, and forwarding vectors. It also holds the **Python derivation** of effective authority from the chain. Output goes to `work/corpus.json` (gitignored, regenerated on every run). |
| `compare.py` | Imports the reference [`verification/bounded/conversation_convergence.py`](../../bounded/conversation_convergence.py), adapts the corpus to it, checks that the reference converges across delivery orders, diffs it against the Rust views, compares the two authority derivations, checks every `expect` on both implementations and the forwarding vectors, and attributes every disagreement. |
| `run.sh` | Builds and runs everything, then writes [`verification/results/v009-materializer-latest.json`](../../results/v009-materializer-latest.json). It exits non-zero on any non-convergence, unattributed disagreement, authority mismatch, failed expectation or failed forwarding vector. |

```sh
verification/prototypes/v009-materializer/run.sh     # ~15 s warm, ~40 s cold build
```

Dependencies: Rust 1.98.1 (pinned in `materializer/rust-toolchain.toml`), `serde_json =1.0.145` (Cargo.lock), and Python 3 (stdlib).

## How the two sides stay independent

* **Authority.** The corpus carries a chain of control operations per epoch. Rust derives role grants minus restrictions and the roster from
  that chain; `gen_corpus.derive` does the same in Python for `compare.py`'s authorization oracle. `compare.py` requires the two per-epoch
  authority tables to be equal in every case (`authority_agree`), so authorization semantics are no longer a shared adapter (G-V9-5).
* **Reference coverage.** The reference now also models edits, deletes, reactions, receipts, replies, expiry and mentions (it covered pins,
  redaction, polls and topics before), and orders every register by `(sending epoch, EventClock)`.
* **Spec expectations.** Cases for R-235, R-236 and R-239 carry hand-written expected values taken from the ADR text, and both
  implementations must match them. That guards against the two sides agreeing on the same mistake.
* **Mutation check.** Making the Rust order key ignore the epoch, or letting `expires_after` override the absolute expiry, makes 15 expectations
  and 14 comparison cases fail (run once, 2026-10-01).

## Corpus coverage (266 cases)

`eventclock`, `edits`, `deletes`, `reactions`, `receipts`, `replies`/threads, `pins`, `redaction`, `polls`, `topics`, `malformed`, `extensions`,
`authorization_race` (now including member removal, a custom role and a restriction across four epochs), `duplication`, 60 `random`,
**`epoch_major`** (R-236 and ADR-0121 rule 3: a vote after `poll.close`, vote/reaction/edit/pin/topic/placement registers, two closes, duplicate
topic creates, a future-dated pin), **`expiry`** (R-235: boundary, local bound earlier, absolute earlier, received after expiry, no receipt time,
malformed carriers), **`mentions`** (R-239: member vs non-member, removed member, edited, deleted, malformed runs) with 3 forwarding vectors, and
120 **`random2`** (3-4 control epochs with removals and restrictions, every event kind, mention runs, expiry, receiver-local clock).

Authority is an externally ordered chain of control epochs. Each event names the epoch it was sent under. This follows
[`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../../../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) §1/§4 and
[`TIME_AND_EXPIRY_SEMANTICS.md`](../../../docs/architecture/events/TIME_AND_EXPIRY_SEMANTICS.md) §1: authority never comes from the HLC.

## Results (run 2026-10-01)

**266 of 266 cases agree, 0 disagreements. Rust: 188,531 delivery orders, all converge. Reference: 124,800 orders, all converge. 266 of 266
authority tables agree. 47 expectations hold on both implementations; 3 of 3 forwarding vectors agree across Rust, Python and the hand-written
expectation.**

The random2 family found two real bugs that the earlier corpus could not (both fixed):

| ID | Side | Bug |
|---|---|---|
| REF-4 | reference | `poll.close` and `poll.vote` looked up the poll in the prefix of the sort order, so an event ordered before its poll's create (reachable once ordering is epoch-major: a close sent in epoch 0 naming a poll created in epoch 2) was dropped. It now resolves polls from the whole set, like pins and redactions already did. |
| MAT-1 | materializer | `topic.status.set` ordered before the winning `topic.create` was applied, while `topic.update` was not. The create is now the initial write of both registers (title, and status `open`). |

Earlier history (2026-09-24): the first run left 25 attributed disagreements (REF-1, REF-3, DA-1..DA-3), all settled by ADR-0121; they no longer occur.

**Limits.** The reference is still the author's own Python, extended by the same agent that wrote the Rust side, and the corpus generator is shared;
independence is language and runtime, not author (stated in the results file). Receiver-local state (`now`, receipt times) is fixed per case.
`expires_after` and mentions are modeled on `message.create` only, and a mention is a diagnostic `runs` node rather than RichText `InlineMention` plus the `mentions` set (the wire carries both; their agreement is a codec-level rule). The 64 KiB limit is measured on JSON, not canonical CBOR.

## Criterion status

| Criterion | Status | Evidence |
|---|---|---|
| second materializer agrees with the reference on the full corpus incl. malformed and authorization-race cases | **met** | 266/266 |
| derives authorization from the control chain rather than a shared adapter (G-V9-5) | **met** | Rust derives from the raw chain; 266/266 authority tables agree with the Python derivation |
| reaction and vote registers epoch-major; a vote after `poll.close` ignored (R-236) | **met** | 11 `epoch_major` cases with expectations, both sides |
| expiry at `min(expiry, receipt + expires_after)`, never after the absolute bound (R-235) | **met** | 12 `expiry` cases with expectations, both sides |
| forwarding strips mentions; a mention of a non-member is inert (R-239) | **met** | 6 `mentions` cases, 3 forwarding vectors |

**Independence (honest statement):** different language and runtime (ADR-0167 section 3), **not a different author**. A Claude agent wrote the Rust materializer, the corpus generator, the Python adapter and the reference extensions. The design package was authored by a ChatGPT design agent and Claude agents. The Rust side was first written from the docs before the reference logic was read; the control-chain derivation, epoch-major registers, expiry and mentions were added later with both sides in view.

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
| D-19 | Which roster decides whether a mention of a principal is live | The latest known roster (latest control epoch). A mention is presentation, not authorization. A removed member's mention renders as text. An edited message carries no mentions. |
| D-20 | Duplicate `topic.create` ordering (ADR-0121 rule 8 says EventClock; rule 3 says registers are epoch-major) | Earliest by `(sending epoch, EventClock)`, so a later-epoch create cannot win by backdating its HLC. The create is the initial write of title and status `open`, and any topic op ordered before the winning create is ignored. |
| D-21 | Type of the envelope `expiry` and of `expires_after` | Integers in one clock unit (seconds on the wire; this prototype counts ms); `expires_after` at least 1 (MessageCreate key 4). Anything else makes the event malformed. |
| D-22 | `expires_after` with no recorded receipt time | Counts from the receiver's current time, so it can never fire early. |
| DA-1 | State operations on missing or wrong-kind targets | Wrong kind: reject. Missing: buffer; never displayed; reported as `dangling`. |

## Design gaps found (status after ADR-0174)

1. **G-V9-1 (HLC last-writer-wins lets a skewed clock dominate shared state): resolved** by ADR-0121 rule 3 and ADR-0128 rule 17 (epoch-major registers); the materializer and the reference now implement them. The residual risk inside one epoch stays documented (`epochmajor-vote-backdated-same-epoch`).
2. **G-V9-2 (stale-epoch acceptance window is local policy): resolved** by ADR-0121 rule 4 (`past_epoch_window`, 0..8).
3. **G-V9-3 (EventID equivocation unspecified): resolved** by ADR-0121 rule 5; the conformance corpus has the equivocation vector.
4. **G-V9-4 (capability bindings unstated): resolved** by ADR-0121 rule 7.
5. **G-V9-5 (reference authorization is an oracle): resolved for the comparison**: authority is derived from the raw control chain independently on both sides and the two derivations are compared. The Python reference itself still takes an `authorized` flag, supplied by `compare.py` from its own derivation.

New in this session, recorded in [ADR-0174](../../../decisions/ADR-0174-materialization-rulings-from-the-second-materializer.md): D-19..D-22 and the clarification of ADR-0121 rule 8 (D-20).
