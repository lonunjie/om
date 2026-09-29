#!/usr/bin/env python3
"""Feasibility prototype for V-005. Not product code. Not for production.

Generates the W0 object-level golden vectors (positive and negative) from the
proposed registries in spec/registries/. Canonical bytes of every positive
vector come from the existing, unmodified verification/cbor/python_canonical.py
encoder; this file adds a small raw emitter that can produce deliberately
non-canonical bytes for negative vectors. Output is deterministic.

Usage: gen_vectors.py OUT.json
"""
from __future__ import annotations

import copy
import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "verification" / "cbor"))
from python_canonical import enc as py_enc  # noqa: E402  (existing independent encoder, unmodified)

TEST_PROFILE = 65535  # insecure-test-profile (om-crypto-profiles.json)
TEST_SIG = b"insecure_test_signature"  # placeholder, never a real signature

# V-005 covers the frozen W0 wire; later-wave types (e.g. R16C W3 types 24-27) carry their own codec evidence.
TYPES = {t["name"]: t for t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"] if t.get("wave") == "W0"}
ROLE = {"controller": 1, "recovery": 2, "device": 3, "intro_identity": 4, "provider": 5, "legacy": 6}

# ---------------------------------------------------------------- value helpers (JSON diagnostic form)

def det(label: str, n: int) -> bytes:
    out, i = b"", 0
    while len(out) < n:
        out += hashlib.sha256(f"om-v005-vector:{label}:{i}".encode()).digest()
        i += 1
    return out[:n]


def B(b: bytes) -> dict:
    return {"$bytes": b.hex()}


def M(*pairs) -> dict:
    return {"$map": [list(p) for p in pairs]}


def setof(items: list) -> list:
    return sorted(items, key=lambda v: py_enc(v))


def top(type_name: str, *pairs) -> dict:
    return M([0, TYPES[type_name]["type_id"]], [1, 1], *pairs)


def digest_of(label: str, body: bytes) -> dict:
    return M([0, 1], [1, B(hashlib.sha256(ds_hash(label, body)).digest())])


def fake_digest(label: str) -> dict:
    return M([0, 1], [1, B(det(label, 32))])


def sealed_route_binding(relay_uri: str, capability: bytes) -> dict:
    # ADR-0142: SHA-256(OM-CBOR [label, relay_uri, deposit_capability]).
    body = py_enc(["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, B(capability)])
    return M([0, 1], [1, B(hashlib.sha256(body).digest())])


def ds_sig(label: str, profile: int, role: int, signer: bytes, body: bytes) -> bytes:
    return py_enc([label, profile, role, B(signer), B(body)])


def ds_hash(label: str, body: bytes) -> bytes:
    return py_enc([label, B(body)])

# ---------------------------------------------------------------- raw emitter (for non-canonical negatives)
# Markers understood only by this emitter:
#   {"$rawmap": [[k, v], ...]}   emit pairs in the given order, no sorting, duplicates allowed
#   {"$nonmin": v, "w": 2|3|5|9} encode int v or a string/container length with a wider head
#   {"$indef": v}                 encode bytes/text/array/map v with indefinite length
#   {"$raw": "hex"}               splice raw bytes
#   {"$wrap": v}                  bytes = canonical-or-raw encoding of v (lets mutated bodies nest)

def head(major: int, n: int, width: int | None = None) -> bytes:
    if width is None:
        if n < 24: width = 1
        elif n < 256: width = 2
        elif n < 65536: width = 3
        elif n < 2**32: width = 5
        else: width = 9
    ai = {1: None, 2: 24, 3: 25, 5: 26, 9: 27}[width]
    if ai is None:
        assert n < 24
        return bytes([(major << 5) | n])
    return bytes([(major << 5) | ai]) + n.to_bytes(width - 1, "big")


def raw(v, width=None) -> bytes:
    if isinstance(v, dict):
        if "$nonmin" in v: return raw(v["$nonmin"], width=v["w"])
        if "$raw" in v: return bytes.fromhex(v["$raw"])
        if "$indef" in v:
            x = v["$indef"]
            if isinstance(x, list): return b"\x9f" + b"".join(raw(i) for i in x) + b"\xff"
            if isinstance(x, dict) and "$bytes" in x:
                b = bytes.fromhex(x["$bytes"]); h = len(b) // 2
                return b"\x5f" + head(2, h) + b[:h] + head(2, len(b) - h) + b[h:] + b"\xff"
            if isinstance(x, dict) and "$map" in x:
                return b"\xbf" + b"".join(raw(k) + raw(val) for k, val in sorted_pairs(x["$map"])) + b"\xff"
            raise TypeError("indef")
        if "$wrap" in v:
            b = raw(v["$wrap"]); return head(2, len(b), width) + b
        if "$bytes" in v:
            b = bytes.fromhex(v["$bytes"]); return head(2, len(b), width) + b
        if "$rawmap" in v:
            ps = v["$rawmap"]; return head(5, len(ps), width) + b"".join(raw(k) + raw(x) for k, x in ps)
        if "$map" in v:
            ps = sorted_pairs(v["$map"]); return head(5, len(ps), width) + b"".join(raw(k) + raw(x) for k, x in ps)
        raise TypeError(v)
    if v is None: return b"\xf6"
    if v is True: return b"\xf5"
    if v is False: return b"\xf4"
    if isinstance(v, int): return head(0, v, width) if v >= 0 else head(1, -1 - v, width)
    if isinstance(v, str):
        b = v.encode(); return head(3, len(b), width) + b
    if isinstance(v, list): return head(4, len(v), width) + b"".join(raw(i) for i in v)
    raise TypeError(type(v))


def sorted_pairs(pairs):
    # RFC 8949 §4.2.1: bytewise lexicographic order of the encoded keys (independent of py_enc's ordering)
    return sorted(pairs, key=lambda kv: raw(kv[0]))

# ---------------------------------------------------------------- map editing helpers

def mget(m, k):
    for kk, v in m["$map"]:
        if kk == k: return v
    raise KeyError(k)


def mset(m, k, v):
    m = copy.deepcopy(m)
    for p in m["$map"]:
        if p[0] == k:
            p[1] = v; return m
    m["$map"].append([k, v]); return m


def mdel(m, k):
    m = copy.deepcopy(m); m["$map"] = [p for p in m["$map"] if p[0] != k]; return m

# ---------------------------------------------------------------- identities used by examples

ACC = det("account-alice", 32)
ACC2 = det("account-alice-successor", 32)
D1, D2, D3, D4 = (det(f"device-{i}", 16) for i in range(1, 5))
RECOVERY_SIGNER = det("recovery-root-public-key-id", 32)
INTRO = det("alice-intro-identity-key", 32)
HOME = det("alice-home-provider-key", 32)
SIGNERS = {"controller": D1, "recovery": RECOVERY_SIGNER, "device": D1, "intro_identity": INTRO, "provider": HOME, "legacy": det("legacy-guardian", 32)}
T0 = 1790208000  # 2026-09-24T00:00:00Z

# ---------------------------------------------------------------- W0 object builders (typical + minimal)

def recovery_policy():
    typical = top("RecoveryPolicy",
        [2, 1],
        [3, [M([0, 1], [1, B(det("passkey-recovery-pub", 32))]), M([0, 2], [1, B(det("recovery-key-pub", 32))])]],
        [4, M([0, 1], [1, True])],
        [5, setof([1, 3, 4])],
        [6, M([0, 259200], [1, 1], [2, True], [3, True])],
        [7, 1])
    minimal = top("RecoveryPolicy", [2, 3], [3, []], [4, M([0, 1], [1, True])], [5, [1]], [6, M([0, 86400], [1, 1])])
    return typical, minimal

RP_TYPICAL, RP_MIN = recovery_policy()
RP_HASH = digest_of("OM-RECOVERY-POLICY-HASH-v1", py_enc(RP_TYPICAL))


def dev_cred(dev, klass):
    return top("DeviceCredential", [2, B(dev)], [3, B(ACC)], [4, 1],
               [5, B(det(f"sign-pk-{dev.hex()}", 32))], [6, B(det(f"kem-pk-{dev.hex()}", 32))],
               [7, klass], [8, setof(["om.msg", "om.control"] if klass == 2 else ["om.msg"])], [9, T0])

DC1 = dev_cred(D1, 2)
DC_MIN = top("DeviceCredential", [2, B(D3)], [3, B(ACC)], [4, 1], [5, B(det("pk3", 32))], [6, B(det("kem3", 32))], [7, 1])

ACS0 = top("AccountControlState", [2, B(ACC)], [3, 0], [5, [B(D1)]], [9, M([0, 1])], [11, 1])
ACS0_HASH = digest_of("OM-ACS-HASH-v1", py_enc(ACS0))
ACS1 = top("AccountControlState",
    [2, B(ACC)], [3, 1], [4, ACS0_HASH],
    [5, setof([B(D1), B(D2)])],
    [7, [B(D3)]],
    [8, [B(D4)]],
    [9, M([0, 1])],
    [10, RP_HASH],
    [11, 1],
    [12, setof([M([0, B(D1)], [1, 3]), M([0, B(D2)], [1, 1])])],
    [13, setof([M([0, B(D1)], [1, digest_of("OM-DEVICE-CREDENTIAL-HASH-v1", py_enc(DC1))]),
                M([0, B(D2)], [1, fake_digest("cred-d2")])])])
ACS1_HASH = digest_of("OM-ACS-HASH-v1", py_enc(ACS1))

HEAD = top("AccountControlHead", [2, B(ACC)], [3, 1], [4, ACS1_HASH], [5, 7])
HEAD_MIN = top("AccountControlHead", [2, B(det("relationship-scoped-subject", 16))], [3, 0], [4, ACS0_HASH])

REQ = top("RecoveryRequest", [2, B(ACC)], [3, 1], [4, B(det("new-device", 16))],
          [5, fake_digest("new-device-cred")], [6, ACS1_HASH], [7, B(det("req-nonce", 16))])
VETO = top("RecoveryVeto", [2, B(ACC)], [3, 1], [4, digest_of("OM-RECOVERY-REQUEST-HASH-v1", py_enc(REQ))])

RET = top("AccountRetirement", [2, B(ACC)], [3, ACS1_HASH], [4, 3], [5, B(ACC2)], [7, T0], [8, 3], [9, 604800])
RET_MIN = top("AccountRetirement", [2, B(ACC)], [3, ACS1_HASH], [4, 1], [8, 1])

NOTICE = top("DeviceEnrollmentNotice", [2, B(ACC)], [3, B(D3)], [4, 1], [5, 1], [6, ACS1_HASH], [7, B(D1)], [8, 1], [9, T0])


def prekey(kid, kind, n, signed):
    item = M([0, kid], [1, kind], [2, B(det(f"prekey-{kid}", n))])
    if signed:
        item = mset(item, 3, B(TEST_SIG))
    return item

AKB = top("AsyncKeyBundle", [2, B(D1)], [5, 1], [6, 4],
          [7, prekey(1, 1, 32, False)], [8, prekey(2, 2, 1184, False)],
          [9, [prekey(10, 3, 32, True), prekey(11, 3, 32, True)]],
          [10, [prekey(20, 4, 1184, True)]],
          [11, prekey(30, 5, 1184, False)],
          [12, B(det("provider-pool-A", 16))], [13, T0 + 30 * 86400])
AKB_MIN = top("AsyncKeyBundle", [2, B(D1)], [5, 1], [6, 1],
              [7, prekey(1, 1, 32, False)], [8, prekey(2, 2, 1184, False)],  # key 8 mandatory (ADR-0126)
              [12, B(det("provider-pool-B", 16))], [13, T0 + 86400])

ALICE = M([0, 1], [1, B(ACC)])
BOB = M([0, 1], [1, B(det("account-bob", 32))])
CONV = det("conversation-alice-bob", 32)
RICH = M([0, 1], [1, [
    M([0, 1], [1, [M([0, 1], [1, "Hi Bob, "]), M([0, 1], [1, "look"], [2, setof([1, 2])]),
                   M([0, 2], [3, "https://example.org/a"], [4, [M([0, 1], [1, "this"])]]),
                   M([0, 3], [4, [M([0, 1], [1, "@Bob"])]], [5, BOB])]]),
    M([0, 3], [2, "print('ok')"], [3, "python"])]])
ATT = M([0, B(det("blob-1", 32))], [1, "image/jpeg"], [2, 183422],
        [3, digest_of("OM-BLOB-CIPHERTEXT-HASH-v1", det("ciphertext-1", 64))],
        [4, M([0, 1], [1, B(det("content-key-1", 32))])],
        [5, ["https://blobs.example.net/o/abc"]], [6, "photo.jpg"], [7, 1280], [8, 960],
        [10, B(det("repair-cap", 32))])


def event(eid, kind, body, **kw):
    pairs = [[2, 1], [3, B(det(f"event-{eid}", 16))], [4, B(CONV)], [5, ALICE], [6, B(D1)],
             [7, M([0, 1790208000123], [1, 0], [2, B(D1[:8])])], [8, kind], [9, 1], [11, body]]
    for k, v in kw.items():
        pairs.append([int(k[1:]), v])
    return top("ConversationEvent", *pairs)

EV_CREATE = event("create", 1, M([0, RICH], [1, [ATT]], [2, B(det("event-prev", 16))], [3, [BOB]], [4, 86400]),
                  k10=["om.richtext.v1"])
EV_CREATE_MIN = event("create-min", 1, M([1, []]))
EV_EDIT = event("edit", 2, M([0, B(det("event-create", 16))], [1, M([0, 1], [1, [M([0, 1], [1, [M([0, 1], [1, "fixed"])]])]])]))
EV_DELETE = event("delete", 3, M([0, B(det("event-create", 16))]))
EV_REACT = event("react", 4, M([0, B(det("event-create", 16))], [1, "\U0001F44D"], [2, True]))
EV_RECEIPT = event("receipt", 5, M([0, 2], [1, setof([B(det("event-create", 16)), B(det("event-edit", 16))])]))
EV_W1_KIND = event("pin", 6, M([0, B(det("event-create", 16))], [1, True]))

DESC = top("ContactDescriptor", [2, B(det("descriptor-1", 16))], [3, "Alice"], [6, [1]],
           [7, [M([0, "https://intro.alice-home.example/g"], [1, B(det("introgate-key", 32))])]],
           [8, ["https://relay.alice-home.example"]], [10, [1]], [11, T0], [12, T0 + 14 * 86400],
           [13, M([0, 1])])
DESC_MIN = top("ContactDescriptor", [2, B(det("descriptor-2", 16))], [6, [1]],
               [7, [M([0, "https://intro.example/x"])]], [12, T0 + 86400], [13, M([0, 1])])


def envelope(type_name: str, body_value, roles=None, body_bytes=None):
    t = TYPES[type_name]
    roles = roles or [t["signed"]["roles"][0]]
    body = body_bytes if body_bytes is not None else py_enc(body_value)
    sigs = [M([0, B(SIGNERS[r])], [1, TEST_PROFILE], [2, ROLE[r]], [3, B(TEST_SIG)]) for r in roles]
    return M([1, B(body)], [2, sigs]), body

DESC_ENV, _ = envelope("ContactDescriptor", DESC)
CAPSULE = top("ContactCapsule", [2, 1], [3, B(det("share-1", 16))], [4, 2], [5, B(INTRO)],
              [6, B(py_enc(DESC_ENV))], [7, T0 + 7 * 86400], [8, 1])
CAPSULE_HASH = digest_of("OM-CONTACT-CAPSULE-HASH-v1", py_enc(CAPSULE))
REFERRAL = top("ReferralAttestation", [2, CAPSULE_HASH], [3, B(det("carol-bob-relationship", 16))], [4, T0])
INVITE = top("Invite", [2, B(det("invite-1", 16))], [3, B(det("invite-secret", 32))],
             [4, [M([0, "https://intro.alice-home.example/g"])]], [5, T0], [6, T0 + 7 * 86400], [7, 1],
             [8, setof([1, 3])], [11, B(det("sponsorship-ticket", 16))])
TICKET = top("IntroductionTicket", [2, B(det("ticket-1", 16))], [3, B(det("service-binding", 32))],
             [4, T0 + 600], [5, 1], [6, 4096], [7, False], [8, False], [9, B(det("ticket-nonce", 16))])
GRANT = top("SponsoredMailboxGrant", [2, B(det("grant-1", 16))], [3, "https://alice-home.example"],
            [4, B(det("deposit-cap", 32))], [5, B(det("drain-cap", 32))],
            [6, M([0, 50 * 2**20], [1, 600], [2, 65536], [3, 20 * 2**20], [4, 14 * 86400])],
            [7, T0 + 90 * 86400], [8, True], [9, False], [10, 1])
RELAYSET = top("RelaySet", [2, 3], [3, fake_digest("relayset-v2")], [4, [
    M([0, "https://relay.alice-home.example"], [1, B(det("cap-r1", 32))], [2, 0], [3, 10], [4, T0 + 30 * 86400],
      [5, setof(["https", "wss"])], [6, "home-a"]),
    M([0, "https://relay.community.example"], [1, B(det("cap-r2", 32))], [2, 1], [4, T0 + 30 * 86400])]])
BLR = top("BlobLocationRecord", [2, B(det("blob-1", 32))], [3, 2], [4, fake_digest("blr-v1")],
          [5, ["https://blobs.example.net/o/abc", "https://mirror.example.org/o/abc"]], [6, T0], [7, T0 + 7 * 86400])

# ADR-0126 first-contact objects (types 19-21), RequestKey, fork bridge
REQKEY = M([0, 1], [1, B(det("request-key-pub", 1216))], [2, T0 + 30 * 86400])
REQKEY_MIN = M([0, TEST_PROFILE], [1, B(det("request-key-pub-min", 32))])  # test profile: no pinned HPKE sizes
REQKEY_DIGEST = digest_of("OM-REQUEST-KEY-HASH-v1", py_enc(REQKEY))
SENDER_INTRO = det("bob-intro-identity-key", 32)
REPLY_URI = "https://relay.bob-home.example"
REPLY_CAP = det("reply-cap", 32)
REPLY_ROUTE = M([0, REPLY_URI], [1, B(REPLY_CAP)], [2, 0], [4, T0 + 7 * 86400])
REPLY_DKM = M([0, 1], [1, B(det("reply-delivery-key-id", 16))],
                [2, B(det("reply-delivery-pk", 1216))],
                [3, sealed_route_binding(REPLY_URI, REPLY_CAP)], [4, T0 + 7 * 86400])
REF_ENV, _ = envelope("ReferralAttestation", REFERRAL, ["device"])
MREQ = top("MessageRequest", [2, B(det("ticket-1", 16))], [3, REQKEY_DIGEST], [4, 1],
           [5, B(det("hpke-enc", 1120))], [6, B(det("mreq-ciphertext", 512))], [7, B(det("admission-proof", 64))])
MREQ_MIN = top("MessageRequest", [2, B(det("ticket-2", 16))], [3, REQKEY_DIGEST], [4, 1],
               [5, B(det("hpke-enc-min", 1120))], [6, B(det("mreq-ct-min", 17))])  # profile 1: enc is exactly Nenc = 1120
MRP = top("MessageRequestPayload", [2, B(det("ticket-1", 16))], [3, REQKEY_DIGEST], [4, B(SENDER_INTRO)],
          [5, B(py_enc(DESC_ENV))], [6, RICH], [7, [B(py_enc(REF_ENV))]], [8, REPLY_ROUTE], [9, REPLY_DKM])
MRP_MIN = top("MessageRequestPayload", [2, B(det("ticket-2", 16))], [3, REQKEY_DIGEST], [4, B(SENDER_INTRO)],
              [5, B(py_enc(DESC_ENV))], [8, REPLY_ROUTE], [9, REPLY_DKM])
IIB = top("IntroIdentityBinding", [2, B(ACC)], [3, B(INTRO)], [4, REQKEY_DIGEST], [5, 1], [6, ACS1_HASH], [7, T0])
IIB_MIN = top("IntroIdentityBinding", [2, B(ACC)], [3, B(INTRO)], [4, REQKEY_DIGEST], [5, 0], [6, ACS0_HASH])
DESC_RK = mset(DESC, 15, REQKEY)
TICKET_RK = mset(TICKET, 11, REQKEY_MIN)
ACS_FORK = mset(mset(ACS1, 3, 2), 23, M(["om.acs.fork_ack", M([0, True], [1, M([0, setof([
    fake_digest("branch-head-a"), fake_digest("branch-head-b")])])])]))

# ADR-0142 established 1:1 sealed delivery. The key is relationship-private and exact-route bound.
SEALED_URI = "https://relay.alice-home.example"
SEALED_CAP = det("cap-r1", 32)
SEALED_DKM = M([0, 1], [1, B(det("sealed-delivery-key-id", 16))],
                 [2, B(det("sealed-delivery-pk", 1216))],
                 [3, sealed_route_binding(SEALED_URI, SEALED_CAP)], [4, T0 + 30 * 86400])
SDKR = top("SealedDeliveryKeyRecord", [2, B(D1)], [3, SEALED_DKM])
SDE = top("SealedDeliveryEnvelope", [2, 1], [3, B(det("sealed-delivery-key-id", 16))],
          [4, B(det("sealed-delivery-id", 16))], [5, B(det("sealed-delivery-enc", 1120))],
          [6, B(det("sealed-delivery-ct", 4096 + 16))], [7, T0 + 3600])

# ADR-0131 rule 9 / ADR-0135 §5 (R-281): a MessageRequest actually sealed with the pinned draft-ietf-hpke-pq-05 suite,
# taken from the V-001 prototype's vector file (Rust HPKE, reproduced by the independent Python HPKE there).
V001 = json.loads((REPO / "verification/prototypes/v001-crypto/vectors/om-v001-proto-vectors.json").read_text())["hpke_message_request"]
HPKE_RK_BYTES = bytes.fromhex(V001["request_key"])
HPKE_RK = M([0, 1], [1, B(bytes.fromhex(V001["pk"]))])
assert py_enc(HPKE_RK) == HPKE_RK_BYTES, "V-001 request_key is not the canonical RequestKey"
HPKE_RK_DIGEST = digest_of("OM-REQUEST-KEY-HASH-v1", HPKE_RK_BYTES)
assert mget(HPKE_RK_DIGEST, 1)["$bytes"] == V001["request_key_digest"], "request_key_digest differs from V-001"
MREQ_HPKE = top("MessageRequest", [2, B(bytes.fromhex(V001["ticket_id"]))], [3, HPKE_RK_DIGEST], [4, 1],
                [5, B(bytes.fromhex(V001["enc"]))], [6, B(bytes.fromhex(V001["ciphertext"]))])
DESC_HPKE_RK = mset(DESC, 15, HPKE_RK)

# ADR-0131 rule 8 / R-280: reciprocal IntroIdentityBinding. Bob (the requester) has his own account and ACS.
ACC_BOB = det("account-bob", 32)
E1, E2 = det("bob-device-1", 16), det("bob-device-2", 16)
ACSB = top("AccountControlState", [2, B(ACC_BOB)], [3, 4], [5, setof([B(E1), B(E2)])], [6, [B(E2)]], [9, M([0, 1])], [11, 1])
ACSB_HASH = digest_of("OM-ACS-HASH-v1", py_enc(ACSB))
IIB_A = top("IntroIdentityBinding", [2, B(ACC)], [3, B(INTRO)], [4, REQKEY_DIGEST], [5, 1], [6, ACS1_HASH])
IIB_B = top("IntroIdentityBinding", [2, B(ACC_BOB)], [3, B(SENDER_INTRO)], [4, REQKEY_DIGEST], [5, 4], [6, ACSB_HASH])

# (object name, id suffix, body value, signer roles or None, bench?)
POSITIVES = [
    ("AccountControlState", "genesis", ACS0, ["controller"], False),
    ("AccountControlState", "typical", ACS1, ["controller", "recovery"], True),
    ("AccountControlHead", "typical", HEAD, None, True),
    ("AccountControlHead", "minimal", HEAD_MIN, None, False),
    ("RecoveryPolicy", "typical", RP_TYPICAL, None, True),
    ("RecoveryPolicy", "minimal", RP_MIN, None, False),
    ("RecoveryRequest", "typical", REQ, None, False),
    ("RecoveryVeto", "typical", VETO, None, False),
    ("AccountRetirement", "legacy", RET, ["legacy"], False),
    ("AccountRetirement", "minimal", RET_MIN, None, False),
    ("DeviceCredential", "typical", DC1, ["device", "controller"], True),
    ("DeviceCredential", "minimal", DC_MIN, ["device"], False),
    ("DeviceEnrollmentNotice", "typical", NOTICE, None, False),
    ("AsyncKeyBundle", "typical", AKB, None, True),
    ("AsyncKeyBundle", "minimal", AKB_MIN, None, False),
    ("ConversationEvent", "create-typical", EV_CREATE, None, True),
    ("ConversationEvent", "create-minimal", EV_CREATE_MIN, None, True),
    ("ConversationEvent", "edit", EV_EDIT, None, False),
    ("ConversationEvent", "delete", EV_DELETE, None, False),
    ("ConversationEvent", "reaction", EV_REACT, None, False),
    ("ConversationEvent", "receipt", EV_RECEIPT, None, False),
    ("ConversationEvent", "w1-kind-opaque-body", EV_W1_KIND, None, False),
    ("ContactDescriptor", "typical", DESC, None, False),
    ("ContactDescriptor", "minimal", DESC_MIN, None, False),
    ("ContactCapsule", "typical", CAPSULE, None, True),
    ("ReferralAttestation", "typical", REFERRAL, ["device"], False),
    ("Invite", "typical", INVITE, None, True),
    ("IntroductionTicket", "typical", TICKET, None, False),
    ("SponsoredMailboxGrant", "typical", GRANT, None, False),
    ("RelaySet", "typical", RELAYSET, None, True),
    ("BlobLocationRecord", "typical", BLR, None, False),
    ("SealedDeliveryKeyRecord", "typical", SDKR, ["device"], True),
    ("SealedDeliveryEnvelope", "typical", SDE, None, True),
    ("MessageRequest", "typical", MREQ, None, True),
    ("MessageRequest", "minimal", MREQ_MIN, None, False),
    ("MessageRequestPayload", "typical", MRP, None, True),
    ("MessageRequestPayload", "minimal", MRP_MIN, None, False),
    ("IntroIdentityBinding", "typical", IIB, None, True),
    ("IntroIdentityBinding", "minimal", IIB_MIN, None, False),
    ("ContactDescriptor", "with-request-key", DESC_RK, None, False),
    ("IntroductionTicket", "with-request-key", TICKET_RK, None, False),
    ("AccountControlState", "fork-bridge", ACS_FORK, ["controller"], False),
    ("MessageRequest", "hpke-pq-05-sealed", MREQ_HPKE, None, False),
    ("ContactDescriptor", "with-hpke-pq-05-request-key", DESC_HPKE_RK, None, False),
    ("AccountControlState", "requester-bob", ACSB, ["controller"], False),
    ("IntroIdentityBinding", "recipient-alice", IIB_A, None, False),
    ("IntroIdentityBinding", "requester-bob", IIB_B, None, False),
]

MINIMALS = {n: v for n, s, v, r, b in POSITIVES if s in ("minimal", "genesis", "create-minimal")}
for n, s, v, r, b in POSITIVES:
    MINIMALS.setdefault(n, v)

# ---------------------------------------------------------------- derived (signature/hash inputs)

def derived_for(type_name: str, body: bytes, env) -> list:
    t = TYPES[type_name]; out = []
    if env is not None:
        for sig in mget(env, 2):
            role, signer = mget(sig, 2), bytes.fromhex(mget(sig, 0)["$bytes"])
            tbs = ds_sig(t["signed"]["label"], TEST_PROFILE, role, signer, body)
            out.append({"kind": "sig_input", "label": t["signed"]["label"], "role": role, "hex": tbs.hex()})
    if t["hash"]:
        hi = ds_hash(t["hash"], body)
        out.append({"kind": "hash", "label": t["hash"], "input_hex": hi.hex(), "sha256": hashlib.sha256(hi).hexdigest()})
    return out


def prekey_derived(body_value) -> list:
    out = []
    items = []
    for k in (7, 8, 9, 10, 11):  # field-key order; arrays flattened in element order
        try: x = mget(body_value, k)
        except KeyError: continue
        items.extend(x if isinstance(x, list) else [x])
    dev = bytes.fromhex(mget(body_value, 2)["$bytes"])
    for it in items:
        try: mget(it, 3)
        except KeyError: continue
        kind = mget(it, 1)
        if kind not in (3, 4):
            continue
        context = [1, B(dev), mget(body_value, 5), mget(body_value, 6), mget(body_value, 12),
                   kind, mget(it, 0), mget(it, 2)]
        tbs = ds_sig("OM-PREKEY-SIG-v1", mget(body_value, 5), 3, dev, py_enc(context))
        out.append({"kind": "sig_input", "label": "OM-PREKEY-SIG-v1", "role": 3, "key_id": mget(it, 0), "hex": tbs.hex()})
    return out

# ---------------------------------------------------------------- vector assembly

VECTORS: list[dict] = []


def emit_positive(name, suffix, body_value, roles, bench):
    t = TYPES[name]
    body = py_enc(body_value)
    assert raw(body_value) == body, f"raw emitter disagrees with python_canonical for {name}/{suffix}"
    env = None
    if t["signed"]:
        env, _ = envelope(name, body_value, roles)
        top_bytes = py_enc(env)
        assert raw(env) == top_bytes
        value = env
    else:
        top_bytes, value = body, body_value
    d = derived_for(name, body, env)
    if name == "AsyncKeyBundle":
        d += prekey_derived(body_value)
    v = {"id": f"{name}/{suffix}", "object": name, "form": "signed" if env else "bare", "expect": "accept",
         "error": None, "cddl": "valid", "bench": bench, "hex": top_bytes.hex(), "value": value, "derived": d}
    if env:
        v["body_value"] = body_value
    VECTORS.append(v)


def emit_negative(name, suffix, error, top_bytes: bytes, cddl="n/a", note=""):
    VECTORS.append({"id": f"{name}/neg/{suffix}", "object": name, "form": "signed" if TYPES[name]["signed"] else "bare",
                    "expect": "reject", "error": error, "cddl": cddl, "bench": False, "hex": top_bytes.hex(),
                    "note": note})


def wrap_top(name, mutated_body, env_roles=None) -> bytes:
    """Encode a (possibly non-canonical) mutated body; wrap in a canonical envelope for signed types."""
    body = raw(mutated_body)
    if TYPES[name]["signed"]:
        env, _ = envelope(name, None, env_roles, body_bytes=body)
        return raw(env)
    return body


def body_mutations(name, base):
    """Generic single-fault mutations of a top-level body. Returns (suffix, error, mutated, cddl)."""
    muts = []
    pairs = base["$map"]
    ks = sorted(p[0] for p in pairs)
    # unsorted keys: swap the first two object-specific keys
    a, b = [p for p in sorted(pairs, key=lambda p: p[0]) if p[0] >= 2][:2]
    rest = [p for p in sorted(pairs, key=lambda p: p[0]) if p not in (a, b)]
    order = [p for p in rest if p[0] < 2] + [b, a] + [p for p in rest if p[0] >= 2]
    muts.append(("unsorted-keys", "unsorted_keys", {"$rawmap": order}, "n/a"))
    # duplicate key: repeat key 2
    srt = sorted(pairs, key=lambda p: p[0])
    dup = []
    for p in srt:
        dup.append(p)
        if p[0] == 2: dup.append(p)
    muts.append(("duplicate-key", "duplicate_key", {"$rawmap": dup}, "n/a"))
    # non-minimal int: object_type as 2-byte head
    muts.append(("nonminimal-int", "noncanonical_int", mset(base, 0, {"$nonmin": mget(base, 0), "w": 2}), "n/a"))
    # non-minimal length: the body map's own item count in a wider head
    muts.append(("nonminimal-map-length", "noncanonical_length", {"$nonmin": base, "w": 2}, "n/a"))
    # indefinite-length map
    muts.append(("indefinite-map", "indefinite_length", {"$indef": base}, "n/a"))
    # unknown critical field (unassigned key 22)
    muts.append(("unknown-field-22", "unknown_field", mset(base, 22, 1), "invalid"))
    # missing required field (key 2 is required on every W0 object)
    muts.append(("missing-field-2", "missing_field", mdel(base, 2), "invalid"))
    # wrong type: key 2 as text string
    muts.append(("wrong-type-key-2", "wrong_type", mset(base, 2, "not-the-right-type"), "invalid"))
    # null instead of absent for key 2
    muts.append(("null-key-2", "null_forbidden", mset(base, 2, None), "invalid"))
    # float in place of object_version
    muts.append(("float-version", "float_forbidden", mset(base, 1, {"$raw": "f93c00"}), "n/a"))
    # tag around object_type
    muts.append(("tagged-type", "tag_forbidden", mset(base, 0, {"$raw": "c1" + raw(mget(base, 0)).hex()}), "n/a"))
    # wrong object type / version
    other = 2 if TYPES[name]["type_id"] != 2 else 1
    muts.append(("wrong-object-type", "wrong_object_type", mset(base, 0, other), "invalid"))
    muts.append(("unsupported-version", "unsupported_version", mset(base, 1, 2), "invalid"))
    # extensions
    muts.append(("unknown-critical-extension", "unknown_critical_extension",
                 mset(base, 23, M(["org.example.must-understand", M([0, True], [1, 1])])), "valid"))
    # ADR-0157/R-346/T-136: the reserved critical prefix is mandatory-to-understand even when
    # a malicious sender lies with critical=false. Old/new V1 registry versions therefore both reject.
    muts.append(("unknown-critical-prefix-marked-noncritical", "unknown_critical_extension",
                 mset(base, 23, M(["om.critical.future.example", M([0, False], [1, 1])])), "valid"))
    muts.append(("empty-extensions", "value_range", mset(base, 23, {"$map": []}), "invalid"))
    return muts


def gen():
    for p in POSITIVES:
        emit_positive(*p)

    # positive: unknown non-critical extension is accepted and preserved (bytes stay inside the signed body)
    for name in ("AccountControlState", "ConversationEvent", "RelaySet"):
        base = MINIMALS[name]
        ext = mset(base, 23, M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])]))
        emit_positive(name, "noncritical-extension-preserved", ext, None, False)
    # R-237 (ADR-0128 rule 18): later-wave content (a W1 custom-emoji node) travels in a named non-critical
    # extension beside a W0-valid body; the same node inside the W0 body is the richtext-custom-emoji-w1 negative
    later = mset(EV_CREATE_MIN, 23, M(["om.richtext.w1-nodes", M([0, False], [1, [M([0, 4], [6, B(b"a")])]])]))
    emit_positive("ConversationEvent", "later-wave-content-in-extension", later, None, False)
    later_att = mset(mset(EV_CREATE_MIN, 11, M([1, [ATT]])), 23,
                     M(["om.attachment.w1-fields", M([0, False], [1, [M([0, 0], [1, B(det("w1-thumbnail", 64))])]])]))
    emit_positive("ConversationEvent", "later-wave-attachment-field-in-extension", later_att, None, False)

    # generic mutations on every W0 type (minimal instance)
    for name in TYPES:
        base = MINIMALS[name]
        for suffix, err, mut, cddl in body_mutations(name, base):
            emit_negative(name, suffix, err, wrap_top(name, mut), cddl)
        # trailing byte after the top-level item
        emit_negative(name, "trailing-bytes", "trailing_bytes", wrap_top(name, base) + b"\x00")
        # truncated: drop the last byte
        emit_negative(name, "truncated", "truncated", wrap_top(name, base)[:-1])
        # over the per-object size limit: a non-critical extension whose value alone is max_bytes long
        big = mset(base, 23, M(["org.example.padding", M([0, False], [1, B(b"\x00" * TYPES[name]["max_bytes"])])]))
        emit_negative(name, "over-size-limit", "size_limit", wrap_top(name, big))
        if TYPES[name]["signed"]:
            # body inside the envelope is non-canonical (keys unsorted) although the envelope is canonical
            srt = sorted(base["$map"], key=lambda p: p[0])
            body = raw({"$rawmap": [srt[1], srt[0]] + srt[2:]})
            env, _ = envelope(name, None, None, body_bytes=body)
            emit_negative(name, "noncanonical-inner-body", "unsorted_keys", raw(env))
            # body bstr carries a trailing byte
            env2, _ = envelope(name, None, None, body_bytes=raw(base) + b"\x00")
            emit_negative(name, "inner-body-trailing-bytes", "trailing_bytes", raw(env2))
            # envelope-level faults
            env3, _ = envelope(name, base)
            emit_negative(name, "envelope-no-signatures", "value_range", raw(mset(env3, 2, [])))
            emit_negative(name, "envelope-unknown-key", "unknown_field", raw(mset(env3, 3, 0)))
            emit_negative(name, "envelope-indefinite-body", "indefinite_length",
                          raw(mset(env3, 1, {"$indef": mget(env3, 1)})))
            sig = mget(env3, 2)[0]
            emit_negative(name, "signature-role-out-of-range", "value_range", raw(mset(env3, 2, [mset(sig, 2, 7)])))
            emit_negative(name, "signature-profile-zero", "value_range", raw(mset(env3, 2, [mset(sig, 1, 0)])))

    # object-specific schema faults
    def neg(name, suffix, err, mutated, cddl="invalid"):
        emit_negative(name, suffix, err, wrap_top(name, mutated), cddl)

    neg("AccountControlState", "set-unsorted", "set_order", mset(ACS1, 5, [B(D2), B(D1)] if D2 > D1 else [B(D1), B(D2)]), "valid")
    neg("AccountControlState", "set-duplicate", "set_order", mset(ACS1, 5, [B(D1), B(D1)]), "valid")
    neg("AccountControlState", "empty-optional-set", "value_range", mset(ACS1, 7, []))
    neg("AccountControlState", "device-id-15-bytes", "value_range", mset(ACS1, 5, [B(D1[:15])]))
    neg("AccountControlState", "account-id-33-bytes", "value_range", mset(ACS1, 2, B(ACC + b"\x00")))
    neg("AccountControlState", "digest-alg-unknown", "value_range", mset(ACS1, 4, M([0, 2], [1, B(det("x", 32))])))
    neg("AccountControlState", "profile-zero", "value_range", mset(ACS1, 11, 0))
    neg("AccountControlState", "negative-generation", "wrong_type", mset(ACS1, 3, -1))
    neg("RecoveryPolicy", "delay-below-24h", "value_range", mset(RP_TYPICAL, 6, M([0, 3600], [1, 1])))
    neg("RecoveryPolicy", "vote-weight-2", "value_range", mset(RP_TYPICAL, 4, M([0, 2], [1, True])))
    neg("RecoveryPolicy", "cosign-false", "value_range", mset(RP_TYPICAL, 4, M([0, 1], [1, False])))
    neg("RecoveryPolicy", "policy-commitment-field", "unknown_field", mset(RP_TYPICAL, 8, fake_digest("self")))
    neg("RecoveryPolicy", "method-kind-7", "value_range", mset(RP_TYPICAL, 3, [M([0, 7], [1, B(det("k", 32))])]))
    neg("SponsoredMailboxGrant", "may-sponsor-further-true", "value_range", mset(GRANT, 9, True))
    neg("SponsoredMailboxGrant", "quota-field-missing", "missing_field", mset(GRANT, 6, M([0, 1], [1, 1], [2, 1], [3, 1])))
    neg("DeviceCredential", "device-class-3", "value_range", mset(DC1, 7, 3))
    neg("DeviceCredential", "capability-set-unsorted", "set_order", mset(DC1, 8, ["om.control", "om.msg"]), "valid")
    neg("AsyncKeyBundle", "retired-acs-generation-key-3", "unknown_field", mset(AKB_MIN, 3, 1))
    neg("AsyncKeyBundle", "retired-acs-state-hash-key-4", "unknown_field", mset(AKB_MIN, 4, ACS1_HASH))
    neg("AsyncKeyBundle", "missing-pq-signed-prekey", "missing_field", mdel(AKB_MIN, 8))
    neg("AsyncKeyBundle", "prekey-kind-6", "value_range", mset(AKB_MIN, 7, M([0, 1], [1, 6], [2, B(det("pk", 32))])))
    neg("AsyncKeyBundle", "prekey-31-bytes", "value_range", mset(AKB_MIN, 7, M([0, 1], [1, 1], [2, B(det("pk", 31))])))
    neg("AsyncKeyBundle", "too-many-pq-one-time", "value_range",
        mset(AKB_MIN, 10, [M([0, i], [1, 4], [2, B(det("q", 32))]) for i in range(33)]))
    neg("ConversationEvent", "reserved-representation-13", "unknown_field", mset(EV_CREATE_MIN, 13, M([0, 1])))
    neg("ConversationEvent", "reserved-foreign-provenance-14", "unknown_field", mset(EV_CREATE_MIN, 14, M([0, 1])))
    neg("ConversationEvent", "kind-body-mismatch", "wrong_type", mset(EV_CREATE_MIN, 8, 2))
    neg("ConversationEvent", "kind-15-unregistered", "value_range", mset(EV_CREATE_MIN, 8, 15))
    neg("ConversationEvent", "event-id-32-bytes", "value_range", mset(EV_CREATE_MIN, 3, B(det("e", 32))))
    neg("ConversationEvent", "principal-kind-4", "value_range", mset(EV_CREATE_MIN, 5, M([0, 4], [1, B(ACC)])))
    neg("ConversationEvent", "reply-to-null", "null_forbidden", mset(EV_CREATE_MIN, 11, M([1, []], [2, None])))
    neg("ConversationEvent", "reaction-asset-reserved", "unknown_field",
        mset(EV_REACT, 11, M([0, B(det("event-create", 16))], [1, "x"], [2, True], [3, B(b"asset")])))
    neg("ConversationEvent", "richtext-custom-emoji-w1", "value_range",
        mset(EV_CREATE_MIN, 11, M([0, M([0, 1], [1, [M([0, 1], [1, [M([0, 4], [6, B(b"a")])]])]])], [1, []])))
    neg("ConversationEvent", "richtext-version-2", "value_range", mset(EV_CREATE_MIN, 11, M([0, M([0, 2], [1, []])], [1, []])))
    neg("ConversationEvent", "mark-set-unsorted", "set_order",
        mset(EV_CREATE_MIN, 11, M([0, M([0, 1], [1, [M([0, 1], [1, [M([0, 1], [1, "x"], [2, [2, 1]])]])]])], [1, []])), "valid")
    neg("ConversationEvent", "attachments-33", "value_range", mset(EV_CREATE_MIN, 11, M([1, [ATT] * 33])))
    neg("ConversationEvent", "text-field-bytes-not-text", "wrong_type",
        mset(EV_CREATE_MIN, 11, M([0, M([0, 1], [1, [M([0, 3], [2, B(b"code")])]])], [1, []])))
    neg("Invite", "scope-4", "value_range", mset(INVITE, 8, [1, 4]))
    neg("Invite", "max-uses-zero", "value_range", mset(INVITE, 7, 0))
    neg("IntroductionTicket", "payload-limit-over-64k", "value_range", mset(TICKET, 6, 65537))
    neg("RelaySet", "priority-256", "value_range", mset(RELAYSET, 4, [mset(mget(RELAYSET, 4)[0], 2, 256)]))
    neg("RelaySet", "entries-empty", "value_range", mset(RELAYSET, 4, []))
    neg("BlobLocationRecord", "version-zero", "value_range", mset(BLR, 3, 0))
    neg("ContactCapsule", "descriptor-not-signed-envelope", "unknown_field", mset(CAPSULE, 6, B(py_enc(DESC))))
    neg("ContactCapsule", "descriptor-inner-unknown-field", "unknown_field",
        mset(CAPSULE, 6, {"$wrap": envelope("ContactDescriptor", mset(DESC, 22, 0))[0]}))
    neg("ContactCapsule", "referral-inside-capsule", "unknown_field", mset(CAPSULE, 9, B(b"referral")))

    # R-281: the profile-1 HPKE suite fixes exact lengths (om-crypto-profiles message_request_hpke)
    neg("MessageRequest", "profile-1-classical-x25519-enc-32", "value_range", mset(MREQ_HPKE, 5, B(det("x25519-enc", 32))), "valid")
    neg("MessageRequest", "profile-1-enc-1121", "value_range", mset(MREQ_HPKE, 5, B(det("enc", 1121))), "valid")
    neg("MessageRequest", "profile-1-pq-only-enc-1088", "value_range", mset(MREQ_HPKE, 5, B(det("enc", 1088))), "valid")
    neg("MessageRequest", "ciphertext-16-bytes", "value_range", mset(MREQ_HPKE, 6, B(det("ct", 16))))
    neg("MessageRequest", "ticket-id-15-bytes", "value_range", mset(MREQ_HPKE, 2, B(det("t", 15))))
    neg("MessageRequest", "request-key-digest-alg-2", "value_range", mset(MREQ_HPKE, 3, M([0, 2], [1, B(det("d", 32))])))
    neg("MessageRequest", "admission-proof-empty", "value_range", mset(MREQ_HPKE, 7, B(b"")))
    neg("MessageRequest", "sender-account-field", "unknown_field", mset(MREQ_HPKE, 8, B(ACC_BOB)))
    neg("MessageRequestPayload", "sender-descriptor-unsigned", "unknown_field", mset(MRP, 5, B(py_enc(DESC))))
    neg("MessageRequestPayload", "referrals-5", "value_range", mset(MRP, 7, [B(py_enc(REF_ENV))] * 5))
    neg("MessageRequestPayload", "sender-intro-identity-31-bytes", "value_range", mset(MRP, 4, B(det("i", 31))))
    neg("MessageRequestPayload", "reply-route-missing", "missing_field", mdel(MRP, 8))
    neg("MessageRequestPayload", "reply-delivery-key-missing", "missing_field", mdel(MRP, 9))
    neg("MessageRequestPayload", "reply-delivery-key-profile1-public-key-32", "value_range",
        mset(MRP, 9, mset(REPLY_DKM, 2, B(det("reply-dk-pk32", 32)))), "valid")
    neg("MessageRequestPayload", "reply-delivery-key-profile1-public-key-1215", "value_range",
        mset(MRP, 9, mset(REPLY_DKM, 2, B(det("reply-dk-pk1215", 1215)))), "valid")
    neg("SealedDeliveryKeyRecord", "profile1-public-key-32", "value_range",
        mset(SDKR, 3, mset(SEALED_DKM, 2, B(det("sealed-pk32", 32)))), "valid")
    neg("SealedDeliveryKeyRecord", "profile1-public-key-1217", "value_range",
        mset(SDKR, 3, mset(SEALED_DKM, 2, B(det("sealed-pk1217", 1217)))), "valid")
    neg("SealedDeliveryEnvelope", "profile1-classical-x25519-enc-32", "value_range",
        mset(SDE, 5, B(det("sealed-enc32", 32))), "valid")
    neg("SealedDeliveryEnvelope", "profile1-enc-1121", "value_range",
        mset(SDE, 5, B(det("sealed-enc1121", 1121))), "valid")
    neg("SealedDeliveryEnvelope", "ciphertext-not-padding-bucket", "value_range",
        mset(SDE, 6, B(det("sealed-ct1041", 1041))), "valid")
    neg("SealedDeliveryEnvelope", "delivery-id-15-bytes", "value_range",
        mset(SDE, 4, B(det("sealed-id15", 15))))
    neg("IntroIdentityBinding", "intro-identity-31-bytes", "value_range", mset(IIB, 3, B(det("i", 31))))
    neg("IntroIdentityBinding", "acs-hash-alg-2", "value_range", mset(IIB, 6, M([0, 2], [1, B(det("h", 32))])))
    neg("IntroIdentityBinding", "account-id-31-bytes", "value_range", mset(IIB, 2, B(ACC[:31])))
    neg("IntroIdentityBinding", "request-key-digest-missing", "missing_field", mdel(IIB, 4))
    # RequestKey inside ContactDescriptor key 15 and IntroductionTicket key 11
    for host, base, k in (("ContactDescriptor", DESC_HPKE_RK, 15), ("IntroductionTicket", mset(TICKET, 11, HPKE_RK), 11)):
        rk = mget(base, k)
        neg(host, f"key{k}-profile-1-public-key-32", "value_range", mset(base, k, mset(rk, 1, B(det("pk", 32)))), "valid")
        neg(host, f"key{k}-profile-1-public-key-1217", "value_range", mset(base, k, mset(rk, 1, B(det("pk", 1217)))), "valid")
        neg(host, f"key{k}-profile-zero", "value_range", mset(base, k, mset(rk, 0, 0)))
        neg(host, f"key{k}-unknown-field", "unknown_field", mset(base, k, mset(rk, 3, 1)))
        neg(host, f"key{k}-public-key-missing", "missing_field", mset(base, k, mdel(rk, 1)))
        neg(host, f"key{k}-not-a-map", "wrong_type", mset(base, k, B(det("rk", 32))))
    # om.acs.fork_ack (ForkAck, ADR-0126/0127)
    def fork(heads, critical=True):
        return mset(ACS_FORK, 23, M(["om.acs.fork_ack", M([0, critical], [1, M([0, heads])])]))
    ha, hb = fake_digest("branch-head-a"), fake_digest("branch-head-b")
    neg("AccountControlState", "fork-ack-single-head", "value_range", cddl="valid", mutated=fork([ha]))
    neg("AccountControlState", "fork-ack-heads-unsorted", "set_order", fork(list(reversed(setof([ha, hb])))), "valid")
    neg("AccountControlState", "fork-ack-heads-duplicate", "set_order", fork([ha, ha]), "valid")
    neg("AccountControlState", "fork-ack-marked-noncritical", "value_range", fork(setof([ha, hb]), critical=False), "valid")
    neg("AccountControlState", "fork-ack-17-heads", "value_range", cddl="valid", mutated=fork(setof([fake_digest(f"h{i}") for i in range(17)])))
    neg("AccountControlState", "fork-ack-head-alg-2", "value_range", cddl="valid", mutated=fork(setof([M([0, 2], [1, B(det("a", 32))]), hb])))
    neg("AccountControlState", "fork-ack-value-not-a-map", "wrong_type", cddl="valid", mutated=
        mset(ACS_FORK, 23, M(["om.acs.fork_ack", M([0, True], [1, [ha, hb]])])))
    # ADR-0141: registered LegacyPolicy commitment is critical. A wire parser that knows the
    # extension registry accepts the correctly critical commitment and rejects a false criticality bit.
    legacy_policy = M([0, 1], [1, [M([0, B(det("legacy-guardian-id", 16))], [1, TEST_PROFILE],
                                     [2, B(det("legacy-guardian-vk", 32))])]],
                      [2, 1], [3, 86400], [4, 86400], [5, B(ACC2)])
    legacy_critical = mset(ACS1, 23, M(["om.acs.legacy_policy", M([0, True], [1, legacy_policy])]))
    emit_positive("AccountControlState", "legacy-policy-critical", legacy_critical, ["controller"], False)
    neg("AccountControlState", "legacy-policy-marked-noncritical", "value_range",
        mset(ACS1, 23, M(["om.acs.legacy_policy", M([0, False], [1, legacy_policy])])), "valid")
    # R-237: later-wave attachment field inside a W0 body is rejected ...
    att_w1 = mset(ATT, 11, B(det("w1-thumbnail", 64)))
    neg("ConversationEvent", "attachment-later-wave-field-11", "unknown_field", mset(EV_CREATE_MIN, 11, M([1, [att_w1]])))

    # decoder-level faults carried in a bare object's extension value (RecoveryPolicy is unsigned)
    def ext_value(v):
        return mset(RP_MIN, 23, M(["org.example.x", M([0, False], [1, v])]))

    def dneg(suffix, err, v, cddl="n/a"):
        emit_negative("RecoveryPolicy", "decoder/" + suffix, err, raw(ext_value(v)), cddl)

    dneg("invalid-utf8", "invalid_utf8", {"$raw": "62c328"})
    dneg("undefined", "invalid_simple", {"$raw": "f7"})
    dneg("simple-16", "invalid_simple", {"$raw": "f0"})
    dneg("simple-two-byte", "invalid_simple", {"$raw": "f820"})
    dneg("float64", "float_forbidden", {"$raw": "fb3ff0000000000000"})
    dneg("tag-bignum", "tag_forbidden", {"$raw": "c24101"})
    dneg("indefinite-bytes", "indefinite_length", {"$indef": B(b"abcd")})
    dneg("indefinite-array", "indefinite_length", {"$indef": [1, 2]})
    dneg("break-outside-indefinite", "indefinite_length", {"$raw": "ff"})
    dneg("reserved-ai-28", "malformed", {"$raw": "1c"})
    dneg("nonminimal-uint-8byte", "noncanonical_int", {"$nonmin": 1, "w": 9})
    dneg("nonminimal-negint", "noncanonical_int", {"$nonmin": -5, "w": 2})
    dneg("nonminimal-text-length", "noncanonical_length", {"$nonmin": "abc", "w": 2})
    dneg("nonminimal-array-length", "noncanonical_length", {"$nonmin": [1], "w": 3})
    dneg("mixed-key-types", "key_type", {"$rawmap": [[1, 1], ["a", 2]]})
    dneg("negative-map-key", "key_type", {"$rawmap": [[-1, 1]]})
    dneg("bytes-map-key", "key_type", {"$rawmap": [[B(b"k"), 1]]})
    dneg("nested-duplicate-key", "duplicate_key", {"$rawmap": [["a", 1], ["a", 2]]})
    dneg("nested-unsorted", "unsorted_keys", {"$rawmap": [["b", 1], ["a", 2]]})
    dneg("uint-24-vs-nint", "key_type", {"$rawmap": [[24, 1], [-1, 2]]})
    dneg("declared-bstr-4GiB", "size_limit", {"$raw": "5affffffff00"})
    dneg("declared-array-4097", "count_limit", {"$raw": "991001" + "00" * 4097})
    dneg("declared-map-2^32", "count_limit", {"$raw": "bbffffffffffffffff"})
    nest = 1
    for _ in range(21): nest = [nest]
    emit_positive("RecoveryPolicy", "decoder/depth-24-boundary", ext_value(nest), None, False)
    emit_positive("RecoveryPolicy", "decoder/any-uint64-max-and-nint", ext_value([2**64 - 1, -(2**63), -1]), None, False)
    deeper = [nest]
    dneg("depth-25", "depth_limit", deeper)
    emit_negative("RecoveryPolicy", "decoder/empty-input", "truncated", b"")
    emit_negative("RecoveryPolicy", "decoder/two-top-level-items", "trailing_bytes", py_enc(RP_MIN) + py_enc(RP_MIN))


