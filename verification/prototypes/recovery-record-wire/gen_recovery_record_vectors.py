#!/usr/bin/env python3
"""ADR-0199/ADR-0200 recovery-record vectors. Feasibility evidence, not product code.

Three families in one file (verification/vectors/recovery-record-vectors.json.gz):

  "vectors"  wire vectors for RecoveryRecordEnvelope (35), RecoveryRecord (36), RecoveryRedirect (37) and RecoveryKit (38),
             in the shape of the V-005 corpus: the two prototype codecs, the product codec and the two CDDL tools replay
             them. A vector with "semantic_expect": "reject" is wire-valid and breaks one rule of ADR-0199 section 4
             (it needs the Core reader, not a codec); the codecs see it as a positive.
  "crypto"   key derivation, associated data, padding, sealed envelopes, the second layer, generation arithmetic and
             freshness, computed here by independent code (hashlib/hmac, siv.py for AES-256-GCM-SIV written from RFC 8452,
             and a brute-force pad search) and replayed by Core.

Usage: gen_recovery_record_vectors.py OUT.json
"""
from __future__ import annotations

import hashlib
import hmac
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
sys.path.insert(0, str(HERE))
import gen_vectors as g  # noqa: E402
from gen_vectors import B, M, det, mdel, mget, mset, py_enc, raw  # noqa: E402
import siv  # noqa: E402

for _t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"]:
    g.TYPES.setdefault(_t["name"], _t)
T0 = g.T0

LBL = {
    "read": "OM-RECOVERY-RECORD-READ-CAP-v1",
    "write": "OM-RECOVERY-RECORD-WRITE-CAP-v1",
    "key": "OM-RECOVERY-RECORD-KEY-v1",
    "snap": "OM-RECOVERY-RELATIONSHIP-SNAPSHOT-KEY-v1",
    "loc": "OM-RECOVERY-RECORD-LOCATOR-v1",
    "ad": "OM-RECOVERY-RECORD-AEAD-AD-v1",
    "sad": "OM-RECOVERY-RELATIONSHIP-SNAPSHOT-AEAD-AD-v1",
}
BUCKETS = [16384, 65536, 262144, 1048000]


# ------------------------------------------------------------------------------------------ primitives
def hkdf(ikm: bytes, info: bytes, n: int = 32) -> bytes:
    prk = hmac.new(b"\0" * 32, ikm, hashlib.sha256).digest()  # no salt = HashLen zeros (RFC 5869)
    out, t, i = b"", b"", 1
    while len(out) < n:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest()
        out += t
        i += 1
    return out[:n]


def derive(root, label, scope):
    return hkdf(root, label.encode() + scope)


def locator(read_cap):
    return hashlib.sha256(g.ds_hash(LBL["loc"], read_cap)).digest()


def ad_bytes(scope, loc):
    return py_enc([LBL["ad"], B(scope), B(loc), 1])


def snap_ad(scope, generation):
    return py_enc([LBL["sad"], B(scope), generation, 1])


def pad_length(base_len: int, bucket: int):
    """Brute force: the pad length l with base + 1 (key) + head(l) + l == bucket and distance >= 3, else None."""
    d = bucket - base_len
    if d < 3:
        return None
    for l in range(max(0, d - 12), d):
        if 1 + len(raw(B(b"\0" * l))) == d:  # key byte + (bstr head + payload)
            return l
    return None


def pad_plain(body_value):
    base = py_enc(body_value)
    for bucket in BUCKETS:
        l = pad_length(len(base), bucket)
        if l is not None:
            out = py_enc(mset(body_value, 22, B(b"\0" * l)))
            assert len(out) == bucket
            return out, bucket
    raise ValueError("too large")


# ------------------------------------------------------------------------------------------ fixtures
ACC = g.ACC
D1, D2 = g.D1, g.D2


def cred(dev):
    return g.top("DeviceCredential", [2, B(dev)], [3, B(ACC)], [4, 1], [5, B(det(f"rr-pk-{dev.hex()}", 32))],
                 [6, B(det(f"rr-ik-{dev.hex()}", 32))], [7, 1])


C1, C2 = cred(D1), cred(D2)
C1B, C2B = py_enc(C1), py_enc(C2)


def cred_ref(dev, c):
    return M([0, B(dev)], [1, g.digest_of("OM-DEVICE-CREDENTIAL-HASH-v1", py_enc(c))])


POLICY = g.RP_TYPICAL
POLICY_B = py_enc(POLICY)
POLICY2_B = py_enc(g.RP_MIN)
POLICY_HASH = g.digest_of("OM-RECOVERY-POLICY-HASH-v1", POLICY_B)


