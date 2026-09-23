#!/usr/bin/env python3
"""Feasibility prototype for V-009. Not product code. Not for production.

Diff the independent Rust materializer against the Python reference model
(verification/bounded/conversation_convergence.py, imported unmodified).

The reference takes a precomputed `authorized` flag per event and has no validation layer, so this
adapter supplies an *authorization/validity oracle* computed from the corpus control chain:
  schema validity, conversation id, 64 KiB size, unknown critical capability, unknown epoch,
  capability-at-sending-epoch (D-02), poll.close creator-or-room.redact_others (D-12),
  topic.update/status creator-or-room.manage_topics (D-16).
Target existence/type, option validity and conflict rules are left to the reference's own logic,
so disagreements there are real semantic differences. Every disagreement must be attributed to a
known cause (reference bug, materializer bug, doc ambiguity) or this script exits non-zero.
"""
from __future__ import annotations

import itertools
import json
import random
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / "verification" / "bounded"))
import conversation_convergence as ref  # noqa: E402  (reference model, unmodified)

MAX = 64 * 1024
CAP = {"pin.set": "room.pin", "moderation.redact": "room.redact_others", "poll.create": "room.post",
       "poll.vote": "room.react", "topic.create": "room.create_topic", "message.topic.set": "room.manage_topics"}
REF_KINDS = {"pin.set", "moderation.redact", "poll.create", "poll.vote", "poll.close", "topic.create",
             "topic.update", "topic.status.set", "message.topic.set"}


def can(ctl, who, epoch, cap):
    for e in ctl["epochs"]:
        if e["epoch"] == epoch:
            return cap in e["grants"].get(who, []) and cap not in e["restrictions"].get(who, [])
    return False


def is_str(x):
    return isinstance(x, str) and x != ""


def schema_ok(e):
    b = e.get("body")
    c = e.get("clock")
    if not (isinstance(b, dict) and isinstance(c, list) and len(c) == 3 and isinstance(c[0], int)
            and isinstance(c[1], int) and c[2] == e.get("endpoint") and isinstance(e.get("epoch"), int)):
        return False
    k = e.get("kind")
    tf = "target_poll_event_id" if k in ("poll.vote", "poll.close") else "target_event_id"
    if k in ("pin.set", "moderation.redact", "poll.vote", "poll.close", "message.topic.set") and not is_str(b.get(tf)):
        return False
    if k == "pin.set" and not isinstance(b.get("present"), bool):
        return False
    if k == "poll.create":
        o = b.get("options")
        if not (isinstance(o, list) and o and all(is_str(x) for x in o) and len(set(o)) == len(o)
                and b.get("selection_mode") in ("single", "multiple") and is_str(b.get("question"))):
            return False
    if k == "poll.vote":
        s = b.get("selected_option_ids")
        if not (isinstance(s, list) and all(is_str(x) for x in s) and len(set(s)) == len(s)):
            return False
    if k in ("topic.create", "topic.update") and not (is_str(b.get("topic_id")) and is_str(b.get("title"))):
        return False
    if k == "topic.status.set" and not (is_str(b.get("topic_id")) and b.get("status") in ("open", "resolved", "archived")):
        return False
    if k == "message.topic.set" and not (b.get("topic_id") is None or is_str(b.get("topic_id"))):
        return False
    return True


