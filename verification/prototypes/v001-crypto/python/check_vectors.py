# SPDX-License-Identifier: Apache-2.0
"""Feasibility prototype for V-001. Not product code. Not for production.

Second, independent implementation (Python; different language, different libraries) that
consumes the Rust prototype's vectors and must reproduce them from the public specs:

  * Signal PQXDH rev 3 §2.2 KDF, §3.3 DH1..DH4, SK, AD
  * XEdDSA rev 1 §3.4 verification (u -> Edwards y = (u-1)/(u+1), sign 0, then Ed25519 verify)
  * FIPS 203 ML-KEM-768 via kyber-py (keygen from d||z, encaps with m, decaps)
  * Double Ratchet rev 4 §3 (KDF_RK, KDF_CK, skipped keys, DH ratchet), §6 KDF_HYBRID, §7.1 split
  * RFC 8452 AES-256-GCM-SIV via `cryptography` (deterministic, so ciphertexts must match bit-for-bit)
  * the prototype header CBOR [profile, dh, pn, n, scka] via this file's own RFC 8949 codec

Only SPQR message keys (pq_mk) are taken as inputs: they come from Signal's `spqr` crate.
Also runs the official NIST ACVP ML-KEM-768 KATs against kyber-py (an ML-KEM implementation
independent of the Rust side's libcrux).

Written by a Claude agent, from the specifications, without reading the Rust code's logic.
"""

from __future__ import annotations

import hashlib
import hmac
import json
import sys

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCMSIV
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from kyber_py.ml_kem import ML_KEM_768

H = bytes.fromhex
P = 2**255 - 19


# ---------------------------------------------------------------- minimal deterministic CBOR
def cbor_head(major: int, n: int) -> bytes:
    if n < 24:
        return bytes([major << 5 | n])
    for ai, size in ((24, 1), (25, 2), (26, 4), (27, 8)):
        if n < 1 << (8 * size):
            return bytes([major << 5 | ai]) + n.to_bytes(size, "big")
    raise ValueError("int too large")


def cbor_enc(v) -> bytes:
    if isinstance(v, bool) or v is None:
        return {False: b"\xf4", True: b"\xf5", None: b"\xf6"}[v]
    if isinstance(v, int):
        return cbor_head(0, v)
    if isinstance(v, bytes):
        return cbor_head(2, len(v)) + v
    if isinstance(v, str):
        b = v.encode()
        return cbor_head(3, len(b)) + b
    if isinstance(v, list):
        return cbor_head(4, len(v)) + b"".join(cbor_enc(x) for x in v)
    if isinstance(v, dict):
        return cbor_head(5, len(v)) + b"".join(cbor_enc(k) + cbor_enc(v[k]) for k in sorted(v))
    raise TypeError(type(v))


def cbor_dec(b: bytes):
    def item(i: int):
        ib = b[i]
        major, ai = ib >> 5, ib & 31
        i += 1
        if ai < 24:
            n = ai
        elif ai in (24, 25, 26, 27):
            size = 1 << (ai - 24)
            n = int.from_bytes(b[i : i + size], "big")
            i += size
        else:
            raise ValueError("indefinite/reserved")
        if major == 0:
            return n, i
        if major == 2:
            return b[i : i + n], i + n
        if major == 3:
            return b[i : i + n].decode(), i + n
        if major == 4:
            out = []
            for _ in range(n):
                x, i = item(i)
                out.append(x)
            return out, i
        if major == 5:
            out = {}
            for _ in range(n):
                k, i = item(i)
                out[k], i = item(i)
            return out, i
        if major == 7 and n in (20, 21, 22):
            return {20: False, 21: True, 22: None}[n], i
        raise ValueError("unsupported")

    v, end = item(0)
    if end != len(b) or cbor_enc(v) != b:
        raise ValueError("trailing bytes or non-canonical CBOR")
    return v


# ---------------------------------------------------------------- primitives
def hkdf(salt: bytes, ikm: bytes, info: bytes, n: int) -> bytes:
    return HKDF(algorithm=hashes.SHA256(), length=n, salt=salt, info=info).derive(ikm)


def x25519_pub(sk: bytes) -> bytes:
    return X25519PrivateKey.from_private_bytes(sk).public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )


def dh(sk: bytes, pk: bytes) -> bytes:
    return X25519PrivateKey.from_private_bytes(sk).exchange(X25519PublicKey.from_public_bytes(pk))