def state(n, prev_hash=None, account=ACC, refs=None, control=None):
    refs = refs if refs is not None else [cred_ref(D1, C1), cred_ref(D2, C2)]
    ps = [[2, B(account)], [3, n], [5, g.setof([B(d) for d in (control or [D1, D2])])], [9, M([0, 1])], [10, POLICY_HASH],
          [11, 1], [13, g.setof(refs)]]
    if prev_hash is not None:
        ps.append([4, prev_hash])
    return g.top("AccountControlState", *ps)


def state_hash(st):
    return g.digest_of("OM-ACS-HASH-v1", py_enc(st))


def chain(n, **kw):
    """n states, generations 1..n, each linked to the one before."""
    out, prev = [], None
    for i in range(1, n + 1):
        st = state(i, prev, **kw)
        out.append(st)
        prev = state_hash(st)
    return out


def signed(st):
    env, _ = g.envelope("AccountControlState", st, ["controller"])
    return B(py_enc(env))


RELAY = M([0, "https://relay.example.net"], [1, B(det("rr-cap", 32))], [2, 0], [4, T0 + 7 * 86400])
DKM = M([0, 1], [1, B(det("rr-dk-id", 16))], [2, B(det("rr-dk-pk", 1216))],
        [3, g.sealed_route_binding("https://relay.example.net", det("rr-cap", 32))], [4, T0 + 7 * 86400])
HEAD_REF_NAME = "head"


def entry(i=0, label=True, extras=True):
    ps = [[0, B(det(f"conv-{i}", 32))], [1, B(det(f"peer-{i}", 32))], [3, [RELAY]], [4, [M([0, B(det(f"pdev-{i}", 16))], [1, DKM])]]]
    if label:
        ps.append([2, f"Friend {i}"])
    if extras:
        ps.append([5, B(det(f"fetch-{i}", 32))])
        ps.append([6, M([0, 2], [1, g.fake_digest(f"peer-head-{i}")])])
    return M(*ps)


SCOPE = det("rr-scope", 32)
SCOPE2 = det("rr-scope-2", 32)
HOME = M([0, "home.example.net"])
HOME_FULL = M([0, "home.example.net"], [1, 8443], [2, B(SCOPE)],
              [3, B(py_enc(g.envelope("RelaySet", g.RELAYSET, ["provider"])[0]))], [4, B(det("rr-admission", 32))])
SNAP = M([0, [entry(0), entry(1, label=False, extras=False)]])


def head_ref(st):
    n = mget(st, 3)
    return M([0, n], [1, state_hash(st)])


def evidence(states, creds=(C1B, C2B), policy=POLICY_B):
    *tail, head = states
    return M([0, signed(head)], [1, [signed(t) for t in tail]], [2, [B(c) for c in creds]], [3, B(policy)])


def record(states=None, generation=None, epoch=1, snapshot=("plain", SNAP), providers=(HOME,), refresh=0, ext=None,
           evid=None, headref=None):
    states = states or chain(3)
    head = states[-1]
    gen = generation if generation is not None else ((mget(head, 3) << 20) | refresh)
    ps = [[2, gen], [3, epoch], [4, headref or head_ref(head)], [5, evid or evidence(states)],
          [6, M([0, list(providers)])]]
    kind, val = snapshot
    if kind == "plain":
        ps.append([7, val])
    elif kind == "sealed":
        ps.append([8, val])
    elif kind == "both":
        ps += [[7, val], [8, B(det("rr-sealed", 40))]]
    if ext is not None:
        ps.append([23, ext])
    return g.top("RecoveryRecord", *ps)


EXT = M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])])


# ------------------------------------------------------------------------------------------ wire vectors
VEC: list[dict] = []


def pos(name, suffix, value, cddl="valid", semantic=None, note=""):
    body = py_enc(value)
    assert raw(value) == body
    v = {"id": f"{name}/{suffix}", "object": name, "form": "bare", "expect": "accept", "error": None, "cddl": cddl,
         "bench": False, "hex": body.hex(), "value": value, "derived": []}
    if semantic:
        v["semantic_expect"] = "reject"
        v["semantic_rule"] = semantic
    if note:
        v["note"] = note
    VEC.append(v)


def neg(name, suffix, error, value_or_bytes, cddl="invalid", note=""):
    b = value_or_bytes if isinstance(value_or_bytes, bytes) else raw(value_or_bytes)
    VEC.append({"id": f"{name}/neg/{suffix}", "object": name, "form": "bare", "expect": "reject", "error": error,
                "cddl": cddl, "bench": False, "hex": b.hex(), "note": note})


R = "RecoveryRecord"
CH3 = chain(3)
TYP = record(CH3)