# ---------------------------------------------------------------- R-280 reciprocal IntroIdentityBinding sequences
# ADR-0131 rule 8: recipient sends its binding, the requester verifies it and only then sends its own; each side is
# ACTIVE only after it verified the peer's binding AND sent its own. Verification (both checkers implement it from the
# ADR, independently): exactly one signature, role device, valid signature (placeholder: bytes == insecure_test_signature),
# signer active and not quarantined/revoked in the peer's trusted ACS; body account = ACS account, intro identity = the
# one used in first contact, request_key_digest = the request's, acs_generation / acs_state_hash = the trusted head.
SEQUENCES: list[dict] = []
BAD_SIG = b"bad_signature_placeholder"


def sig_env(body_value, signer, role=3, sig=TEST_SIG, copies=1):
    s = M([0, B(signer)], [1, TEST_PROFILE], [2, role], [3, B(sig)])
    return ("env", M([1, B(py_enc(body_value))], [2, [s] * copies]), body_value)


def seq(sid, perspective, peer_intro, peer_acs, steps, expect):
    ctx = {"peer_intro_expected": peer_intro.hex(), "request_key_digest": mget(REQKEY_DIGEST, 1)["$bytes"],
           "peer_acs_body": py_enc(peer_acs).hex(),
           "trusted_peer_acs_hash": mget(digest_of("OM-ACS-HASH-v1", py_enc(peer_acs)), 1)["$bytes"]}
    out = []
    for s in steps:
        if isinstance(s, tuple):
            _, env_v, body_v = s
            env = py_enc(env_v)
            out.append({"op": "receive_peer_binding", "binding": env.hex()})
            VECTORS.append({"id": f"IntroIdentityBinding/seq/{sid}/{len(out)}", "object": "IntroIdentityBinding", "form": "signed",
                            "expect": "accept", "error": None, "cddl": "valid", "bench": False, "hex": env.hex(),
                            "value": env_v, "body_value": body_v,
                            "derived": derived_for("IntroIdentityBinding", py_enc(body_v), env_v),
                            "note": "codec-valid; the sequence decides semantic acceptance"})
        else:
            out.append({"op": s})
    assert len(expect) == len(out)
    SEQUENCES.append({"id": sid, "perspective": perspective, "context": ctx, "steps": out,
                      "expect": [{"result": r, "state": st} for r, st in expect]})


