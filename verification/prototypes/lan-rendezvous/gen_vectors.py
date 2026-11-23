#!/usr/bin/env python3
"""LanDeliveryProfile-v1 rendezvous construction vectors (ADR-0158 sections 1 to 2), written with the standard library only,
independently of the Rust product code. Output: verification/vectors/lan-rendezvous-vectors.json (replayed by
crates/om-core/tests/lan_rendezvous_vectors.rs). Run: python3 gen_vectors.py [--check]"""
import hashlib, hmac, json, struct, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "verification/vectors/lan-rendezvous-vectors.json"
EPOCH = 900


def head(major, n):
    if n < 24: return bytes([major << 5 | n])
    for ai, fmt in ((24, ">B"), (25, ">H"), (26, ">I"), (27, ">Q")):
        if n < 1 << (8 * struct.calcsize(fmt)): return bytes([major << 5 | ai]) + struct.pack(fmt, n)
    raise ValueError(n)


def cbor(x):
    if isinstance(x, int): return head(0, x)
    if isinstance(x, bytes): return head(2, len(x)) + x
    if isinstance(x, str): b = x.encode(); return head(3, len(b)) + b
    if isinstance(x, list): return head(4, len(x)) + b"".join(cbor(i) for i in x)
    raise TypeError(x)


def hkdf(ikm, info, n=32):
    prk = hmac.new(b"\0" * 32, ikm, hashlib.sha256).digest()  # empty salt = HashLen zero bytes (RFC 5869)
    t, okm = b"", b""
    for i in range(1, -(-n // 32) + 1):
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest(); okm += t
    return okm[:n]


def pair_seed(sk, profile, ini, resp): return hkdf(sk, cbor(["OM-LAN-PAIR-SEED-v1", profile, ini, resp]))
def invite_seed(secret, invite_id): return hkdf(secret, cbor(["OM-LAN-INVITE-SEED-v1", invite_id]))
def tag(seed, e): return hmac.new(seed, cbor(["OM-LAN-RENDEZVOUS-TAG-v1", e]), hashlib.sha256).digest()[:16]


def cbor_map(pairs):
    return head(5, len(pairs)) + b"".join(cbor(k) + cbor(v) for k, v in sorted(pairs))


def signal_envelope(seed, kind, sid, epoch, nonce, sdp):
    """LanSignalEnvelopeV1 (ADR-0158 section 3). Needs the `cryptography` package for AES-256-GCM-SIV (run where it exists)."""
    from cryptography.hazmat.primitives.ciphers.aead import AESGCMSIV
    key = hkdf(seed, cbor(["OM-LAN-SIGNAL-KEY-v1", sid, epoch]))
    aad = cbor(["OM-LAN-SIGNAL-AAD-v1", 1, kind, sid, epoch])
    ct = AESGCMSIV(key).encrypt(nonce, sdp.encode(), aad)
    return key, aad, cbor_map([(0, 1), (1, kind), (2, sid), (3, epoch), (4, nonce), (5, ct)])


def pat(start, n): return bytes((start + i) & 255 for i in range(n))


def build():
    sk = pat(0x00, 32); ini = pat(0x40, 16); resp = pat(0x60, 16)
    isec = pat(0x80, 32); iid = pat(0xA0, 16)
    ps, is_ = pair_seed(sk, 1, ini, resp), invite_seed(isec, iid)
    posix = [0, 899, 900, 1_795_000_000, 1_795_000_899, 1_795_000_900]
    tags = lambda s: [{"posix": p, "epoch": p // EPOCH, "tag": tag(s, p // EPOCH).hex()} for p in posix]
    sdp = "v=0\r\no=- 1 1 IN IP4 192.168.1.20\r\ns=-\r\nt=0 0\r\na=ice-ufrag:abcd\r\na=ice-pwd:0123456789abcdef01234567\r\n"
    sig = []
    for name, kind, sid, epoch, nonce in (("offer", 1, pat(0xC0, 16), 1_994_444, pat(0xE0, 12)), ("answer", 2, pat(0xD0, 16), 1_994_445, pat(0xF0, 12))):
        key, aad, env = signal_envelope(ps, kind, sid, epoch, nonce, sdp)
        sig.append({"name": name, "kind": kind, "session_id": sid.hex(), "epoch": epoch, "nonce": nonce.hex(), "sdp": sdp, "key": key.hex(), "aad": aad.hex(), "envelope": env.hex()})
    return {
        "signal": sig,
        "profile": "LanDeliveryProfile-v1",
        "pair": {"sk": sk.hex(), "crypto_profile": 1, "initiator": ini.hex(), "responder": resp.hex(), "seed": ps.hex(), "tags": tags(ps)},
        "pair_swapped_roles_seed": pair_seed(sk, 1, resp, ini).hex(),
        "pair_other_profile_seed": pair_seed(sk, 2, ini, resp).hex(),
        "invite": {"secret": isec.hex(), "invite_id": iid.hex(), "seed": is_.hex(), "tags": tags(is_)},
    }


if __name__ == "__main__":
    text = json.dumps(build(), indent=1) + "\n"
    if "--check" in sys.argv: sys.exit(0 if OUT.read_text() == text else 1)
    OUT.write_text(text); print(f"wrote {OUT}")
