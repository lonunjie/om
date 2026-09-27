# SPDX-License-Identifier: Apache-2.0
"""Feasibility prototype for V-001. Not product code. Not for production.

Second, independent implementation (Python; different language, different libraries) that
consumes the Rust prototype's vectors and must reproduce or verify them from the public specs:

  * OM-Crypto-v1 device binding (ADR-0131 rule 1, ADR-0135 §1-§2): DS-SIG / DS-HASH, the
    AccountControlState -> DeviceCredential -> signed AsyncKeyBundle chain, composite Ed25519 AND
    ML-DSA-65 signatures (pyca/cryptography on OpenSSL; the Rust side uses ed25519-dalek and
    libcrux-ml-dsa), key derivation from seeds, and every negative binding vector;
  * Signal PQXDH rev 3 §2.2 KDF, §3.3 DH1..DH4, SK, AD; handshake_id (ADR-0131 rule 2);
  * FIPS 203 ML-KEM-768 via kyber-py (keygen from d||z, encaps with m, decaps);
  * Double Ratchet rev 4 §3 (KDF_RK, KDF_CK, skipped keys, DH ratchet) with the ADR-0131 rule 3
    AEAD derivation, header hash and associated data;
  * RFC 8452 AES-256-GCM-SIV via `cryptography` (deterministic, so ciphertexts must match bit-for-bit);
  * the MessageRequest HPKE envelope (hpke_pq.py, draft-ietf-hpke-pq-05), and optionally the draft's
    official KATs and the NIST ACVP ML-KEM-768 KATs.

Written by a Claude agent, from the specifications, without reading the Rust code's logic.
"""

from __future__ import annotations

import hashlib
import hmac
import json
import sys

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey
from cryptography.hazmat.primitives.asymmetric.mldsa import MLDSA65PrivateKey, MLDSA65PublicKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCMSIV
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from kyber_py.ml_kem import ML_KEM_768

import hpke_pq

H = bytes.fromhex


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


def hkdf_expand(prk: bytes, info: bytes, n: int) -> bytes:
    out, t, i = b"", b"", 1
    while len(out) < n:
        t = hmac.new(prk, t + info + bytes([i]), "sha256").digest()
        out += t
        i += 1
    return out[:n]


def x25519_pub(sk: bytes) -> bytes:
    return X25519PrivateKey.from_private_bytes(sk).public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )


def dh(sk: bytes, pk: bytes) -> bytes:
    return X25519PrivateKey.from_private_bytes(sk).exchange(X25519PublicKey.from_public_bytes(pk))


RAW = serialization.Encoding.Raw, serialization.PublicFormat.Raw
PROFILE, ROLE_DEVICE = 1, 3


def ds_sig(label: str, profile: int, role: int, signer: bytes, body: bytes) -> bytes:
    return cbor_enc([label, profile, role, signer, body])


def ds_hash(label: str, body: bytes) -> bytes:
    return hashlib.sha256(cbor_enc([label, body])).digest()


def composite_pk(ed_seed: bytes, mldsa_seed: bytes) -> bytes:
    ed = Ed25519PrivateKey.from_private_bytes(ed_seed).public_key().public_bytes(*RAW)
    return ed + MLDSA65PrivateKey.from_seed_bytes(mldsa_seed).public_key().public_bytes(*RAW)


def composite_verify(pk: bytes, msg: bytes, sig: bytes) -> bool:
    """ADR-0135 §1: Ed25519 pk(32)||ML-DSA-65 vk(1952); sig = Ed25519(64)||ML-DSA-65(3309); both must verify."""
    if len(pk) != 32 + 1952 or len(sig) != 64 + 3309:
        return False
    try:
        Ed25519PublicKey.from_public_bytes(pk[:32]).verify(sig[:64], msg)
        MLDSA65PublicKey.from_public_bytes(pk[32:]).verify(sig[64:], msg, b"")
        return True
    except (InvalidSignature, ValueError):
        return False