def gen_sequences():
    U, A = "ACCEPTED_UNBOUND", "ACTIVE"
    good_b = sig_env(IIB_B, E1)
    # recipient (Alice) view: peer is Bob
    R = lambda sid, steps, exp: seq(sid, "recipient", SENDER_INTRO, ACSB, steps, exp)
    R("recipient-happy", ["accept_request", "send_own_binding", good_b], [("ok", U), ("ok", U), ("ok", A)])
    R("recipient-only-own-binding-stays-unbound", ["accept_request", "send_own_binding"], [("ok", U), ("ok", U)])
    R("recipient-peer-wrong-intro-identity", ["accept_request", "send_own_binding", sig_env(mset(IIB_B, 3, B(det("other-intro", 32))), E1)],
      [("ok", U), ("ok", U), ("rejected", U)])
    R("recipient-peer-request-key-digest-mismatch", ["accept_request", "send_own_binding", sig_env(mset(IIB_B, 4, fake_digest("other-rk")), E1)],
      [("ok", U), ("ok", U), ("rejected", U)])
    R("recipient-peer-signed-by-quarantined-device", ["accept_request", "send_own_binding", sig_env(IIB_B, E2)],
      [("ok", U), ("ok", U), ("rejected", U)])
    R("recipient-peer-signed-by-unknown-device", ["accept_request", "send_own_binding", sig_env(IIB_B, det("stranger", 16))],
      [("ok", U), ("ok", U), ("rejected", U)])
    R("recipient-peer-stale-acs-head", ["accept_request", "send_own_binding", sig_env(mset(mset(IIB_B, 5, 3), 6, fake_digest("old-acs-b")), E1)],
      [("ok", U), ("ok", U), ("rejected", U)])
    R("recipient-peer-bad-signature", ["accept_request", "send_own_binding", sig_env(IIB_B, E1, sig=BAD_SIG)],
      [("ok", U), ("ok", U), ("rejected", U)])
    R("recipient-peer-signed-with-intro-identity-role", ["accept_request", "send_own_binding", sig_env(IIB_B, E1, role=4)],
      [("ok", U), ("ok", U), ("rejected", U)])
    R("recipient-peer-binding-before-own-then-happy", ["accept_request", good_b, "send_own_binding", good_b],
      [("ok", U), ("rejected", U), ("ok", U), ("ok", A)])
    # requester (Bob) view: peer is Alice; Bob must not send before verifying Alice's binding
    Q = lambda sid, steps, exp: seq(sid, "requester", INTRO, ACS1, steps, exp)
    good_a = sig_env(IIB_A, D1)
    Q("requester-happy", [good_a, "send_own_binding"], [("ok", U), ("ok", A)])
    Q("requester-send-before-verify-refused", ["send_own_binding", good_a, "send_own_binding"], [("refused", U), ("ok", U), ("ok", A)])
    Q("requester-peer-account-mismatch", [sig_env(mset(IIB_A, 2, B(ACC2)), D1), "send_own_binding"], [("rejected", U), ("refused", U)])
    Q("requester-peer-signed-by-revoked-device", [sig_env(IIB_A, D4), "send_own_binding"], [("rejected", U), ("refused", U)])
    Q("requester-peer-signed-by-messaging-device", [sig_env(IIB_A, D3), "send_own_binding"], [("ok", U), ("ok", A)])
    Q("requester-peer-two-signatures", [sig_env(IIB_A, D1, copies=2), "send_own_binding"], [("rejected", U), ("refused", U)])


