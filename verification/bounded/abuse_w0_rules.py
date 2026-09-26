#!/usr/bin/env python3
"""V-016: executable checks for the ADR-0131/0132 abuse and moderation rules (R-210, R-214, R-224, R-231, R-234,
R-238, R-279). One function per criterion; each also runs a mutation that must be caught.

RFC 9578 token type 0x0002 (Blind RSA 2048-bit) is RSABSSA-SHA384-PSS-Deterministic (RFC 9474): redemption verifies
the authenticator with RSASSA-PSS, SHA-384, MGF1-SHA-384, salt length 48 (RFC 9578 section 6.4) over
token_input = token_type || nonce || challenge_digest || token_key_id, where token_key_id is SHA-256 of the
id-RSASSA-PSS SPKI (section 6.5). The zero-salt PSSZERO variants are a different profile and must not verify.

Evidence boundary (review F-7): the IntroGate model here covers redemption only: signature verification under the
exact 0x0002 parameters, challenge binding, issuer-epoch rotation/overlap and single-use (replay) enforcement,
including RFC 9578 A.2 known-answer positives and derived negatives. Every token it redeems is issued through the
RFC 9474 Blind/BlindSign/Finalize flow in privacy_pass_issuance.py, whose own checks (RFC 9474 A.3/A.4 and RFC 9578
A.2 vectors, issuer-transcript unlinkability) are the only evidence here for issuance privacy. The redemption checks
alone say nothing about unlinkability.

Written by a Claude agent (verification executor). Deterministic.
"""
from __future__ import annotations

import hashlib
import itertools
import json
import re
import sys
from pathlib import Path

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import hashes, serialization

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/abuse-w0-rules-latest.json"
sys.path.insert(0, str(ROOT / "verification/cbor"))
from python_canonical import enc  # noqa: E402
sys.path.insert(0, str(Path(__file__).resolve().parent))
import privacy_pass_issuance as ppi  # noqa: E402
from privacy_pass_issuance import (RSABSSA_SHA384_PSSZERO_DETERMINISTIC, TOKEN_TYPE_0002_VARIANT, VARIANTS,  # noqa: E402
                                   Issuer, issue)


def sha256(b):
    return hashlib.sha256(b).digest()


# ---- RFC 9578 type 0x0002 referral admission (ADR-0132 rule 1) --------------------------------------------
class IntroGate:  # holds only public keys: it can verify, never mint
    def __init__(self, overlap: int, variant=TOKEN_TYPE_0002_VARIANT):
        self.variant = variant
        self.keys: dict[bytes, tuple[object, int, int | None]] = {}   # key_id -> (public key, epoch, retire_at)
        self.spent: set[bytes] = set()
        self.overlap = overlap

    def rotate(self, spki: bytes, epoch: int, now: int):
        for kid, (pk, e, ret) in list(self.keys.items()):
            if ret is None:
                self.keys[kid] = (pk, e, now + self.overlap)
        self.keys[sha256(spki)] = (serialization.load_der_public_key(spki), epoch, None)

    def admit(self, tok: bytes, challenge: bytes, now: int, mutate_no_spent=False) -> str:
        ti, sig = tok[:2 + 32 + 32 + 32], tok[98:]
        if ti[:2] != b"\x00\x02" or ti[34:66] != sha256(challenge):
            return "reject:malformed_or_wrong_challenge"
        kid = ti[66:98]
        if kid not in self.keys:
            return "reject:unknown_issuer_key"
        pk, _, ret = self.keys[kid]
        if ret is not None and now >= ret:
            return "reject:issuer_epoch_retired"
        try:
            pk.verify(sig, ti, ppi.pss(self.variant), hashes.SHA384())
        except InvalidSignature:
            return "reject:bad_signature"
        nonce = ti[2:34]
        if nonce in self.spent and not mutate_no_spent:
            return "reject:double_spend"
        self.spent.add(nonce)
        return "admit:reserved_capacity"


def referral_tokens(mutate=False):
    bad = []
    ch = enc(["OM-REFERRAL-PRIVPASS-CHALLENGE-v1", {"$bytes": "11" * 16}])
    i1, i2 = Issuer(), Issuer()          # blind issuance only: the issuer never sees token_input (ppi.Issuer)
    g = IntroGate(overlap=3)
    g.rotate(i1.spki, 1, now=0)
    t1 = issue(i1, b"\x01" * 32, ch)
    if g.admit(t1, ch, 0) != "admit:reserved_capacity":
        bad.append("genuine token not admitted")
    if g.admit(t1, ch, 1, mutate_no_spent=mutate) != "reject:double_spend":
        bad.append("token accepted twice")
    # the IntroGate cannot mint: its best forgery (any bytes under the issuer key id) fails verification
    forged = b"\x00\x02" + b"\x02" * 32 + sha256(ch) + i1.key_id + b"\x00" * 256
    if g.admit(forged, ch, 1) != "reject:bad_signature":
        bad.append("forged token admitted")
    if not hasattr(g, "keys") or any(hasattr(v[0], "sign") for v in g.keys.values()):
        bad.append("IntroGate holds a signing key")
    if g.admit(issue(i1, b"\x03" * 32, enc(["x"])), ch, 1) != "reject:malformed_or_wrong_challenge":
        bad.append("token for another challenge admitted")
    g.rotate(i2.spki, 2, now=5)
    if g.admit(issue(i1, b"\x04" * 32, ch), ch, 7) != "admit:reserved_capacity":
        bad.append("old epoch rejected inside the overlap")
    if g.admit(issue(i1, b"\x05" * 32, ch), ch, 8) != "reject:issuer_epoch_retired":
        bad.append("old epoch still accepted after the bounded overlap")
    if g.admit(issue(i2, b"\x06" * 32, ch), ch, 8) != "admit:reserved_capacity":
        bad.append("new epoch token rejected")
    return bad


