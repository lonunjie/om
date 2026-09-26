#!/usr/bin/env python3
"""V-016 (review F-6/F-7): RFC 9578 token type 0x0002 issuance and finalization with RFC 9474 blind RSA.

Implements Blind / BlindSign / Finalize (RFC 9474 section 4) for RSABSSA-SHA384-PSS-Deterministic with integer RSA
math, the RFC 9578 section 6 TokenRequest/TokenResponse/Token framing, and the section 6.5 id-RSASSA-PSS SPKI
token_key_id. Checks:

- blind_rsa_rfc9474_kat: RFC 9474 A.3 reproduced byte for byte; A.3 and A.4 cross-profile signatures must not verify.
- privacy_pass_issuance_rfc9578_kat: all five RFC 9578 A.2 vectors reproduced (SPKI, key id, TokenRequest,
  TokenResponse, Token).
- issuer_transcript_unlinkable: what the issuer receives and returns (TokenRequest, TokenResponse) contains neither
  the redeemed token_input nor its authenticator nor any encoding of them, and every issuer transcript is
  algebraically consistent with every redeemed token (a blinding factor exists for each pairing), so the issuer view
  cannot tell which issuance produced which redeemed token.

Profiles are named exactly: token type 0x0002 is RSABSSA-SHA384-PSS-Deterministic (sLen 48, RFC 9578 section 6.4),
never RSABSSA-SHA384-PSSZERO-* (sLen 0). The structural unlinkability test is evidence that the issuer's messages
carry no redemption-side value; it is not a proof of RFC 9474 blindness, which rests on the RFC's analysis and on r
being uniform. Mutations: 0x0002 bound to the PSSZERO variant (KATs), and an unblinded client r = 1 (transcript).

Written by a Claude agent (verification executor). Deterministic verdicts (fresh keys and blinds, fixed checks).
"""
from __future__ import annotations

import hashlib
import json
import math
import secrets
import sys
from dataclasses import dataclass
from pathlib import Path

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding, rsa

ROOT = Path(__file__).resolve().parents[2]
VECTORS = ROOT / "verification/vectors/privacy-pass-blind-rsa-vectors.json"


@dataclass(frozen=True)
class RSABSSAVariant:        # RFC 9474 section 5: SHA-384, MGF1-SHA-384, PrepareIdentity; only sLen differs here
    name: str
    salt_len: int


RSABSSA_SHA384_PSS_DETERMINISTIC = RSABSSAVariant("RSABSSA-SHA384-PSS-Deterministic", 48)
RSABSSA_SHA384_PSSZERO_DETERMINISTIC = RSABSSAVariant("RSABSSA-SHA384-PSSZERO-Deterministic", 0)
VARIANTS = {v.name: v for v in (RSABSSA_SHA384_PSS_DETERMINISTIC, RSABSSA_SHA384_PSSZERO_DETERMINISTIC)}
TOKEN_TYPE_0002 = b"\x00\x02"
TOKEN_TYPE_0002_VARIANT = RSABSSA_SHA384_PSS_DETERMINISTIC   # RFC 9578 section 6.4: sLen = 48
NK = 256
# RFC 9578 section 6.5 / 8.2.2: SPKI with id-RSASSA-PSS and RSASSA-PSS-params(sha384, mgf1-sha384, saltLength 48),
# 2048-bit key; the RSAPublicKey DER follows. Checked against every RFC 9578 A.2 pkI below.
PSS_SPKI_PREFIX_2048 = bytes.fromhex(
    "30820152303d06092a864886f70d01010a3030a00d300b0609608648016503040202a11a301806092a864886f70d010108300b06096086"
    "48016503040202a2030201300382010f00")


def pss(variant: RSABSSAVariant):
    return padding.PSS(mgf=padding.MGF1(hashes.SHA384()), salt_length=variant.salt_len)


def i2b(x: int, n: int) -> bytes:
    return x.to_bytes(n, "big")


def b2i(b: bytes) -> int:
    return int.from_bytes(b, "big")