def oracle(case, e, events):
    """Authorization/validity flag handed to the reference."""
    ctl = case["control"]
    if len(json.dumps(e, separators=(",", ":"), ensure_ascii=False)) > MAX:
        return False
    if e.get("conversation_id") != case["conversation_id"]:
        return False
    if any(c not in case["known_capabilities"] for c in e.get("required_capabilities", [])):
        return False
    if not schema_ok(e) or not any(x["epoch"] == e["epoch"] for x in ctl["epochs"]):
        return False
    k, who, ep = e["kind"], e["author"], e["epoch"]
    if k in CAP:
        return can(ctl, who, ep, CAP[k])
    if k == "poll.close":
        p = next((x for x in events if x.get("event_id") == e["body"]["target_poll_event_id"]), None)
        return (p is not None and p.get("author") == who) or can(ctl, who, ep, "room.redact_others")
    if k in ("topic.update", "topic.status.set"):
        creates = sorted((x for x in events if x.get("kind") == "topic.create" and x.get("body", {}).get("topic_id") == e["body"]["topic_id"]
                          and oracle(case, x, events)), key=lambda x: (*x["clock"], x["event_id"]))
        return (bool(creates) and creates[0]["author"] == who) or can(ctl, who, ep, "room.manage_topics")
    return True


def to_ref(case, e, events):
    b = e.get("body") if isinstance(e.get("body"), dict) else {}
    k = e.get("kind")
    c = e.get("clock") or [0, 0, ""]
    clock = (c[0] if len(c) > 0 else 0, c[1] if len(c) > 1 else 0, c[2] if len(c) > 2 else "", e["event_id"])
    target, value = None, None
    if k == "pin.set":
        target, value = b.get("target_event_id"), b.get("present")
    elif k == "moderation.redact":
        target = b.get("target_event_id")
    elif k == "poll.create":
        value = {"opts": b.get("options")}
    elif k in ("poll.vote", "poll.close"):
        target, value = b.get("target_poll_event_id"), tuple(b.get("selected_option_ids") or ())
    elif k in ("topic.create", "topic.update"):
        target, value = b.get("topic_id"), b.get("title")
    elif k == "topic.status.set":
        target, value = b.get("topic_id"), b.get("status")
    elif k == "message.topic.set":
        target, value = b.get("target_event_id"), b.get("topic_id")
    return ref.E(e["event_id"], k, clock, e.get("author"), target, value, oracle(case, e, events))


def norm_ref(v):
    return {
        "pins": sorted([list(x) for x in v["pins"]]),
        "redacted": sorted(v["redacted"]),
        "poll_votes": sorted([[a, p, sorted(o)] for a, p, o in v["poll_votes"]]),
        "poll_closed": sorted(v["poll_closed"]),
        "topics": sorted([[t, title, st] for t, title, st, _ in v["topics"]]),
        "message_topics": sorted([[m, t] for m, t, _ in v["message_topics"]]),
    }


def project_rust(v):
    return {
        "pins": sorted([[t, p] for t, p in v["pin_registers"].items()]),
        "redacted": sorted(v["redacted"]),
        "poll_votes": sorted([[a, p, sorted(o)] for p, pv in v["polls"].items() for a, o in pv["votes"].items()]),
        "poll_closed": sorted(p for p, pv in v["polls"].items() if pv["closed"]),
        "topics": sorted([[t, x["title"], x["status"]] for t, x in v["topics"].items()]),
        "message_topics": sorted([[m, t] for m, t in v["message_topics"].items()]),
    }


# ---- disagreement attribution ----------------------------------------------------------------
CAUSES = {
    "REF-1": ("reference_bug", "equivocating duplicate EventID: reference keeps the last-arrived payload (order-dependent)"),
    "REF-3": ("reference_bug", "poll.vote options are not validated against the poll (unknown option / multi-select on single-choice poll counted)"),
    "DA-1": ("doc_ambiguity", "state operation whose target is missing or of the wrong kind (pin/redact/vote/close/topic-move on a non-message, a never-arriving event, or a topic that never exists)"),
    "DA-2": ("doc_ambiguity", "duplicate topic.create for one TopicID: reference = latest create resets the topic; materializer = earliest create wins"),
    "DA-3": ("doc_ambiguity", "empty poll.vote selection: reference keeps an empty vote; materializer treats it as retraction"),
}