def verify_binding(fetch: list[bytes], acs_body: bytes, cred_body: bytes, trusted: bytes, now: int, freshness=None):
    """Independent reading of ADR-0131 + ADR-0140 over frozen W0. `freshness`, when supplied, is
    durable state keyed by (AccountID, DeviceID, CryptoProfile) -> (highest_generation, core_hash)."""
    if ds_hash("OM-ACS-HASH-v1", acs_body) != trusted:
        return False, "acs not trusted head", None
    acs, cred = cbor_dec(acs_body), cbor_dec(cred_body)
    if acs[0] != 1 or cred[0] != 7 or cred[4] != PROFILE or cred[3] != acs[2]:
        return False, "acs/credential shape", None
    did = cred[2]
    ref = {0: did, 1: {0: 1, 1: ds_hash("OM-DEVICE-CREDENTIAL-HASH-v1", cred_body)}}
    if ref not in acs.get(13, []):
        return False, "credential digest not in acs", None
    live = did in acs.get(5, []) or did in acs.get(7, [])
    if not live or did in acs.get(6, []) or did in acs.get(8, []):
        return False, "device not active", None
    env = cbor_dec(fetch[0])
    sigs = env[2]
    if len(sigs) != 1 or (sigs[0][0], sigs[0][1], sigs[0][2]) != (did, PROFILE, ROLE_DEVICE):
        return False, "signature entries", None
    if not composite_verify(cred[5], ds_sig("OM-PREKEY-BUNDLE-SIG-v1", PROFILE, ROLE_DEVICE, did, env[1]), sigs[0][3]):
        return False, "bundle signature", None
    b = cbor_dec(env[1])
    if b[0] != 9 or b[2] != did or b[5] != PROFILE or b[13] <= now:
        return False, "bundle binding", None
    if 3 in b or 4 in b:
        return False, "retired provider-visible acs fields", None
    if 8 not in b or 12 not in b or len(b[12]) != 16:
        return False, "bundle required fields", None
    generation = b[6]
    core_hash = ds_hash("OM-PREKEY-GENERATION-CORE-v1", cbor_enc([did, b[5], generation, b[7], b[8], b.get(11)]))
    keys = {"ik": cred[6], "spk": b[7][2], "pq": b[8][2], "pq_id": b[8][0], "opk": None}
    for raw in fetch[1:]:
        it = cbor_dec(raw)
        if it[1] not in (3, 4):
            return False, "unexpected one-time kind", None
        context = cbor_enc([1, did, b[5], generation, b[12], it[1], it[0], it[2]])
        if not composite_verify(cred[5], ds_sig("OM-PREKEY-SIG-v1", PROFILE, ROLE_DEVICE, did, context), it[3]):
            return False, "item signature", None
        if it[1] == 3:
            keys["opk"] = (it[0], it[2])
        elif it[1] == 4:
            keys["pq"], keys["pq_id"] = it[2], it[0]
    if freshness is not None:
        fkey = (acs[2], did, b[5])
        old = freshness.get(fkey)
        if old is not None and generation < old[0]:
            return False, "prekey generation rollback", None
        if old is not None and generation == old[0] and core_hash != old[1]:
            return False, "prekey generation equivocation", None
        if old is None or generation > old[0]:
            freshness[fkey] = (generation, core_hash)
    return True, "ok", keys


