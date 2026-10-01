# ADR-0174 — Materialization rulings from the second materializer (V-009)

- **Status:** Accepted
- **Date:** 2026-10-01
- **Decided by:** design agent (Claude, sole design agent, ADR-0167), from the V-009 evidence below
- **Source:** [`verification/prototypes/v009-materializer/`](../verification/prototypes/v009-materializer/README.md) (266 cases),
  [`conversation_convergence.py`](../verification/bounded/conversation_convergence.py)
- **Clarifies:** ADR-0121 rule 8 (duplicate `topic.create`), ADR-0128 rules 16 and 20
- **Does not decide:** any wire encoding.

## Context

V-009 asked for a second materializer that derives authorization from the control chain and covers R-235, R-236 and R-239.
Building it (Rust, from a raw chain of control operations) and extending the Python reference to the same event kinds exposed
four points the accepted text leaves open. A fifth finding was a bug on each side, now fixed, so it needs no ruling.

## Decision

1. **Duplicate `topic.create` is ordered like every other register.** ADR-0121 rule 8 says "the earliest in EventClock order
   wins" and rule 3 says registers order by `(sending epoch, EventClock)`. They are read together: the earliest create by
   `(sending epoch, EventClock)` wins, so a later-epoch create cannot win by backdating its HLC. The winning create is the
   initial write of both the title register and the status register (status `open`), and a `topic.update` or
   `topic.status.set` ordered before it has no effect.
2. **A mention is live only for a current member.** A receiver renders a `Mention` as a live mention when the mentioned
   principal is in the latest known roster, and as plain text with its display label otherwise (ADR-0128 rule 20). This is
   presentation, not authorization, so the sending epoch's roster is not used: a member removed after the message was sent
   is rendered as text. An edit replaces the content with plain text, so an edited message carries no mention.
3. **Expiry carriers are unsigned integers.** The envelope `expiry` and `MessageCreate.expires_after` are unsigned integer
   milliseconds; any other type makes the event malformed. The deadline is `min(expiry, receipt + expires_after)`, so it is
   never after `expiry` even when the message is received after it (it is then deleted on arrival). If a receiver has no
   recorded receipt time for an event, `expires_after` counts from the receiver's current time and cannot fire early.
4. **Targets are resolved from the whole event set.** A `poll.close`, `poll.vote` or topic operation whose `(epoch, clock)`
   position precedes its target's create is judged against the target, not against the prefix of the sort order. A topic
   operation ordered before the winning create is the one exception, by rule 1. (The reference had resolved polls from the
   prefix, and the materializer applied a pre-create status change; both were bugs, REF-4 and MAT-1.)

## Consequences

- Requirements R-397 (rule 1), R-398 (rule 2) and R-399 (rule 3), verified by V-009.
- [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md), [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md) carry the rulings.
- The conformance corpus (V-009 pre-release) must include the `epoch_major`, `expiry` and `mentions` families.
- V-009 pre-engineering criteria 0-4 are met on this evidence; the independence is a different language and runtime, not a
  different author (stated in the results file).