# --- RecoveryRecord positives
pos(R, "typical", TYP)
pos(R, "head-only-genesis", record(chain(1)))
pos(R, "tail-8-states", record(chain(9)))
pos(R, "refresh-slot-5", record(CH3, refresh=5))
pos(R, "refresh-slot-max", record(CH3, refresh=(1 << 20) - 1))
pos(R, "maximum-privacy-second-layer", record(CH3, snapshot=("sealed", B(det("rr-sealed", 28)))))
pos(R, "second-layer-1000-bytes", record(CH3, snapshot=("sealed", B(det("rr-sealed", 1000)))))
pos(R, "empty-relationship-snapshot", record(CH3, snapshot=("plain", M([0, []]))))
pos(R, "truncated-relationship-snapshot", record(CH3, snapshot=("plain", M([0, [entry(0)]], [1, True]))))
pos(R, "four-provider-homes", record(CH3, providers=(HOME, HOME_FULL, M([0, "b.example.org"], [1, 65535]), M([0, "c.example.org"]))))
pos(R, "relationship-snapshot-256-entries", record(CH3, snapshot=("plain", M([0, [entry(i % 3, extras=False) for i in range(256)]]))),
    cddl="n/a", note="bound only: the Ruby CDDL tool takes minutes on 256 entries; the codecs check the bound")
pos(R, "noncritical-extension-preserved", record(CH3, ext=EXT))
pos(R, "pad-key-22", mset(TYP, 22, B(b"\0" * 100)))
pos(R, "credentials-320", record(CH3, evid=M([0, signed(CH3[-1])], [1, [signed(t) for t in CH3[:-1]]],
                                                [2, [B(py_enc(mset(C1, 2, B(det(f"c{i}", 16))))) for i in range(320)]], [3, B(POLICY_B)])),
    cddl="n/a", semantic=None, note="wire bound only: the 320 bodies are not the ones the states reference (a semantic negative)")
VEC[-1]["semantic_expect"] = "reject"
VEC[-1]["semantic_rule"] = "the credential set must be exactly the one the carried states reference"

# --- semantic negatives (wire-valid)
OTHER = det("rr-other-account", 32)
BROKEN = [CH3[0], state(2, g.fake_digest("rr-not-the-parent")), CH3[2]]
SEM = {
    "head-hash-differs-from-key-4": (record(CH3, headref=M([0, 3], [1, g.fake_digest("rr-other-head")])),
        "the head's OM-ACS-HASH-v1 must equal key 4's state_hash"),
    "head-generation-differs-from-key-4": (record(CH3, headref=M([0, 2], [1, state_hash(CH3[2])])),
        "the head's generation must equal key 4's generation"),
    "generation-shift-mismatch": (record(CH3, generation=(2 << 20) | 1),
        "record_generation >> 20 must equal the head's generation"),
    "tail-link-broken": (record(BROKEN), "each tail state links to the next by previous_state_hash"),
    "tail-last-not-predecessor-of-head": (record(CH3, evid=evidence([CH3[0], CH3[2]])),
        "the last tail state must be the head's predecessor"),
    "tail-in-wrong-order": (record(CH3, evid=M([0, signed(CH3[2])], [1, [signed(CH3[1]), signed(CH3[0])]],
                                                  [2, [B(C1B), B(C2B)]], [3, B(POLICY_B)])),
        "the tail is oldest first"),
    "state-of-another-account": (record(CH3, evid=evidence([CH3[0], state(2, state_hash(CH3[0]), account=OTHER), CH3[2]])),
        "every carried state is of one account"),
    "credential-missing": (record(CH3, evid=evidence(CH3, creds=(C1B,))),
        "every credential the states reference needs its body"),
    "credential-extra": (record(CH3, evid=evidence(CH3, creds=(C1B, C2B, py_enc(cred(g.D3))))),
        "no credential body the states do not reference"),
    "credential-duplicated": (record(CH3, evid=evidence(CH3, creds=(C1B, C1B, C2B))), "no duplicate credential bodies"),
    "credential-body-differs-from-its-reference": (record(CH3, evid=evidence(CH3, creds=(C1B, py_enc(mset(C2, 5, B(det("rr-x", 32)))))) ),
        "each body must hash to its reference"),
    "policy-hash-differs-from-commitment": (record(CH3, evid=evidence(CH3, policy=POLICY2_B)),
        "the policy body must hash to the head's recovery_policy_commitment"),
    "both-relationship-keys": (record(CH3, snapshot=("both", SNAP)), "exactly one of keys 7 and 8"),
    "neither-relationship-key": (record(CH3, snapshot=("none", None)), "exactly one of keys 7 and 8"),
}
for k, (v, rule) in SEM.items():
    pos(R, "semantic-" + k, v, semantic=rule)

# --- wire negatives
for suffix, err, mut, cddl in g.body_mutations(R, TYP):
    if suffix == "unknown-field-22":
        continue  # key 22 is the pad of this type
    if suffix == "nonminimal-int":
        mut = mset(TYP, 0, {"$nonmin": mget(TYP, 0), "w": 3})
    neg(R, suffix, err, g.wrap_top(R, mut), cddl)