def xeddsa_verify(u_bytes: bytes, msg: bytes, sig: bytes) -> bool:
    """XEdDSA §3.4: reject u >= p or s >= 2^253; A = convert_mont(u) with sign 0; Ed25519 check."""
    u = int.from_bytes(u_bytes, "little")
    if u >= P or sig[63] & 0xE0:
        return False
    y = (u - 1) * pow(u + 1, P - 2, P) % P
    try:
        Ed25519PublicKey.from_public_bytes(y.to_bytes(32, "little")).verify(sig, msg)
        return True
    except (InvalidSignature, ValueError):
        return False


# ---------------------------------------------------------------- EC Double Ratchet (spec §3)
class DR:
    MAX_SKIP = 1000

    def __init__(self, consts):
        self.rk_info = consts["dr_rk_info"].encode()
        self.skipped = {}

    def kdf_rk(self, rk, dh_out):
        o = hkdf(rk, dh_out, self.rk_info, 64)
        return o[:32], o[32:]

    @staticmethod
    def kdf_ck(ck):
        return hmac.new(ck, b"\x02", "sha256").digest(), hmac.new(ck, b"\x01", "sha256").digest()

    def init_alice(self, sk, bob_pub, dhs_secret):
        self.dhs, self.dhr = dhs_secret, bob_pub
        self.rk, self.cks = self.kdf_rk(sk, dh(dhs_secret, bob_pub))
        self.ckr, self.ns, self.nr, self.pn = None, 0, 0, 0

    def init_bob(self, sk, spk_secret):
        self.dhs, self.dhr, self.rk = spk_secret, None, sk
        self.cks = self.ckr = None
        self.ns = self.nr = self.pn = 0

    def send_key(self):
        self.cks, mk = self.kdf_ck(self.cks)
        n = self.ns
        self.ns += 1
        return n, mk

    def skip(self, until):
        if self.nr + self.MAX_SKIP < until:
            raise ValueError("too many skipped")
        if self.ckr is not None:
            while self.nr < until:
                self.ckr, mk = self.kdf_ck(self.ckr)
                self.skipped[(self.dhr, self.nr)] = mk
                self.nr += 1

    def receive_key(self, dh_pub, pn, n, new_secret):
        if (dh_pub, n) in self.skipped:
            return self.skipped.pop((dh_pub, n)), False
        stepped = False
        if dh_pub != self.dhr:
            self.skip(pn)
            self.pn, self.ns, self.nr, self.dhr = self.ns, 0, 0, dh_pub
            self.rk, self.ckr = self.kdf_rk(self.rk, dh(self.dhs, self.dhr))
            if new_secret is None:
                raise ValueError("vector lacks the new ratchet secret for a DH ratchet step")
            self.dhs = new_secret
            self.rk, self.cks = self.kdf_rk(self.rk, dh(self.dhs, self.dhr))
            stepped = True
        self.skip(n)
        self.ckr, mk = self.kdf_ck(self.ckr)
        self.nr += 1
        return mk, stepped


# ---------------------------------------------------------------- vector checking
class Checker:
    def __init__(self):
        self.results = []

    def eq(self, name, a, b):
        ok = a == b
        self.results.append((name, ok))
        if not ok:
            print(f"MISMATCH {name}", file=sys.stderr)
        return ok


