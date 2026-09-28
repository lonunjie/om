#!/usr/bin/env python3
"""Bounded V-003 evidence for ADR-0144 bootstrap/stale-ACS composition.

Architecture-state evidence only. This is not product code or cryptographic-runtime
proof. It checks the request-scoped reply_delivery_key contract and the composition
between ADR-0143 replay reservation and K7/K12 authenticated peer-ACS refresh.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field, replace
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "verification" / "cbor"))
from python_canonical import enc as cbor_enc  # noqa: E402

OUT = ROOT / "verification/results/sealed-delivery-adr0144-composition-latest.json"


def route_binding(uri: str, capability: bytes) -> bytes:
    return hashlib.sha256(
        cbor_enc(["OM-SEALED-DELIVERY-ROUTE-v1", uri, {"$bytes": capability.hex()}])
    ).digest()


def digest_obj(obj: object) -> str:
    return hashlib.sha256(
        json.dumps(obj, sort_keys=True, separators=(",", ":"), default=lambda x: x.hex() if isinstance(x, bytes) else x).encode()
    ).hexdigest()


@dataclass(frozen=True)
class Route:
    relay_uri: str
    capability: bytes
    expires_at: int


@dataclass(frozen=True)
class ReplyKey:
    crypto_profile: int
    delivery_key_id: bytes
    public_key: bytes
    route_binding: bytes
    expires_at: int


@dataclass(frozen=True)
class PendingRequest:
    ticket_id: bytes
    request_key_digest: bytes
    crypto_profile: int
    reply_route: Route
    reply_delivery_key: ReplyKey


@dataclass
class BootstrapLedger:
    pending: dict[bytes, PendingRequest] = field(default_factory=dict)
    seen_key_ids: set[bytes] = field(default_factory=set)
    seen_public_keys: set[bytes] = field(default_factory=set)
    completed: set[bytes] = field(default_factory=set)
    stable_delivery_key_ids: set[bytes] = field(default_factory=set)
    mutation_ignore_route: bool = False
    mutation_ignore_profile: bool = False
    mutation_ignore_expiry: bool = False
    mutation_allow_reuse: bool = False
    mutation_allow_transplant: bool = False
    mutation_allow_promotion: bool = False

    def register(self, p: PendingRequest) -> str:
        k = p.reply_delivery_key
        if p.ticket_id in self.pending:
            return "ticket_duplicate"
        if not self.mutation_ignore_route:
            expected = route_binding(p.reply_route.relay_uri, p.reply_route.capability)
            if k.route_binding != expected:
                return "route_binding_mismatch"
        if not self.mutation_ignore_profile and k.crypto_profile != p.crypto_profile:
            return "profile_mismatch"
        if not self.mutation_ignore_expiry and k.expires_at > p.reply_route.expires_at:
            return "key_outlives_route"
        if not self.mutation_allow_reuse:
            if k.delivery_key_id in self.seen_key_ids:
                return "delivery_key_id_reuse"
            if k.public_key in self.seen_public_keys:
                return "keypair_reuse"
        self.pending[p.ticket_id] = p
        self.seen_key_ids.add(k.delivery_key_id)
        self.seen_public_keys.add(k.public_key)
        return "registered"

    def bootstrap_use(self, ticket_id: bytes, key: ReplyKey) -> str:
        p = self.pending.get(ticket_id)
        if p is None:
            return "unknown_or_retired_request"
        if ticket_id in self.completed:
            return "bootstrap_complete"
        if not self.mutation_allow_transplant and key != p.reply_delivery_key:
            return "bootstrap_key_transplant"
        return "bootstrap_allowed"

    def complete_bootstrap(self, ticket_id: bytes) -> str:
        if ticket_id not in self.pending:
            return "unknown_or_retired_request"
        self.completed.add(ticket_id)
        del self.pending[ticket_id]
        return "retired"

    def promote_to_stable(self, ticket_id: bytes, key: ReplyKey) -> str:
        if not self.mutation_allow_promotion:
            return "promotion_forbidden"
        self.stable_delivery_key_ids.add(key.delivery_key_id)
        return "promoted"


@dataclass(frozen=True)
class Wrapper:
    delivery_key_id: str
    delivery_id: str
    event_id: str
    body: str

    @property
    def digest(self) -> str:
        return digest_obj(self.__dict__)


@dataclass
class ReplayReservation:
    wrapper_digest: str
    stage: str = "RESERVED"  # RESERVED | COMPLETE | REJECTED


@dataclass
class DeferredAuthWorld:
    replay: dict[tuple[str, str], ReplayReservation] = field(default_factory=dict)
    durable_events: set[str] = field(default_factory=set)
    released_events: set[str] = field(default_factory=set)
    acked: set[tuple[str, str]] = field(default_factory=set)
    mutation_stale_to_rejected: bool = False
    trace: list[dict] = field(default_factory=list)

    def reserve(self, w: Wrapper) -> str:
        rk = (w.delivery_key_id, w.delivery_id)
        old = self.replay.get(rk)
        if old is not None:
            if old.wrapper_digest != w.digest:
                self.trace.append({"op": "reserve", "result": "collision"})
                return "collision"
            if old.stage == "RESERVED":
                self.trace.append({"op": "reserve", "result": "resume_reserved"})
                return "resume_reserved"
            if old.stage == "COMPLETE":
                self.trace.append({"op": "reserve", "result": "complete_idempotent"})
                return "complete_idempotent"
            self.trace.append({"op": "reserve", "result": "rejected_terminal"})
            return "rejected_terminal"
        self.replay[rk] = ReplayReservation(w.digest)
        self.trace.append({"op": "reserve", "result": "reserved"})
        return "reserved"

    def inner_auth(self, w: Wrapper, relationship_state: str) -> str:
        """relationship_state: stale | current_accept | current_reject."""
        rk = (w.delivery_key_id, w.delivery_id)
        r = self.replay.get(rk)
        if r is None or r.wrapper_digest != w.digest or r.stage != "RESERVED":
            self.trace.append({"op": "inner_auth", "state": relationship_state, "result": "not_reserved"})
            return "not_reserved"
        if relationship_state == "stale":
            if self.mutation_stale_to_rejected:
                r.stage = "REJECTED"
                result = "premature_rejected"
            else:
                result = "deferred_refresh"
            self.trace.append({"op": "inner_auth", "state": relationship_state, "result": result})
            return result
        if relationship_state == "current_reject":
            r.stage = "REJECTED"
            self.trace.append({"op": "inner_auth", "state": relationship_state, "result": "rejected"})
            return "rejected"
        if relationship_state != "current_accept":
            raise ValueError(relationship_state)
        # K2 + ADR-0143 composition: durable inner acceptance and COMPLETE precede release/ack.
        self.durable_events.add(w.event_id)
        r.stage = "COMPLETE"
        self.trace.append({"op": "inner_auth", "state": relationship_state, "result": "complete"})
        return "complete"

    def release(self, w: Wrapper) -> str:
        rk = (w.delivery_key_id, w.delivery_id)
        r = self.replay.get(rk)
        if r is None or r.stage != "COMPLETE" or w.event_id not in self.durable_events:
            self.trace.append({"op": "release", "result": "blocked"})
            return "blocked"
        self.released_events.add(w.event_id)
        self.acked.add(rk)
        self.trace.append({"op": "release", "result": "released"})
        return "released"


def base_request(n: int = 1) -> PendingRequest:
    route = Route(f"https://relay-{n}.example", bytes([0x40 + n]) * 32, 2000 + n)
    key = ReplyKey(
        crypto_profile=1,
        delivery_key_id=bytes([0x10 + n]) * 16,
        public_key=bytes([0x70 + n]) * 1216,
        route_binding=route_binding(route.relay_uri, route.capability),
        expires_at=1900 + n,
    )
    return PendingRequest(
        ticket_id=bytes([0x20 + n]) * 16,
        request_key_digest=bytes([0x30 + n]) * 32,
        crypto_profile=1,
        reply_route=route,
        reply_delivery_key=key,
    )


def scenario_rows() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    def record(name: str, got, expected, trace=None):
        ok = got == expected
        rows.append({"id": name, "ok": ok, "expected": expected, "got": got, "trace": trace or []})
        if not ok:
            failures.append(f"{name}: expected {expected!r}, got {got!r}")

    # Bootstrap scope/binding negatives.
    p1 = base_request(1)
    record("bootstrap_valid_request", BootstrapLedger().register(p1), "registered")

    bad_route = replace(p1, reply_delivery_key=replace(p1.reply_delivery_key, route_binding=b"x" * 32))
    record("bootstrap_wrong_route_binding", BootstrapLedger().register(bad_route), "route_binding_mismatch")

    bad_profile = replace(p1, reply_delivery_key=replace(p1.reply_delivery_key, crypto_profile=2))
    record("bootstrap_profile_mismatch", BootstrapLedger().register(bad_profile), "profile_mismatch")

    bad_expiry = replace(p1, reply_delivery_key=replace(p1.reply_delivery_key, expires_at=p1.reply_route.expires_at + 1))
    record("bootstrap_key_outlives_route", BootstrapLedger().register(bad_expiry), "key_outlives_route")

    led = BootstrapLedger()
    first = led.register(p1)
    p2 = base_request(2)
    reuse_id = replace(p2, reply_delivery_key=replace(p2.reply_delivery_key, delivery_key_id=p1.reply_delivery_key.delivery_key_id))
    record("bootstrap_delivery_key_id_reuse", [first, led.register(reuse_id)], ["registered", "delivery_key_id_reuse"])

    led = BootstrapLedger()
    first = led.register(p1)
    reuse_key = replace(p2, reply_delivery_key=replace(p2.reply_delivery_key, public_key=p1.reply_delivery_key.public_key))
    record("bootstrap_keypair_reuse", [first, led.register(reuse_key)], ["registered", "keypair_reuse"])

    led = BootstrapLedger()
    a, b = led.register(p1), led.register(p2)
    record("bootstrap_cross_request_transplant", [a, b, led.bootstrap_use(p2.ticket_id, p1.reply_delivery_key)],
           ["registered", "registered", "bootstrap_key_transplant"])

    led = BootstrapLedger()
    led.register(p1)
    record("bootstrap_never_promoted_to_stable", led.promote_to_stable(p1.ticket_id, p1.reply_delivery_key), "promotion_forbidden")

    led = BootstrapLedger()
    led.register(p1)
    before = led.bootstrap_use(p1.ticket_id, p1.reply_delivery_key)
    retired = led.complete_bootstrap(p1.ticket_id)
    after = led.bootstrap_use(p1.ticket_id, p1.reply_delivery_key)
    record("bootstrap_retired_after_completion", [before, retired, after],
           ["bootstrap_allowed", "retired", "unknown_or_retired_request"])

    # RESERVED x stale-ACS cross product.
    w = Wrapper("k1", "d1", "event-1", "sealed-body-A")
    world = DeferredAuthWorld()
    got = [world.reserve(w), world.inner_auth(w, "stale"), world.release(w), world.reserve(w),
           world.inner_auth(w, "current_accept"), world.release(w)]
    record("reserved_stale_refresh_then_complete", got,
           ["reserved", "deferred_refresh", "blocked", "resume_reserved", "complete", "released"], world.trace)

    w = Wrapper("k2", "d2", "event-2", "sealed-body-B")
    world = DeferredAuthWorld()
    got = [world.reserve(w), world.inner_auth(w, "stale"), world.reserve(w),
           world.inner_auth(w, "current_reject"), world.release(w)]
    record("reserved_stale_refresh_then_reject", got,
           ["reserved", "deferred_refresh", "resume_reserved", "rejected", "blocked"], world.trace)

    original = Wrapper("k3", "d3", "event-3", "sealed-body-C")
    collision = Wrapper("k3", "d3", "event-evil", "different-body")
    world = DeferredAuthWorld()
    got = [world.reserve(original), world.inner_auth(original, "stale"), world.reserve(collision),
           world.reserve(original)]
    record("reserved_collision_rejected_original_resumable", got,
           ["reserved", "deferred_refresh", "collision", "resume_reserved"], world.trace)

    return rows, failures


def mutation_rows() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    def caught(name: str, vulnerable_result, bad_accept_value):
        is_caught = vulnerable_result == bad_accept_value
        rows.append({"id": name, "caught": is_caught, "mutated_result": vulnerable_result})
        if not is_caught:
            failures.append(f"mutation {name} was not observable")

    p1 = base_request(1)
    p2 = base_request(2)

    led = BootstrapLedger(mutation_ignore_route=True)
    bad = replace(p1, reply_delivery_key=replace(p1.reply_delivery_key, route_binding=b"x" * 32))
    caught("M1_route_binding_check_removed", led.register(bad), "registered")

    led = BootstrapLedger(mutation_ignore_profile=True)
    bad = replace(p1, reply_delivery_key=replace(p1.reply_delivery_key, crypto_profile=2))
    caught("M2_profile_check_removed", led.register(bad), "registered")

    led = BootstrapLedger(mutation_ignore_expiry=True)
    bad = replace(p1, reply_delivery_key=replace(p1.reply_delivery_key, expires_at=p1.reply_route.expires_at + 1))
    caught("M3_expiry_bound_removed", led.register(bad), "registered")

    led = BootstrapLedger(mutation_allow_reuse=True)
    led.register(p1)
    bad = replace(p2, reply_delivery_key=replace(p2.reply_delivery_key, delivery_key_id=p1.reply_delivery_key.delivery_key_id))
    caught("M4_key_id_reuse_guard_removed", led.register(bad), "registered")

    led = BootstrapLedger(mutation_allow_transplant=True)
    led.register(p1); led.register(p2)
    caught("M5_cross_request_transplant_guard_removed", led.bootstrap_use(p2.ticket_id, p1.reply_delivery_key), "bootstrap_allowed")

    led = BootstrapLedger(mutation_allow_promotion=True)
    led.register(p1)
    caught("M6_authority_promotion_guard_removed", led.promote_to_stable(p1.ticket_id, p1.reply_delivery_key), "promoted")

    w = Wrapper("k4", "d4", "event-4", "sealed-body-D")
    world = DeferredAuthWorld(mutation_stale_to_rejected=True)
    world.reserve(w)
    bad_result = world.inner_auth(w, "stale")
    retry = world.reserve(w)
    caught("M7_stale_state_prematurely_rejected", [bad_result, retry], ["premature_rejected", "rejected_terminal"])

    return rows, failures


def structural_checks() -> tuple[dict, list[str]]:
    failures: list[str] = []
    numeric = json.loads((ROOT / "spec/registries/om-numeric-fields.json").read_text())
    ds = json.loads((ROOT / "spec/registries/om-domain-separation.json").read_text())
    session_doc = (ROOT / "docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md").read_text()
    adr = (ROOT / "decisions/ADR-0144-pre-review-sealed-delivery-bootstrap-and-stale-head-closure.md").read_text()

    mrp = {int(x["key"]): x for x in numeric["structs"]["MessageRequestPayload"]["fields"]}
    key9 = mrp.get(9, {})
    if key9.get("name") != "reply_delivery_key" or key9.get("req") is not True:
        failures.append("MessageRequestPayload.reply_delivery_key is not required")
    labels = {x.get("label") for x in ds.get("labels", [])}
    if "OM-SEALED-DELIVERY-ROUTE-v1" not in labels:
        failures.append("sealed-delivery route-binding domain label missing")
    for token in ["fresh request-scoped `reply_delivery_key`", "never promoted into stable relationship delivery state"]:
        if token not in session_doc:
            failures.append(f"ONE_TO_ONE_SESSION_MANAGEMENT missing ADR-0144 token: {token}")
    if not ("wrapper stays" in session_doc and "`RESERVED`" in session_doc and "stale" in session_doc):
        failures.append("ONE_TO_ONE_SESSION_MANAGEMENT does not preserve RESERVED while stale-head refresh is pending")
    for token in ["cross-request key/id reuse", "RESERVED -> REJECTED", "authenticated monotonic peer-head refresh"]:
        if token not in adr:
            failures.append(f"ADR-0144 missing required semantic phrase: {token}")
    return {
        "reply_delivery_key_required": key9.get("name") == "reply_delivery_key" and key9.get("req") is True,
        "route_binding_domain_present": "OM-SEALED-DELIVERY-ROUTE-v1" in labels,
        "session_doc_names_bootstrap_scope": "fresh request-scoped `reply_delivery_key`" in session_doc,
        "session_doc_forbids_promotion": "never promoted into stable relationship delivery state" in session_doc,
        "session_doc_preserves_reserved_on_stale": "wrapper stays" in session_doc and "`RESERVED`" in session_doc and "stale" in session_doc,
    }, failures


def main() -> int:
    scenarios, scenario_failures = scenario_rows()
    mutations, mutation_failures = mutation_rows()
    structural, structural_failures = structural_checks()
    failures = scenario_failures + mutation_failures + structural_failures
    result = {
        "checker": "sealed_delivery_adr0144_composition",
        "gate": "V-003",
        "scope": "ADR-0144 request-scoped reply_delivery_key and ADR-0143 RESERVED x K7/K12 stale-ACS composition; bounded architecture evidence only",
        "passed": not failures,
        "summary": {
            "scenarios": len(scenarios),
            "scenarios_passed": sum(1 for x in scenarios if x["ok"]),
            "mutations": len(mutations),
            "mutations_caught": sum(1 for x in mutations if x["caught"]),
        },
        "structural_checks": structural,
        "scenarios": scenarios,
        "mutations": mutations,
        "failures": failures,
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