neg(R, "trailing-bytes", "trailing_bytes", raw(TYP) + b"\0", "n/a")
neg(R, "truncated", "truncated", raw(TYP)[:-1], "n/a")
neg(R, "unknown-field-9", "unknown_field", mset(TYP, 9, 1))
neg(R, "unknown-field-24", "unknown_field", mset(TYP, 24, 1))
for k, name in ((2, "generation"), (3, "epoch"), (4, "head-ref"), (5, "evidence"), (6, "providers")):
    neg(R, f"missing-{name}", "missing_field", mdel(TYP, k))
neg(R, "generation-zero", "value_range", mset(TYP, 2, 0))
neg(R, "epoch-text", "wrong_type", mset(TYP, 3, "one"))
neg(R, "head-ref-state-hash-31-bytes", "value_range", mset(TYP, 4, M([0, 3], [1, M([0, 1], [1, B(det("x", 31))])])))
neg(R, "head-ref-missing-generation", "missing_field", mset(TYP, 4, M([1, g.fake_digest("x")])))
neg(R, "head-ref-digest-alg-2", "value_range", mset(TYP, 4, M([0, 3], [1, M([0, 2], [1, B(det("x", 32))])])))
ev = mget(TYP, 5)
neg(R, "evidence-head-missing", "missing_field", mset(TYP, 5, mdel(ev, 0)))
neg(R, "evidence-tail-missing", "missing_field", mset(TYP, 5, mdel(ev, 1)))
neg(R, "evidence-credentials-missing", "missing_field", mset(TYP, 5, mdel(ev, 2)))
neg(R, "evidence-policy-missing", "missing_field", mset(TYP, 5, mdel(ev, 3)))
neg(R, "evidence-unknown-key", "unknown_field", mset(TYP, 5, mset(ev, 4, 1)))
neg(R, "evidence-head-inline-map", "wrong_type", mset(TYP, 5, mset(ev, 0, g.envelope("AccountControlState", CH3[2], ["controller"])[0])))
neg(R, "evidence-head-is-a-request", "wrong_object_type", mset(TYP, 5, mset(ev, 0, B(py_enc(g.envelope("RecoveryRequest", g.REQ, ["recovery"])[0])))))
neg(R, "evidence-tail-9-states", "value_range", mset(TYP, 5, mset(ev, 1, [signed(s) for s in chain(9)])))
neg(R, "evidence-credentials-321", "value_range", mset(TYP, 5, mset(ev, 2, [B(C1B)] * 321)), "n/a")
neg(R, "evidence-credential-empty", "value_range", mset(TYP, 5, mset(ev, 2, [B(b"")])), "n/a")
neg(R, "evidence-credential-32769-bytes", "value_range", mset(TYP, 5, mset(ev, 2, [B(b"\0" * 32769)])), "n/a")
neg(R, "evidence-policy-empty", "value_range", mset(TYP, 5, mset(ev, 3, B(b""))), "n/a")
neg(R, "evidence-policy-24577-bytes", "value_range", mset(TYP, 5, mset(ev, 3, B(b"\0" * 24577))), "n/a")
neg(R, "evidence-policy-inline-map", "wrong_type", mset(TYP, 5, mset(ev, 3, POLICY)))
neg(R, "evidence-null-tail", "null_forbidden", mset(TYP, 5, mset(ev, 1, None)))
neg(R, "providers-empty", "value_range", mset(TYP, 6, M([0, []])))
neg(R, "providers-5", "value_range", mset(TYP, 6, M([0, [HOME] * 5])))
neg(R, "providers-missing-array", "missing_field", mset(TYP, 6, M()))
neg(R, "provider-host-missing", "missing_field", record(CH3, providers=(M([1, 443]),)))
neg(R, "provider-host-empty", "value_range", record(CH3, providers=(M([0, ""]),)))
neg(R, "provider-host-254-chars", "value_range", record(CH3, providers=(M([0, "a" * 254]),)))
neg(R, "provider-port-0", "value_range", record(CH3, providers=(M([0, "h.example.net"], [1, 0]),)))
neg(R, "provider-port-65536", "value_range", record(CH3, providers=(M([0, "h.example.net"], [1, 65536]),)))
neg(R, "provider-scope-31-bytes", "value_range", record(CH3, providers=(M([0, "h.example.net"], [2, B(det("x", 31))]),)))
neg(R, "provider-credential-15-bytes", "value_range", record(CH3, providers=(M([0, "h.example.net"], [4, B(det("x", 15))]),)))
neg(R, "provider-credential-257-bytes", "value_range", record(CH3, providers=(M([0, "h.example.net"], [4, B(det("x", 257))]),)))
neg(R, "provider-unknown-key", "unknown_field", record(CH3, providers=(M([0, "h.example.net"], [5, 1]),)))
neg(R, "provider-relayset-inline-map", "wrong_type", record(CH3, providers=(M([0, "h.example.net"], [3, g.envelope("RelaySet", g.RELAYSET, ["provider"])[0]]),)))
neg(R, "snapshot-257-entries", "value_range", record(CH3, snapshot=("plain", M([0, [entry(i % 3, extras=False) for i in range(257)]]))), "n/a")
neg(R, "snapshot-truncated-false", "value_range", record(CH3, snapshot=("plain", M([0, []], [1, False]))))
neg(R, "snapshot-entries-missing", "missing_field", record(CH3, snapshot=("plain", M())))
neg(R, "second-layer-27-bytes", "value_range", record(CH3, snapshot=("sealed", B(det("x", 27)))))
neg(R, "second-layer-text", "wrong_type", record(CH3, snapshot=("sealed", "ciphertext")))
E0 = entry(0)
for suffix, err, e in (
    ("entry-conversation-missing", "missing_field", mdel(E0, 0)),
    ("entry-peer-missing", "missing_field", mdel(E0, 1)),
    ("entry-routes-missing", "missing_field", mdel(E0, 3)),
    ("entry-targets-missing", "missing_field", mdel(E0, 4)),
    ("entry-routes-empty", "value_range", mset(E0, 3, [])),
    ("entry-routes-5", "value_range", mset(E0, 3, [RELAY] * 5)),
    ("entry-targets-empty", "value_range", mset(E0, 4, [])),
    ("entry-targets-5", "value_range", mset(E0, 4, [M([0, B(det("d", 16))], [1, DKM])] * 5)),
    ("entry-label-65-chars", "value_range", mset(E0, 2, "x" * 65)),
    ("entry-label-empty", "value_range", mset(E0, 2, "")),
    ("entry-fetch-cap-15-bytes", "value_range", mset(E0, 5, B(det("x", 15)))),
    ("entry-fetch-cap-97-bytes", "value_range", mset(E0, 5, B(det("x", 97)))),
    ("entry-conversation-31-bytes", "value_range", mset(E0, 0, B(det("x", 31)))),
    ("entry-unknown-key", "unknown_field", mset(E0, 7, 1)),
    ("entry-target-device-15-bytes", "value_range", mset(E0, 4, [M([0, B(det("d", 15))], [1, DKM])])),
    ("entry-target-key-missing", "missing_field", mset(E0, 4, [M([0, B(det("d", 16))])])),
    ("entry-route-inline-text", "wrong_type", mset(E0, 3, ["https://relay.example.net"])),
    ("entry-peer-head-missing-hash", "missing_field", mset(E0, 6, M([0, 2]))),
):
    neg(R, suffix, err, record(CH3, snapshot=("plain", M([0, [e]]))))