def compress_hex(h: str):
    """Represent runs of >=256 identical bytes as {"fill": "xx", "len": n} to keep the file small."""
    b = bytes.fromhex(h); out = []; i = 0; start = 0
    while i < len(b):
        j = i
        while j < len(b) and b[j] == b[i]: j += 1
        if j - i >= 256:
            if start < i: out.append(b[start:i].hex())
            out.append({"fill": f"{b[i]:02x}", "len": j - i}); start = j
        i = j
    if start < len(b): out.append(b[start:].hex())
    return h if len(out) == 1 and isinstance(out[0], str) else out


def main(out_path):
    gen()
    gen_sequences()
    ids = [v["id"] for v in VECTORS]
    assert len(ids) == len(set(ids)), "duplicate vector ids"
    for v in VECTORS:
        v["hex"] = compress_hex(v["hex"])
        if v["expect"] == "reject":
            v.pop("value", None)
    doc = {
        "status": "Feasibility prototype for V-005. Not product code. Not for production. Generated by gen_vectors.py.",
        "profile": "OM-CBOR-v1 (spec/registries/om-cbor-profile.json)",
        "signature_placeholder": {"crypto_profile": TEST_PROFILE, "bytes_ascii": TEST_SIG.decode(),
                                  "note": "insecure_test_signature: vectors fix signature *inputs*, not signatures"},
        "count": len(VECTORS),
        "vectors": VECTORS,
        "intro_binding_sequences": SEQUENCES,
    }
    text = json.dumps(doc, separators=(",", ":"), sort_keys=True) + "\n"
    Path(out_path).write_text(text)
    # deterministic gzip (mtime=0, no filename) for the committed golden copy
    import gzip, io
    buf = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buf, mtime=0, compresslevel=9) as gz:
        gz.write(text.encode())
    Path(str(out_path) + ".gz").write_bytes(buf.getvalue())
    pos = sum(1 for v in VECTORS if v["expect"] == "accept")
    print(f"wrote {len(VECTORS)} vectors ({pos} positive, {len(VECTORS) - pos} negative) to {out_path}")


if __name__ == "__main__":
    main(sys.argv[1])