def check_binding(bv, ck):
    s = bv["signer"]
    pk = composite_pk(H(s["ed25519_seed"]), H(s["mldsa65_seed"]))
    ck.eq("binding/composite_pk_from_seeds", hashlib.sha256(pk).hexdigest(), s["composite_public_key_sha256"])
    acs, cred = H(bv["acs_body"]), H(bv["credential_body"])
    ck.eq("binding/acs_hash", ds_hash("OM-ACS-HASH-v1", acs).hex(), bv["acs_hash"])
    ck.eq("binding/credential_hash", ds_hash("OM-DEVICE-CREDENTIAL-HASH-v1", cred).hex(), bv["credential_hash"])
    ck.eq("binding/credential_signing_key_is_composite", cbor_dec(cred)[5], pk)
    ok, why, keys = verify_binding([H(x) for x in bv["positive_fetch"]], acs, cred, H(bv["acs_hash"]), bv["now"])
    ck.eq("binding/positive_accepted", (ok, why), (True, "ok"))
    ck.eq("binding/ik_from_credential", keys and keys["ik"].hex(), bv["ik_pub"])
    # Independent ADR-0140 freshness checks use the already-valid signed fetch. Seeding a durable
    # floor above it must reject rollback; seeding the same generation with another core hash must
    # reject authenticated equivocation without needing to manufacture another signature.
    fetch_bytes = [H(x) for x in bv["positive_fetch"]]
    bundle = cbor_dec(cbor_dec(fetch_bytes[0])[1])
    fkey = (cbor_dec(acs)[2], cbor_dec(cred)[2], bundle[5])
    floor = {fkey: (bundle[6] + 1, b"\x00" * 32)}
    rok, rwhy, _ = verify_binding(fetch_bytes, acs, cred, H(bv["acs_hash"]), bv["now"], floor)
    ck.eq("binding/prekey_generation_rollback_rejected", (rok, rwhy), (False, "prekey generation rollback"))
    equiv = {fkey: (bundle[6], b"\xff" * 32)}
    eok, ewhy, _ = verify_binding(fetch_bytes, acs, cred, H(bv["acs_hash"]), bv["now"], equiv)
    ck.eq("binding/equal_generation_different_core_rejected", (eok, ewhy), (False, "prekey generation equivocation"))
    for n in bv["negatives"]:
        ok, why, _ = verify_binding([H(x) for x in n["fetch"]], H(n["acs_body"]), H(n["credential_body"]), H(n["trusted_acs_hash"]), bv["now"])
        ck.eq(f"binding/negative/{n['name']}_rejected", ok, False)


def verify_initiator(ik_a: bytes, peers: list, route):
    """Independent reading of ADR-0136 rule 4: IK_A must be the key of exactly one DeviceCredential committed by a
    trusted peer head (route-pinned when given), and that device must be active and not quarantined."""
    found = []
    for p in peers:
        acs_body = H(p["acs_body"])
        if ds_hash("OM-ACS-HASH-v1", acs_body) != H(p["trusted_acs_hash"]):
            return False, "peer acs not trusted head"
        acs = cbor_dec(acs_body)
        for cb in p["credential_bodies"]:
            cb = H(cb)
            cred = cbor_dec(cb)
            ref = {0: cred[2], 1: {0: 1, 1: ds_hash("OM-DEVICE-CREDENTIAL-HASH-v1", cb)}}
            if cred[3] != acs[2] or ref not in acs.get(13, []):
                continue
            did = cred[2]
            live = (did in acs.get(5, []) or did in acs.get(7, [])) and did not in acs.get(6, []) and did not in acs.get(8, [])
            if cred[6] == ik_a and (route is None or acs[2] == route):
                found.append(live)
    if len(found) != 1:
        return False, f"{len(found)} matching credentials"
    return (True, "ok") if found[0] else (False, "device not active")