def check_case(c, consts, ck: Checker):
    n = c["name"]
    bob, alice, hs = c["bob"], c["alice"], c["handshake"]
    ec, kem = bytes([consts["ec_type_byte"]]), bytes([consts["kem_type_byte"]])
    ik_b, spk_b = x25519_pub(H(bob["ik_secret"])), x25519_pub(H(bob["spk_secret"]))
    ck.eq(f"{n}/bob_ik_pub", ik_b.hex(), bob["ik_pub"])
    ck.eq(f"{n}/bob_spk_pub", spk_b.hex(), bob["spk_pub"])
    seed = H(bob["pq_seed_d_z"])
    ek, dk = ML_KEM_768._keygen_internal(seed[:32], seed[32:])
    ck.eq(f"{n}/mlkem_ek_from_seed", hashlib.sha256(ek).hexdigest(), bob["pq_pub_sha256"])
    ck.eq(f"{n}/xeddsa_spk_sig_verifies", xeddsa_verify(ik_b, ec + spk_b, H(bob["spk_sig"])), True)
    ck.eq(f"{n}/xeddsa_pq_sig_verifies", xeddsa_verify(ik_b, kem + ek, H(bob["pq_sig"])), True)
    ck.eq(f"{n}/xeddsa_wrong_msg_rejected", xeddsa_verify(ik_b, kem + ek[:-1] + b"\x00", H(bob["pq_sig"])), False)

    # Bundle CBOR: map 1..10 as the prototype defines it (decoded with this file's codec).
    bundle = cbor_dec(H(bob["bundle_cbor"]))
    ck.eq(f"{n}/bundle_fields", (bundle[1], bundle[3], bundle[7], bundle[8]), (ik_b, spk_b, ek, H(bob["pq_sig"])))

    ik_a_sk, ek_sk = H(alice["ik_secret"]), H(alice["ek_secret"])
    ck.eq(f"{n}/alice_ik_pub", x25519_pub(ik_a_sk).hex(), alice["ik_pub"])
    ck.eq(f"{n}/alice_ek_pub", x25519_pub(ek_sk).hex(), alice["ek_pub"])
    k, ct = ML_KEM_768._encaps_internal(ek, H(alice["kem_encaps_m"]))
    ck.eq(f"{n}/mlkem_ct", ct.hex(), hs["ct"])
    ck.eq(f"{n}/mlkem_ss", k.hex(), hs["ss"])
    ck.eq(f"{n}/mlkem_decaps", ML_KEM_768._decaps_internal(dk, ct).hex(), hs["ss"])

    dhs = [dh(ik_a_sk, spk_b), dh(ek_sk, ik_b), dh(ek_sk, spk_b)]
    if bob["opk_secret"] is not None:
        dhs.append(dh(ek_sk, x25519_pub(H(bob["opk_secret"]))))
    ck.eq(f"{n}/dh_outputs", [d.hex() for d in dhs], hs["dh"])
    # Responder-side DH values must agree with initiator-side ones (§3.4).
    bob_side = [dh(H(bob["spk_secret"]), x25519_pub(ik_a_sk)), dh(H(bob["ik_secret"]), x25519_pub(ek_sk)), dh(H(bob["spk_secret"]), x25519_pub(ek_sk))]
    ck.eq(f"{n}/dh_responder_agrees", [d.hex() for d in bob_side], hs["dh"][:3])
    sk = hkdf(b"\x00" * 32, b"\xff" * 32 + b"".join(dhs) + k, consts["pqxdh_info"].encode(), 32)
    ck.eq(f"{n}/pqxdh_sk", sk.hex(), hs["sk"])
    ad = ec + x25519_pub(ik_a_sk) + ec + ik_b
    ck.eq(f"{n}/pqxdh_ad", ad.hex(), hs["ad"])
    split = hkdf(b"\x00" * 32, sk, consts["tr_split_info"].encode(), 64)
    ck.eq(f"{n}/tr_split", (split[:32].hex(), split[32:].hex()), (hs["sk_ec"], hs["sk_scka"]))
    init = cbor_dec(H(hs["init_header_cbor"]))
    ck.eq(f"{n}/init_header", (init[1], init[2], init[3], init[4], init[5], init.get(6)),
          (x25519_pub(ik_a_sk), x25519_pub(ek_sk), ct, bob["spk_id"], bob["pq_id"], bob["opk_id"]))

    # Ratchets: Alice from her recorded initial ratchet secret, Bob from SPK_B (DR §7.1).
    party = {"A": DR(consts), "B": DR(consts)}
    party["A"].init_alice(split[:32], spk_b, H(alice["ratchet_init_secret"]))
    party["B"].init_bob(split[:32], H(bob["spk_secret"]))
    sent = {}
    info = consts["tr_protocol_info"].encode()
    for ev in c["events"]:
        lab = f"{n}/{ev['type']}:{ev['label']}"
        if ev["type"] == "send":
            d = party[ev["from"]]
            num, ec_mk = d.send_key()
            ck.eq(f"{lab}/ec_mk", ec_mk.hex(), ev["ec_mk"])
            hdr_bytes = H(ev["header"])
            hdr = cbor_dec(hdr_bytes)
            ck.eq(f"{lab}/header", (hdr[0], hdr[1], hdr[2], hdr[3]), (consts["header_profile"], x25519_pub(d.dhs), d.pn, num))
            okm = hkdf(H(ev["pq_mk"]), ec_mk, info, 44)
            ck.eq(f"{lab}/kdf_hybrid", (okm[:32].hex(), okm[32:].hex()), (ev["aead_key"], ev["aead_nonce"]))
            ctext = AESGCMSIV(okm[:32]).encrypt(okm[32:], H(ev["plaintext"]), ad + hdr_bytes)
            ck.eq(f"{lab}/ciphertext", ctext.hex(), ev["ciphertext"])
            ck.eq(f"{lab}/init_prefix_until_reply", ev["has_init_prefix"], ev["from"] == "A" and party["A"].ckr is None)
            sent[ev["label"]] = (hdr, hdr_bytes, ctext)
        else:
            d = party[ev["to"]]
            hdr, hdr_bytes, ctext = sent[ev["label"]]
            new = H(ev["new_ratchet_secret"]) if ev["new_ratchet_secret"] else None
            ec_mk, stepped = d.receive_key(hdr[1], hdr[2], hdr[3], new)
            ck.eq(f"{lab}/ec_mk", ec_mk.hex(), ev["ec_mk"])
            ck.eq(f"{lab}/dh_ratchet_step_matches", stepped, new is not None)
            okm = hkdf(H(ev["pq_mk"]), ec_mk, info, 44)
            pt = AESGCMSIV(okm[:32]).decrypt(okm[32:], ctext, ad + hdr_bytes)
            ck.eq(f"{lab}/plaintext", pt.hex(), ev["plaintext"])
            try:
                AESGCMSIV(okm[:32]).decrypt(okm[32:], ctext, ad + hdr_bytes[:-1] + bytes([hdr_bytes[-1] ^ 1]))
                ck.eq(f"{lab}/tampered_header_rejected", False, True)
            except Exception:
                ck.eq(f"{lab}/tampered_header_rejected", True, True)


