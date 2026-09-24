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


def materialize(delivery):
    # EventID de-duplication, then deterministic EventClock order. Authorization
    # is authenticated input to materialization and wall time is irrelevant.
    # One EventID with two different payloads is equivocation: every version is
    # rejected, independent of arrival order (ADR-0121 rule 5).
    by_id, equivocal = {}, set()
    for e in delivery:
        if e.eid in by_id and by_id[e.eid] != e:
            equivocal.add(e.eid)
        by_id.setdefault(e.eid, e)
    events = sorted((e for i, e in by_id.items() if i not in equivocal), key=lambda e: e.clock)
    pins = {}
    redacted = set()
    poll_create = {}
    poll_votes = {}
    poll_closed_at = {}
    topics = {}
    message_topics = {}
    # ADR-0121 rule 6: a state operation needs a target of the right kind. A missing
    # target is held (never applied) until it arrives, so the result is order-independent.
    content = {e.eid: e.kind for e in events if e.authorized and e.kind in ("message.create", "poll.create")}
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
        elif e.kind == "poll.create":
            poll_create[e.eid] = e.value
        elif e.kind == "poll.close":
            if e.target in poll_create and e.target not in poll_closed_at:
                poll_closed_at[e.target] = e.clock
        elif e.kind == "poll.vote":
            if e.target not in poll_create:
                continue
            close_clock = poll_closed_at.get(e.target)
            if close_clock is not None and e.clock > close_clock:
                continue
            # ADR-0121 rule 8: options are validated; an empty selection retracts.
            spec = poll_create[e.target] or {}
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
            topics[e.target] = {"title": e.value, "status": "open", "clock": e.clock}
        elif e.kind == "topic.update":
            if e.target in topics and e.clock >= topics[e.target]["clock"]:
                topics[e.target]["title"] = e.value
                topics[e.target]["clock"] = e.clock
        elif e.kind == "topic.status.set":
            if e.target in topics and e.clock >= topics[e.target]["clock"]:
                topics[e.target]["status"] = e.value
                topics[e.target]["clock"] = e.clock
        elif e.kind == "message.topic.set":
            old = message_topics.get(e.target)
            if old is None or e.clock >= old[1]:
                message_topics[e.target] = (e.value, e.clock)
    return {
        "pins": tuple(sorted(pins.items())),
        "redacted": tuple(sorted(redacted)),
        "poll_votes": tuple(sorted((a, p, tuple(v)) for (a,p),v in poll_votes.items())),
        "poll_closed": tuple(sorted(poll_closed_at)),
        "topics": tuple(sorted((k, v["title"], v["status"], v["clock"]) for k,v in topics.items())),
        "message_topics": tuple(sorted((k, v[0], v[1]) for k,v in message_topics.items())),
    }


def exhaustive_permutations(events, with_duplicate=True):
    expected = materialize(events)
    count = 0
    for perm in itertools.permutations(events):
        count += 1
        assert materialize(perm) == expected
        if with_duplicate:
            # Duplicate one event at both ends; EventID de-dup must preserve view.
            d = perm[0]
            assert materialize((d,) + perm + (d,)) == expected
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
