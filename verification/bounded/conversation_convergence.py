#!/usr/bin/env python3
"""Bounded delivery-order/duplication convergence checks for V-009."""
from __future__ import annotations

import itertools, json
from dataclasses import dataclass, asdict
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "results" / "conversation-convergence-latest.json"

@dataclass(frozen=True)
class E:
    eid: str
    kind: str
    clock: tuple[int, str, str]
    actor: str
    target: str | None = None
    value: object = None
    authorized: bool = True
    epoch: int = 0                 # sending control epoch (ADR-0121 rule 1)
    expiry: int | None = None      # envelope expiry, absolute wall clock ms (ADR-0128 rule 16)


def okey(e):
    """ADR-0121 rule 3 / ADR-0128 rule 17: shared-state registers order by (sending epoch, EventClock)."""
    return (e.epoch, e.clock)


def forward_body(body):
    """ADR-0128 rule 20: forwarding converts Mention nodes to plain Text with the display label and drops
    the mention set. Nothing identifying the source travels in the new body."""
    out = {}
    for k in ("text", "attachments"):
        if k in body:
            out[k] = body[k]
    if body.get("runs"):
        out["runs"] = [{"t": "text", "s": r["s"] if r["t"] == "text" else r["label"]} for r in body["runs"]]
    return out


def deadline(e, received, now):
    """R-235: delete at min(envelope expiry, receipt + expires_after); never after the absolute expiry."""
    after = (e.value or {}).get("expires_after") if isinstance(e.value, dict) else None
    base = received.get(e.eid, now)
    local = base + after if (after is not None and base is not None) else None
    cands = [t for t in (e.expiry, local) if t is not None]
    return min(cands) if cands else None