neg(R, "extensions-not-a-map", "wrong_type", mset(TYP, 23, [1]))
neg(R, "over-the-registered-size", "size_limit", raw(mset(TYP, 22, B(b"\0" * 65000))) + b"", "n/a")  # replaced below

# the oversize vector must exceed 1,048,000 bytes: sixteen 65,000-byte pads in noncritical extensions
VEC.pop()
big = M(*[[f"org.example.pad-{i:02d}", M([0, False], [1, B(b"\0" * 65000)])] for i in range(17)])
neg(R, "over-the-registered-size", "size_limit", raw(mset(TYP, 23, big)), "n/a")

# --- RecoveryRecordEnvelope
EN = "RecoveryRecordEnvelope"
ENV = g.top(EN, [2, 1], [3, B(det("rr-nonce", 12))], [4, B(det("rr-ct", 16400))])
pos(EN, "typical", ENV)
pos(EN, "ciphertext-16400-bytes-minimum", ENV)
pos(EN, "noncritical-extension-preserved", mset(ENV, 23, EXT))
pos(EN, "any-profile-id", mset(ENV, 2, 65535), note="the registry bounds the profile id; the reader refuses an unknown profile")
for suffix, err, mut, cddl in g.body_mutations(EN, ENV):
    if suffix == "nonminimal-int":
        mut = mset(ENV, 0, {"$nonmin": mget(ENV, 0), "w": 3})
    neg(EN, suffix, err, g.wrap_top(EN, mut), cddl)
neg(EN, "trailing-bytes", "trailing_bytes", raw(ENV) + b"\0", "n/a")
neg(EN, "truncated", "truncated", raw(ENV)[:-1], "n/a")
neg(EN, "profile-missing", "missing_field", mdel(ENV, 2))
neg(EN, "profile-0", "value_range", mset(ENV, 2, 0))
neg(EN, "nonce-missing", "missing_field", mdel(ENV, 3))
neg(EN, "nonce-11-bytes", "value_range", mset(ENV, 3, B(det("x", 11))))
neg(EN, "nonce-13-bytes", "value_range", mset(ENV, 3, B(det("x", 13))))
neg(EN, "nonce-text", "wrong_type", mset(ENV, 3, "nonce"))
neg(EN, "ciphertext-missing", "missing_field", mdel(ENV, 4))
neg(EN, "ciphertext-16399-bytes", "value_range", mset(ENV, 4, B(det("x", 16399))))
neg(EN, "ciphertext-empty", "value_range", mset(ENV, 4, B(b"")))
neg(EN, "ciphertext-null", "null_forbidden", mset(ENV, 4, None))
neg(EN, "unknown-field-5", "unknown_field", mset(ENV, 5, B(b"x")))
neg(EN, "plaintext-type-in-envelope", "wrong_object_type", mset(ENV, 0, 36))