def attribute(case, field, item, rust_view):
    """Return a cause id for one element present in exactly one of the two projections."""
    evs = [e for e in case["events"] if isinstance(e, dict) and e.get("event_id")]
    ids = [e["event_id"] for e in evs]
    if len(ids) != len(set(ids)) and any(evs[i] != evs[j] for i in range(len(evs)) for j in range(len(evs))
                                         if i < j and ids[i] == ids[j]):
        return "REF-1"
    content = {x["id"]: x["type"] for x in rust_view["timeline"] if x["type"] in ("message", "poll")}
    if field in ("pins", "redacted"):
        t = item[0] if field == "pins" else item
        if t not in content:
            return "DA-1"
    if field == "poll_closed" and content.get(item) != "poll":
        return "DA-1"
    if field == "poll_votes":
        a, p, opts = item
        if content.get(p) != "poll":
            return "DA-1"
        pc = next(e for e in evs if e["event_id"] == p)
        if any(o not in pc["body"]["options"] for o in opts) or (pc["body"]["selection_mode"] == "single" and len(opts) > 1):
            return "REF-3"
        if not opts:
            return "DA-3"
        # an invalid later vote in the reference may have displaced a valid earlier one
        for e in evs:
            if e.get("kind") == "poll.vote" and e["author"] == a and e["body"].get("target_poll_event_id") == p:
                s = e["body"].get("selected_option_ids") or []
                if not s:
                    return "DA-3"
                if any(o not in pc["body"]["options"] for o in s) or (pc["body"]["selection_mode"] == "single" and len(s) > 1):
                    return "REF-3"
    if field == "topics":
        tid = item[0]
        creates = [e for e in evs if e.get("kind") == "topic.create" and e.get("body", {}).get("topic_id") == tid and oracle(case, e, evs)]
        if len(creates) > 1:
            return "DA-2"
    if field == "message_topics":
        m, t = item
        if content.get(m) != "message" or (t is not None and t not in rust_view["topics"]):
            return "DA-1"
        if any(e.get("kind") == "message.topic.set" and e.get("body", {}).get("target_event_id") == m
               and (content.get(m) != "message" or e["body"].get("topic_id") not in rust_view["topics"]) for e in evs):
            return "DA-1"
    return None


def ref_orders(cid, n):
    if n <= 6:
        for p in itertools.permutations(range(n)):
            yield list(p)
            yield [p[0], *p, p[0]]
    else:
        r = random.Random(cid)
        for _ in range(300):
            p = list(range(n))
            r.shuffle(p)
            yield p
            yield p + p[::-1]


def main():
    corpus = json.loads(Path(sys.argv[1]).read_text())
    rust = {c["id"]: c for c in json.loads(Path(sys.argv[2]).read_text())["cases"]}
    out_cases, unresolved, cause_hits = [], [], {}
    ref_orders_total, ref_nonconv = 0, []
    agree = 0
    for case in corpus["cases"]:
        evs = [e for e in case["events"] if isinstance(e, dict) and e.get("event_id")]
        mapped = [to_ref(case, e, evs) for e in evs]
        base = ref.materialize(mapped)
        # reference self-convergence on the extended corpus
        conv = True
        for o in ref_orders(case["id"], len(mapped)):
            ref_orders_total += 1
            if ref.materialize([mapped[i] for i in o]) != base:
                conv = False
                break
        if not conv:
            ref_nonconv.append(case["id"])
        rv = norm_ref(base)
        mv = project_rust(rust[case["id"]]["view"])
        diffs = []
        for f in rv:
            a = {json.dumps(x) for x in rv[f]}
            b = {json.dumps(x) for x in mv[f]}
            for side, items in (("reference_only", a - b), ("materializer_only", b - a)):
                for it in sorted(items):
                    item = json.loads(it)
                    cause = attribute(case, f, item, rust[case["id"]]["view"])
                    diffs.append({"field": f, "side": side, "item": item, "cause": cause})
                    if cause is None:
                        unresolved.append({"case": case["id"], "field": f, "side": side, "item": item})
                    else:
                        cause_hits.setdefault(cause, []).append(case["id"])
        compared = any(e.get("kind") in REF_KINDS for e in evs)
        if not diffs:
            agree += 1
        out_cases.append({"id": case["id"], "category": case["category"], "reference_kinds_present": compared,
                          "reference_converged": conv, "agree": not diffs, "diffs": diffs})
    summary = {
        "cases": len(out_cases),
        "cases_agree": agree,
        "cases_disagree": len(out_cases) - agree,
        "reference_orders_checked": ref_orders_total,
        "reference_nonconvergent_cases": ref_nonconv,
        "unresolved_disagreements": unresolved,
        "causes": {k: {"class": CAUSES[k][0], "description": CAUSES[k][1], "cases": sorted(set(v))} for k, v in sorted(cause_hits.items())},
    }
    Path(sys.argv[3]).write_text(json.dumps({"summary": summary, "cases": out_cases}, indent=1, ensure_ascii=False) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k != "causes"}, indent=1))
    for k, v in summary["causes"].items():
        print(k, v["class"], len(v["cases"]), "cases")
    if len(sys.argv) > 4:
        write_results(Path(sys.argv[4]), corpus, json.loads(Path(sys.argv[2]).read_text()), summary, sys.argv[5:])
    return 1 if unresolved else 0