def mgf1(seed: bytes, length: int) -> bytes:
    out = b"".join(hashlib.sha384(seed + i2b(c, 4)).digest() for c in range(-(-length // 48)))
    return out[:length]


def emsa_pss_encode(msg: bytes, em_bits: int, salt: bytes) -> bytes:   # RFC 8017 section 9.1.1, SHA-384
    em_len, h_len = -(-em_bits // 8), 48
    if em_len < h_len + len(salt) + 2:
        raise ValueError("encoding error")
    h = hashlib.sha384(b"\x00" * 8 + hashlib.sha384(msg).digest() + salt).digest()
    db = b"\x00" * (em_len - len(salt) - h_len - 2) + b"\x01" + salt
    masked = bytearray(a ^ b for a, b in zip(db, mgf1(h, em_len - h_len - 1)))
    masked[0] &= 0xFF >> (8 * em_len - em_bits)
    return bytes(masked) + h + b"\xbc"


# ---- RFC 9474 section 4 ------------------------------------------------------------------------------------------
def blind(pk, msg: bytes, variant: RSABSSAVariant, salt: bytes | None = None, r: int | None = None):
    n, e = pk.public_numbers().n, pk.public_numbers().e
    k = (n.bit_length() + 7) // 8
    salt = secrets.token_bytes(variant.salt_len) if salt is None else salt
    if len(salt) != variant.salt_len:
        raise ValueError(f"{variant.name} needs a {variant.salt_len}-byte salt")
    m = b2i(emsa_pss_encode(msg, n.bit_length() - 1, salt))
    if math.gcd(m, n) != 1:
        raise ValueError("invalid input")
    while r is None or math.gcd(r, n) != 1:
        r = secrets.randbelow(n - 1) + 1
    return i2b(m * pow(r, e, n) % n, k), pow(r, -1, n)


def blind_sign(sk, blinded_msg: bytes) -> bytes:
    pn = sk.private_numbers()
    n, e, d = pn.public_numbers.n, pn.public_numbers.e, pn.d
    m = b2i(blinded_msg)
    if m >= n:
        raise ValueError("message representative out of range")
    s = pow(m, d, n)
    if pow(s, e, n) != m:
        raise ValueError("signing failure")
    return i2b(s, len(blinded_msg))


def finalize(pk, msg: bytes, blind_sig: bytes, inv: int, variant: RSABSSAVariant) -> bytes:
    n = pk.public_numbers().n
    k = (n.bit_length() + 7) // 8
    if len(blind_sig) != k:
        raise ValueError("unexpected input size")
    sig = i2b(b2i(blind_sig) * inv % n, k)
    pk.verify(sig, msg, pss(variant), hashes.SHA384())      # raises InvalidSignature
    return sig


def verifies(pk, sig: bytes, msg: bytes, variant: RSABSSAVariant) -> bool:
    try:
        pk.verify(sig, msg, pss(variant), hashes.SHA384())
        return True
    except InvalidSignature:
        return False


# ---- RFC 9578 section 6 ------------------------------------------------------------------------------------------
def spki_0002(pk) -> bytes:
    assert pk.key_size == 2048
    return PSS_SPKI_PREFIX_2048 + pk.public_bytes(serialization.Encoding.DER, serialization.PublicFormat.PKCS1)


def token_input(nonce: bytes, challenge: bytes, key_id: bytes) -> bytes:
    return TOKEN_TYPE_0002 + nonce + hashlib.sha256(challenge).digest() + key_id


class Issuer:   # the recipient's referral issuer: holds the private key, sees only TokenRequest bytes
    def __init__(self, sk=None):
        self.sk = sk or rsa.generate_private_key(public_exponent=65537, key_size=2048)
        self.spki = spki_0002(self.sk.public_key())
        self.key_id = hashlib.sha256(self.spki).digest()
        self.transcript: list[tuple[bytes, bytes]] = []    # (TokenRequest received, TokenResponse returned)

    def respond(self, req: bytes) -> bytes:
        if req[:2] != TOKEN_TYPE_0002 or req[2] != self.key_id[-1] or len(req) != 3 + NK:
            raise ValueError("422")
        resp = blind_sign(self.sk, req[3:])
        self.transcript.append((req, resp))
        return resp


class Client:
    def __init__(self, spki: bytes, variant=TOKEN_TYPE_0002_VARIANT):
        self.pk, self.key_id, self.variant = serialization.load_der_public_key(spki), hashlib.sha256(spki).digest(), variant

    def request(self, challenge: bytes, nonce: bytes, salt=None, r=None):
        ti = token_input(nonce, challenge, self.key_id)
        blinded, inv = blind(self.pk, ti, self.variant, salt, r)
        return TOKEN_TYPE_0002 + self.key_id[-1:] + blinded, (ti, inv)

    def finalize(self, state, resp: bytes) -> bytes:
        ti, inv = state
        return ti + finalize(self.pk, ti, resp, inv, self.variant)


def issue(issuer: Issuer, nonce: bytes, challenge: bytes, variant=TOKEN_TYPE_0002_VARIANT, r=None) -> bytes:
    c = Client(issuer.spki, variant)
    req, st = c.request(challenge, nonce, r=r)
    return c.finalize(st, issuer.respond(req))


def load_vectors() -> dict:
    return json.loads(VECTORS.read_text())


# ---- checks ------------------------------------------------------------------------------------------------------
def rfc9474_kat(mutate=False):
    bad, V = [], load_vectors()["rfc9474"]
    variant = RSABSSA_SHA384_PSSZERO_DETERMINISTIC if mutate else TOKEN_TYPE_0002_VARIANT
    if TOKEN_TYPE_0002_VARIANT.name != "RSABSSA-SHA384-PSS-Deterministic" or TOKEN_TYPE_0002_VARIANT.salt_len != 48:
        bad.append("token type 0x0002 bound to the wrong RSABSSA variant")
    for sec, own in (("A.3", variant), ("A.4", RSABSSA_SHA384_PSSZERO_DETERMINISTIC)):
        v = {k: (bytes.fromhex(x) if k not in ("section", "variant", "salt_len") else x) for k, x in V[sec].items()}
        n, e, d, p, q = (b2i(v[x]) for x in ("n", "e", "d", "p", "q"))
        sk = rsa.RSAPrivateNumbers(p, q, d, d % (p - 1), d % (q - 1), pow(q, -1, p), rsa.RSAPublicNumbers(e, n)).private_key()
        pk, k = sk.public_key(), len(v["n"])
        if v["variant"] != own.name and sec == "A.3" and not mutate:
            bad.append(f"{sec} variant label mismatch")
        try:
            if emsa_pss_encode(v["prepared_msg"], n.bit_length() - 1, v["salt"]) != v["encoded_msg"]:
                bad.append(f"{sec} encoded_msg mismatch")
            blinded, inv = blind(pk, v["prepared_msg"], own, v["salt"], pow(b2i(v["inv"]), -1, n))
            if blinded != v["blinded_msg"] or inv != b2i(v["inv"]):
                bad.append(f"{sec} blinded_msg mismatch")
            if blind_sign(sk, blinded) != v["blind_sig"]:
                bad.append(f"{sec} blind_sig mismatch")
            if finalize(pk, v["prepared_msg"], v["blind_sig"], inv, own) != v["sig"]:
                bad.append(f"{sec} sig mismatch")
        except (ValueError, InvalidSignature) as ex:
            bad.append(f"{sec} under {own.name}: {type(ex).__name__} {ex}")
        other = [x for x in VARIANTS.values() if x.salt_len != V[sec]["salt_len"]][0]
        if verifies(pk, v["sig"], v["prepared_msg"], other):
            bad.append(f"{sec} signature verifies under {other.name} (profiles interchangeable)")
        if k != 512:
            bad.append(f"{sec} unexpected modulus size")
    return bad


def rfc9578_kat(mutate=False):
    bad = []
    variant = RSABSSA_SHA384_PSSZERO_DETERMINISTIC if mutate else TOKEN_TYPE_0002_VARIANT
    for v in load_vectors()["rfc9578"]["vectors"]:
        h = {k: bytes.fromhex(x) for k, x in v.items() if k != "id"}
        tag = f"RFC 9578 A.2 vector {v['id']}"
        issuer = Issuer(serialization.load_pem_private_key(h["skI"], None))
        if issuer.spki != h["pkI"]:
            bad.append(f"{tag}: SPKI encoding differs from pkI"); continue
        tok = h["token"]
        if tok[:98] != token_input(h["nonce"], h["token_challenge"], issuer.key_id):
            bad.append(f"{tag}: token_input mismatch")
        c = Client(h["pkI"], variant)
        try:
            req, st = c.request(h["token_challenge"], h["nonce"], h["salt"], b2i(h["blind"]))
            if req != h["token_request"]:
                bad.append(f"{tag}: TokenRequest mismatch")
            resp = issuer.respond(req)
            if resp != h["token_response"]:
                bad.append(f"{tag}: TokenResponse mismatch")
            if c.finalize(st, resp) != tok:
                bad.append(f"{tag}: Token mismatch")
        except (ValueError, InvalidSignature) as ex:
            bad.append(f"{tag} under {variant.name}: {type(ex).__name__} {ex}")
    return bad


def transcript_problems(issuer: Issuer, tokens: list[bytes]) -> list[str]:
    bad = []
    pk = issuer.sk.public_key()
    n, e = pk.public_numbers().n, pk.public_numbers().e
    heads = {req[:3] for req, _ in issuer.transcript}
    if len(heads) != 1:
        bad.append("TokenRequest header varies across issuances")
    for tok in tokens:
        ti, sig = tok[:98], tok[98:]
        forms = {"token_input": ti, "nonce": ti[2:34], "authenticator": sig, "sha384(token_input)": hashlib.sha384(ti).digest(),
                 "token_input as int": i2b(b2i(ti), NK), "EMSA-PSS(token_input)": i2b(pow(b2i(sig), e, n), NK)}
        for req, resp in issuer.transcript:
            for name, f in forms.items():
                if f in req or f in resp:
                    bad.append(f"issuer transcript contains {name}")
    for req, resp in issuer.transcript:           # every transcript fits every token under r = blind_sig / sig
        z, bs = b2i(req[3:]), b2i(resp)
        for tok in tokens:
            s = b2i(tok[98:])
            r = bs * pow(s, -1, n) % n
            if pow(r, e, n) * pow(s, e, n) % n != z:
                bad.append("an issuer transcript is inconsistent with some redeemed token (linkable)")
    return sorted(set(bad))


def issuer_transcript_unlinkable(mutate=False):
    bad = []
    issuer = Issuer()
    ch = b"OM-REFERRAL-PRIVPASS-CHALLENGE-v1 unlinkability probe"
    toks = [issue(issuer, bytes([i]) * 32, ch, r=1 if mutate else None) for i in range(1, 5)]
    bad += transcript_problems(issuer, toks)
    # detector control: the pre-F-7 direct-signing issuer (receives token_input, returns the final signature)
    direct = Issuer(issuer.sk)
    ti = token_input(b"\x09" * 32, ch, direct.key_id)
    sig = direct.sk.sign(ti, pss(TOKEN_TYPE_0002_VARIANT), hashes.SHA384())
    direct.transcript.append((TOKEN_TYPE_0002 + direct.key_id[-1:] + ti, sig))
    if not transcript_problems(direct, [ti + sig]):
        bad.append("detector missed a direct-signing issuer")
    return bad


CHECKS = {"blind_rsa_rfc9474_kat": rfc9474_kat, "privacy_pass_issuance_rfc9578_kat": rfc9578_kat,
          "issuer_transcript_unlinkable": issuer_transcript_unlinkable}


def main() -> int:
    res = {k: (fn(), fn(mutate=True)) for k, fn in CHECKS.items()}
    passed = all(not ok and mut for ok, mut in res.values())
    print(json.dumps({"passed": passed, **{k: {"problems": ok, "mutation_caught": bool(mut), "mutation_problems": mut[:2]}
                                           for k, (ok, mut) in res.items()}}, indent=1))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
