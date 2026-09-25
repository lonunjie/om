# SPDX-License-Identifier: Apache-2.0
"""Feasibility prototype for V-001. Not product code. Not for production.

Independent Python implementation of the OM-Crypto-v1 MessageRequest HPKE suite, written from the
specifications only (ADR-0131 rule 9):

  draft-ietf-hpke-pq-05 §4      MLKEM768-X25519 (kem_id 0x647a), HPKE DeriveKeyPair via SHAKE256.LabeledDerive
  draft-irtf-cfrg-concrete-hybrid-kems-03 §4.2   CG framework, SHAKE256 PRG, SHA3-256 KDF, label "\\.//^\\"
  draft-irtf-cfrg-hybrid-kems-12 §5.1, §5.5, App. A   expandDecapsKeyG, C2PRICombiner, EncapsDerand
  draft-ietf-hpke-hpke-03 §4.4, §5.1          LabeledDerive / two-stage key schedule (HKDF-SHA256), base mode
  RFC 8439                                     ChaCha20Poly1305

Components: ML-KEM-768 decapsulation and seed expansion from pyca/cryptography (OpenSSL), deterministic
ML-KEM encapsulation from kyber-py (only for KAT reproduction), X25519/HKDF/ChaCha20Poly1305 from cryptography.
"""
from __future__ import annotations

import hashlib
import hmac

from cryptography.hazmat.primitives.asymmetric import mlkem, x25519
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives import serialization
from kyber_py.ml_kem import ML_KEM_768

KEM_ID, KDF_ID, AEAD_ID = 0x647A, 0x0001, 0x0003
KEM_SUITE = b"KEM" + KEM_ID.to_bytes(2, "big")
HPKE_SUITE = b"HPKE" + KEM_ID.to_bytes(2, "big") + KDF_ID.to_bytes(2, "big") + AEAD_ID.to_bytes(2, "big")
LABEL = b"\\.//^\\"  # 5C2E2F2F5E5C
N_PQ_SEED, N_T_SEED = 64, 32
N_EK_PQ, N_CT_PQ, N_ELEM = 1184, 1088, 32
NK, NN, NH = 32, 12, 32
MODE_BASE = 0

RAW = serialization.Encoding.Raw, serialization.PublicFormat.Raw


def i2osp(n: int, w: int) -> bytes:
    return n.to_bytes(w, "big")


def length_prefixed(x: bytes) -> bytes:
    return i2osp(len(x), 2) + x


# --- one-stage SHAKE256 LabeledDerive (used only by DeriveKeyPair) -------------------------------------
def labeled_derive_shake256(suite: bytes, ikm: bytes, label: bytes, context: bytes, L: int) -> bytes:
    m = ikm + b"HPKE-v1" + suite + length_prefixed(label) + i2osp(L, 2) + context
    return hashlib.shake_256(m).digest(L)


# --- MLKEM768-X25519 (CG framework) -----------------------------------------------------------------
def _expand(seed: bytes):
    full = hashlib.shake_256(seed).digest(N_PQ_SEED + N_T_SEED)
    seed_pq, seed_t = full[:N_PQ_SEED], full[N_PQ_SEED:]
    dk_pq = mlkem.MLKEM768PrivateKey.from_seed_bytes(seed_pq)
    ek_pq = dk_pq.public_key().public_bytes(*RAW)
    dk_t = x25519.X25519PrivateKey.from_private_bytes(seed_t)
    ek_t = dk_t.public_key().public_bytes(*RAW)
    return ek_pq, ek_t, dk_pq, dk_t


def _combine(ss_pq: bytes, ss_t: bytes, ct_t: bytes, ek_t: bytes) -> bytes:
    return hashlib.sha3_256(ss_pq + ss_t + ct_t + ek_t + LABEL).digest()


def derive_key_pair(ikm: bytes) -> tuple[bytes, bytes]:
    seed = labeled_derive_shake256(KEM_SUITE, ikm, b"DeriveKeyPair", b"", 32)
    ek_pq, ek_t, _, _ = _expand(seed)
    return seed, ek_pq + ek_t


def encaps_derand(ek: bytes, randomness: bytes) -> tuple[bytes, bytes]:
    if len(ek) != N_EK_PQ + N_ELEM or len(randomness) != 32 + N_T_SEED:
        raise ValueError("EncapError")
    ek_pq, ek_t = ek[:N_EK_PQ], ek[N_EK_PQ:]
    ss_pq, ct_pq = ML_KEM_768._encaps_internal(ek_pq, randomness[:32])
    sk_e = x25519.X25519PrivateKey.from_private_bytes(randomness[32:])
    ct_t = sk_e.public_key().public_bytes(*RAW)
    ss_t = sk_e.exchange(x25519.X25519PublicKey.from_public_bytes(ek_t))
    return _combine(ss_pq, ss_t, ct_t, ek_t), ct_pq + ct_t


def decaps(dk_seed: bytes, ct: bytes) -> bytes:
    if len(ct) != N_CT_PQ + N_ELEM or len(dk_seed) != 32:
        raise ValueError("DecapError")
    ct_pq, ct_t = ct[:N_CT_PQ], ct[N_CT_PQ:]
    _, ek_t, dk_pq, dk_t = _expand(dk_seed)
    ss_pq = dk_pq.decapsulate(ct_pq)
    ss_t = dk_t.exchange(x25519.X25519PublicKey.from_public_bytes(ct_t))
    return _combine(ss_pq, ss_t, ct_t, ek_t)