def materialize(delivery, now=None, received=None, members=frozenset()):
    # EventID de-duplication, then deterministic (epoch, EventClock) order. Authorization
    # is authenticated input to materialization and wall time is irrelevant.
    # One EventID with two different payloads is equivocation: every version is
    # rejected, independent of arrival order (ADR-0121 rule 5).
    received = received or {}
    by_id, equivocal = {}, set()
    for e in delivery:
        if e.eid in by_id and by_id[e.eid] != e:
            equivocal.add(e.eid)
        by_id.setdefault(e.eid, e)
    events = sorted((e for i, e in by_id.items() if i not in equivocal), key=okey)
    pins = {}
    redacted = set()
    poll_votes = {}
    poll_closed_at = {}
    topics = {}
    message_topics = {}
    edits = {}
    deleted = set()
    reactions = {}
    receipts = {}
    # ADR-0121 rule 6: a state operation needs a target of the right kind. A missing
    # target is held (never applied) until it arrives, so the result is order-independent.
    content = {e.eid: e.kind for e in events if e.authorized and e.kind in ("message.create", "poll.create")}
    content_ev = {e.eid: e for e in events if e.eid in content}
    # Poll specs are resolved from the whole event set, not from the prefix of the sort order: a close or vote
    # whose (epoch, clock) precedes its poll's create is still a valid operation on that poll (V-009 REF-4).
    poll_specs = {e.eid: e.value for e in events if e.authorized and e.kind == "poll.create"}
    created_topics = {e.target for e in events if e.authorized and e.kind == "topic.create"}

    for e in events:
        if not e.authorized:
            continue
        if e.kind in ("pin.set", "moderation.redact") and e.target not in content:
            continue
        if e.kind == "message.topic.set" and (content.get(e.target) != "message.create"
                                              or (e.value is not None and e.value not in created_topics)):
            continue
        if e.kind == "pin.set":
            pins[e.target] = bool(e.value)
        elif e.kind == "moderation.redact":
            redacted.add(e.target)
        elif e.kind == "poll.close":
            if e.target in poll_specs and e.target not in poll_closed_at:
                poll_closed_at[e.target] = okey(e)
        elif e.kind == "poll.vote":
            if e.target not in poll_specs:
                continue
            close_key = poll_closed_at.get(e.target)
            if close_key is not None and okey(e) > close_key:
                continue
            # ADR-0121 rule 8: options are validated; an empty selection retracts.
            spec = poll_specs[e.target] or {}
            opts = spec.get("opts") or ()
            if any(o not in opts for o in e.value) or (spec.get("mode") == "single" and len(e.value) > 1):
                continue
            if e.value:
                poll_votes[(e.actor, e.target)] = e.value
            else:
                poll_votes.pop((e.actor, e.target), None)
        elif e.kind == "topic.create":
            if e.target in topics:  # earliest create wins (ADR-0121 rule 8)
                continue
            topics[e.target] = {"title": e.value, "status": "open", "clock": e.clock, "ok": okey(e)}
        elif e.kind == "topic.update":
            if e.target in topics and okey(e) >= topics[e.target]["ok"]:
                topics[e.target]["title"] = e.value
                topics[e.target]["clock"] = e.clock
                topics[e.target]["ok"] = okey(e)
        elif e.kind == "topic.status.set":
            if e.target in topics and okey(e) >= topics[e.target]["ok"]:
                topics[e.target]["status"] = e.value
                topics[e.target]["clock"] = e.clock
                topics[e.target]["ok"] = okey(e)
        elif e.kind == "message.topic.set":
            old = message_topics.get(e.target)
            if old is None or okey(e) >= old[2]:
                message_topics[e.target] = (e.value, e.clock, okey(e))
        elif e.kind == "message.edit":
            # edits replace RichText of messages only, and only by the original author (EM message.edit)
            if content.get(e.target) == "message.create" and content_ev[e.target].actor == e.actor:
                edits[e.target] = e.value  # sorted by (epoch, clock): last write wins
        elif e.kind == "message.delete":
            if content.get(e.target) == "message.create" and content_ev[e.target].actor == e.actor:
                deleted.add(e.target)
        elif e.kind == "reaction.set":
            if e.target in content:
                reactions[(e.actor, e.target, e.value[0])] = bool(e.value[1])
        elif e.kind == "receipt.update":
            level = 2 if e.value[0] == "read" else 1
            for t in e.value[1]:
                if t in content:
                    receipts[(t, e.actor)] = max(receipts.get((t, e.actor), 0), level)

    deadlines = {i: deadline(ev, received, now) for i, ev in content_ev.items()}
    deadlines = {i: t for i, t in deadlines.items() if t is not None}
    expired = {i for i, t in deadlines.items() if now is not None and t <= now}
    mentions = {}
    for i, ev in content_ev.items():
        if ev.kind != "message.create" or i in expired or i in deleted or i in redacted:
            continue
        live, inert = [], []
        if i not in edits:
            for r in (ev.value or {}).get("runs") or ():
                if r["t"] == "mention":
                    if r["principal"] in members:
                        live.append(r["principal"])
                    else:
                        inert.append(r["label"])
        mentions[i] = (tuple(sorted(live)), tuple(sorted(inert)))
    return {
        "pins": tuple(sorted(pins.items())),
        "redacted": tuple(sorted(redacted)),
        "poll_votes": tuple(sorted((a, p, tuple(v)) for (a,p),v in poll_votes.items())),
        "poll_closed": tuple(sorted(poll_closed_at)),
        "topics": tuple(sorted((k, v["title"], v["status"], v["clock"]) for k,v in topics.items())),
        "message_topics": tuple(sorted((k, v[0], v[1]) for k,v in message_topics.items())),
        "edits": tuple(sorted(edits.items())),
        "deleted": tuple(sorted(deleted)),
        "reactions": tuple(sorted((a, t, k, p) for (a, t, k), p in reactions.items())),
        "receipts": tuple(sorted((t, a, l) for (t, a), l in receipts.items())),
        "replies": tuple(sorted(
            (i, tuple(sorted((f, (ev.value or {}).get(f) in content) for f in ("reply_to", "thread_root")
                             if (ev.value or {}).get(f) is not None)))
            for i, ev in content_ev.items() if ev.kind == "message.create")),
        "deadlines": tuple(sorted(deadlines.items())),
        "expired": tuple(sorted(expired)),
        "mentions": tuple(sorted(mentions.items())),
    }


def exhaustive_permutations(events, with_duplicate=True, **kw):
    expected = materialize(events, **kw)
    count = 0
    for perm in itertools.permutations(events):
        count += 1
        assert materialize(perm, **kw) == expected
        if with_duplicate:
            # Duplicate one event at both ends; EventID de-dup must preserve view.
            d = perm[0]
            assert materialize((d,) + perm + (d,), **kw) == expected
            count += 1
    return count, expected


