#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Regenerate the committed V-012 seed corpus (ADR-0251 ruling 1) as seeds.json.gz: {target: {file name: hex}}.

Sources, all committed: the frozen expressive wire vectors (positive and negative rows), the pack-file golden
vectors, the container budget rows (every over-ceiling case, one over and exactly at), and the hostile WebP chunk
shapes of verification/prototypes/expressive-wire/hostile_inputs.rs. Truncated, bit-flipped and appended variants of
the golden pack files and manifests are added deterministically.

File names carry the expectation: `ok-*` must be accepted, `bad-*` must be refused, `x-*` only must not crash or
hang. `crates/om-core/tests/expressive_fuzz.rs` replays every seed and enforces that.

    python3 gen_seed_corpus.py                  # rewrite seeds.json.gz
    python3 gen_seed_corpus.py --check          # fail if the archive differs from what this script would write
    python3 gen_seed_corpus.py --unpack DIR     # write DIR/<target>/<file> for libFuzzer and AFL++ (what run_fuzz.py does)
"""
import gzip
import hashlib
import io
import json
import re
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ARCHIVE = HERE / "seeds.json.gz"
WIRE = ROOT / "verification/vectors/expressive-wire-vectors.json.gz"
PACKS = ROOT / "verification/vectors/pack-file-vectors.json"

out: dict[str, bytes] = {}


def put(target: str, label: str, slug: str, data: bytes) -> None:
    slug = re.sub(r"[^A-Za-z0-9._-]+", "-", slug).strip("-")[:80]
    digest = hashlib.sha256(data).hexdigest()[:8]
    out[f"{target}/{label}-{slug}-{digest}"] = data


def verdict(expect: str) -> str:
    return "ok" if expect in ("accept", "valid") else "bad"


def cbor_read(data: bytes, at: int = 0):
    """Minimal reader for the deterministic subset the pack file uses. Returns (value, next offset)."""
    head = data[at]
    major, info = head >> 5, head & 31
    at += 1
    if info < 24:
        n = info
    else:
        width = {24: 1, 25: 2, 26: 4, 27: 8}[info]
        n = int.from_bytes(data[at : at + width], "big")
        at += width
    if major == 0:
        return n, at
    if major == 2:
        return data[at : at + n], at + n
    if major == 3:
        return data[at : at + n].decode(), at + n
    if major == 4:
        items = []
        for _ in range(n):
            v, at = cbor_read(data, at)
            items.append(v)
        return items, at
    if major == 5:
        fields = []
        for _ in range(n):
            k, at = cbor_read(data, at)
            v, at = cbor_read(data, at)
            fields.append((k, v))
        return fields, at
    raise ValueError(f"unsupported major type {major}")


def variants(target: str, slug: str, data: bytes) -> None:
    """Deterministic truncation, flip and append variants. They carry no expectation."""
    n = len(data)
    for cut in sorted({0, 1, 2, 7, 12, n // 3, n // 2, n - 33, n - 2, n - 1}):
        if 0 <= cut < n:
            put(target, "x", f"{slug}-trunc{cut}", data[:cut])
    for i, at in enumerate(sorted({0, 1, 2, 5, 9, n // 4, n // 2, (3 * n) // 4, n - 1})):
        if 0 <= at < n:
            flipped = bytearray(data)
            flipped[at] ^= 1 << (i % 8)
            put(target, "x", f"{slug}-flip{at}", bytes(flipped))
    put(target, "x", f"{slug}-trailing", data + b"\x00")


def wire_rows() -> None:
    doc = json.load(gzip.open(WIRE, "rt"))
    for row in doc["vectors"]:
        target = {
            "ContentPackRelease": "release",
            "ConversationEvent": "event",
        }.get(row["object"])
        if target:
            label = verdict(row["expect"])
            if label == "ok" and "/semantic-" in row["id"]:
                # Wire-grammar accepts whose product verdict is a named semantic refusal (alt text scalars, canvas
                # and size ceilings, lineage shape, ...). The replay test cannot assert accept for these.
                label = "x"
            if target == "event" and label == "ok":
                # Wire-grammar accepts. Core additionally binds the clock tiebreak to the authoring device
                # (ADR-0121) and the required capability, which these standalone vector bodies do not model, so
                # they carry no product verdict; refusals stay `bad-` because every grammar refusal is final.
                label = "x"
            put(target, label, row["id"], bytes.fromhex(row["hex"]))
        elif row["object"] == "ContentPackControlState":
            # The control-state target needs a release in front. Pair it with the genesis manifest; most pairings
            # are refusals, which is the point: they reach the envelope and lineage checks.
            put("control_state", "x", row["id"], PAIR_PREFIX + bytes.fromhex(row["hex"]))
    for row in doc["structs"]:
        if row["struct"] == "ExpressiveAssetRef":
            ref = bytes.fromhex(row["hex"])
            # The wire grammar bounds alt_text at 2048 bytes; Core adds the 512-scalar rule of the architecture, so the
            # 2048-byte accept row is a wire-level verdict only and carries no product expectation.
            label = "x" if row["id"].endswith("alt-text-2048-bytes") else verdict(row["expect"])
            put("asset_ref", label, row["id"], struct.pack(">H", len(ref)) + ref)
            put("asset_ref", "x", row["id"] + "-with-header", struct.pack(">H", len(ref)) + ref + STILL)
    kinds = {"sticker.v1": 0, "custom-emoji.v1": 1, "loop-animation.v1": 3}
    for row in doc["container"]:
        f = row["facts"]
        frames = f["frames"]
        animated = len(frames) > 1 or any(fr[4] != 0 for fr in frames)
        data = struct.pack(
            "<BIIIB",
            kinds[row["resource_profile"]],
            f["encoded_size"],
            f["canvas_w"],
            f["canvas_h"],
            1 if animated else 0,
        )
        data += b"".join(struct.pack("<5I", *fr) for fr in frames)
        put("facts", verdict(row["expect"]), row["id"], data)


def chunk(tag: bytes, payload: bytes) -> bytes:
    return tag + struct.pack("<I", len(payload)) + payload + (b"\x00" if len(payload) % 2 else b"")


def riff(chunks: bytes) -> bytes:
    return b"RIFF" + struct.pack("<I", len(chunks) + 4) + b"WEBP" + chunks


STILL = b"RIFF\x12\0\0\0WEBPVP8L\x06\0\0\0\x2f\0\0\0\0\0"


def animation(payload: bytes, frames: int = 2) -> bytes:
    chunks = chunk(b"VP8X", bytes([2, 0, 0, 0, 0, 0, 0, 0, 0, 0])) + chunk(b"ANIM", bytes(6))
    for _ in range(frames):
        frame = bytearray(16)
        frame[12] = 34
        chunks += chunk(b"ANMF", bytes(frame) + payload)
    return riff(chunks)


def vp8x(width: int, height: int, flags: int, rest: bytes) -> bytes:
    canvas = (width - 1).to_bytes(3, "little") + (height - 1).to_bytes(3, "little")
    return riff(chunk(b"VP8X", bytes([flags, 0, 0, 0]) + canvas) + rest)


def webp_rows() -> None:
    good = animation(STILL[12:])
    put("webp", "ok", "still-lossless", STILL)
    put("webp", "ok", "animation-two-frames", good)
    variants("webp", "animation", good)
    variants("webp", "still", STILL)
    for len_field in (1, 5, 7, 0xFFFFFFFF):
        bad = bytearray(good)
        bad[16:20] = struct.pack("<I", len_field)
        put("webp", "x", f"chunk-length-{len_field}", bytes(bad))
    put("webp", "x", "nested-riff", animation(chunk(b"RIFF", good)))
    put("webp", "x", "nested-anmf", animation(chunk(b"ANMF", good)))
    put("webp", "x", "overlapping-payload", animation(STILL[12:] + STILL[12:]))
    put("webp", "x", "tail-of-animation", riff(good[30:]))
    # Declared canvas at and one over each profile ceiling (still image inside an extended header).
    for label, side in (("sticker", 512), ("custom-emoji", 256), ("loop", 1024)):
        for delta in (0, 1):
            w = side + delta
            put("webp", "x", f"canvas-{label}-{w}", vp8x(w, w, 0, chunk(b"VP8L", STILL[20:])))
    # Frame counts at and one over each ceiling (frames of a 1x1 payload).
    for label, count in (("90", 90), ("91", 91), ("300", 300), ("301", 301)):
        put("webp", "x", f"frames-{label}", animation(STILL[12:], count))
    # Maximum canvas in the extended header with no frame data; 24-bit fields at their limits.
    put("webp", "x", "canvas-max-24bit", vp8x(1 << 24, 1 << 24, 2, b""))
    put("webp", "x", "unknown-flag-bits", vp8x(1, 1, 0xC1, chunk(b"VP8L", STILL[20:])))


def pack_rows() -> dict:
    vectors = json.load(open(PACKS))["vectors"]
    files = {r["id"]: bytes.fromhex(r["hex"]) for r in vectors}
    labels = {"genesis": "ok", "portable-update": "ok", "pinned-update": "x"}
    for name, data in files.items():
        put("pack_file", labels[name], f"golden-{name}", data)
        variants("pack_file", name, data)
    # Manifests and the genesis control envelope, taken out of the genesis file.
    fields, _ = cbor_read(files["genesis"])
    manifest = dict(fields)[2]
    control = dict(fields)[3][0]
    put("release", "ok", "golden-genesis-manifest", manifest)
    put("control_state", "ok", "golden-genesis-pair", struct.pack(">H", len(manifest)) + manifest + control)
    put("control_state", "x", "golden-genesis-no-envelope", struct.pack(">H", len(manifest)) + manifest)
    variants("release", "golden-genesis-manifest", manifest)
    variants("control_state", "golden-genesis-pair", struct.pack(">H", len(manifest)) + manifest + control)
    for name in ("portable-update", "pinned-update"):
        f, _ = cbor_read(files[name])
        d = dict(f)
        put("release", "ok", f"golden-{name}-manifest", d[2])
        put("control_state", "x", f"golden-{name}-pair", struct.pack(">H", len(d[2])) + d[2] + d[3][-1])
    return {"manifest": manifest}


PAIR_PREFIX = b""


def main() -> int:
    global PAIR_PREFIX
    info = pack_rows()
    PAIR_PREFIX = struct.pack(">H", len(info["manifest"])) + info["manifest"]
    wire_rows()
    webp_rows()

    tree: dict[str, dict[str, str]] = {}
    for name, data in sorted(out.items()):
        target, file = name.split("/", 1)
        tree.setdefault(target, {})[file] = data.hex()
    blob = io.BytesIO()
    with gzip.GzipFile(fileobj=blob, mode="wb", mtime=0, compresslevel=9) as z:
        z.write(json.dumps(tree, sort_keys=True, separators=(",", ":")).encode())
    packed = blob.getvalue()
    counts = {t: len(f) for t, f in tree.items()}

    if "--unpack" in sys.argv:
        dest = Path(sys.argv[sys.argv.index("--unpack") + 1])
        for target, files in json.loads(gzip.decompress(ARCHIVE.read_bytes())).items():
            (dest / target).mkdir(parents=True, exist_ok=True)
            for file, hexed in files.items():
                (dest / target / file).write_bytes(bytes.fromhex(hexed))
        print(f"unpacked {sum(counts.values())} seeds to {dest}")
        return 0
    if "--check" in sys.argv:
        if not ARCHIVE.exists() or ARCHIVE.read_bytes() != packed:
            print("seed corpus archive differs from what gen_seed_corpus.py writes", file=sys.stderr)
            return 1
        print(f"seed corpus current: {sum(counts.values())} files")
        return 0
    ARCHIVE.write_bytes(packed)
    print(json.dumps(counts, sort_keys=True), f"total {sum(counts.values())}", f"{len(packed)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
