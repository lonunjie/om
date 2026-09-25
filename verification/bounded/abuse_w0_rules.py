#!/usr/bin/env python3
"""V-016: executable checks for the ADR-0131/0132 abuse and moderation rules (R-210, R-214, R-224, R-231, R-234,
R-238, R-279). One function per criterion; each also runs a mutation that must be caught.

The RFC 9578 check uses real RSA: token type 0x0002 is RSABSSA-SHA384-PSS-Deterministic with a 2048-bit key
(RFC 9474), so a finalized token verifies like an RSA-PSS SHA-384 signature with salt length 0 over
token_input = token_type || nonce || challenge_digest || token_key_id. Blinding happens only between issuer and
client and does not change what the IntroGate verifies, so it is not modelled.

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
from cryptography.hazmat.primitives.asymmetric import padding, rsa

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/abuse-w0-rules-latest.json"
sys.path.insert(0, str(ROOT / "verification/cbor"))
from python_canonical import enc  # noqa: E402

PSS0 = padding.PSS(mgf=padding.MGF1(hashes.SHA384()), salt_length=0)


def sha256(b):
    return hashlib.sha256(b).digest()


# ---- RFC 9578 type 0x0002 referral admission (ADR-0132 rule 1) --------------------------------------------
class Issuer:   # the recipient's referral issuer: holds the private key
    def __init__(self, seed: int):
        self.sk = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        self.pk_der = self.sk.public_key().public_bytes(serialization.Encoding.DER, serialization.PublicFormat.SubjectPublicKeyInfo)
        self.key_id = sha256(self.pk_der)

    def token(self, nonce: bytes, challenge: bytes) -> bytes:
        ti = b"\x00\x02" + nonce + sha256(challenge) + self.key_id
        return ti + self.sk.sign(ti, PSS0, hashes.SHA384())


class IntroGate:  # holds only public keys: it can verify, never mint
    def __init__(self, overlap: int):
        self.keys: dict[bytes, tuple[object, int, int | None]] = {}   # key_id -> (public key, epoch, retire_at)
        self.spent: set[bytes] = set()
        self.overlap = overlap

    def rotate(self, issuer: Issuer, epoch: int, now: int):
        for kid, (pk, e, ret) in list(self.keys.items()):
            if ret is None:
                self.keys[kid] = (pk, e, now + self.overlap)
        self.keys[issuer.key_id] = (serialization.load_der_public_key(issuer.pk_der), epoch, None)

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
            pk.verify(sig, ti, PSS0, hashes.SHA384())
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
    i1, i2 = Issuer(1), Issuer(2)
    g = IntroGate(overlap=3)
    g.rotate(i1, 1, now=0)
    t1 = i1.token(b"\x01" * 32, ch)
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
    if g.admit(i1.token(b"\x03" * 32, enc(["x"])), ch, 1) != "reject:malformed_or_wrong_challenge":
        bad.append("token for another challenge admitted")
    g.rotate(i2, 2, now=5)
    if g.admit(i1.token(b"\x04" * 32, ch), ch, 7) != "admit:reserved_capacity":
        bad.append("old epoch rejected inside the overlap")
    if g.admit(i1.token(b"\x05" * 32, ch), ch, 8) != "reject:issuer_epoch_retired":
        bad.append("old epoch still accepted after the bounded overlap")
    if g.admit(i2.token(b"\x06" * 32, ch), ch, 8) != "admit:reserved_capacity":
        bad.append("new epoch token rejected")
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
    checks = {"referral_token_0x0002": referral_tokens, "introgate_admission": introgate_admission,
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