# ---------------------------------------------------------------- ACVP (kyber-py)
def ek_check(ek: bytes, k: int = 3) -> bool:
    """FIPS 203 §7.2 encapsulation key type + modulus check."""
    if len(ek) != 384 * k + 32:
        return False
    body = ek[: 384 * k]
    for i in range(0, len(body), 3):
        b0, b1, b2 = body[i], body[i + 1], body[i + 2]
        if (b0 | (b1 & 0x0F) << 8) >= 3329 or (b1 >> 4 | b2 << 4) >= 3329:
            return False
    return True


def dk_check(dk: bytes, k: int = 3) -> bool:
    """FIPS 203 §7.3 decapsulation key type + hash check."""
    if len(dk) != 768 * k + 96:
        return False
    return hashlib.sha3_256(dk[384 * k : 768 * k + 32]).digest() == dk[768 * k + 32 : 768 * k + 64]


def acvp(keygen: dict, encdec: dict) -> dict:
    counts: dict[str, list[int]] = {}

    def tally(name, ok):
        c = counts.setdefault(name, [0, 0])
        c[0] += 1
        c[1] += bool(ok)

    for g in keygen["testGroups"]:
        if g["parameterSet"] != "ML-KEM-768":
            continue
        for t in g["tests"]:
            ek, dk = ML_KEM_768._keygen_internal(H(t["d"]), H(t["z"]))
            tally("keyGen", ek == H(t["ek"]) and dk == H(t["dk"]))
    for g in encdec["testGroups"]:
        if g["parameterSet"] != "ML-KEM-768":
            continue
        f = g["function"]
        for t in g["tests"]:
            if f == "encapsulation":
                k, c = ML_KEM_768._encaps_internal(H(t["ek"]), H(t["m"]))
                tally(f, k == H(t["k"]) and c == H(t["c"]))
            elif f == "decapsulation":
                tally(f, ML_KEM_768._decaps_internal(H(t["dk"]), H(t["c"])) == H(t["k"]))
            elif f == "encapsulationKeyCheck":
                tally(f, ek_check(H(t["ek"])) == t["testPassed"])
            elif f == "decapsulationKeyCheck":
                tally(f, dk_check(H(t["dk"])) == t["testPassed"])
    total = sum(c[0] for c in counts.values())
    passed = sum(c[1] for c in counts.values())
    return {
        "implementation": "kyber-py (ML_KEM_768 internal functions) + FIPS 203 §7.2/§7.3 checks written here",
        "groups": {k: {"cases": v[0], "passed": v[1]} for k, v in sorted(counts.items())},
        "total": total,
        "passed": passed,
        "all_passed": total > 0 and total == passed,
    }


def main() -> int:
    vectors = json.load(open(sys.argv[1]))
    ck = Checker()
    for c in vectors["cases"]:
        check_case(c, vectors["constants"], ck)
    out = {
        "vector_checks": len(ck.results),
        "vector_checks_passed": sum(ok for _, ok in ck.results),
        "vector_messages": sum(1 for c in vectors["cases"] for e in c["events"] if e["type"] == "send"),
        "vector_cases": [c["name"] for c in vectors["cases"]],
        "failures": [name for name, ok in ck.results if not ok],
    }
    out["vectors_ok"] = out["vector_checks"] == out["vector_checks_passed"] and out["vector_checks"] > 0
    if len(sys.argv) > 3:
        out["acvp"] = acvp(json.load(open(sys.argv[2])), json.load(open(sys.argv[3])))
    ok = out["vectors_ok"] and out.get("acvp", {"all_passed": True})["all_passed"]
    print(json.dumps(out, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
