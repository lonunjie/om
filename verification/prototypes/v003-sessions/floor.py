#!/usr/bin/env python3
"""Feasibility prototype for V-003. Not product code. Not for production.

R-206 downgrade floor by security_generation (CRYPTOGRAPHY_AND_AGILITY.md "Downgrade floor", ADR-0126, ADR-0120 K3),
driven by the real registry spec/registries/om-crypto-profiles.json plus a vector-only extension in the experimental
id range (65280..65534, "never accepted by a production build"), because v1 has a single 1:1 profile today.

Rules implemented (independently of sessions.py, which abstracts profiles as 1 = weak / 2 = strong):
  * the floor of a 1:1 relationship is a security_generation, never a profile id;
  * a peer bundle / initial message whose profile has no one_to_one generation, is not usable in this build, or has a
    generation below the floor is refused (the sender) or rejected with an authenticated reject (the receiver);
  * accepting a profile raises the floor to its generation (never lowers it);
  * ADR-0161 pins ACS minimum_crypto_profile to security_generation.one_to_one; group-only ACS minima are invalid; after restore the floor is max(backed-up one_to_one generation, ACS one_to_one minimum) (ADR-0120 K3);
  * a signed SecurityAdvisory that forbids a profile moves the floor only to a non-forbidden profile of EQUAL or HIGHER
    generation; if none exists the relationship stops sending; a move to a lower generation is rejected.

Writes verification/vectors/one-to-one-floor-vectors.json and checks every vector. Usage: floor.py [out.json]
"""
from __future__ import annotations

import copy
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
REGISTRY = ROOT / "spec/registries/om-crypto-profiles.json"
VECTORS = ROOT / "verification/vectors/one-to-one-floor-vectors.json"
CT = "one_to_one"

# Vector-only profiles (experimental range): a future stronger 1:1 profile, an equal-generation alternative of it,
# and a group-only profile. They exist only to exercise generation ordering.
TEST_PROFILES = [
    {"id": 65280, "name": "test-1to1-gen2", "status": "experimental", "security_generation": {"one_to_one": 2}},
    {"id": 65281, "name": "test-1to1-gen2-alt", "status": "experimental", "security_generation": {"one_to_one": 2}},
    {"id": 65282, "name": "test-group-only", "status": "experimental", "security_generation": {"group": 1}},
    {"id": 65283, "name": "test-mixed-1to1-gen2-group0", "status": "experimental", "security_generation": {"one_to_one": 2, "group": 0}},
    {"id": 65284, "name": "test-mixed-1to1-gen1-group99", "status": "experimental", "security_generation": {"one_to_one": 1, "group": 99}},
]
USABLE = {"experimental", "allowed", "preferred", "deprecated"}   # 'test_only', 'forbidden', 'reserved' are not


def registry() -> dict[int, dict]:
    reg = {p["id"]: p for p in json.loads(REGISTRY.read_text())["profiles"]}
    for p in TEST_PROFILES:
        assert p["id"] not in reg and 65280 <= p["id"] <= 65534
        reg[p["id"]] = p
    return reg


def gen(reg, pid) -> int | None:
    p = reg.get(pid)
    return None if p is None else (p.get("security_generation") or {}).get(CT)


def admit(reg, floor: int, pid: int, forbidden: set[int]) -> tuple[bool, str]:
    p = reg.get(pid)
    if p is None or p.get("status") not in USABLE or pid in forbidden:
        return False, "profile unknown, not usable in this build, or forbidden by a SecurityAdvisory"
    g = gen(reg, pid)
    if g is None:
        return False, "profile has no one_to_one security_generation"
    if g < floor:
        return False, f"security_generation {g} below the relationship floor {floor}"
    return True, "ok"


def accept(reg, state: dict, pid: int) -> tuple[bool, str, dict]:
    ok, why = admit(reg, state["floor"], pid, set(state["forbidden"]))
    s = copy.deepcopy(state)
    if ok:
        s["floor"] = max(s["floor"], gen(reg, pid))
        s["profile"] = pid
    return ok, why if ok else "authenticated reject naming the message; sender retries at the floor (ADR-0120)", s


def acs_min_generation(reg, acs_min_profile: int) -> int:
    """ADR-0161: ACS minimum_crypto_profile is valid only on the one_to_one axis."""
    g = gen(reg, acs_min_profile)
    if g is None:
        raise ValueError("ACS minimum_crypto_profile lacks security_generation.one_to_one")
    return g


def restore_floor(reg, backed_up: int, acs_min_profile: int) -> int:
    return max(backed_up, acs_min_generation(reg, acs_min_profile))


def advisory(reg, state: dict, forbid: int, new_profile: int | None) -> tuple[bool, str, dict]:
    """A signed SecurityAdvisory forbids `forbid`; the client proposes `new_profile` as the relationship profile."""
    s = copy.deepcopy(state)
    s["forbidden"] = sorted(set(s["forbidden"]) | {forbid})
    if state["profile"] != forbid:
        return True, "current profile not affected", s
    if new_profile is None:
        s["sending"] = False
        return True, "no admissible profile of equal or higher generation: stop sending and say so", s
    ok, why = admit(reg, s["floor"], new_profile, set(s["forbidden"]))
    if not ok:
        s["sending"] = False
        return False, f"advisory move rejected ({why}); the floor never moves lower", s
    s["profile"] = new_profile
    s["floor"] = max(s["floor"], gen(reg, new_profile))
    return True, "moved to an equal-or-higher generation profile", s