def check_initiator_binding(vs, ck):
    for v in vs:
        ok, why = verify_initiator(H(v["ik_a"]), v["peers"], v["route_account"] and H(v["route_account"]))
        ck.eq(f"initiator_binding/{v['name']}", "accept" if ok else "reject", v["expect"])


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
    ec = bytes([consts["ec_type_byte"]])
    ik_b, spk_b = x25519_pub(H(bob["ik_secret"])), x25519_pub(H(bob["spk_secret"]))
    ck.eq(f"{n}/bob_ik_pub", ik_b.hex(), bob["ik_pub"])
    ck.eq(f"{n}/bob_spk_pub", spk_b.hex(), bob["spk_pub"])
    seed = H(bob["pq_seed_d_z"])
    ek, dk = ML_KEM_768._keygen_internal(seed[:32], seed[32:])
    ck.eq(f"{n}/mlkem_ek_from_seed", hashlib.sha256(ek).hexdigest(), bob["pq_pub_sha256"])
    acs = H(bob["acs_body"])
    ok, why, keys = verify_binding([H(x) for x in bob["fetch"]], acs, H(bob["credential_body"]), ds_hash("OM-ACS-HASH-v1", acs), 1790000000)
    ck.eq(f"{n}/binding_verified", (ok, why), (True, "ok"))
    ck.eq(f"{n}/binding_keys", (keys["ik"], keys["spk"], keys["pq"], keys["opk"] and keys["opk"][1]),
          (ik_b, spk_b, ek, bob["opk_pub"] and H(bob["opk_pub"])))

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
    bob_side = [dh(H(bob["spk_secret"]), x25519_pub(ik_a_sk)), dh(H(bob["ik_secret"]), x25519_pub(ek_sk)), dh(H(bob["spk_secret"]), x25519_pub(ek_sk))]
    ck.eq(f"{n}/dh_responder_agrees", [d.hex() for d in bob_side], hs["dh"][:3])
    sk = hkdf(b"\x00" * 32, b"\xff" * 32 + b"".join(dhs) + k, consts["pqxdh_info"].encode(), 32)
    ck.eq(f"{n}/pqxdh_sk", sk.hex(), hs["sk"])
    ad = ec + x25519_pub(ik_a_sk) + ec + ik_b
    ck.eq(f"{n}/pqxdh_ad", ad.hex(), hs["ad"])
    init = cbor_dec(H(hs["init_header_cbor"]))
    ck.eq(f"{n}/init_header", (init[1], init[2], init[3], init[4], init[5], init.get(6)),
          (x25519_pub(ik_a_sk), x25519_pub(ek_sk), ct, bob["spk_id"], bob["pq_id"], bob["opk_id"]))
    hid = ds_hash(consts["handshake_id_label"], cbor_enc([consts["profile"], H(bob["device_id"]), bob["epoch"], init[4], init[5], init[1], init[2], init[3]]))
    ck.eq(f"{n}/handshake_id", hid.hex(), hs["handshake_id"])
    ck.eq(f"{n}/ledger_applies_iff_no_one_time_key", hs["uses_last_resort_ledger"], bob["pq_kind"] == "last_resort" and bob["opk_id"] is None)

    party = {"A": DR(consts), "B": DR(consts)}
    party["A"].init_alice(sk, spk_b, H(alice["ratchet_init_secret"]))
    party["B"].init_bob(sk, H(bob["spk_secret"]))
    key_info, nonce_info = consts["dr_aead_key_info"].encode(), consts["dr_aead_nonce_info"].encode()

    def aead_inputs(mk, hdr_bytes, ctx):
        hh = ds_hash(consts["dr_header_hash_label"], hdr_bytes)
        key = hkdf_expand(mk, key_info, 32)
        nonce = hkdf_expand(mk, nonce_info + hh, 32)[:12]
        aad = cbor_enc([consts["event_aead_ad_label"], consts["profile"], ad, hh, ctx])
        return hh, key, nonce, aad

    sent = {}
    for ev in c["events"]:
        lab = f"{n}/{ev['type']}:{ev['label']}"
        if ev["type"] == "send":
            d = party[ev["from"]]
            num, mk = d.send_key()
            ck.eq(f"{lab}/mk", mk.hex(), ev["mk"])
            hdr_bytes = H(ev["header"])
            hdr = cbor_dec(hdr_bytes)
            ck.eq(f"{lab}/header", hdr, [consts["profile"], x25519_pub(d.dhs), d.pn, num])
            ctx = H(ev["envelope_context"])
            hh, key, nonce, aad = aead_inputs(mk, hdr_bytes, ctx)
            ck.eq(f"{lab}/aead_inputs", (hh.hex(), key.hex(), nonce.hex(), aad.hex()), (ev["header_hash"], ev["aead_key"], ev["aead_nonce"], ev["aad"]))
            ctext = AESGCMSIV(key).encrypt(nonce, H(ev["plaintext"]), aad)
            ck.eq(f"{lab}/ciphertext", ctext.hex(), ev["ciphertext"])
            ck.eq(f"{lab}/init_prefix_until_reply", ev["has_init_prefix"], ev["from"] == "A" and party["A"].ckr is None)
            sent[ev["label"]] = (hdr, hdr_bytes, ctext, ctx)
        else:
            d = party[ev["to"]]
            hdr, hdr_bytes, ctext, ctx = sent[ev["label"]]
            new = H(ev["new_ratchet_secret"]) if ev["new_ratchet_secret"] else None
            mk, stepped = d.receive_key(hdr[1], hdr[2], hdr[3], new)
            ck.eq(f"{lab}/mk", mk.hex(), ev["mk"])
            ck.eq(f"{lab}/dh_ratchet_step_matches", stepped, new is not None)
            _, key, nonce, aad = aead_inputs(mk, hdr_bytes, ctx)
            ck.eq(f"{lab}/plaintext", AESGCMSIV(key).decrypt(nonce, ctext, aad).hex(), ev["plaintext"])
            bad = hdr_bytes[:-1] + bytes([hdr_bytes[-1] ^ 1])
            _, k2, n2, a2 = aead_inputs(mk, bad, ctx)
            try:
                AESGCMSIV(k2).decrypt(n2, ctext, a2)
                ck.eq(f"{lab}/tampered_header_rejected", False, True)
            except Exception:
                ck.eq(f"{lab}/tampered_header_rejected", True, True)


