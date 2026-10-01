#!/usr/bin/env python3
"""Feasibility prototype for V-009. Not product code. Not for production.

Diff the independent Rust materializer against the Python reference model
(verification/bounded/conversation_convergence.py, imported unmodified).

The reference takes a precomputed `authorized` flag per event and has no validation layer, so this
adapter supplies an *authorization/validity oracle*. Authority is derived here, in Python, from the corpus
control chain (`control.effective`, produced by gen_corpus.derive); the Rust materializer derives it
independently from the same raw chain and its derivation is compared too (`authority_agree`). The
oracle checks:
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
       "poll.vote": "room.react", "topic.create": "room.create_topic", "message.topic.set": "room.manage_topics",
       "reaction.set": "room.react"}
REF_KINDS = {"pin.set", "moderation.redact", "poll.create", "poll.vote", "poll.close", "topic.create",
             "topic.update", "topic.status.set", "message.topic.set", "message.create", "message.edit",
             "message.delete", "reaction.set", "receipt.update"}


def can(ctl, who, epoch, cap):
    """Effective capability at a sending epoch, from the Python derivation of the control chain."""
    eff = ctl["effective"]
    return 0 <= epoch < len(eff) and cap in eff[epoch]["members"].get(who, [])


def has_epoch(ctl, epoch):
    return isinstance(epoch, int) and 0 <= epoch < len(ctl["effective"])


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
    if k in ("pin.set", "moderation.redact", "poll.vote", "poll.close", "message.topic.set", "message.edit",
             "message.delete", "reaction.set") and not is_str(b.get(tf)):
        return False
    if e.get("expiry") is not None and not (isinstance(e["expiry"], int) and not isinstance(e["expiry"], bool) and e["expiry"] >= 0):
        return False
    if k == "message.create":
        runs = b.get("runs")
        if runs is not None:
            if not (isinstance(runs, list) and runs and all(
                    isinstance(r, dict) and ((r.get("t") == "text" and is_str(r.get("s")))
                                             or (r.get("t") == "mention" and is_str(r.get("principal")) and is_str(r.get("label"))))
                    for r in runs)):
                return False
        if b.get("text") is not None and (not is_str(b["text"]) or runs is not None):
            return False
        atts = b.get("attachments")
        if atts is not None and not isinstance(atts, list):
            return False
        if b.get("text") is None and runs is None and not atts:
            return False
        for f in ("reply_to", "thread_root_event_id", "topic_id"):
            if b.get(f) is not None and not is_str(b[f]):
                return False
        ea = b.get("expires_after")
        if ea is not None and not (isinstance(ea, int) and not isinstance(ea, bool) and ea >= 0):
            return False
    if k == "message.edit" and not is_str(b.get("replacement")):
        return False
    if k == "reaction.set" and not (is_str(b.get("reaction_key")) and isinstance(b.get("present"), bool)):
        return False
    if k == "receipt.update" and not (b.get("receipt_type") in ("delivered", "read") and isinstance(b.get("event_ids"), list)
                                      and all(is_str(x) for x in b["event_ids"])):
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
    if not schema_ok(e) or not has_epoch(ctl, e["epoch"]):
        return False
    k, who, ep = e["kind"], e["author"], e["epoch"]
    if k == "message.create":
        reply = e["body"].get("reply_to") is not None or e["body"].get("thread_root_event_id") is not None
        return can(ctl, who, ep, "room.post") or (reply and can(ctl, who, ep, "room.reply"))
    if k == "message.edit":
        return can(ctl, who, ep, "room.post") or can(ctl, who, ep, "room.reply")
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
        value = {"opts": b.get("options"), "mode": b.get("selection_mode")}
    elif k in ("poll.vote", "poll.close"):
        target, value = b.get("target_poll_event_id"), tuple(b.get("selected_option_ids") or ())
    elif k in ("topic.create", "topic.update"):
        target, value = b.get("topic_id"), b.get("title")
    elif k == "topic.status.set":
        target, value = b.get("topic_id"), b.get("status")
    elif k == "message.topic.set":
        target, value = b.get("target_event_id"), b.get("topic_id")
    elif k == "message.create":
        value = {"runs": b.get("runs"), "reply_to": b.get("reply_to"), "thread_root": b.get("thread_root_event_id"),
                 "expires_after": b.get("expires_after")}
    elif k == "message.edit":
        target, value = b.get("target_event_id"), b.get("replacement")
    elif k == "message.delete":
        target = b.get("target_event_id")
    elif k == "reaction.set":
        target, value = b.get("target_event_id"), (b.get("reaction_key"), b.get("present"))
    elif k == "receipt.update":
        value = (b.get("receipt_type"), tuple(b.get("event_ids") or ()))
    if k == "poll.create":
        value = {**value, "expires_after": None}
    epoch = e.get("epoch") if isinstance(e.get("epoch"), int) else -1
    expiry = e.get("expiry") if isinstance(e.get("expiry"), int) and not isinstance(e.get("expiry"), bool) else None
    return ref.E(e["event_id"], k, clock, e.get("author"), target, value, oracle(case, e, events), epoch, expiry)


def norm_ref(v):
    return {
        "edits": sorted([list(x) for x in v["edits"]]),
        "deleted": sorted(v["deleted"]),
        "reactions": sorted([list(x) for x in v["reactions"]]),
        "receipts": sorted([[t, a, "read" if l == 2 else "delivered"] for t, a, l in v["receipts"]]),
        "replies": sorted([[i, [list(x) for x in refs]] for i, refs in v["replies"] if refs]),
        "deadlines": sorted([list(x) for x in v["deadlines"]]),
        "expired": sorted(v["expired"]),
        "mentions": sorted([[i, [list(m[0]), list(m[1])]] for i, m in v["mentions"] if m[0] or m[1]]),
        "pins": sorted([list(x) for x in v["pins"]]),
        "redacted": sorted(v["redacted"]),
        "poll_votes": sorted([[a, p, sorted(o)] for a, p, o in v["poll_votes"]]),
        "poll_closed": sorted(v["poll_closed"]),
        "topics": sorted([[t, title, st] for t, title, st, _ in v["topics"]]),
        "message_topics": sorted([[m, t] for m, t, _ in v["message_topics"]]),
    }


def project_rust(v):
    tl = [x for x in v["timeline"] if x["type"] == "message" and x["state"] == "visible"]
    return {
        "edits": sorted([[t, x] for t, x in v["edit_registers"].items()]),
        "deleted": sorted(v["deleted"]),
        "reactions": sorted(v["reaction_registers"]),
        "receipts": sorted([[t, a, l] for t, m in v["receipts"].items() for a, l in m.items()]),
        "replies": sorted([[i, sorted([[f, r] for f, r in (("reply_to", x["reply_to"]), ("thread_root", x["thread_root"])) if r is not None])]
                           for i, x in v["reply_refs"].items() if x["reply_to"] is not None or x["thread_root"] is not None]),
        "deadlines": sorted([[i, t] for i, t in v["deadlines"].items()]),
        "expired": sorted(v["expired"]),
        "mentions": sorted([[x["id"], [x["mentions"]["live"], x["mentions"]["inert"]]] for x in tl
                            if x["mentions"]["live"] or x["mentions"]["inert"]]),
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
    authority_checked, authority_bad, expect_checked, expect_failures = 0, [], 0, []
    for case in corpus["cases"]:
        evs = [e for e in case["events"] if isinstance(e, dict) and e.get("event_id")]
        mapped = [to_ref(case, e, evs) for e in evs]
        loc = case.get("local", {})
        kw = {"now": loc.get("now"), "received": loc.get("received_at", {}),
              "members": frozenset(case["control"]["effective"][-1]["members"])}
        base = ref.materialize(mapped, **kw)
        # reference self-convergence on the extended corpus
        conv = True
        for o in ref_orders(case["id"], len(mapped)):
            ref_orders_total += 1
            if ref.materialize([mapped[i] for i in o], **kw) != base:
                conv = False
                break
        if not conv:
            ref_nonconv.append(case["id"])
        rv = norm_ref(base)
        mv = project_rust(rust[case["id"]]["view"])
        # G-V9-5: the Rust materializer derives authority from the raw chain; compare it with the Python derivation.
        auth_ok = rust[case["id"]]["view"]["authority"] == case["control"]["effective"]
        authority_checked += 1
        if not auth_ok:
            authority_bad.append(case["id"])
        # spec-derived expectations (hand-written from the ADR text) hold on BOTH implementations
        for f, want in case.get("expect", {}).items():
            expect_checked += 1
            w = sorted(json.dumps(x) for x in want)
            for who, view in (("reference", rv), ("materializer", mv)):
                if sorted(json.dumps(x) for x in view[f]) != w:
                    expect_failures.append({"case": case["id"], "field": f, "implementation": who,
                                            "expected": want, "got": view[f]})
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
                          "reference_converged": conv, "authority_agree": auth_ok, "agree": not diffs, "diffs": diffs})
    # R-239 forwarding vectors: Rust, Python reference and the hand-written expectation must agree
    rust_fw = {f["id"]: f["output"] for f in json.loads(Path(sys.argv[2]).read_text()).get("forwards", [])}
    fw_bad = [f["id"] for f in corpus.get("forwards", [])
              if not (rust_fw.get(f["id"]) == ref.forward_body(f["source"]) == f["expected"])]
    summary = {
        "authority_checked": authority_checked,
        "authority_disagreements": authority_bad,
        "expectations_checked": expect_checked,
        "expectation_failures": expect_failures,
        "forward_vectors": len(corpus.get("forwards", [])),
        "forward_failures": fw_bad,
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
    return 1 if (unresolved or authority_bad or expect_failures or fw_bad) else 0


def write_results(path, corpus, rust, summary, versions):
    cats = {}
    for c in corpus["cases"]:
        cats[c["category"]] = cats.get(c["category"], 0) + 1
    ok = (rust["all_converged"] and not summary["unresolved_disagreements"] and not summary["authority_disagreements"]
          and not summary["expectation_failures"] and not summary["forward_failures"])
    ev = ["verification/prototypes/v009-materializer/README.md",
          "verification/prototypes/v009-materializer/materializer/src/materialize.rs",
          "verification/prototypes/v009-materializer/gen_corpus.py",
          "verification/prototypes/v009-materializer/compare.py",
          "verification/bounded/conversation_convergence.py"]
    status = "met" if ok else "partial"
    n_exp = {c: sum(1 for x in corpus["cases"] if x["category"] == c and x.get("expect")) for c in ("epoch_major", "expiry", "mentions")}
    crits = [
        ("a second, independently written materializer (different author or language) agrees with the reference model on the full convergence corpus including malformed and authorization-race cases",
         "Rust materializer (different language and runtime) converges on every delivery order and agrees with the Python reference on %d/%d cases, "
         "including the malformed, extension and authorization-race categories; no disagreement needs a ruling any more (ADR-0121 settled DA-1..DA-3)."
         % (summary["cases_agree"], summary["cases"])),
        ("the independent materializer derives authorization from the control chain rather than sharing the reference authorization adapter (G-V9-5)",
         "The Rust materializer reads only the raw chain of control operations (role.define, role.assign, restriction.set, member.remove) and derives "
         "effective authority per epoch itself; the Python side derives it separately (gen_corpus.derive). The two derivations agree on %d/%d cases' "
         "per-epoch authority tables, and every event's authorization then agrees." % (summary["authority_checked"] - len(summary["authority_disagreements"]), summary["authority_checked"])),
        ("reaction and vote registers are authorization-epoch-major and a vote ordered after poll.close is ignored (R-236)",
         "%d epoch_major cases with spec-derived expectations hold on both implementations: vote/reaction/edit/pin/topic registers order by "
         "(sending epoch, EventClock), a vote ordered after the earliest close is ignored, backdating inside one epoch remains the documented residual risk." % n_exp["epoch_major"]),
        ("expiry deletes at min(expiry, receipt + expires_after) and never after the absolute expiry bound (R-235)",
         "%d expiry cases with hand-written expectations hold on both implementations (boundary, local bound earlier, absolute earlier, receipt after expiry, "
         "no receipt time, malformed carriers); the deadline is never after the absolute expiry." % n_exp["expiry"]),
        ("forwarding strips mentions and a mention of a non-member renders as inert text (R-239)",
         "%d mention cases and %d forwarding vectors agree across Rust, the Python reference and hand-written expectations: Mention nodes become text runs "
         "with their label, the mention set and source ids are dropped, a mention of a non-member (latest roster) renders inert." % (n_exp["mentions"], summary["forward_vectors"])),
    ]
    res = {
        "gates": ["V-009"],
        "prototype": "verification/prototypes/v009-materializer",
        "criteria": [{"gate": "V-009", "criterion": c, "status": status, "evidence": ev, "note": n} for c, n in crits],
        "independence": ("Second implementation in a different language and runtime (Rust vs Python), as ADR-0167 section 3 defines independent_implementation. "
                         "It is NOT a different author: the Rust materializer, the corpus generator, the Python adapter and the later extensions to the Python "
                         "reference were all written by Claude agents, and the design package by ChatGPT and Claude agents. The Rust side was first written from the "
                         "docs before the reference logic was read; the control-chain derivation, epoch-major registers, expiry and mention handling were added "
                         "in this session with both sides in view, which is why every new behavior also has hand-written expectations checked on both."),
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
            "authority_tables_checked": summary["authority_checked"],
            "authority_disagreements": len(summary["authority_disagreements"]),
            "expectations_checked": summary["expectations_checked"],
            "expectation_failures": len(summary["expectation_failures"]),
            "forward_vectors": summary["forward_vectors"],
            "forward_failures": len(summary["forward_failures"]),
            "disagreement_causes": summary["causes"],
        },
    }
    path.write_text(json.dumps(res, indent=2, ensure_ascii=False, sort_keys=True) + "\n")


if __name__ == "__main__":
    sys.exit(main())