def vectors(reg) -> list[dict]:
    V = []
    base = {"floor": 1, "profile": 1, "forbidden": [], "sending": True}
    high = {"floor": 2, "profile": 65280, "forbidden": [], "sending": True}

    def a(name, st, pid, expect):
        ok, why, after = accept(reg, st, pid)
        assert ok == expect, (name, why)
        V.append({"name": name, "op": "accept_profile", "state": st, "profile": pid, "expect_accept": expect, "reason": why, "after": after})

    def adv(name, st, forbid, new, expect, expect_floor, expect_sending=True):
        ok, why, after = advisory(reg, st, forbid, new)
        assert ok == expect and after["floor"] == expect_floor and after["sending"] == expect_sending, (name, why, after)
        V.append({"name": name, "op": "security_advisory", "state": st, "forbid": forbid, "new_profile": new,
                  "expect_accept": expect, "reason": why, "after": after})

    a("accept_om_crypto_v1_at_floor_1", base, 1, True)
    a("reject_test_profile_generation_0_below_floor_1", base, 65535, False)          # R-206 negative
    a("reject_profile_below_floor_after_upgrade", high, 1, False)                     # R-206 negative (gen 1 < 2)
    a("reject_group_only_profile_for_1to1", base, 65282, False)
    a("reject_unknown_profile", base, 4242, False)
    a("accept_higher_generation_raises_floor", base, 65280, True)
    a("acs_1to1_axis_ignores_lower_group_generation", base, 65283, True)
    a("large_group_generation_cannot_bypass_1to1_floor", high, 65284, False)
    a("accept_equal_generation_alternative", high, 65281, True)
    a("reject_forbidden_profile", dict(high, forbidden=[65281]), 65281, False)
    adv("advisory_move_to_equal_generation", high, 65280, 65281, True, 2)
    adv("advisory_move_to_lower_generation_rejected", high, 65280, 1, False, 2, expect_sending=False)   # R-206 negative
    adv("advisory_with_no_admissible_profile_stops_sending", {"floor": 2, "profile": 65280, "forbidden": [65281], "sending": True},
        65280, None, True, 2, expect_sending=False)
    adv("advisory_on_other_profile_changes_nothing", high, 1, None, True, 2)
    V.append({"name": "restore_floor_is_max_of_backup_and_acs_minimum", "op": "restore_floor", "backed_up": 2,
              "acs_minimum_crypto_profile": 1, "expect_floor": restore_floor(reg, 2, 1)})
    V.append({"name": "restore_floor_lifted_by_acs_minimum", "op": "restore_floor", "backed_up": 0,
              "acs_minimum_crypto_profile": 1, "expect_floor": restore_floor(reg, 0, 1)})
    try:
        restore_floor(reg, 1, 65282)
    except ValueError as e:
        V.append({"name": "restore_rejects_group_only_acs_minimum", "op": "restore_invalid_acs_minimum",
                  "backed_up": 1, "acs_minimum_crypto_profile": 65282, "expect_error": str(e)})
    else:
        raise AssertionError("group-only profile accepted as ACS minimum")
    assert V[-3]["expect_floor"] == 2 and V[-2]["expect_floor"] == 1
    return V


def replay(reg, V) -> list[str]:
    bad = []
    for v in V:
        if v["op"] == "accept_profile":
            ok, _, after = accept(reg, v["state"], v["profile"])
            if ok != v["expect_accept"] or after != v["after"]:
                bad.append(v["name"])
        elif v["op"] == "security_advisory":
            ok, _, after = advisory(reg, v["state"], v["forbid"], v["new_profile"])
            if ok != v["expect_accept"] or after != v["after"]:
                bad.append(v["name"])
        elif v["op"] == "restore_floor":
            if restore_floor(reg, v["backed_up"], v["acs_minimum_crypto_profile"]) != v["expect_floor"]:
                bad.append(v["name"])
        elif v["op"] == "restore_invalid_acs_minimum":
            try:
                restore_floor(reg, v["backed_up"], v["acs_minimum_crypto_profile"])
            except ValueError as e:
                if str(e) != v["expect_error"]:
                    bad.append(v["name"] + ":wrong_error")
            else:
                bad.append(v["name"] + ":accepted")
        # generic safety: no vector ever lowers a floor
        if v["op"] not in ("restore_floor", "restore_invalid_acs_minimum") and v["after"]["floor"] < v["state"]["floor"]:
            bad.append(v["name"] + ":floor_lowered")
    return bad


def main() -> int:
    reg = registry()
    V = vectors(reg)
    VECTORS.write_text(json.dumps({"gate": "V-003", "requirement": "R-206", "source": "verification/prototypes/v003-sessions/floor.py",
                                   "registry": "spec/registries/om-crypto-profiles.json + vector-only experimental profiles",
                                   "test_profiles": TEST_PROFILES, "vectors": V}, indent=1) + "\n")
    bad = replay(reg, json.loads(VECTORS.read_text())["vectors"])
    neg = [v["name"] for v in V if v.get("expect_accept") is False]
    out = {"ok": not bad, "vectors": len(V), "negative": neg, "failed": bad}
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "work/floor.json"
    path.parent.mkdir(exist_ok=True)
    path.write_text(json.dumps(out, indent=1) + "\n")
    print(json.dumps(out))
    return 0 if not bad else 1


if __name__ == "__main__":
    raise SystemExit(main())