# --- RecoveryRedirect
RD = "RecoveryRedirect"
PLACE = M([0, "new-home.example.org"], [1, 8443], [2, B(SCOPE2)])
RED = g.top(RD, [2, 7 << 20], [3, PLACE])
pos(RD, "typical", RED)
pos(RD, "default-port", g.top(RD, [2, 1], [3, M([0, "new-home.example.org"], [2, B(SCOPE2)])]))
pos(RD, "with-pad", mset(RED, 22, B(b"\0" * 50)))
pos(RD, "semantic-place-without-scope", g.top(RD, [2, 1], [3, M([0, "new-home.example.org"])]),
    semantic="the new place's host and recovery_scope_id are required")
for suffix, err, mut, cddl in g.body_mutations(RD, RED):
    if suffix == "unknown-field-22":
        continue
    if suffix == "nonminimal-int":
        mut = mset(RED, 0, {"$nonmin": mget(RED, 0), "w": 3})
    neg(RD, suffix, err, g.wrap_top(RD, mut), cddl)
neg(RD, "trailing-bytes", "trailing_bytes", raw(RED) + b"\0", "n/a")
neg(RD, "generation-missing", "missing_field", mdel(RED, 2))
neg(RD, "generation-zero", "value_range", mset(RED, 2, 0))
neg(RD, "place-missing", "missing_field", mdel(RED, 3))
neg(RD, "place-host-missing", "missing_field", mset(RED, 3, M([1, 443], [2, B(SCOPE2)])))
neg(RD, "place-scope-31-bytes", "value_range", mset(RED, 3, M([0, "h.example.org"], [2, B(det("x", 31))])))
neg(RD, "place-port-0", "value_range", mset(RED, 3, M([0, "h.example.org"], [1, 0], [2, B(SCOPE2)])))
neg(RD, "unknown-field-4", "unknown_field", mset(RED, 4, 1))
neg(RD, "place-text", "wrong_type", mset(RED, 3, "new-home.example.org"))

# --- RecoveryKit
KT = "RecoveryKit"
KIT = g.top(KT, [2, B(SCOPE)], [3, "home.example.net"], [4, 8443], [5, B(det("kit-read", 32))], [6, B(det("kit-write", 32))],
            [7, B(det("kit-key", 32))], [8, B(det("kit-inbox", 32))], [9, B(det("kit-snap", 32))], [10, 3])
pos(KT, "typical", KIT)
pos(KT, "minimal", g.top(KT, [2, B(SCOPE)], [3, "home.example.net"], [5, B(det("kit-read", 32))], [6, B(det("kit-write", 32))],
                          [7, B(det("kit-key", 32))], [8, B(det("kit-inbox", 32))], [10, 0]))
for suffix, err, mut, cddl in g.body_mutations(KT, KIT):
    if suffix == "unknown-field-22":
        continue
    if suffix == "nonminimal-int":
        mut = mset(KIT, 0, {"$nonmin": mget(KIT, 0), "w": 3})
    neg(KT, suffix, err, g.wrap_top(KT, mut), cddl)
neg(KT, "unknown-field-11", "unknown_field", mset(KIT, 11, 1))
for k, name in ((2, "scope"), (3, "host"), (5, "read-cap"), (6, "write-cap"), (7, "record-key"), (8, "inbox-cap"), (10, "epoch")):
    neg(KT, f"missing-{name}", "missing_field", mdel(KIT, k))
for k, name in ((2, "scope"), (5, "read-cap"), (6, "write-cap"), (7, "record-key"), (8, "inbox-cap"), (9, "snapshot-key")):
    neg(KT, f"{name}-31-bytes", "value_range", mset(KIT, k, B(det("x", 31))))
neg(KT, "host-empty", "value_range", mset(KIT, 3, ""))
neg(KT, "port-0", "value_range", mset(KIT, 4, 0))
neg(KT, "port-65536", "value_range", mset(KIT, 4, 65536))
neg(KT, "trailing-bytes", "trailing_bytes", raw(KIT) + b"\0", "n/a")


