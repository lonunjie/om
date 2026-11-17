#!/usr/bin/env python3
"""ADR-0192 device-link ceremony vectors. Feasibility evidence, not product code.

Independent of the Rust implementation: a minimal canonical CBOR encoder (unsigned ints, byte strings, text strings,
arrays: all the construction uses), hashlib/hmac for SHA-256, HKDF and HMAC, and a pure-Python HChaCha20,
ChaCha20-Poly1305 for the one sealed-message vector. Inputs are fixed, so the output is deterministic.

Usage: gen_device_link_ceremony_vectors.py OUT.json
"""
from __future__ import annotations

import hashlib
import hmac
import json
import struct
import sys

LBL_MAC = "OM-DEVICE-LINK-CEREMONY-MAC-v1"
LBL_KDF = "OM-DEVICE-LINK-CEREMONY-KDF-v1"
LBL_AD = "OM-DEVICE-LINK-CEREMONY-AEAD-AD-v1"
LBL_MBX = "OM-DEVICE-LINK-CEREMONY-MAILBOX-HASH-v1"
LBL_MANIFEST = "OM-DEVICE-LINK-CEREMONY-MANIFEST-HASH-v1"  # ADR-0195
LBL_CONT = "OM-DEVICE-LINK-CEREMONY-CONTINUATION-MAC-v1"  # ADR-0195


class B(bytes):
    """A CBOR byte string."""


def head(major: int, n: int) -> bytes:
    if n < 24:
        return bytes([major << 5 | n])
    for ai, w in ((24, 1), (25, 2), (26, 4), (27, 8)):
        if n < 1 << (8 * w):
            return bytes([major << 5 | ai]) + n.to_bytes(w, "big")
    raise ValueError(n)


def enc(v) -> bytes:
    if isinstance(v, bool):
        raise TypeError("no booleans in this construction")
    if isinstance(v, int):
        return head(0, v)
    if isinstance(v, B):
        return head(2, len(v)) + bytes(v)
    if isinstance(v, str):
        b = v.encode()
        return head(3, len(b)) + b
    if isinstance(v, list):
        return head(4, len(v)) + b"".join(enc(x) for x in v)
    raise TypeError(type(v))


def ds_hash(label: str, body: bytes) -> bytes:
    return hashlib.sha256(enc([label, B(body)])).digest()


def hkdf(salt: bytes, ikm: bytes, info: bytes, n: int = 32) -> bytes:
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    out, t, i = b"", b"", 1
    while len(out) < n:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest()
        out += t
        i += 1
    return out[:n]


def context(expiry_s, home_uri: bytes, a_cap: bytes, b_recv: bytes, version: int = 2) -> bytes:
    return enc([version, expiry_s, B(home_uri), B(a_cap), B(b_recv)])


def keys(ctx: bytes, secret: bytes) -> dict:
    cid = ds_hash(LBL_KDF, ctx)
    out = {"ceremony_id": cid, "prk": hmac.new(cid, secret, hashlib.sha256).digest()}
    for p in ("req", "resp", "code", "bind"):
        out[p] = hkdf(cid, secret, enc([LBL_KDF, p]))
    return out


def ad(cid: bytes, role: int) -> bytes:
    return enc([LBL_AD, B(cid), role])


def macin(role: int, body: bytes) -> bytes:
    return enc([LBL_MAC, role, B(body)])


def code_of(k_code: bytes, request: bytes):
    m = hmac.new(k_code, macin(1, request), hashlib.sha256).digest()
    return m, "%06d" % (int.from_bytes(m[:8], "big") % 1_000_000)


def binding(k_bind, req_digest, approver, state_hash, notice_digest, roster, hist) -> bytes:
    body = enc([B(req_digest), B(approver), B(state_hash), B(notice_digest), roster, hist])
    return hmac.new(k_bind, macin(2, body), hashlib.sha256).digest()


def manifest_digest(states, creds) -> bytes:
    return ds_hash(LBL_MANIFEST, enc([[B(x) for x in states], [B(x) for x in creds]]))


def cont_binding(k_bind, tag, digest, n_states, n_creds, n_chunks) -> bytes:
    body = enc([B(tag), B(digest), n_states, n_creds, n_chunks])
    return hmac.new(k_bind, enc([LBL_CONT, B(body)]), hashlib.sha256).digest()


# ------------------------------------------------------------ ChaCha20-Poly1305 and HChaCha20 (RFC 8439, XChaCha draft)
M32 = 0xFFFFFFFF


def rotl(x, n):
    return ((x << n) & M32) | (x >> (32 - n))


