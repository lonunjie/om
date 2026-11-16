#!/usr/bin/env python3
"""ADR-0194 direct-transfer receipt vectors. Feasibility evidence, not product code.

Two parts, independent of the Rust implementation:
  "constructions": fixed content key, BlobID, ciphertext and transfer id, with the expected ciphertext_digest, K_rcpt,
      H_pos, MAC input and receipt_tag (hashlib/hmac and a minimal canonical CBOR encoder of this file), and negatives:
      every bound input changed on its own gives another tag.
  "structs": the `object_receipt` frame (LanFrameV1 kind 8, key 15) and the earlier lan frames as controls, as bytes with the
      exact error class the strict codecs (the product Rust codec and the Node decoder of check_node.mjs) must give; "cddl" says what the two CDDL
      tools must say about spec/cddl/lan.cddl. The registry accepts the frame fields independently of the kind (the
      kind-specific combinations are CDDL's job), so a kind-8 frame without a tag is accepted by the codecs and refused by CDDL.

Usage: gen_direct_receipt_vectors.py OUT.json
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
import gen_vectors as g  # noqa: E402
from gen_vectors import M, mdel, mset, raw  # noqa: E402

LBL_KDF = "OM-DIRECT-RECEIPT-KDF-v1"
LBL_MAC = "OM-DIRECT-RECEIPT-MAC-v1"
LBL_POS = "OM-DIRECT-RECEIPT-POSSESSION-HASH-v1"
LBL_CT = "OM-BLOB-CIPHERTEXT-HASH-v1"


class CB(bytes):
    """A CBOR byte string (this file's encoder)."""


def head(major: int, n: int) -> bytes:
    if n < 24:
        return bytes([major << 5 | n])
    for ai, w in ((24, 1), (25, 2), (26, 4), (27, 8)):
        if n < 1 << (8 * w):
            return bytes([major << 5 | ai]) + n.to_bytes(w, "big")
    raise ValueError(n)


def enc(v) -> bytes:
    if isinstance(v, bool):
        raise TypeError("no booleans")
    if isinstance(v, int):
        return head(0, v)
    if isinstance(v, CB):
        return head(2, len(v)) + bytes(v)
    if isinstance(v, str):
        b = v.encode()
        return head(3, len(b)) + b
    if isinstance(v, list):
        return head(4, len(v)) + b"".join(enc(x) for x in v)
    raise TypeError(type(v))


def ds_hash(label: str, body: bytes) -> bytes:
    return hashlib.sha256(enc([label, CB(body)])).digest()


def hkdf(salt: bytes, ikm: bytes, info: bytes, n: int = 32) -> bytes:
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    out, t, i = b"", b"", 1
    while len(out) < n:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest()
        out += t
        i += 1
    return out[:n]


def stream(label: str, n: int) -> bytes:
    out, i = b"", 0
    while len(out) < n:
        out += hashlib.sha256(f"{label}/{i}".encode()).digest()
        i += 1
    return out[:n]


def parts(key, digest, blob_id, tid, total, ct_for_pos):
    k = hkdf(digest, key, enc([LBL_KDF, CB(blob_id)]))
    h_pos = ds_hash(LBL_POS, enc([CB(tid), CB(ct_for_pos)]))
    mac_in = enc([LBL_MAC, CB(tid), CB(blob_id), total, CB(digest), CB(h_pos)])
    return k, h_pos, mac_in, hmac.new(k, mac_in, hashlib.sha256).digest()


KEY = stream("direct-receipt/content-key", 32)
BLOB = stream("direct-receipt/blob-id", 32)
TID = stream("direct-receipt/transfer-id", 16)
SIZES = {"min-17-bytes": 17, "4096-bytes": 4096, "one-chunk-16384": 16384, "over-16-kib-20000": 20000}


def construction():
    cases, negs = [], []
    for name, n in SIZES.items():
        ct = stream(f"direct-receipt/ciphertext/{name}", n)
        digest = ds_hash(LBL_CT, ct)
        k, h_pos, mac_in, tag = parts(KEY, digest, BLOB, TID, n, ct)
        cases.append({"id": name, "content_key": KEY.hex(), "blob_id": BLOB.hex(), "transfer_id": TID.hex(),
                      "ciphertext": ct.hex(), "total_bytes": n, "ciphertext_digest": digest.hex(), "k_rcpt": k.hex(),
                      "h_pos": h_pos.hex(), "mac_input": mac_in.hex(), "receipt_tag": tag.hex(),
                      "frame": raw(M([0, 1], [1, 8], [2, g.B(TID)], [15, g.B(tag)])).hex()})
    # negatives are taken against the 4096-byte case; each changes one bound input
    base = next(c for c in cases if c["id"] == "4096-bytes")
    ct = bytes.fromhex(base["ciphertext"])
    digest = bytes.fromhex(base["ciphertext_digest"])
    want = base["receipt_tag"]

    def neg(nid, why, key=KEY, dig=digest, blob=BLOB, tid=TID, total=len(ct), body=ct, pos_tid=None):
        _, _, _, t = parts(key, dig, blob, tid, total, body) if pos_tid is None else _with_pos(key, dig, blob, tid, total, body, pos_tid)
        negs.append({"id": nid, "against": "4096-bytes", "why": why, "content_key": key.hex(), "blob_id": blob.hex(),
                     "transfer_id": tid.hex(), "ciphertext": body.hex(), "total_bytes": total,
                     "ciphertext_digest": dig.hex(), "pos_transfer_id": (pos_tid or tid).hex(), "receipt_tag": t.hex(),
                     "differs": t.hex() != want})

    def _with_pos(key, dig, blob, tid, total, body, pos_tid):
        k = hkdf(dig, key, enc([LBL_KDF, CB(blob)]))
        h_pos = ds_hash(LBL_POS, enc([CB(pos_tid), CB(body)]))
        mac_in = enc([LBL_MAC, CB(tid), CB(blob), total, CB(dig), CB(h_pos)])
        return k, h_pos, mac_in, hmac.new(k, mac_in, hashlib.sha256).digest()

    other_tid = stream("direct-receipt/other-transfer-id", 16)
    flipped = bytearray(ct)
    flipped[100] ^= 1
    neg("another-transfer-id", "replay: the tag of another transfer id", tid=other_tid, body=ct)
    neg("possession-hash-of-another-transfer", "right transfer id in the MAC, possession hash over another id", pos_tid=other_tid)
    neg("flipped-ciphertext-bit", "one bit of the object changed", body=bytes(flipped))
    neg("truncated-ciphertext", "the object cut by one byte (total_bytes follows)", body=ct[:-1], total=len(ct) - 1)
    neg("truncated-ciphertext-same-total", "possession hash over a truncated object, total_bytes unchanged", body=ct[:-1], total=len(ct))
    neg("wrong-digest", "another ciphertext_digest", dig=stream("direct-receipt/other-digest", 32))
    neg("wrong-total-bytes", "total_bytes one more than the object length", total=len(ct) + 1)
    neg("wrong-key", "another content key", key=stream("direct-receipt/other-key", 32))
    neg("another-blob-id", "a tag from a different blob id", blob=stream("direct-receipt/other-blob", 32))
    resumed = {"id": "resumed-transfer-same-tag", "against": "4096-bytes",
               "why": "a resumed transfer is confirmed like a fresh one: the tag covers the whole object and depends only on the transfer id",
               "receipt_tag": want}
    return cases, negs, resumed


# ------------------------------------------------------------------ wire frames (LanFrameV1)
def frame_vectors(tag: bytes):
    out = []
    TIDB = g.B(TID)

    def add(suffix, expect, error, value, cddl, note=""):
        b = value if isinstance(value, bytes) else raw(value)
        out.append({"id": f"LanFrameV1/{suffix}", "struct": "LanFrameV1", "expect": expect,
                    "error": error, "cddl": cddl, "hex": b.hex(), "note": note})

    R = M([0, 1], [1, 8], [2, TIDB], [15, g.B(tag)])
    add("receipt", "accept", None, R, "valid")
    add("receipt-other-id-and-tag", "accept", None, M([0, 1], [1, 8], [2, g.B(b"\xee" * 16)], [15, g.B(b"\x01" * 32)]), "valid")
    # controls: the earlier frames are untouched by the new rule
    sha = g.B(b"\x11" * 32)
    add("control/object-begin", "accept", None, M([0, 1], [1, 1], [2, TIDB], [3, 8], [4, 4112], [5, sha]), "valid")
    add("control/object-chunk", "accept", None, M([0, 1], [1, 2], [2, TIDB], [6, 0], [7, g.B(b"abc")]), "valid")
    add("control/object-end", "accept", None, M([0, 1], [1, 3], [2, TIDB]), "valid")
    add("control/ack", "accept", None, M([0, 1], [1, 4], [2, TIDB], [14, 4112]), "valid")
    add("control/error", "accept", None, M([0, 1], [1, 7], [2, TIDB], [13, 3]), "valid")
    add("control/fetch-blob", "accept", None, M([0, 1], [1, 6], [2, TIDB], [10, g.B(b"\x05" * 32)], [11, g.B(b"\x09" * 32)], [12, g.B(b"\x07" * 32)]), "valid")
    add("neg/tag-31-bytes", "reject", "value_range", mset(R, 15, g.B(tag[:31])), "invalid")
    add("neg/tag-33-bytes", "reject", "value_range", mset(R, 15, g.B(tag + b"\x00")), "invalid")
    add("neg/tag-empty", "reject", "value_range", mset(R, 15, g.B(b"")), "invalid")
    add("neg/tag-is-text", "reject", "wrong_type", mset(R, 15, "tag"), "invalid")
    add("neg/tag-null", "reject", "null_forbidden", mset(R, 15, None), "invalid")
    add("neg/transfer-id-15-bytes", "reject", "value_range", mset(R, 2, g.B(TID[:15])), "invalid")
    add("neg/transfer-id-missing", "reject", "missing_field", mdel(R, 2), "invalid")
    add("neg/version-2", "reject", "value_range", mset(R, 0, 2), "invalid")
    add("neg/kind-9", "reject", "value_range", mset(R, 1, 9), "invalid")
    add("neg/kind-0", "reject", "value_range", mset(R, 1, 0), "invalid")
    add("neg/unknown-key-16", "reject", "unknown_field", mset(R, 16, 1), "invalid")
    add("neg/key-23-extension", "reject", "unknown_field", mset(R, 23, M([0, 1])), "invalid",
        "frames are transport records: no extension point")
    add("neg/unsorted-keys", "reject", "unsorted_keys", {"$rawmap": [R["$map"][3], R["$map"][0]] + R["$map"][1:3]}, "n/a")
    add("neg/trailing-bytes", "reject", "trailing_bytes", raw(R) + b"\x00", "n/a")
    add("neg/truncated", "reject", "truncated", raw(R)[:-1], "n/a")
    # the registry checks fields, CDDL checks the kind-specific combination: the codecs accept, CDDL refuses
    add("cddl-only/receipt-without-tag", "accept", None, mdel(R, 15), "invalid",
        "kind 8 requires receipt_tag (key 15): the registry lists the key as optional, so only CDDL refuses")
    add("cddl-only/receipt-with-ack-offset", "accept", None, mset(R, 14, 4112), "invalid",
        "an object_receipt carries no other kind's field: only CDDL refuses")
    add("cddl-only/ack-with-receipt-tag", "accept", None, M([0, 1], [1, 4], [2, TIDB], [14, 1], [15, g.B(tag)]), "invalid",
        "receipt_tag belongs to kind 8 only")
    return out


def main(out):
    cases, negs, resumed = construction()
    vecs = frame_vectors(bytes.fromhex(cases[1]["receipt_tag"]))
    ids = [v["id"] for v in vecs]
    assert len(ids) == len(set(ids))
    doc = {"labels": {"kdf": LBL_KDF, "mac": LBL_MAC, "hash": LBL_POS, "ciphertext_digest": LBL_CT},
           "constructions": cases, "negatives": negs, "resumed": resumed, "structs": vecs}
    Path(out).write_text(json.dumps(doc, indent=1) + "\n")
    print(f"wrote {len(cases)} constructions, {len(negs)} negatives, {len(vecs)} frame vectors to {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