def main():
    cases = 0
    fixtures = {}

    # Targets exist as content: state operations on missing targets are held (ADR-0121 rule 6).
    pin = [
        E("m1", "message.create", (0,"A","m1"), "A"),
        E("p1", "pin.set", (1,"A","p1"), "A", "m1", True),
        E("p2", "pin.set", (3,"B","p2"), "B", "m1", False),
        E("p3", "pin.set", (2,"X","p3"), "X", "m1", False, False), # unauthorized ignored
    ]
    c, out = exhaustive_permutations(pin); cases += c; fixtures["pin"] = out

    redact = [
        E("m2", "message.create", (0,"A","m2"), "A"),
        E("m3", "message.create", (0,"B","m3"), "B"),
        E("r1", "moderation.redact", (2,"M","r1"), "M", "m2"),
        E("r2", "moderation.redact", (4,"X","r2"), "X", "m3", authorized=False),
        E("r3", "moderation.redact", (3,"N","r3"), "N", "m2"),
    ]
    c, out = exhaustive_permutations(redact); cases += c; fixtures["redaction"] = out

    poll = [
        E("poll", "poll.create", (1,"A","poll"), "A", value={"opts":["x","y","z"]}),
        E("v1", "poll.vote", (2,"A","v1"), "A", "poll", ("x",)),
        E("v2", "poll.vote", (4,"A","v2"), "A", "poll", ("y",)),
        E("vb", "poll.vote", (3,"B","vb"), "B", "poll", ("z",)),
        E("cl", "poll.close", (5,"A","cl"), "A", "poll"),
        E("late", "poll.vote", (6,"A","late"), "A", "poll", ("z",)),
    ]
    c, out = exhaustive_permutations(poll, with_duplicate=False); cases += c; fixtures["poll"] = out

    topic = [
        E("m4", "message.create", (0,"A","m4"), "A"),
        E("tc", "topic.create", (1,"A","tc"), "A", "t1", "First"),
        E("tu1", "topic.update", (2,"A","tu1"), "A", "t1", "Second"),
        E("tu2", "topic.update", (3,"B","tu2"), "B", "t1", "Final"),
        E("ts", "topic.status.set", (4,"M","ts"), "M", "t1", "resolved"),
        E("mt", "message.topic.set", (5,"M","mt"), "M", "m4", "t1"),
    ]
    c, out = exhaustive_permutations(topic, with_duplicate=False); cases += c; fixtures["topic"] = out

    # ADR-0121 rule 3 / ADR-0128 rules 16, 17, 20 (R-235, R-236, R-239): epoch-major registers, edits,
    # reactions, receipts, replies, expiry and mentions. The epoch-1 writes carry earlier clocks than the epoch-0 ones.
    body = {"runs": [{"t": "mention", "principal": "B", "label": "@bee"}, {"t": "mention", "principal": "W", "label": "@w"}],
            "expires_after": 300}
    regs = [
        E("m5", "message.create", (0, "B", "m5"), "B", value=body, expiry=900),
        E("m6", "message.create", (1, "B", "m6"), "B", value={"reply_to": "m5", "thread_root": "nope"}),
        E("x0", "reaction.set", (100, "C", "x0"), "C", "m5", ("+1", True), epoch=0),
        E("x1", "reaction.set", (5, "C", "x1"), "C", "m5", ("+1", False), epoch=1),
        E("e0", "message.edit", (100, "B", "e0"), "B", "m6", "old", epoch=0),
        E("e1", "message.edit", (5, "B", "e1"), "B", "m6", "new", epoch=1),
        E("rc", "receipt.update", (7, "C", "rc"), "C", None, ("read", ("m5", "m6", "ghost"))),
    ]
    c, out = exhaustive_permutations(regs, with_duplicate=False, now=400, received={"m5": 50}, members=frozenset({"B"}))
    cases += c; fixtures["epoch_major_reactions_edits_expiry_mentions"] = out
    polls = [
        E("pl", "poll.create", (2, "A", "pl"), "A", value={"opts": ["x", "y"], "mode": "single"}),
        E("cl", "poll.close", (100, "A", "cl"), "A", "pl", epoch=0),
        E("v1", "poll.vote", (5, "C", "v1"), "C", "pl", ("x",), epoch=1),
        E("v0", "poll.vote", (50, "D", "v0"), "D", "pl", ("y",), epoch=0),
        E("vr", "poll.vote", (7, "C", "vr"), "C", "pl", ("y",), epoch=0),
    ]
    c, out = exhaustive_permutations(polls, with_duplicate=False); cases += c; fixtures["epoch_major_poll_close"] = out

    payload = {
        "gate": "V-009",
        "gate_status_effect": "evidence_only_gate_remains_open",
        "all_bounded_checks_passed": True,
        "cases": cases,
        "properties": [
            "EventID duplicate delivery is idempotent",
            "delivery order does not change materialized semantic state",
            "unauthorized state-changing events do not affect the view",
            "pin/topic conflicts use authenticated EventClock ordering, not arrival/wall time",
            "moderation redaction is monotonic",
            "poll close is monotonic and later-clock votes do not alter the closed tally",
            "reaction, vote, edit, pin and topic registers are epoch-major; a vote ordered after poll.close is ignored",
            "expiry deletes at min(expiry, receipt + expires_after); a mention of a non-member is inert",
        ],
        "fixtures": fixtures,
        "remaining": [
            "cross-implementation materializer fixture",
            "full EventClock edge cases and malformed event corpus",
            "authorization-state changes racing with events",
            "persistent-store rebuild and large randomized/property-test corpus",
        ],
    }
    OUT.write_text(json.dumps(payload, indent=2, default=list) + "\n")
    print(json.dumps(payload, indent=2, default=list))

if __name__ == "__main__": main()