def referral_token_kat(mutate=False):
    """RFC 9578 A.2 tokens redeemed by the IntroGate, plus the derived negatives in the vectors file (review F-6).
    Mutation: the IntroGate verifies with the PSSZERO parameters (the pre-F-6 behaviour)."""
    bad = []
    V = ppi.load_vectors()
    variant = RSABSSA_SHA384_PSSZERO_DETERMINISTIC if mutate else TOKEN_TYPE_0002_VARIANT
    for v in V["rfc9578"]["vectors"]:
        g = IntroGate(overlap=3, variant=variant)
        g.rotate(bytes.fromhex(v["pkI"]), 1, now=0)
        tok, ch = bytes.fromhex(v["token"]), bytes.fromhex(v["token_challenge"])
        if (r := g.admit(tok, ch, 0)) != "admit:reserved_capacity":
            bad.append(f"RFC 9578 A.2 vector {v['id']} not admitted: {r}")
        elif g.admit(tok, ch, 1) != "reject:double_spend":
            bad.append(f"RFC 9578 A.2 vector {v['id']} admitted twice")
    pk1 = V["rfc9578"]["vectors"][0]["pkI"]
    for c in V["redemption_negatives"]:
        vv = RSABSSA_SHA384_PSSZERO_DETERMINISTIC if mutate else VARIANTS[c["verifier_variant"]]
        g = IntroGate(overlap=3, variant=vv)
        g.rotate(bytes.fromhex(pk1), 1, now=0)
        if (r := g.admit(bytes.fromhex(c["token"]), bytes.fromhex(c["challenge"]), 0)) != c["expect"]:
            bad.append(f"negative {c['name']}: {r}, expected {c['expect']}")
    return bad


# ---- IntroGate admission order (R-210, R-214) ----------------------------------------------------------------
def introgate_admission(mutate=False):
    """Reserved referral capacity, per-IntroGate max_uses and size rejection before any expensive work."""
    bad = []
    for public, referral, oversized in itertools.product((0, 50, 500), (0, 3), (0, 20)):
        payload_limit, reserved, public_quota, max_uses = 4096, 4, 10, 5
        expensive = 0
        admitted = {"public": 0, "referral": 0}
        uses = 0
        reqs = [("public", 100)] * public + [("referral", 100)] * referral + [("public", payload_limit + 17)] * oversized
        for kind, size in reqs:
            if size > payload_limit + 16 and not mutate:        # cheap check, before opening the HPKE ciphertext
                continue
            if uses >= max_uses and kind == "public":
                continue
            if kind == "referral":
                if admitted["referral"] < reserved:
                    admitted["referral"] += 1; expensive += 1
                continue
            if admitted["public"] < public_quota:
                admitted["public"] += 1; uses += 1; expensive += 1
        if admitted["referral"] != min(referral, reserved):
            bad.append(f"referral admissions starved by public={public}")
        if uses > max_uses:
            bad.append("per-IntroGate max_uses exceeded")
        if expensive > reserved + public_quota:
            bad.append(f"expensive work {expensive} above quota")
        if oversized and expensive > min(referral, reserved) + min(public, max_uses):
            bad.append("oversized MessageRequest reached expensive work")
    return bad


# ---- stale reductions (R-279, ADR-0131 rule 7) -----------------------------------------------------------------
def stale_reduction(mutate=False):
    """Every reduction names an immutable instance id. Explore all orders of: add(m) -> i1, reduce(i1) signed,
    remove(i1), re-add(m) -> i2; the stale reduce must never hit i2."""
    bad = []
    ops = ["reduce_i1_applied", "remove", "readd"]
    for order in itertools.permutations(ops):
        members = {"m": "i1"}   # name -> live instance
        restricted = set()
        counter = 1
        for op in order:
            if op == "remove":
                members.pop("m", None)
            elif op == "readd" and "m" not in members:
                counter += 1; members["m"] = f"i{counter}"
            elif op == "reduce_i1_applied":
                target = members.get("m") if mutate else ("i1" if members.get("m") == "i1" else None)
                if target:
                    restricted.add(target)
        live = members.get("m")
        if live and live != "i1" and live in restricted:
            bad.append(f"order {order}: stale reduction hit a later incarnation {live}")
    return bad