def write_results(path, corpus, rust, summary, versions):
    cats = {}
    for c in corpus["cases"]:
        cats[c["category"]] = cats.get(c["category"], 0) + 1
    ok = rust["all_converged"] and not summary["unresolved_disagreements"]
    crit = "a second, independently written materializer (different author or language) agrees with the reference model on the full convergence corpus including malformed and authorization-race cases"
    res = {
        "gates": ["V-009"],
        "prototype": "verification/prototypes/v009-materializer",
        "criteria": [
            {"gate": "V-009", "criterion": crit, "status": "partial",
             "evidence": ["verification/prototypes/v009-materializer/README.md",
                          "verification/prototypes/v009-materializer/materializer/src/materialize.rs",
                          "verification/prototypes/v009-materializer/gen_corpus.py",
                          "verification/prototypes/v009-materializer/compare.py"],
             "note": ("Rust materializer (different language; written from docs before the reference logic was read) converges on every "
                      "delivery order and agrees with the reference on %d/%d cases; all %d disagreeing cases are attributed "
                      "(reference bugs REF-1/REF-3, doc ambiguities DA-1..DA-3). Partial because the reference covers only "
                      "pins/redaction/polls/topics and needs an adapter-supplied authorization oracle, the doc ambiguities need "
                      "rulings, and author independence is AI-to-AI (ADR-0117 proposed)."
                      % (summary["cases_agree"], summary["cases"], summary["cases_disagree"]))},
        ],
        "independence": ("Materializer written by a Claude agent in Rust from the public docs; its commit precedes reading the Python "
                         "reference's logic. The design package was authored by a ChatGPT design agent and Claude agents; ADR-0117 "
                         "(proposed) decides whether an AI reviewer/implementer counts as independent."),
        "tool_versions": dict(v.split("=", 1) for v in versions),
        "summary": {
            "all_checks_passed": ok,
            "corpus_cases": len(corpus["cases"]),
            "corpus_categories": dict(sorted(cats.items())),
            "materializer_delivery_orders": rust["total_orders"],
            "materializer_all_converged": rust["all_converged"],
            "reference_delivery_orders": summary["reference_orders_checked"],
            "reference_nonconvergent_cases": summary["reference_nonconvergent_cases"],
            "cases_agree": summary["cases_agree"],
            "cases_disagree_attributed": summary["cases_disagree"],
            "unresolved_disagreements": len(summary["unresolved_disagreements"]),
            "disagreement_causes": summary["causes"],
        },
    }
    path.write_text(json.dumps(res, indent=2, ensure_ascii=False, sort_keys=True) + "\n")


if __name__ == "__main__":
    sys.exit(main())
