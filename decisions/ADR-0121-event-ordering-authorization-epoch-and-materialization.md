# ADR-0121 — Event ordering, authorization epoch and materialization

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator (delegated 2026-09-24: "draft the design gap rulings ... check it and accept it if good")
- **Source:** V-009 ([`verification/prototypes/v009-materializer/`](../verification/prototypes/v009-materializer/README.md)) and V-016 gap 2. **Amends:**
  [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md), [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md),
  [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../docs/architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md).

## Context

A second materializer, written independently, converged on every delivery order it tried. Comparing it with the
reference exposed rules the docs never state, two reference bugs, and two ways that views can diverge: a
future-dated clock in last-writer-wins registers, and receiver-dependent acceptance of old-epoch messages.

## Decision

1. **Authorization epoch.** An event is authorized against the control state of its **sending epoch**, the MLS epoch
   that authenticated it. Neither the latest known state nor the HLC time is used. An event that names an epoch outside
   the authenticated chain is not authorized: a client holds it pending and never applies it.
2. **Total order.** EventClock order is `(physical_ms, counter, endpoint, EventID)`. `endpoint_tiebreak` MUST equal
   the authenticated endpoint, or the event is malformed.
3. **Shared-state registers are epoch-major.** Last-writer-wins registers (pins, topic title/status, topic placement,
   edits) order by `(sending epoch, EventClock)`, so a write from a later epoch always supersedes. Receivers never
   clamp by their own clock, which would break convergence. A client shows a write whose `physical_ms` is more than 10
   minutes past its own clock as a clock anomaly. Any authorized member can end a future-dated write's dominance by
   advancing the epoch (an update commit) before its corrective write. The residual risk is a skewed write that
   dominates within one epoch.
4. **Past-epoch window.** `past_epoch_window` is a normative GroupProfile parameter: a count of epochs, default 2,
   maximum 8. A receiver deletes an epoch's secrets when it processes the commit that moves the epoch out of the
   window. Convergence is required over the events every member can decrypt. An event that can no longer be decrypted
   renders as an explicit gap. Divergence from a message that arrives late at one receiver and in time at another is a
   documented residual risk, bounded by the window. Exact sequencing through a coordinator is out of scope for W0.
5. **Equivocation.** Two payloads with one EventID are equivocation. Every version is rejected in an order-independent
   way, and a diagnostic is surfaced. A conformance vector covers it.
6. **Targets.** A state operation on a target of the wrong kind is rejected. One on a missing target is buffered as
   `dangling`, is never displayed, and applies if the target arrives. An unresolved `reply_to` renders as
   "original unavailable". An event whose own `conversation_id` is foreign is rejected. Pins and reactions on deleted or
   redacted content are kept in state and hidden from the view.
7. **Kind-to-capability bindings** (at the sending epoch):

   | Operation | Capability |
   |---|---|
   | message | `room.post`, or `room.reply` when `reply_to` or `thread_root_event_id` is set |
   | edit | the same as the original post; authorship alone is not enough after demotion |
   | `message.delete` of one's own content | authorship only |
   | `poll.vote` | `room.react` |
   | `poll.close` | the poll author, or `room.redact_others` |
   | `topic.update`, `topic.status.set` | the topic creator, or `room.manage_topics` |
   | known state operation with an unknown required capability | refused, with no placeholder |

8. **Polls and topics.** Vote options are validated: an unknown option, or several options on a single-choice poll,
   is rejected. An empty selection retracts a vote. For duplicate `topic.create` events with one TopicID, the earliest
   in EventClock order wins. `message.topic.set` with `topic_id: null` clears the placement.
9. **Size limit.** The 64 KiB limit applies to the canonical encoded event.

## Consequences

- The reference model [`verification/bounded/conversation_convergence.py`](../verification/bounded/conversation_convergence.py) is corrected for REF-1 (equivocation) and REF-3
  (vote validation). It must also derive authorization from the control chain (G-V9-5), and the V-009 comparison is
  re-run after that.
- The V-016 Room model's late-write divergence is covered by rule 4.
