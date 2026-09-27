#!/usr/bin/env python3
"""V-003 feasibility model for ADR-0142/0143 sealed-delivery session state.

Not product code. Not cryptographic evidence. This bounded model checks state-machine semantics around
per-device/per-route delivery keys, replay reservation, crash/retry, route migration, expiry, and the
mandatory no-clear-sender fallback. It intentionally models outer HPKE as a boolean precondition; V-001
owns cryptographic suite/KAT evidence.
"""
from __future__ import annotations

import copy
import hashlib
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
VECTORS = ROOT / "verification/vectors/sealed-delivery-session-vectors.json"


def h(obj) -> str:
    return hashlib.sha256(json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


@dataclass(frozen=True)
class Key:
    key_id: str
    device: str
    route: str
    expires: int
    send_current: bool = True
    receive_overlap: bool = True


@dataclass(frozen=True)
class Wrapper:
    event_id: str
    route: str
    key_id: str
    delivery_id: str
    expires: int
    inner_auth: bool = True
    sealed: bool = True

    @property
    def digest(self) -> str:
        return h(self.__dict__)


@dataclass
class Reservation:
    digest: str
    stage: str  # RESERVED | COMPLETE | REJECTED


@dataclass
class World:
    now: int = 1000
    relationship_active: bool = True
    device_active: bool = True
    keys: dict[str, Key] = field(default_factory=dict)
    current_by_route: dict[str, str] = field(default_factory=dict)
    replay: dict[tuple[str, str], Reservation] = field(default_factory=dict)
    durable_events: set[str] = field(default_factory=set)
    released_events: list[str] = field(default_factory=list)
    acked: set[tuple[str, str]] = field(default_factory=set)
    seq: int = 0
    log: list[dict] = field(default_factory=list)
    mutation_drop_reserved_retry: bool = False
    mutation_clear_fallback: bool = False
    mutation_route_agnostic: bool = False
    mutation_outer_authenticates_sender: bool = False
    mutation_reuse_id_cross_route: bool = False

    def add_key(self, key_id: str, route: str, *, expires: int = 2000, current: bool = True, overlap: bool = True):
        self.keys[key_id] = Key(key_id, "b1", route, expires, current, overlap)
        if current:
            old = self.current_by_route.get(route)
            if old and old in self.keys:
                k = self.keys[old]
                self.keys[old] = Key(k.key_id, k.device, k.route, k.expires, False, k.receive_overlap)
            self.current_by_route[route] = key_id
        self.log.append({"op": "add_key", "key": key_id, "route": route})

    def revoke_for_send(self, key_id: str):
        k = self.keys[key_id]
        self.keys[key_id] = Key(k.key_id, k.device, k.route, k.expires, False, k.receive_overlap)
        if self.current_by_route.get(k.route) == key_id:
            del self.current_by_route[k.route]
        self.log.append({"op": "revoke_for_send", "key": key_id})

    def retire_receive(self, key_id: str):
        k = self.keys[key_id]
        self.keys[key_id] = Key(k.key_id, k.device, k.route, k.expires, k.send_current, False)
        self.log.append({"op": "retire_receive", "key": key_id})

    def seal(self, event_id: str, route: str, *, inner_auth: bool = True, force_id: str | None = None):
        if not self.relationship_active or not self.device_active:
            self.log.append({"op": "seal", "event": event_id, "route": route, "result": "ineligible"})
            return None
        kid = self.current_by_route.get(route)
        k = self.keys.get(kid) if kid else None
        if not k or not k.send_current or self.now >= k.expires:
            if self.mutation_clear_fallback:
                self.seq += 1
                w = Wrapper(event_id, route, "CLEAR", f"clear-{self.seq}", self.now + 100, inner_auth, sealed=False)
                self.log.append({"op": "seal", "event": event_id, "route": route, "result": "clear_fallback", "wrapper": w.__dict__})
                return w
            self.log.append({"op": "seal", "event": event_id, "route": route, "result": "delivery_key_unknown_or_stale"})
            return None
        self.seq += 1
        did = force_id or f"d{self.seq:04d}"
        w = Wrapper(event_id, route, k.key_id, did, min(k.expires, self.now + 300), inner_auth, sealed=True)
        self.log.append({"op": "seal", "event": event_id, "route": route, "result": "sealed", "wrapper": w.__dict__})
        return w

    def reseal_other_route(self, prior: Wrapper, route: str):
        force = prior.delivery_id if self.mutation_reuse_id_cross_route else None
        return self.seal(prior.event_id, route, inner_auth=prior.inner_auth, force_id=force)

    def reserve(self, w: Wrapper, *, persist_ok: bool = True):
        if not w.sealed:
            self.log.append({"op": "reserve", "result": "sealed_delivery_required"})
            return "sealed_delivery_required"
        k = self.keys.get(w.key_id)
        if not k or not k.receive_overlap or self.now >= k.expires:
            self.log.append({"op": "reserve", "result": "delivery_key_unknown_or_stale"})
            return "delivery_key_unknown_or_stale"
        if not self.mutation_route_agnostic and k.route != w.route:
            self.log.append({"op": "reserve", "result": "delivery_route_mismatch"})
            return "delivery_route_mismatch"
        if self.now >= w.expires:
            self.log.append({"op": "reserve", "result": "delivery_expired"})
            return "delivery_expired"
        rk = (w.key_id, w.delivery_id)
        old = self.replay.get(rk)
        if old:
            if old.digest != w.digest:
                self.log.append({"op": "reserve", "result": "delivery_replay_collision"})
                return "delivery_replay_collision"
            if old.stage == "COMPLETE":
                self.acked.add(rk)  # idempotent transport ack is safe; no inner effect re-runs.
                self.log.append({"op": "reserve", "result": "delivery_replay_complete_ack"})
                return "delivery_replay_complete_ack"
            if old.stage == "REJECTED":
                self.log.append({"op": "reserve", "result": "delivery_replay_rejected"})
                return "delivery_replay_rejected"
            if self.mutation_drop_reserved_retry:
                self.log.append({"op": "reserve", "result": "delivery_replay_dropped_reserved"})
                return "delivery_replay_dropped_reserved"
            self.log.append({"op": "reserve", "result": "resume_reserved"})
            return "resume_reserved"
        if not persist_ok:
            self.log.append({"op": "reserve", "result": "persistence_failed"})
            return "persistence_failed"
        self.replay[rk] = Reservation(w.digest, "RESERVED")
        self.log.append({"op": "reserve", "result": "reserved"})
        return "reserved"

    def complete_inner(self, w: Wrapper, *, persist_ok: bool = True):
        rk = (w.key_id, w.delivery_id)
        r = self.replay.get(rk)
        if not r or r.digest != w.digest or r.stage != "RESERVED":
            self.log.append({"op": "complete_inner", "result": "not_reserved"})
            return "not_reserved"
        if not w.inner_auth and not self.mutation_outer_authenticates_sender:
            r.stage = "REJECTED"
            self.log.append({"op": "complete_inner", "result": "inner_auth_failed"})
            return "inner_auth_failed"
        if not persist_ok:
            self.log.append({"op": "complete_inner", "result": "persistence_failed_reserved"})
            return "persistence_failed_reserved"
        # K2 + ADR-0143: inner dedupe/session state and replay COMPLETE become durable before release/ack.
        self.durable_events.add(w.event_id)
        r.stage = "COMPLETE"
        self.log.append({"op": "complete_inner", "result": "complete"})
        return "complete"

    def release_and_ack(self, w: Wrapper):
        rk = (w.key_id, w.delivery_id)
        r = self.replay.get(rk)
        if not r or r.stage != "COMPLETE" or w.event_id not in self.durable_events:
            self.log.append({"op": "release", "result": "not_complete"})
            return "not_complete"
        if w.event_id not in self.released_events:
            self.released_events.append(w.event_id)
        self.acked.add(rk)
        self.log.append({"op": "release", "result": "released_or_deduped"})
        return "released_or_deduped"

    def receive_all(self, w: Wrapper, *, reserve_persist=True, complete_persist=True):
        x = self.reserve(w, persist_ok=reserve_persist)
        if x not in {"reserved", "resume_reserved"}:
            return x
        y = self.complete_inner(w, persist_ok=complete_persist)
        if y != "complete":
            return y
        return self.release_and_ack(w)

    def crash(self):
        # Durable keys/replay/dedupe survive. Transient HPKE/open/application state is intentionally absent.
        self.log.append({"op": "crash", "result": "durable_state_preserved"})

    def invariant_violations(self) -> list[str]:
        bad = []
        if len(self.released_events) != len(set(self.released_events)):
            bad.append("NoDoubleRelease")
        if any(not w for w in self.durable_events):
            bad.append("BadEventId")
        # Every release must have durable authenticated completion somewhere.
        for e in self.released_events:
            if e not in self.durable_events:
                bad.append("ReleaseBeforeDurableInnerAccept")
        return sorted(set(bad))


def base_world(**mut) -> World:
    w = World(**mut)
    w.add_key("kA", "routeA")
    return w


def scenarios() -> list[dict]:
    out = []

    def add(name, fn, expect):
        w = base_world()
        got = fn(w)
        violations = w.invariant_violations()
        ok = got == expect and not violations
        out.append({"id": name, "ok": ok, "expected": expect, "got": got, "violations": violations, "trace": w.log})

    add("normal", lambda w: (lambda x: w.receive_all(x))(w.seal("e1", "routeA")), "released_or_deduped")

    def same_route_retry(w):
        x = w.seal("e1", "routeA")
        a = w.receive_all(x)
        b = w.receive_all(x)
        return {"first": a, "retry": b, "effects": w.released_events}
    add("same_route_exact_retry", same_route_retry,
        {"first": "released_or_deduped", "retry": "delivery_replay_complete_ack", "effects": ["e1"]})

    def crash_reserved(w):
        x = w.seal("e1", "routeA")
        a = w.reserve(x)
        w.crash()
        b = w.receive_all(x)
        return {"reserve": a, "retry": b, "effects": w.released_events}
    add("crash_after_reserve_then_exact_retry", crash_reserved,
        {"reserve": "reserved", "retry": "released_or_deduped", "effects": ["e1"]})

    def crash_complete(w):
        x = w.seal("e1", "routeA")
        a = w.reserve(x); b = w.complete_inner(x)
        w.crash()
        c = w.receive_all(x)
        return {"reserve": a, "complete": b, "retry": c, "effects": w.released_events, "acked": bool(w.acked)}
    add("crash_after_complete_before_release", crash_complete,
        {"reserve": "reserved", "complete": "complete", "retry": "delivery_replay_complete_ack", "effects": [], "acked": True})

    def persistence_fail_reserve(w):
        x = w.seal("e1", "routeA")
        a = w.receive_all(x, reserve_persist=False)
        return {"result": a, "effects": w.released_events, "replay": len(w.replay)}
    add("reserve_persistence_failure_fail_closed", persistence_fail_reserve,
        {"result": "persistence_failed", "effects": [], "replay": 0})

    def persistence_fail_complete(w):
        x = w.seal("e1", "routeA")
        a = w.receive_all(x, complete_persist=False)
        w.crash(); b = w.receive_all(x)
        return {"first": a, "retry": b, "effects": w.released_events}
    add("complete_persistence_failure_resumes", persistence_fail_complete,
        {"first": "persistence_failed_reserved", "retry": "released_or_deduped", "effects": ["e1"]})

    def wrong_route(w):
        x = w.seal("e1", "routeA")
        x2 = Wrapper(x.event_id, "routeB", x.key_id, x.delivery_id, x.expires, x.inner_auth, x.sealed)
        return w.receive_all(x2)
    add("wrong_route_rejected", wrong_route, "delivery_route_mismatch")

    def expiry(w):
        x = w.seal("e1", "routeA"); w.now = x.expires
        return w.receive_all(x)
    add("wrapper_expiry_rejected", expiry, "delivery_expired")

    def no_key_no_clear(w):
        w.revoke_for_send("kA")
        x = w.seal("e1", "routeA")
        return {"wrapper": x, "clear_seen": any(e.get("result") == "clear_fallback" for e in w.log)}
    add("missing_key_never_clear_fallback", no_key_no_clear, {"wrapper": None, "clear_seen": False})

    def rotate(w):
        old = w.seal("old", "routeA")
        w.add_key("kA2", "routeA")
        new = w.seal("new", "routeA")
        a = w.receive_all(old); b = w.receive_all(new)
        return {"old_key": old.key_id, "new_key": new.key_id, "old": a, "new": b, "effects": w.released_events}
    add("key_refresh_old_overlap_and_new_current", rotate,
        {"old_key": "kA", "new_key": "kA2", "old": "released_or_deduped", "new": "released_or_deduped", "effects": ["old", "new"]})

    def retire_old(w):
        old = w.seal("e1", "routeA"); w.add_key("kA2", "routeA"); w.retire_receive("kA")
        return w.receive_all(old)
    add("retired_old_key_rejects_queued_wrapper", retire_old, "delivery_key_unknown_or_stale")

    def migrate(w):
        first = w.seal("e1", "routeA")
        w.add_key("kB", "routeB")
        second = w.reseal_other_route(first, "routeB")
        a = w.receive_all(first); b = w.receive_all(second)
        return {"ids_differ": first.delivery_id != second.delivery_id, "keys": [first.key_id, second.key_id],
                "first": a, "second": b, "effects": w.released_events}
    add("route_migration_reseals_fresh_id_and_dedupes_event", migrate,
        {"ids_differ": True, "keys": ["kA", "kB"], "first": "released_or_deduped", "second": "released_or_deduped", "effects": ["e1"]})

    def collision(w):
        x = w.seal("e1", "routeA"); w.reserve(x)
        y = Wrapper("e2", x.route, x.key_id, x.delivery_id, x.expires, True, True)
        return w.reserve(y)
    add("same_key_id_delivery_id_different_wrapper_rejected", collision, "delivery_replay_collision")

    def invalid_inner(w):
        x = w.seal("e1", "routeA", inner_auth=False)
        a = w.receive_all(x); b = w.receive_all(x)
        return {"first": a, "retry": b, "effects": w.released_events}
    add("outer_open_never_authenticates_sender", invalid_inner,
        {"first": "inner_auth_failed", "retry": "delivery_replay_rejected", "effects": []})

    def revoked_device(w):
        w.device_active = False
        return w.seal("e1", "routeA")
    add("revoked_device_no_new_send", revoked_device, None)

    return out


def mutations() -> list[dict]:
    rows = []

    # The verification-derived ADR-0143 finding: treating RESERVED as terminal replay loses an accepted-route message
    # after crash before the inner transaction. The scenario must fail under this mutation.
    w = base_world(mutation_drop_reserved_retry=True)
    x = w.seal("e1", "routeA"); w.reserve(x); w.crash(); got = w.receive_all(x)
    caught = got == "delivery_replay_dropped_reserved" and w.released_events == []
    rows.append({"id": "M1_drop_reserved_retry", "caught": caught, "got": got, "trace": w.log,
                 "violates": "same-route retry liveness after crash"})

    w = base_world(mutation_clear_fallback=True); w.revoke_for_send("kA"); x = w.seal("e1", "routeA")
    caught = x is not None and not x.sealed
    rows.append({"id": "M2_clear_sender_fallback", "caught": caught, "trace": w.log, "violates": "no clear-sender fallback"})

    w = base_world(mutation_route_agnostic=True); x = w.seal("e1", "routeA")
    y = Wrapper(x.event_id, "routeB", x.key_id, x.delivery_id, x.expires, True, True); got = w.receive_all(y)
    caught = got == "released_or_deduped"
    rows.append({"id": "M3_route_binding_disabled", "caught": caught, "got": got, "trace": w.log,
                 "violates": "route isolation"})

    w = base_world(mutation_outer_authenticates_sender=True); x = w.seal("e1", "routeA", inner_auth=False); got = w.receive_all(x)
    caught = got == "released_or_deduped" and w.released_events == ["e1"]
    rows.append({"id": "M4_outer_open_authenticates_sender", "caught": caught, "got": got, "trace": w.log,
                 "violates": "inner sender authentication remains authoritative"})

    w = base_world(mutation_reuse_id_cross_route=True); x = w.seal("e1", "routeA"); w.add_key("kB", "routeB"); y = w.reseal_other_route(x, "routeB")
    caught = x.delivery_id == y.delivery_id
    rows.append({"id": "M5_cross_route_delivery_id_reuse", "caught": caught, "trace": w.log,
                 "violates": "fresh delivery_id on different route"})
    return rows


def main() -> int:
    sc = scenarios()
    mut = mutations()
    ok = all(r["ok"] for r in sc) and all(r["caught"] for r in mut)
    payload = {
        "ok": ok,
        "gate": "V-003",
        "scope": "ADR-0142/ADR-0143 sealed-delivery state semantics only; HPKE primitive/KAT execution belongs to V-001",
        "scenarios": sc,
        "mutations": mut,
        "summary": {"scenarios": len(sc), "passed": sum(r["ok"] for r in sc), "mutations": len(mut), "mutations_caught": sum(r["caught"] for r in mut)},
    }
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "work/sealed-delivery-state.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, indent=1, sort_keys=True) + "\n")

    # Keep compact, reviewable regression vectors in the canonical vector tree.
    vectors = {
        "format": "om-v003-sealed-delivery-state-v1",
        "cases": [{"id": r["id"], "expected": r["expected"]} for r in sc],
        "mutation_controls": [{"id": r["id"], "violates": r["violates"]} for r in mut],
    }
    VECTORS.write_text(json.dumps(vectors, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload["summary"], sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