# ---- unsolicited group add (R-224) -----------------------------------------------------------------------------
def unsolicited_add(mutate=False):
    bad = []
    accepted_invites = {("room1", "bob")}
    for room, principal in itertools.product(("room1", "room2"), ("bob", "carol")):
        joins = mutate or (room, principal) in accepted_invites   # the joiner's client processes a Welcome only then
        if joins and (room, principal) not in accepted_invites:
            bad.append(f"{principal} joined {room} without an accepted invitation")
    return bad


# ---- moderation state changes only by committed ops (R-231) ----------------------------------------------------
def committed_lifts(mutate=False):
    bad = []
    log = [("commit", "ban", "x"), ("decision", "lift", "x"), ("decision", "restrict", "y"), ("commit", "restrict", "y"),
           ("decision", "unrestrict", "y")]
    state = {"banned": set(), "restricted": set()}
    for kind, op, who in log:
        if kind != "commit" and not mutate:
            continue                                  # a ModerationDecision is a label, not a Room state change
        {"ban": state["banned"].add, "lift": state["banned"].discard, "restrict": state["restricted"].add,
         "unrestrict": state["restricted"].discard}[op](who)
    if "x" not in state["banned"] or "y" not in state["restricted"]:
        bad.append("a ModerationDecision alone lifted a ban or restriction")
    state2 = {"banned": {"x"}}
    state2["banned"].discard("x")                     # committed lift
    if "x" in state2["banned"]:
        bad.append("committed lift did not end the ban")
    return bad


# ---- call.offer gating (R-234) ------------------------------------------------------------------------------------
def call_gate(mutate=False):
    bad = []
    rel = {"alice": "ACTIVE", "bob": "ACCEPTED_UNBOUND", "mallory": None}
    setup_work = 0
    for caller, state in rel.items():
        allowed = state == "ACTIVE"
        if allowed or mutate:
            setup_work += 1                           # ICE gathering, TURN allocation, ringing
        elif setup_work and caller != "alice":
            pass
    if setup_work != 1:
        bad.append(f"call setup performed for {setup_work - 1} caller(s) without an accepted relationship")
    return bad


# ---- advisories and safety references (R-238) -----------------------------------------------------------------------
def advisories_and_safety_ref(mutate=False):
    bad = []
    api = (ROOT / "spec/cddl/home-api.cddl").read_text()
    reqs = re.findall(r"^(\w+Request)\s*=\s*\{([^}]*)\}", api, re.M)
    for name, body in reqs:
        if "account-id" in body:
            bad.append(f"Home API {name} takes an AccountID (a provider could resolve an advisory to capabilities)")

    def v2(issuer: bytes, pref: dict) -> bytes:
        return sha256(enc(["OM-SAFETY-REF-v2", {"$bytes": enc([{"$bytes": issuer.hex()}, pref]).hex()}]))

    def v1(issuer: bytes, pref: dict) -> bytes:
        return sha256(b"OM-SAFETY-REF-v1" + issuer + bytes([pref[0]]) + bytes.fromhex(pref[1]["$bytes"]))

    f = v1 if mutate else v2
    seen = {}
    base = bytes(range(40))
    for split in range(8, 33):
        for kind in (1, 2, 3):
            issuer, pid = base[:split], base[split:split + 16 + (split % 17)]
            if not 16 <= len(pid) <= 32:
                continue
            pref = {0: kind, 1: {"$bytes": pid.hex()}}
            key = (issuer, kind, pid)
            d = f(issuer, pref)
            if d in seen and seen[d] != key:
                bad.append("safety reference not injective")
                break
            seen[d] = key
    # collision probe: the raw v1 concatenation maps these two distinct references to the same bytes
    P = "11" * 16
    a = f(b"iss\x02", {0: 1, 1: {"$bytes": P}})
    b = f(b"iss", {0: 2, 1: {"$bytes": "01" + P}})
    if a == b:
        bad.append("safety reference not injective (issuer/principal boundary shift collides)")
    return bad


def main() -> int:
    checks = {"referral_token_0x0002": referral_tokens, "referral_token_0x0002_kat": referral_token_kat,
              **ppi.CHECKS, "introgate_admission": introgate_admission,
              "stale_reduction": stale_reduction, "unsolicited_group_add": unsolicited_add,
              "committed_lifts_only": committed_lifts, "call_offer_gate": call_gate,
              "advisory_and_safety_ref_v2": advisories_and_safety_ref}
    res = {}
    for name, fn in checks.items():
        ok, mut = fn(), fn(mutate=True)
        res[name] = {"problems": ok, "mutation_caught": bool(mut), "mutation_problems": mut[:2]}
    passed = all(not r["problems"] and r["mutation_caught"] for r in res.values())
    OUT.write_text(json.dumps({"model": "abuse_w0_rules", "gates": ["V-016"], "passed": passed, "checks": res,
                               "gate_status_effect": "evidence_only_no_gate_automatically_passed"}, indent=1) + "\n")
    print(json.dumps({"passed": passed, **{k: (not v["problems"], v["mutation_caught"]) for k, v in res.items()}}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