# --- two-stage HKDF-SHA256 key schedule (base mode) -------------------------------------------------
def _extract(salt: bytes, ikm: bytes) -> bytes:
    return hmac.new(salt or b"\x00" * NH, ikm, hashlib.sha256).digest()


def _expand_hkdf(prk: bytes, info: bytes, L: int) -> bytes:
    out, t, i = b"", b"", 1
    while len(out) < L:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest()
        out += t
        i += 1
    return out[:L]


def labeled_extract(salt: bytes, label: bytes, ikm: bytes) -> bytes:
    return _extract(salt, b"HPKE-v1" + HPKE_SUITE + label + ikm)


def labeled_expand(prk: bytes, label: bytes, info: bytes, L: int) -> bytes:
    return _expand_hkdf(prk, i2osp(L, 2) + b"HPKE-v1" + HPKE_SUITE + label + info, L)


def key_schedule(shared_secret: bytes, info: bytes):
    psk_id_hash = labeled_extract(b"", b"psk_id_hash", b"")
    info_hash = labeled_extract(b"", b"info_hash", info)
    ctx = bytes([MODE_BASE]) + psk_id_hash + info_hash
    secret = labeled_extract(shared_secret, b"secret", b"")
    return (labeled_expand(secret, b"key", ctx, NK), labeled_expand(secret, b"base_nonce", ctx, NN),
            labeled_expand(secret, b"exp", ctx, NH))


class Context:
    def __init__(self, key: bytes, base_nonce: bytes, exporter_secret: bytes):
        self.key, self.base_nonce, self.exporter_secret, self.seq = key, base_nonce, exporter_secret, 0

    def _nonce(self) -> bytes:
        return bytes(a ^ b for a, b in zip(self.base_nonce, i2osp(self.seq, NN)))

    def seal(self, aad: bytes, pt: bytes) -> bytes:
        ct = ChaCha20Poly1305(self.key).encrypt(self._nonce(), pt, aad)
        self.seq += 1
        return ct

    def open(self, aad: bytes, ct: bytes) -> bytes:
        pt = ChaCha20Poly1305(self.key).decrypt(self._nonce(), ct, aad)
        self.seq += 1
        return pt

    def export(self, exporter_context: bytes, L: int) -> bytes:
        return labeled_expand(self.exporter_secret, b"sec", exporter_context, L)


def setup_base_s_derand(pk_r: bytes, info: bytes, randomness: bytes):
    ss, enc = encaps_derand(pk_r, randomness)
    return enc, ss, Context(*key_schedule(ss, info))


def setup_base_r(enc: bytes, sk_r: bytes, info: bytes):
    ss = decaps(sk_r, enc)
    return ss, Context(*key_schedule(ss, info))


def check_kats(vectors: list[dict]) -> dict:
    """Run every draft-ietf-hpke-pq-05 vector for (0x647a, HKDF-SHA256, ChaCha20Poly1305); return a summary."""
    sel = [t for t in vectors if (t["kem_id"], t["kdf_id"], t["aead_id"], t["mode"]) == (KEM_ID, KDF_ID, AEAD_ID, 0)]
    checks, fails = 0, []
    for n, t in enumerate(sel):
        h = {k: bytes.fromhex(v) for k, v in t.items() if isinstance(v, str)}
        sk, pk = derive_key_pair(h["ikmR"])
        results = [("skRm", sk == h["skRm"]), ("pkRm", pk == h["pkRm"])]
        enc, ss, cs = setup_base_s_derand(h["pkRm"], h["info"], h["ikmE"])
        results += [("enc", enc == h["enc"]), ("shared_secret", ss == h["shared_secret"]),
                    ("key", cs.key == h["key"]), ("base_nonce", cs.base_nonce == h["base_nonce"]),
                    ("exporter_secret", cs.exporter_secret == h["exporter_secret"])]
        ss_r, cr = setup_base_r(h["enc"], h["skRm"], h["info"])
        results.append(("decap", ss_r == h["shared_secret"]))
        for i, e in enumerate(t["encryptions"]):
            e = {k: bytes.fromhex(v) for k, v in e.items()}
            nonce_ok = cs._nonce() == e["nonce"]
            results.append((f"seal[{i}]", nonce_ok and cs.seal(e["aad"], e["pt"]) == e["ct"]))
            results.append((f"open[{i}]", cr.open(e["aad"], e["ct"]) == e["pt"]))
        for i, x in enumerate(t["exports"]):
            results.append((f"export[{i}]", cs.export(bytes.fromhex(x["exporter_context"]), x["L"])
                            == bytes.fromhex(x["exported_value"])))
        checks += len(results)
        fails += [f"vector{n}:{name}" for name, ok in results if not ok]
    return {"vectors": len(sel), "checks": checks, "failed": fails}


if __name__ == "__main__":
    import json
    import sys
    print(json.dumps(check_kats(json.load(open(sys.argv[1]))), indent=1))