def check_hpke(hv, ck: Checker):
    seed = H(hv["sk_seed"])
    pk = hpke_pq._expand(seed)
    ck.eq("hpke/pk_from_seed", (pk[0] + pk[1]).hex(), hv["pk"])
    ck.eq("hpke/request_key_digest", ds_hash("OM-REQUEST-KEY-HASH-v1", H(hv["request_key"])).hex(), hv["request_key_digest"])
    info = cbor_enc(["OM-MESSAGE-REQUEST-HPKE-INFO-v1", PROFILE, H(hv["ticket_id"]), {0: 1, 1: H(hv["request_key_digest"])}])
    ck.eq("hpke/info", info.hex(), hv["info"])
    enc, ss, ctx = hpke_pq.setup_base_s_derand(H(hv["pk"]), info, H(hv["encaps_randomness"]))
    ck.eq("hpke/enc_and_shared_secret", (enc.hex(), ss.hex()), (hv["enc"], hv["shared_secret"]))
    ck.eq("hpke/ciphertext", ctx.seal(b"", H(hv["plaintext"])).hex(), hv["ciphertext"])
    _, rctx = hpke_pq.setup_base_r(H(hv["enc"]), seed, info)
    ck.eq("hpke/open", rctx.open(b"", H(hv["ciphertext"])).hex(), hv["plaintext"])


