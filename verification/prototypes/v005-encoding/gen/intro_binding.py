#!/usr/bin/env python3
"""Feasibility prototype for V-005. Not product code. Not for production.

Python checker for the R-280 reciprocal IntroIdentityBinding sequences in the V-005 vector file (ADR-0131 rule 8).
verification/cbor/node_intro_binding.mjs is the second, independently written checker.

intro_binding.py VECTORS.json -> JSON summary on stdout; exit 1 on any mismatch.
"""
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "cbor"))
from python_canonical import enc  # noqa: E402

TEST_SIG = b"insecure_test_signature"


def dec(b: bytes):
    def item(i):
        ib = b[i]; major, ai = ib >> 5, ib & 31; i += 1
        if ai < 24:
            n = ai
        else:
            w = 1 << (ai - 24); n = int.from_bytes(b[i:i + w], "big"); i += w
        if major == 0: return n, i
        if major == 2: return {"$bytes": b[i:i + n].hex()}, i + n
        if major == 3: return b[i:i + n].decode(), i + n
        if major == 4:
            out = []
            for _ in range(n):
                x, i = item(i); out.append(x)
            return out, i
        if major == 5:
            out = {}
            for _ in range(n):
                k, i = item(i); out[k], i = item(i)
            return out, i
        if major == 7: return {20: False, 21: True, 22: None}[n], i
        raise ValueError("unsupported")
    v, e = item(0)
    assert e == len(b)
    return v


def hx(v):
    return bytes.fromhex(v["$bytes"])


def digest(label, body):
    return hashlib.sha256(enc([label, {"$bytes": body.hex()}])).digest()


def verify(env_bytes: bytes, ctx: dict) -> bool:
    acs_body = bytes.fromhex(ctx["peer_acs_body"])
    if digest("OM-ACS-HASH-v1", acs_body).hex() != ctx["trusted_peer_acs_hash"]:
        return False
    acs = dec(acs_body)
    env = dec(env_bytes)
    sigs = env[2]
    if len(sigs) != 1:
        return False
    s = sigs[0]
    if s[2] != 3 or hx(s[3]) != TEST_SIG:
        return False
    signer = hx(s[0])
    live = {hx(d) for d in acs.get(5, []) + acs.get(7, [])} - {hx(d) for d in acs.get(6, []) + acs.get(8, [])}
    if signer not in live:
        return False
    body = dec(hx(env[1]))
    return (body[0] == 21 and hx(body[2]) == hx(acs[2]) and hx(body[3]).hex() == ctx["peer_intro_expected"]
            and body[4] == {0: 1, 1: {"$bytes": ctx["request_key_digest"]}} and body[5] == acs[3]
            and body[6] == {0: 1, 1: {"$bytes": ctx["trusted_peer_acs_hash"]}})


def run(seq: dict) -> list[dict]:
    state, sent, verified, out = "NONE", False, False, []
    if seq["perspective"] == "requester":
        state = "ACCEPTED_UNBOUND"   # the requester learns of acceptance from the reply
    for st in seq["steps"]:
        op = st["op"]
        if op == "accept_request":
            state, r = "ACCEPTED_UNBOUND", "ok"
        elif op == "send_own_binding":
            if seq["perspective"] == "requester" and not verified:
                r = "refused"   # never reveal the stable AccountID before the recipient's binding verified
            else:
                sent, r = True, "ok"
        else:
            if seq["perspective"] == "recipient" and not sent:
                r = "rejected"  # the requester only answers a binding it has verified
            elif verify(bytes.fromhex(st["binding"]), seq["context"]):
                verified, r = True, "ok"
            else:
                r = "rejected"
        if sent and verified:
            state = "ACTIVE"
        out.append({"result": r, "state": state})
    return out


def main(argv):
    doc = json.load(open(argv[1]))
    bad = [s["id"] for s in doc["intro_binding_sequences"] if run(s) != s["expect"]]
    seqs = doc["intro_binding_sequences"]
    out = {"implementation": "python", "sequences": len(seqs),
           "reach_active": sum(1 for s in seqs if s["expect"][-1]["state"] == "ACTIVE"),
           "mismatches": bad}
    print(json.dumps(out))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
