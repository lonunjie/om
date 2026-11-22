#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Regenerate the committed seed corpus of the owner-recovery and link-ceremony fuzz targets as seeds.json.gz:
{target: {file name: hex}}. Every byte of it comes from committed material and is synthetic.

Sources: the frozen vector files for the recovery sheet, record, notice, owner-recovery ceremony and device-link
ceremony (positive and negative rows), the controller-change vector, and deterministic truncation, bit-flip and append
variants of the positive rows. The first byte of most seeds is the case selector of the entry point (see
crates/om-core/src/flows/recovery_fuzz.rs).

File names carry the expectation: `ok-*` must be accepted, `bad-*` must be refused, `x-*` only must not crash or hang.
`crates/om-core/tests/recovery_fuzz.rs` replays every seed and enforces that.

    python3 gen_seed_corpus.py                  # rewrite seeds.json.gz
    python3 gen_seed_corpus.py --check          # fail if the archive differs from what this script would write
    python3 gen_seed_corpus.py --unpack DIR     # write DIR/<target>/<file> for libFuzzer (what run_fuzz.py does)
"""
import base64
import gzip
import hashlib
import io
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ARCHIVE = HERE / "seeds.json.gz"
VEC = ROOT / "verification/vectors"
TYPES = ROOT / "spec/registries/om-object-types.json"
TARGETS = ("sheet", "record", "kit", "approval", "notice", "link", "frame", "continuation")

out: dict[str, bytes] = {}


def put(target: str, label: str, slug: str, data: bytes) -> None:
    slug = re.sub(r"[^A-Za-z0-9._-]+", "-", slug).strip("-")[:70]
    digest = hashlib.sha256(data).hexdigest()[:8]
    out[f"{target}/{label}-{slug}-{digest}"] = data


def verdict(expect: str) -> str:
    return "ok" if expect in ("accept", "valid", "ok") else "bad"


def gz_json(name: str):
    d = json.load(gzip.open(VEC / name))
    # Rows whose bytes are not one hex string (multi-part vectors) carry no single input: drop them.
    for key in ("vectors", "structs"):
        if isinstance(d, dict) and key in d:
            d[key] = [v for v in d[key] if isinstance(v.get("hex"), str)]
    return d


# ------------------------------------------------------------------------------------------------ tiny CBOR


def head(major: int, n: int) -> bytes:
    m = major << 5
    if n < 24:
        return bytes([m | n])
    for info, width in ((24, 1), (25, 2), (26, 4), (27, 8)):
        if n < 1 << (8 * width):
            return bytes([m | info]) + n.to_bytes(width, "big")
    raise ValueError(n)


def bstr(b: bytes) -> bytes:
    return head(2, len(b)) + b


def item_end(data: bytes, at: int) -> int:
    """End offset of the CBOR item starting at `at` (deterministic subset)."""
    major, info = data[at] >> 5, data[at] & 31
    at += 1
    if info < 24:
        n = info
    else:
        width = {24: 1, 25: 2, 26: 4, 27: 8}[info]
        n = int.from_bytes(data[at : at + width], "big")
        at += width
    if major in (0, 1, 7):
        return at
    if major in (2, 3):
        return at + n
    if major == 4:
        for _ in range(n):
            at = item_end(data, at)
        return at
    if major == 5:
        for _ in range(n):
            at = item_end(data, at)
            at = item_end(data, at)
        return at
    raise ValueError(major)


def map_items(data: bytes):
    """(key, raw value bytes) of a top-level map with small unsigned keys."""
    assert data[0] >> 5 == 5
    n = data[0] & 31
    at, items = 1, []
    for _ in range(n):
        key = data[at]
        assert key >> 5 == 0
        at += 1
        end = item_end(data, at)
        items.append((key & 31 if key < 24 else key, data[at:end]))
        at = end
    return items


def read_bstr(raw: bytes) -> bytes:
    assert raw[0] >> 5 == 2
    info = raw[0] & 31
    if info < 24:
        return raw[1 : 1 + info]
    width = {24: 1, 25: 2, 26: 4}[info]
    n = int.from_bytes(raw[1 : 1 + width], "big")
    return raw[1 + width : 1 + width + n]


def cmap(entries: list[tuple[int, bytes]]) -> bytes:
    return head(5, len(entries)) + b"".join(head(0, k) + v for k, v in sorted(entries))


def carray(items: list[bytes]) -> bytes:
    return head(4, len(items)) + b"".join(items)


def uint(n: int) -> bytes:
    return head(0, n)


BUCKETS = [16384, 65536]


def pad_record(plain: bytes, bucket: int) -> bytes | None:
    """The record plaintext padded to exactly one bucket with a key 22 byte string, as the product writes it."""
    try:
        items = [(k, v) for k, v in map_items(plain) if k != 22]
    except (AssertionError, IndexError, ValueError):
        return None
    for l in range(0, bucket):
        pad = head(0, 22) + bstr(bytes(l))
        body = cmap(items + [(22, bstr(bytes(l)))])
        if len(body) == bucket:
            return body
        if len(body) > bucket:
            return None
    return None


# ------------------------------------------------------------------------------------------------ variants


def variants(target: str, slug: str, data: bytes, keep: int = 1) -> None:
    """Deterministic truncation, flip and append variants of a positive seed (selector byte kept). No expectation."""
    n = len(data)
    if n <= keep:
        return
    cuts = sorted({keep, keep + 1, n // 2, n - 1, max(keep, n - 4)})
    for cut in cuts:
        if keep <= cut < n:
            put(target, "x", f"{slug}-cut{cut}", data[:cut])
    for at in sorted({keep, keep + 1, keep + 3, n // 3, n // 2, n - 1}):
        if keep <= at < n:
            flipped = bytearray(data)
            flipped[at] ^= 0x01
            put(target, "x", f"{slug}-flip{at}", bytes(flipped))
    put(target, "x", f"{slug}-append", data + b"\x00")


def with_variants(target: str, label: str, slug: str, data: bytes, keep: int = 1) -> None:
    put(target, label, slug, data)
    if label == "ok" and len(data) <= 4096:
        variants(target, slug, data, keep)


# ------------------------------------------------------------------------------------------------- sources


def registry_names() -> list[str]:
    return sorted(t["name"] for t in json.load(open(TYPES))["types"])


def sheet_seeds() -> None:
    d = gz_json("recovery-sheet-vectors.json.gz")
    for s in d["sheets"]:
        sid = s["id"]
        binary = bytes.fromhex(s["binary_hex"])
        for text, form in ((s["compact"], "compact"), (s["sheet_form"], "form"), (s["qr"], "qr")):
            with_variants("sheet", "ok", f"{sid}-{form}", b"\x00" + text.encode())
        with_variants("sheet", "ok", f"{sid}-bin", b"\x01" + binary)
        with_variants("sheet", "ok", f"{sid}-bin-nocheck", b"\x02" + binary[:-4])
    for n in d["negatives"]:
        put("sheet", "bad", n["id"], b"\x00" + n["text"].encode())
    for c in d["corrections"]:
        put("sheet", "x", c["id"], b"\x00" + c["typed"].encode())
    for f in d["first_bad_line"]:
        put("sheet", "x", "first-bad-line", b"\x00" + f["text"].encode())
    lg = d["legacy"]
    put("sheet", "x", "legacy-ok", b"\x00" + lg["text"].encode())
    put("sheet", "x", "legacy-bad", b"\x00" + lg["bad_check"].encode())
    for r in d["redirect"]:
        port = r["port"]
        sel = 0 if port is None else 2
        body = bytes([r["hops"], sel]) + (port or 0).to_bytes(2, "big") + r["host"].encode()
        put("sheet", "x", f"redirect-{r['id']}", b"\x05" + body)
    owner = gz_json("owner-recovery-vectors.json.gz")
    for v in owner["vectors"]:
        if v["object"] == "OwnerRecoveryCheckpoint":
            put("sheet", verdict(v["expect"]), v["id"], b"\x03" + bytes.fromhex(v["hex"]))
    s = owner["sheet"]
    put("sheet", "ok", "owner-v2-binary", b"\x01" + bytes.fromhex(s["binary"]))
    put("sheet", "ok", "owner-v2-compact", b"\x00" + s["compact"].encode())
    put("sheet", "ok", "owner-v2-qr", b"\x00" + s["qr"].encode())
    raw = bytes.fromhex(s["binary"])
    put("sheet", "ok", "owner-v2-nocheck", b"\x02" + raw[:-4])


def record_seeds() -> None:
    d = gz_json("recovery-record-vectors.json.gz")
    names = registry_names()
    for v in d["vectors"]:
        data = bytes.fromhex(v["hex"])
        label = verdict(v["expect"])
        mode = {"RecoveryRecord": 0, "RecoveryRedirect": 1, "RecoveryRecordEnvelope": 3}.get(v["object"])
        if mode is None:
            continue
        if mode == 3:
            put("record", "x", v["id"], bytes([3]) + data) if len(data) < 70000 else None
            continue
        # Bare plaintext vectors are unpadded plaintexts: only the typed read must accept them (no verify).
        semantic = "semantic" in v["id"]  # accepted by the wire grammar, refused by a rule of the typed read
        put("record", "x" if label == "bad" or semantic else "ok", v["id"], bytes([mode]) + data)
        if label == "ok" and mode == 0 and len(data) < 4096:
            variants("record", v["id"], bytes([mode]) + data)
    for s in d["crypto"]["seals"]:
        if "envelope_hex" in s:
            put("record", "x", f"envelope-{s['id']}", b"\x03" + bytes.fromhex(s["envelope_hex"]))
        if "record_hex" not in s:
            continue
        plain = bytes.fromhex(s["record_hex"])
        padded = pad_record(plain, s["bucket"]) if s["bucket"] in BUCKETS else None
        if padded:
            put("record", "x", f"padded-{s['id']}", b"\x04" + padded)
            put("record", "x", f"padded-{s['id']}-plaintext", b"\x02" + padded)
    for o in d["crypto"]["open"]:
        if "envelope_hex" in o:
            put("record", "x", f"open-{o['id']}", b"\x03" + bytes.fromhex(o["envelope_hex"]))
    sl = d["crypto"]["second_layer"]
    put("record", "ok", "snapshot", b"\x05" + bytes.fromhex(sl["snapshot_hex"]))
    put("record", "x", "snapshot-layer", b"\x06" + bytes.fromhex(sl["layer_hex"]))
    # Every object of the vector files, through the registry check of the kit target.
    for v in d["vectors"][:400]:
        if v["object"] in names and len(v["hex"]) < 8000:
            put("kit", "x", f"registry-{v['id']}", b"\x01" + bytes([names.index(v["object"])]) + bytes.fromhex(v["hex"]))
    for v in d["vectors"]:
        if v["object"] != "RecoveryKit":
            continue
        data = bytes.fromhex(v["hex"])
        label = verdict(v["expect"])
        with_variants("kit", label, v["id"], b"\x00" + data) if label == "ok" else put("kit", "bad", v["id"], b"\x00" + data)


def approval_seeds() -> None:
    d = gz_json("owner-recovery-vectors.json.gz")
    mode = {
        "RecoveryApprovalProposal": 1,
        "RecoveryApprovalReply": 3,
        "RecoveryApprovalChunk": 4,
        "RecoveryApprovalEvidence": 5,
    }
    for v in d["vectors"]:
        m = mode.get(v["object"])
        if m is None:
            continue
        data = bytes.fromhex(v["hex"])
        label = verdict(v["expect"])
        if label == "ok":
            with_variants("approval", "ok", v["id"], bytes([m]) + data) if len(data) < 6000 else put("approval", "ok", v["id"], bytes([m]) + data)
        else:
            put("approval", "bad", v["id"], bytes([m]) + data)
    c = d["ceremony"]
    put("approval", "ok", "offer", b"\x00" + c["offer"].encode())
    variants("approval", "offer", b"\x00" + c["offer"].encode())
    put("approval", "ok", "descriptor", b"\x02" + bytes.fromhex(c["descriptor"]))
    variants("approval", "descriptor", b"\x02" + bytes.fromhex(c["descriptor"]))
    ev = bytes.fromhex(c["evidence"])
    put("approval", "x", "assembler-evidence", bytes([6, 17]) + ev[:200])
    put("approval", "x", "assembler-small", bytes([6, 3]) + b"abcdefghij")
    for r in c["refusals"]:
        if "offer" in r:
            put("approval", "x", f"refusal-{r['case']}", b"\x00" + r["offer"].encode())
        if "descriptor" in r:
            put("approval", "x", f"refusal-{r['case']}", b"\x02" + bytes.fromhex(r["descriptor"]))
    put("approval", "x", "derivation", b"\x07" + bytes(range(48)))


def notice_seeds() -> None:
    d = gz_json("recovery-notice-vectors.json.gz")
    for v in d["vectors"]:
        data = bytes.fromhex(v["hex"])
        label = verdict(v["expect"])
        if len(data) > 6000:
            put("notice", "x", v["id"], b"\x00" + data[:6000]) if False else None
            continue
        if label == "ok":
            with_variants("notice", "ok", v["id"], b"\x00" + data)
            try:
                items = dict(map_items(data))
                policy = read_bstr(items[2])
                put("notice", "x", f"policy-{v['id']}", b"\x02" + policy)
            except (KeyError, AssertionError, IndexError, ValueError):
                pass
        else:
            put("notice", "x" if label == "bad" else label, v["id"], b"\x00" + data)
    for s in d["structs"]:
        data = bytes.fromhex(s["hex"])
        if s["struct"] == "RecoveryDossierEntry":
            put("notice", "x", s["id"], b"\x01" + carray([data]))
            put("notice", "x", s["id"] + "-bare", b"\x01" + data)
    iid = bytes(range(16))
    put("notice", "ok", "inbox-item-request", b"\x05" + cmap([(0, bstr(iid)), (1, uint(1)), (2, bstr(b"request-bytes"))]))
    put("notice", "ok", "inbox-item-veto", b"\x05" + cmap([(0, bstr(iid)), (1, uint(2)), (2, bstr(b"veto-bytes"))]))
    put("notice", "bad", "inbox-item-kind", b"\x05" + cmap([(0, bstr(iid)), (1, uint(3)), (2, bstr(b"x"))]))
    put("notice", "x", "inbox-item-array", b"\x05" + carray([bstr(iid), uint(1), bstr(b"x")]))


def b64url(b: bytes) -> str:
    return base64.urlsafe_b64encode(b).decode().rstrip("=")


def link_payload(version: int, expiry: int, secret: bytes, a: bytes, b: bytes, uri: str) -> bytes:
    return bytes([version]) + expiry.to_bytes(4, "big") + secret + bytes([len(a)]) + a + bytes([len(b)]) + b + uri.encode()


def link_seeds() -> None:
    d = gz_json("device-link-ceremony-vectors.json.gz")
    i = d["inputs"]
    a, b, secret = bytes.fromhex(i["a_cap"]), bytes.fromhex(i["b_recv"]), bytes.fromhex(i["secret"])
    uri = bytes.fromhex(i["home_uri"]).decode()
    good = "om:link/" + b64url(link_payload(2, i["expiry_s"], secret, a, b, uri))
    put("link", "x", "device-link", b"\x00" + good.encode())
    variants("link", "device-link", b"\x00" + good.encode())
    put("link", "bad", "device-link-v1", b"\x00" + ("om:link/" + b64url(link_payload(1, i["expiry_s"], secret, a, b, uri))).encode())
    put("link", "bad", "device-link-bad-uri", b"\x00" + ("om:link/" + b64url(link_payload(2, i["expiry_s"], secret, a, b, "not a uri"))).encode())
    put("link", "bad", "device-link-short-secret", b"\x00" + ("om:link/" + b64url(link_payload(2, 1, secret[:8], a, b, uri))).encode())
    owner = gz_json("owner-recovery-vectors.json.gz")
    for r in owner["ceremony"]["refusals"]:
        if "offer" in r:
            put("link", "x", f"refusal-{r['case']}", b"\x00" + r["offer"].encode())
    put("link", "ok", "approve-offer", b"\x03" + owner["ceremony"]["offer"].encode())
    variants("link", "approve-offer", b"\x03" + owner["ceremony"]["offer"].encode())
    cc = json.load(open(VEC / "controller-change-v1.json"))
    put("link", "ok", "controller-link", b"\x02" + cc["link"].encode())
    variants("link", "controller-link", b"\x02" + cc["link"].encode())
    put("link", "bad", "controller-link-action", b"\x02" + cc["link"].replace("/0/", "/2/", 1).encode())
    service = b"https://intro.example.test/x"
    token = bytes(range(16)) + bytes(range(32)) + carray([cmap([(0, head(3, len(service)) + service)])])
    put("link", "ok", "invite-token", b"\x01" + token)
    variants("link", "invite-token", b"\x01" + token)
    put("link", "bad", "invite-token-no-service", b"\x01" + bytes(48))
    put("link", "bad", "invite-token-empty-list", b"\x01" + bytes(48) + carray([]))


def frame_body(body: bytes) -> bytes:
    return len(body).to_bytes(4, "big") + body


def frame_seeds() -> None:
    d = gz_json("device-link-ceremony-vectors.json.gz")
    i, cont = d["inputs"], d["continuation"]
    states = [bytes.fromhex(s) for s in cont["states"]]
    creds = [bytes.fromhex(c) for c in cont["credentials"]]
    bodies = {
        "request": bytes.fromhex(i["request"]),
        "chain-chunk": cmap([(0, uint(7)), (1, uint(0)), (2, carray([bstr(s) for s in states])), (3, carray([bstr(c) for c in creds]))]),
        "history-chunk": cmap([(0, uint(6)), (1, uint(0)), (2, uint(3)), (3, uint(1)), (4, bstr(b"snapshot-bytes"))]),
        "intro": cmap([(0, uint(4)), (1, b"\x63abc"), (2, bstr(b"intro"))]),
        "reject": cmap([(0, uint(3))]),
        "plain-notice-bytes": bytes.fromhex(i["notice"]),
    }
    for name, body in bodies.items():
        for sel in (0, 1):
            put("frame", "x", f"{name}-{'approval' if sel else 'ceremony'}", bytes([sel]) + frame_body(body))
    base = frame_body(bodies["chain-chunk"])
    put("frame", "x", "nonzero-padding", bytes([0]) + base + b"\x00\x01")
    put("frame", "x", "length-past-end", bytes([0]) + (10_000).to_bytes(4, "big") + b"abc")
    put("frame", "x", "length-max", bytes([1]) + b"\xff\xff\xff\xff" + b"abc")
    put("frame", "x", "empty", b"\x00")
    put("frame", "x", "zero-length-body", bytes([0]) + frame_body(b""))
    variants("frame", "chain", bytes([0]) + base)
    # A raw envelope-looking item for the pre-AEAD path is the `link` mode 4 case, kept in that target.
    envelope = cmap([(0, uint(23)), (1, uint(1)), (2, uint(1)), (3, bstr(bytes(16))), (4, bstr(bytes(16))), (5, bstr(bytes(1120))), (6, bstr(bytes(1040))), (7, uint(1790000300))])
    put("link", "x", "raw-envelope", b"\x04" + envelope)
    put("link", "x", "raw-envelope-bad-ct", b"\x04" + envelope[:-20])
    put("link", "x", "frame-via-link", b"\x05" + frame_body(bodies["request"]))
    put("link", "x", "frame-via-link-approval", b"\x06" + frame_body(bodies["request"]))


def continuation_seeds() -> None:
    put("continuation", "x", "small", bytes([3, 4, 10, 20, 30, 40, 50, 60, 70]))
    put("continuation", "x", "empty", bytes([0, 0]))
    put("continuation", "x", "one-state", bytes([1, 0, 5]))
    put("continuation", "x", "boundary-big-items", bytes([4, 0, 255, 255, 255, 255]))
    put("continuation", "x", "full-chain", bytes([64, 255, 40]) + bytes([200] * 400))
    put("continuation", "x", "creds-only", bytes([0, 200]) + bytes([100] * 200))
    put("continuation", "x", "spill", bytes([30, 30]) + bytes([250] * 60))
    put("continuation", "x", "max-counts", bytes([65, 65]) + bytes([255] * 130))
    put("continuation", "x", "overflow-counts", bytes([66 + 65, 255]) + bytes([255] * 200))


def build() -> dict[str, dict[str, bytes]]:
    out.clear()
    sheet_seeds()
    record_seeds()
    approval_seeds()
    notice_seeds()
    link_seeds()
    frame_seeds()
    continuation_seeds()
    grouped: dict[str, dict[str, bytes]] = {t: {} for t in TARGETS}
    for key, data in sorted(out.items()):
        target, name = key.split("/", 1)
        grouped[target][name] = data
    return grouped


def archive_bytes(grouped) -> bytes:
    doc = {t: {n: d.hex() for n, d in sorted(files.items())} for t, files in sorted(grouped.items())}
    raw = json.dumps(doc, sort_keys=True, separators=(",", ":")).encode()
    buf = io.BytesIO()
    with gzip.GzipFile(fileobj=buf, mode="wb", mtime=0, compresslevel=9) as z:
        z.write(raw)
    return buf.getvalue()


def main() -> int:
    grouped = build()
    data = archive_bytes(grouped)
    if len(sys.argv) > 1 and sys.argv[1] == "--check":
        if ARCHIVE.read_bytes() != data:
            print("seeds.json.gz differs from what gen_seed_corpus.py writes; rerun it", file=sys.stderr)
            return 1
        print("seeds.json.gz is current")
        return 0
    if len(sys.argv) > 2 and sys.argv[1] == "--unpack":
        dest = Path(sys.argv[2])
        for target, files in grouped.items():
            (dest / target).mkdir(parents=True, exist_ok=True)
            for name, blob in files.items():
                (dest / target / name).write_bytes(blob)
        return 0
    ARCHIVE.write_bytes(data)
    for target, files in grouped.items():
        print(f"{target:13} {len(files):4} seeds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