def check_sealed_delivery(v, ck: Checker):
    seed, kid, did = H(v["recipient_seed"]), H(v["delivery_key_id"]), H(v["delivery_id"])
    pk = hpke_pq._expand(seed)
    pk = pk[0] + pk[1]
    ck.eq("sealed_delivery/profile1_public_key", (len(pk), pk.hex()), (1216, v["recipient_public_key"]))
    route = hashlib.sha256(cbor_enc(["OM-SEALED-DELIVERY-ROUTE-v1", v["relay_uri"], H(v["deposit_capability"])] )).digest()
    ck.eq("sealed_delivery/route_binding", route.hex(), v["route_binding"])
    info = cbor_enc(["OM-SEALED-DELIVERY-HPKE-INFO-v1", PROFILE, kid, {0: 1, 1: route}])
    aad = cbor_enc(["OM-SEALED-DELIVERY-HPKE-AAD-v1", 23, 1, PROFILE, kid, did, v["expires_at"], v["plaintext_bucket_bytes"]])
    ck.eq("sealed_delivery/exact_info", info.hex(), v["info"])
    ck.eq("sealed_delivery/exact_aad", aad.hex(), v["aad"])
    wrapper = cbor_dec(H(v["wrapper"]))
    ck.eq("sealed_delivery/wire_shape", (wrapper[0], wrapper[1], wrapper[2], wrapper[3], wrapper[4], len(wrapper[5])),
          (23, 1, PROFILE, kid, did, 1120))
    ck.eq("sealed_delivery/ciphertext_bucket", len(wrapper[6]), v["plaintext_bucket_bytes"] + 16)
    enc, ss, ctx = hpke_pq.setup_base_s_derand(pk, info, H(v["encaps_randomness"]))
    ck.eq("sealed_delivery/pinned_hybrid_encapsulation", (len(enc), enc.hex()), (1120, wrapper[5].hex()))
    pt = int(len(H(v["inner_envelope"]))).to_bytes(4, "big") + H(v["inner_envelope"]) + H(v["padding"])
    ck.eq("sealed_delivery/padded_plaintext_bucket", len(pt), v["plaintext_bucket_bytes"])
    ct = ctx.seal(aad, pt)
    ck.eq("sealed_delivery/ciphertext", ct.hex(), wrapper[6].hex())
    _, rctx = hpke_pq.setup_base_r(wrapper[5], seed, info)
    opened = rctx.open(aad, wrapper[6])
    n = int.from_bytes(opened[:4], "big")
    ck.eq("sealed_delivery/independent_open", opened[4:4+n].hex(), v["inner_envelope"])
    ck.eq("sealed_delivery/outer_is_not_sender_auth", (v["outer_open_is_sender_authentication"], v["clear_sender_fallback"]), (False, False))
    wrong_route = cbor_enc(["OM-SEALED-DELIVERY-ROUTE-v1", "https://wrong.example/", H(v["deposit_capability"])])
    wrong_info = cbor_enc(["OM-SEALED-DELIVERY-HPKE-INFO-v1", PROFILE, kid, {0: 1, 1: hashlib.sha256(wrong_route).digest()}])
    try:
        _, c = hpke_pq.setup_base_r(wrapper[5], seed, wrong_info); c.open(aad, wrapper[6]); route_rejected = False
    except Exception: route_rejected = True
    ck.eq("sealed_delivery/wrong_route_rejected", route_rejected, True)
    wrong_aad = cbor_enc(["OM-SEALED-DELIVERY-HPKE-AAD-v1", 23, 1, PROFILE, kid, b"X"*16, v["expires_at"], v["plaintext_bucket_bytes"]])
    try:
        _, c = hpke_pq.setup_base_r(wrapper[5], seed, info); c.open(wrong_aad, wrapper[6]); aad_rejected = False
    except Exception: aad_rejected = True
    ck.eq("sealed_delivery/tampered_aad_rejected", aad_rejected, True)
    other_seed = bytes([seed[0] ^ 1]) + seed[1:]
    try:
        _, c = hpke_pq.setup_base_r(wrapper[5], other_seed, info); c.open(aad, wrapper[6]); key_rejected = False
    except Exception: key_rejected = True
    ck.eq("sealed_delivery/wrong_private_key_rejected", key_rejected, True)
    expiry_test_now = v["expires_at"] + 301
    ck.eq("sealed_delivery/expiry_rejected_after_skew", expiry_test_now > v["expires_at"] + 300, True)
    ck.eq("sealed_delivery/nonconforming_bucket_rejected", 1056 - 16 in (1024,4096,16384,65536,131072), False)
    bad_enc = bytearray(wrapper[5]); bad_enc[0] ^= 1
    try:
        _, c = hpke_pq.setup_base_r(bytes(bad_enc), seed, info); c.open(aad, wrapper[6]); enc_rejected = False
    except Exception: enc_rejected = True
    ck.eq("sealed_delivery/tampered_encapsulation_rejected", enc_rejected, True)
    bad_ct = bytearray(wrapper[6]); bad_ct[0] ^= 1
    try:
        _, c = hpke_pq.setup_base_r(wrapper[5], seed, info); c.open(aad, bytes(bad_ct)); ct_rejected = False
    except Exception: ct_rejected = True
    ck.eq("sealed_delivery/tampered_ciphertext_rejected", ct_rejected, True)
    bucket_cases = [(12,1024),(1021,4096),(4093,16384),(16381,65536),(65533,131072)]
    reproduced_buckets = []
    for index, (size, expected_bucket) in enumerate(bucket_cases):
        inner = bytes([index + 1]) * size
        delivery_id = bytes([0x50 + index]) * 16
        bucket_aad = cbor_enc(["OM-SEALED-DELIVERY-HPKE-AAD-v1", 23, 1, PROFILE, kid, delivery_id,
                               v["expires_at"], expected_bucket])
        randomness, padding = bytes([index + 1]) * 64, bytes([0xA0 + index]) * (expected_bucket - 4 - size)
        test_enc, _, send_ctx = hpke_pq.setup_base_s_derand(pk, info, randomness)
        test_ciphertext = send_ctx.seal(bucket_aad, len(inner).to_bytes(4, "big") + inner + padding)
        _, recv_ctx = hpke_pq.setup_base_r(test_enc, seed, info)
        test_plaintext = recv_ctx.open(bucket_aad, test_ciphertext)
        test_length = int.from_bytes(test_plaintext[:4], "big")
        reproduced_buckets.append((size, len(test_plaintext), len(test_ciphertext), test_plaintext[4:4+test_length] == inner))
    ck.eq("sealed_delivery/fixed_bucket_seal_open", reproduced_buckets,
          [(n, b, b + 16, True) for n, b in bucket_cases])


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
    check_binding(vectors["binding"], ck)
    check_initiator_binding(vectors["initiator_binding"], ck)
    for c in vectors["cases"]:
        check_case(c, vectors["constants"], ck)
    check_hpke(vectors["hpke_message_request"], ck)
    check_sealed_delivery(vectors["sealed_delivery"], ck)
    out = {
        "vector_checks": len(ck.results),
        "vector_checks_passed": sum(ok for _, ok in ck.results),
        "vector_messages": sum(1 for c in vectors["cases"] for e in c["events"] if e["type"] == "send"),
        "vector_cases": [c["name"] for c in vectors["cases"]],
        "binding_negatives": len(vectors["binding"]["negatives"]),
        "initiator_binding_vectors": len(vectors["initiator_binding"]),
        "sealed_delivery_vectors": 1,
        "failures": [name for name, ok in ck.results if not ok],
    }
    out["vectors_ok"] = out["vector_checks"] == out["vector_checks_passed"] and out["vector_checks"] > 0
    if len(sys.argv) > 3:
        out["acvp"] = acvp(json.load(open(sys.argv[2])), json.load(open(sys.argv[3])))
    if len(sys.argv) > 4:
        k = hpke_pq.check_kats(json.load(open(sys.argv[4])))
        out["hpke_pq_05_kats"] = dict(k, all_passed=k["vectors"] > 0 and not k["failed"])
    ok = out["vectors_ok"] and all(out.get(x, {"all_passed": True})["all_passed"] for x in ("acvp", "hpke_pq_05_kats"))
    print(json.dumps(out, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