# ------------------------------------------------------------------------------------------ crypto family
def crypto():
    out = {}
    roots = [bytes(range(32)), det("rr-root-2", 32), b"\xff" * 32]
    scopes = [SCOPE, SCOPE2, bytes(32)]
    keys = []
    for r, s in zip(roots, scopes):
        rc = derive(r, LBL["read"], s)
        keys.append({"root": r.hex(), "scope": s.hex(), "read_cap": rc.hex(), "write_cap": derive(r, LBL["write"], s).hex(),
                     "record_key": derive(r, LBL["key"], s).hex(), "snapshot_key": derive(r, LBL["snap"], s).hex(),
                     "locator": locator(rc).hex(),
                     "associated_data": ad_bytes(s, locator(rc)).hex(), "snapshot_associated_data_gen_7": snap_ad(s, 7).hex()})
    out["keys"] = keys
    # a key derived under another scope is a different key (negative, stated as data)
    out["scope_separation"] = {"root": roots[0].hex(), "scope_a": scopes[0].hex(), "scope_b": scopes[1].hex(),
                               "read_cap_a": keys[0]["read_cap"], "read_cap_b": derive(roots[0], LBL["read"], scopes[1]).hex()}
    # padding: (unpadded length, bucket, pad length or null)
    pads = []
    for bucket in BUCKETS:
        for base in (bucket - 600, bucket - 100, bucket - 30, bucket - 28, bucket - 27, bucket - 26, bucket - 5, bucket - 4,
                     bucket - 3, bucket - 2, bucket - 1, bucket):
            pads.append({"unpadded": base, "bucket": bucket, "pad": pad_length(base, bucket)})
    for base in (259 + 16000, 16384 - 259, 65536 - 65538, 16384 - 26, 16384 - 259):
        if base > 0:
            for bucket in BUCKETS:
                pads.append({"unpadded": base, "bucket": bucket, "pad": pad_length(base, bucket)})
    out["padding"] = pads
    # which bucket a body of a given unpadded size lands in
    sizes = []
    for base in (100, 16380, 16381, 16382, 16383, 16384, 16385, 16384 - 26, 65533, 65534, 65535, 65536, 262140, 262141, 262144,
                 1047996, 1047997, 1047998, 1047999):
        for bucket in BUCKETS:
            if pad_length(base, bucket) is not None:
                sizes.append({"unpadded": base, "bucket": bucket if base <= BUCKETS[-1] else None})
                break
        else:
            sizes.append({"unpadded": base, "bucket": None})
    out["bucket_for"] = sizes

    # sealed envelopes: a real record padded into each bucket (the larger ones via a noncritical extension)
    def ext_for(target_unpadded):
        # grow key 23 until the unpadded plaintext is exactly target_unpadded bytes
        base = len(py_enc(record(CH3)))
        small = M(["org.example.p", M([0, False], [1, B(b"")])])
        need = target_unpadded - len(py_enc(record(CH3, ext=small)))
        assert need >= 0, need
        for extra in range(0, 8):
            for n in range(max(0, need - extra - 8), need + 1):
                cand = M(["org.example.p", M([0, False], [1, B(b"\0" * n)])])
                if len(py_enc(record(CH3, ext=cand))) == target_unpadded:
                    return cand
        raise ValueError(target_unpadded)

    seals = []
    for name, want_unpadded in (("bucket-16384", None), ("up-to-65536-by-the-3-byte-rule", 16382),
                                ("bucket-65536-max", 65533), ("up-to-262144-by-the-3-byte-rule", 65534),
                                ("bucket-262144-max", 262141), ("up-to-1048000", 262142), ("bucket-1048000-max", 1047997)):
        v = record(CH3) if want_unpadded is None else record(CH3, ext=ext_for(want_unpadded))
        plain, bucket = pad_plain(v)
        root, scope = roots[0], SCOPE
        rk, rc = derive(root, LBL["key"], scope), derive(root, LBL["read"], scope)
        loc = locator(rc)
        nonce = det(f"rr-nonce-{name}", 12)
        ct = siv.seal(rk, nonce, ad_bytes(scope, loc), plain)
        envelope = py_enc(g.top("RecoveryRecordEnvelope", [2, 1], [3, B(nonce)], [4, B(ct)]))
        case = {"id": name, "root": root.hex(), "scope": scope.hex(), "nonce": nonce.hex(), "bucket": bucket,
                "unpadded": len(py_enc(v)), "plaintext_sha256": hashlib.sha256(plain).hexdigest(),
                "envelope_sha256": hashlib.sha256(envelope).hexdigest(), "envelope_len": len(envelope),
                "record_hex": py_enc(v).hex()}
        if bucket <= 65536:
            case["envelope_hex"] = envelope.hex()
        seals.append(case)
    out["seals"] = seals

    # open negatives against the first case: each must fail with nothing used
    s0 = seals[0]
    env0 = bytes.fromhex(s0["envelope_hex"])
    root, scope = roots[0], SCOPE

    def flip(b, i):
        return b[:i] + bytes([b[i] ^ 1]) + b[i + 1:]

    ct_off = env0.index(det("x", 0) + b"") if False else None
    e = g.top("RecoveryRecordEnvelope", [2, 1], [3, B(bytes.fromhex(s0["nonce"]))], [4, B(b"")])
    full = lambda nonce, ct, profile=1: py_enc(g.top("RecoveryRecordEnvelope", [2, profile], [3, B(nonce)], [4, B(ct)]))
    nonce0 = bytes.fromhex(s0["nonce"])
    ct0 = env0[-(s0["bucket"] + 16):]
    negs = [
        {"id": "flip-ciphertext-bit", "envelope_hex": full(nonce0, flip(ct0, 5)).hex(), "root": root.hex(), "scope": scope.hex(), "expect": "reject"},
        {"id": "flip-tag-bit", "envelope_hex": full(nonce0, flip(ct0, len(ct0) - 1)).hex(), "root": root.hex(), "scope": scope.hex(), "expect": "reject"},
        {"id": "flip-nonce-bit", "envelope_hex": full(flip(nonce0, 0), ct0).hex(), "root": root.hex(), "scope": scope.hex(), "expect": "reject"},
        {"id": "wrong-scope", "envelope_hex": env0.hex(), "root": root.hex(), "scope": scopes[1].hex(), "expect": "reject"},
        {"id": "wrong-root", "envelope_hex": env0.hex(), "root": roots[1].hex(), "scope": scope.hex(), "expect": "reject"},
        {"id": "unknown-profile", "envelope_hex": full(nonce0, ct0, 7).hex(), "root": root.hex(), "scope": scope.hex(), "expect": "reject"},
        {"id": "ciphertext-not-a-bucket", "envelope_hex": full(nonce0, ct0[:-1] + b"\0" * 20).hex(), "root": root.hex(), "scope": scope.hex(), "expect": "reject"},
        {"id": "envelope-version-2", "envelope_hex": py_enc(mset(g.top("RecoveryRecordEnvelope", [2, 1], [3, B(nonce0)], [4, B(ct0)]), 1, 2)).hex(),
         "root": root.hex(), "scope": scope.hex(), "expect": "reject"},
        {"id": "positive", "envelope_hex": env0.hex(), "root": root.hex(), "scope": scope.hex(), "expect": "accept"},
    ]
    # an AEAD-valid envelope whose plaintext is not a bucket, and one whose plaintext is a type the reader does not know
    rk, rc = derive(root, LBL["key"], scope), derive(root, LBL["read"], scope)
    loc = locator(rc)
    short = py_enc(record(CH3))  # not padded
    negs.append({"id": "plaintext-not-a-bucket", "root": root.hex(), "scope": scope.hex(), "expect": "reject",
                 "envelope_hex": full(nonce0, siv.seal(rk, nonce0, ad_bytes(scope, loc), short)).hex()})
    out["open"] = negs

    # second layer
    snap_plain = py_enc(SNAP)
    sk = derive(roots[1], LBL["snap"], SCOPE)
    sn = det("rr-snap-nonce", 12)
    out["second_layer"] = {"key": sk.hex(), "scope": SCOPE.hex(), "generation": 3 << 20, "nonce": sn.hex(),
                           "snapshot_hex": snap_plain.hex(),
                           "layer_hex": (sn + siv.seal(sk, sn, snap_ad(SCOPE, 3 << 20), snap_plain)).hex(),
                           "wrong_generation": (3 << 20) + 1}

    # generation arithmetic
    out["generation"] = [{"acs": a, "refresh": r, "record_generation": (a << 20) | r} for a, r in
                         ((1, 0), (1, 1), (3, 0), (3, 7), (5, (1 << 20) - 1), (6, 0), ((1 << 44) - 1, (1 << 20) - 1))]
    out["generation_reject"] = [{"acs": 1, "refresh": 1 << 20}, {"acs": 1 << 44, "refresh": 0}, {"acs": 0, "refresh": 0}]
    out["freshness"] = [
        {"mark": None, "generation": 5, "same_plaintext": True, "expect": "newer"},
        {"mark": 5, "generation": 6, "same_plaintext": True, "expect": "newer"},
        {"mark": 5, "generation": 5, "same_plaintext": True, "expect": "same"},
        {"mark": 5, "generation": 5, "same_plaintext": False, "expect": "equivocation"},
        {"mark": 5, "generation": 4, "same_plaintext": True, "expect": "stale"},
    ]
    return out


def main(out):
    ids = [v["id"] for v in VEC]
    assert len(ids) == len(set(ids)), [i for i in ids if ids.count(i) > 1][:5]
    doc = {"vectors": VEC, "crypto": crypto(),
           "note": "Feasibility evidence, not product code. 'semantic_expect: reject' vectors are wire-valid; Core's reader refuses them."}
    for v in VEC:
        v["hex"] = g.compress_hex(v["hex"])
        if v["expect"] == "reject":
            v.pop("value", None)
    Path(out).write_text(json.dumps(doc, separators=(",", ":"), sort_keys=True) + "\n")
    pos_n = sum(v["expect"] == "accept" for v in VEC)
    print(f"wrote {len(VEC)} wire vectors ({pos_n} positive, {len(VEC) - pos_n} negative) to {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