def qr(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & M32; s[d] = rotl(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & M32; s[b] = rotl(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & M32; s[d] = rotl(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & M32; s[b] = rotl(s[b] ^ s[c], 7)


def rounds(s):
    for _ in range(10):
        qr(s, 0, 4, 8, 12); qr(s, 1, 5, 9, 13); qr(s, 2, 6, 10, 14); qr(s, 3, 7, 11, 15)
        qr(s, 0, 5, 10, 15); qr(s, 1, 6, 11, 12); qr(s, 2, 7, 8, 13); qr(s, 3, 4, 9, 14)


CONST = [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574]


def hchacha20(key: bytes, n16: bytes) -> bytes:
    s = CONST + list(struct.unpack("<8I", key)) + list(struct.unpack("<4I", n16))
    rounds(s)
    return struct.pack("<8I", *(s[0:4] + s[12:16]))


def chacha_block(key: bytes, counter: int, nonce12: bytes) -> bytes:
    init = CONST + list(struct.unpack("<8I", key)) + [counter] + list(struct.unpack("<3I", nonce12))
    s = init[:]
    rounds(s)
    return struct.pack("<16I", *((a + b) & M32 for a, b in zip(s, init)))


def poly1305(key: bytes, msg: bytes) -> bytes:
    r = int.from_bytes(key[:16], "little") & 0x0FFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF
    sk = int.from_bytes(key[16:], "little")
    p = (1 << 130) - 5
    a = 0
    for i in range(0, len(msg), 16):
        blk = msg[i : i + 16]
        a = (a + int.from_bytes(blk + b"\x01", "little")) * r % p
    return ((a + sk) & ((1 << 128) - 1)).to_bytes(16, "little")


def pad16(b):
    return b + b"\x00" * (-len(b) % 16)


def xchacha_seal(key: bytes, nonce24: bytes, aad: bytes, pt: bytes) -> bytes:
    sub = hchacha20(key, nonce24[:16])
    n12 = b"\x00" * 4 + nonce24[16:]
    otk = chacha_block(sub, 0, n12)[:32]
    ct = b""
    for i in range(0, len(pt), 64):
        ks = chacha_block(sub, 1 + i // 64, n12)
        ct += bytes(x ^ y for x, y in zip(pt[i : i + 64], ks))
    mac = poly1305(otk, pad16(aad) + pad16(ct) + struct.pack("<QQ", len(aad), len(ct)))
    return ct + mac


# ------------------------------------------------------------ vectors
def main(out: str) -> None:
    secret = bytes(range(0x10, 0x20))
    a_cap = bytes([0xA1] * 32)
    b_recv = bytes([0xB2] * 32)
    home = b"https://home.example.test"
    expiry = 1_790_000_300
    request = enc([0, 1, B(bytes([1] * 16)), B(bytes([2] * 32)), B(bytes([3] * 32)), B(bytes([4] * 16)), "New device"])
    approver = bytes([9] * 16)
    state_hash = hashlib.sha256(b"state").digest()
    notice = b"notice bytes"
    notice_digest = hashlib.sha256(notice).digest()
    req_digest = hashlib.sha256(request).digest()

    ctx = context(expiry, home, a_cap, b_recv)
    k = keys(ctx, secret)
    m, code = code_of(k["code"], request)
    bind = binding(k["bind"], req_digest, approver, state_hash, notice_digest, 2, 3)
    nonce = bytes(range(24))
    pt = b"sealed ceremony plaintext"
    sealed = {r: xchacha_seal(k["req" if r == 1 else "resp"], nonce, ad(k["ceremony_id"], r), pt) for r in (1, 2)}

    def hx(b):
        return b.hex()

    v = {
        "description": "ADR-0192 device-link ceremony vectors; independent pure-Python construction (generator and Node cross-check).",
        "codec": "gen_device_link_ceremony_vectors.py minimal canonical CBOR (uint, bstr, tstr, array); hashlib/hmac; pure-Python XChaCha20-Poly1305",
        "labels": {"mac": LBL_MAC, "kdf": LBL_KDF, "aead_ad": LBL_AD, "mailbox": LBL_MBX},
        "inputs": {
            "secret": hx(secret), "a_cap": hx(a_cap), "b_recv": hx(b_recv), "home_uri": hx(home), "expiry_s": expiry,
            "request": hx(request), "approver": hx(approver), "state_hash": hx(state_hash), "notice": hx(notice),
            "roster_count": 2, "history_count": 3, "nonce": hx(nonce), "plaintext": hx(pt),
        },
        "expected": {
            "ctx": hx(ctx), "ceremony_id": hx(k["ceremony_id"]), "prk": hx(k["prk"]),
            "k_req": hx(k["req"]), "k_resp": hx(k["resp"]), "k_code": hx(k["code"]), "k_bind": hx(k["bind"]),
            "ad_req": hx(ad(k["ceremony_id"], 1)), "ad_resp": hx(ad(k["ceremony_id"], 2)),
            "code_mac": hx(m), "code": code, "request_digest": hx(req_digest), "notice_digest": hx(notice_digest),
            "binding": hx(bind),
            "sealed_req": hx(sealed[1]), "sealed_resp": hx(sealed[2]),
            "mailbox": hx(ds_hash(LBL_MBX, b_recv)[:16]),
        },
    }

    # Code digits for chosen MAC prefixes: leading zeros, the modulus, and values near it.
    cases = []
    for n in (0, 7421, 999_999, 1_000_000, 1_000_001, 2**64 - 1, 123_456_789_012):
        mm = n.to_bytes(8, "big") + bytes(24)
        cases.append({"m": hx(mm), "digits": "%06d" % (n % 1_000_000), "grouped": "%06d" % (n % 1_000_000)})
    for c in cases:
        c["grouped"] = c["digits"][:3] + " " + c["digits"][3:]
    v["code_digits"] = cases

    # Negatives: every change of a bound input changes all keys; each is a "must differ" vector.
    def differs(name, ctx2, secret2=secret):
        k2 = keys(ctx2, secret2)
        return {"name": name, "ctx": hx(ctx2), "ceremony_id": hx(k2["ceremony_id"]),
                "k_req": hx(k2["req"]), "k_resp": hx(k2["resp"]), "k_code": hx(k2["code"]), "k_bind": hx(k2["bind"])}

    v["context_changes"] = [
        differs("home_uri", context(expiry, home + b"x", a_cap, b_recv)),
        differs("expiry", context(expiry + 1, home, a_cap, b_recv)),
        differs("version", context(expiry, home, a_cap, b_recv, version=1)),
        differs("a_cap", context(expiry, home, bytes([0xA2] * 32), b_recv)),
        differs("b_recv", context(expiry, home, a_cap, bytes([0xB3] * 32))),
        differs("secret", ctx, bytes(range(0x11, 0x21))),
    ]
    reenc = request + b"\x00"
    other_m, other_code = code_of(k["code"], reenc)
    v["negatives"] = {
        "reencoded_request_code_mac": hx(other_m),
        "code_mac_replayed_as_binding_differs": hx(hmac.new(k["bind"], macin(1, request), hashlib.sha256).digest()),
        "binding_wrong_role_input": hx(hmac.new(k["bind"], macin(1, enc([B(req_digest), B(approver), B(state_hash), B(notice_digest), 2, 3])), hashlib.sha256).digest()),
        "binding_other_request": hx(binding(k["bind"], hashlib.sha256(b"other").digest(), approver, state_hash, notice_digest, 2, 3)),
        "binding_other_state": hx(binding(k["bind"], req_digest, approver, hashlib.sha256(b"s2").digest(), notice_digest, 2, 3)),
        "binding_other_notice": hx(binding(k["bind"], req_digest, approver, state_hash, hashlib.sha256(b"n2").digest(), 2, 3)),
        "binding_other_roster": hx(binding(k["bind"], req_digest, approver, state_hash, notice_digest, 3, 3)),
        "binding_other_history": hx(binding(k["bind"], req_digest, approver, state_hash, notice_digest, 2, 4)),
        "binding_other_approver": hx(binding(k["bind"], req_digest, bytes([8] * 16), state_hash, notice_digest, 2, 3)),
        "sealed_req_under_resp_ad": hx(xchacha_seal(k["req"], nonce, ad(k["ceremony_id"], 2), pt)),
        "sealed_req_under_resp_key": hx(xchacha_seal(k["resp"], nonce, ad(k["ceremony_id"], 1), pt)),
    }
    # ADR-0195: the continuation form's manifest digest and tag over a fixed (opaque) state and credential set.
    states = [bytes([0x50 + i]) * (40 + i) for i in range(3)]
    creds = [bytes([0x60 + i]) * (30 + i) for i in range(4)]
    dg = manifest_digest(states, creds)
    ctag = cont_binding(k["bind"], bind, dg, 3, 4, 2)
    v["labels"]["manifest"] = LBL_MANIFEST
    v["labels"]["continuation_mac"] = LBL_CONT
    v["continuation"] = {
        "states": [hx(x) for x in states], "credentials": [hx(x) for x in creds], "chunks": 2,
        "digest": hx(dg), "cont_binding": hx(ctag),
        "negatives": {
            "digest_states_reordered": hx(manifest_digest([states[1], states[0], states[2]], creds)),
            "digest_creds_reordered": hx(manifest_digest(states, [creds[1], creds[0]] + creds[2:])),
            "digest_state_dropped": hx(manifest_digest(states[:2], creds)),
            "digest_cred_dropped": hx(manifest_digest(states, creds[:3])),
            "digest_state_moved_to_creds": hx(manifest_digest(states[:2], [states[2]] + creds)),
            "digest_byte_changed": hx(manifest_digest([states[0][:-1] + b"\x00"] + states[1:], creds)),
            "cont_binding_other_digest": hx(cont_binding(k["bind"], bind, manifest_digest(states[:2], creds), 3, 4, 2)),
            "cont_binding_other_states": hx(cont_binding(k["bind"], bind, dg, 2, 4, 2)),
            "cont_binding_other_creds": hx(cont_binding(k["bind"], bind, dg, 3, 5, 2)),
            "cont_binding_other_chunks": hx(cont_binding(k["bind"], bind, dg, 3, 4, 3)),
            "cont_binding_other_binding": hx(cont_binding(k["bind"], hashlib.sha256(b"x").digest(), dg, 3, 4, 2)),
            "binding_used_as_cont_binding": hx(bind),
            "cont_binding_under_code_key": hx(cont_binding(k["code"], bind, dg, 3, 4, 2)),
        },
    }
    json.dump(v, open(out, "w"), indent=1, sort_keys=True)
    open(out, "a").write("\n")


if __name__ == "__main__":
    main(sys.argv[1])
