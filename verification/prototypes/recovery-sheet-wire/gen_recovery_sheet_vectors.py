#!/usr/bin/env python3
"""ADR-0200 recovery sheet string vectors (spec/protocol/recovery-sheet.md). Feasibility evidence, not product code.

Independent implementation: the encoder, the parser (with line checks), the one-substitution correction and the redirect
rules are written here from the spec with hashlib and base64 only; Core replays the result
(crates/om-core/tests/recovery_sheet_vectors.rs). Output: verification/vectors/recovery-sheet-vectors.json.gz.

Usage: gen_recovery_sheet_vectors.py OUT.json
"""
from __future__ import annotations

import base64
import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/cbor"))
from python_canonical import enc  # noqa: E402  (the existing independent canonical encoder)

ALPHA = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"


def det(label, n):
    out, i = b"", 0
    while len(out) < n:
        out += hashlib.sha256(f"om-sheet-vector:{label}:{i}".encode()).digest()
        i += 1
    return out[:n]


def ds_hash(label, body):
    # DS-HASH = canonical OM-CBOR [label: tstr, body: bstr]; the digest value is SHA-256 of that
    return hashlib.sha256(enc([label, {"$bytes": body.hex()}])).digest()


def whole_check(body):
    return ds_hash("OM-RECOVERY-SHEET-CHECK-v1", body)[:4]


def line_check(index, payload):
    d = ds_hash("OM-RECOVERY-SHEET-LINE-CHECK-v1", bytes([index]) + payload.encode())
    v = (d[0] << 2) | (d[1] >> 6)
    return ALPHA[(v >> 5) & 31] + ALPHA[v & 31]


def b32(b):
    return base64.b32encode(b).decode().rstrip("=")


def un_b32(s):
    pad = "=" * (-len(s) % 8)
    return base64.b32decode(s + pad)


def layout(kind, root, scope, port, host: bytes, version=1, host_len=None, check=True):
    b = bytes([version, kind])
    if kind == 1:
        b += root
    b += scope + (port or 0).to_bytes(2, "big") + bytes([len(host) if host_len is None else host_len]) + host
    return b + (whole_check(b) if check else b"\0\0\0\0")


def sheet_form(compact):
    groups = [compact[i:i + 4] for i in range(0, len(compact), 4)]
    lines = []
    for i in range(0, len(groups), 6):
        g = groups[i:i + 6]
        lines.append(" ".join(g) + " : " + line_check(i // 6, "".join(g)))
    return "\n".join(lines)


def qr(b):
    return "om:recover/" + base64.urlsafe_b64encode(b).decode().rstrip("=")


def valid_host(h):
    if not h or len(h) > 253 or "." not in h:
        return False
    labels = h.split(".")
    for l in labels:
        if not (1 <= len(l) <= 63) or l.startswith("-") or l.endswith("-"):
            return False
        if any(c not in "abcdefghijklmnopqrstuvwxyz0123456789-" for c in l):
            return False
    return not labels[-1].isdigit()


class Bad(Exception):
    pass


def parse_layout(b):
    if len(b) < 2:
        raise Bad("length")
    if b[0] != 1:
        raise Bad("version")
    if b[1] not in (1, 2):
        raise Bad("kind")
    full = b[1] == 1
    fixed = 2 + (32 if full else 0) + 32 + 2 + 1
    if len(b) < fixed + 4:
        raise Bad("length")
    hl = b[fixed - 1]
    if hl == 0 or hl > 253:
        raise Bad("host")
    if len(b) < fixed + hl + 4:
        raise Bad("length")
    if len(b) > fixed + hl + 4:
        raise Bad("trailing")
    if whole_check(b[:-4]) != b[-4:]:
        raise Bad("check")
    at = 2
    root = None
    if full:
        root = b[at:at + 32]
        at += 32
    scope = b[at:at + 32]
    at += 32
    port = int.from_bytes(b[at:at + 2], "big")
    host = b[fixed:fixed + hl]
    try:
        h = host.decode("ascii")
    except UnicodeDecodeError:
        raise Bad("host")
    if not valid_host(h):
        raise Bad("host")
    return {"root": root, "scope": scope, "port": port or None, "host": h}


def parse_text(text):
    t = text.strip()
    if t.startswith("om:recover/"):
        body = t[len("om:recover/"):]
        if len(body) % 4 == 1 or len(body) > 1024:
            raise Bad("qr")
        try:
            raw = base64.urlsafe_b64decode(body + "=" * (-len(body) % 4))
        except Exception:
            raise Bad("qr")
        if base64.urlsafe_b64encode(raw).decode().rstrip("=") != body:
            raise Bad("qr")  # non-zero trailing bits
        return ("sheet", parse_layout(raw))
    chars = [c for c in t if not c.isspace() and c != "-"]
    payload, line, idx, i, n = "", "", 0, 0, 0
    while i < len(chars):
        c = chars[i]
        i += 1
        n += 1
        if n > 1024:
            raise Bad("length")
        if c == ":":
            got = "".join(chars[i:i + 2]).upper()
            i += 2
            if got != line_check(idx, line):
                raise Bad(f"line_check:{idx + 1}")
            line, idx = "", idx + 1
            continue
        u = c.upper()
        if u not in ALPHA:
            raise Bad("alphabet")
        payload += u
        line += u
    if not payload:
        raise Bad("length")
    raw = un_b32_strict(payload)
    if len(raw) == 35:
        chk = hashlib.sha256(b"OM-RECOVERY-KEY-DISPLAY-CHECK" + raw[:32]).digest()[:3]
        if chk != raw[32:]:
            raise Bad("check")
        return ("legacy", {"root": raw[:32]})
    return ("sheet", parse_layout(raw))


def un_b32_strict(s):
    bits = "".join(f"{ALPHA.index(c):05b}" for c in s)
    nbytes = len(bits) // 8
    if len(bits) - nbytes * 8 >= 5 or any(c == "1" for c in bits[nbytes * 8:]):
        raise Bad("length")
    return bytes(int(bits[i * 8:(i + 1) * 8], 2) for i in range(nbytes))


def correction(text):
    cands = []
    for i, c in enumerate(text):
        for s in {"0": "O", "1": "IL", "8": "B"}.get(c, ""):
            cand = text[:i] + s + text[i + 1:]
            try:
                parse_text(cand)
                cands.append(cand)
            except Bad:
                pass
    if len(cands) != 1:
        return None
    return "".join(c.upper() for c in cands[0] if c.isalnum())


def build():
    out = {}
    long_host = ".".join(["a" * 63] * 3 + ["b" * 61])
    assert len(long_host) == 253 and valid_host(long_host)
    root, scope = det("root", 32), det("scope", 32)
    cases = [
        ("full-typical", 1, root, scope, None, "home.example.net"),
        ("full-shortest-host", 1, det("r2", 32), det("s2", 32), None, "a.b"),
        ("full-longest-host", 1, det("r3", 32), det("s3", 32), None, long_host),
        ("full-port-8443", 1, det("r4", 32), det("s4", 32), 8443, "home.example.net"),
        ("full-port-65535", 1, det("r5", 32), det("s5", 32), 65535, "x.example.org"),
        ("full-explicit-443", 1, det("r6", 32), det("s6", 32), 443, "home.example.net"),
        ("full-punycode-host", 1, det("r7", 32), det("s7", 32), None, "xn--bcher-kva.example"),
        ("place-typical", 2, None, scope, None, "home.example.net"),
        ("place-shortest-host", 2, None, det("s2", 32), None, "a.b"),
        ("place-longest-host", 2, None, det("s3", 32), 1, long_host),
        ("place-port-8443", 2, None, det("s4", 32), 8443, "home.example.net"),
    ]
    sheets = []
    for name, kind, r, s, port, host in cases:
        b = layout(kind, r, s, port, host.encode())
        compact = b32(b)
        sheets.append({"id": name, "kind": kind, "root": r.hex() if r else None, "scope": s.hex(), "port": port or 0,
                       "host": host, "binary_hex": b.hex(), "check_hex": b[-4:].hex(), "compact": compact,
                       "sheet_form": sheet_form(compact), "qr": qr(b), "lines": len(sheet_form(compact).split("\n"))})
        for form in (compact, sheet_form(compact), qr(b), compact.lower(), " ".join(compact[i:i + 3] for i in range(0, len(compact), 3))):
            k, p = parse_text(form)
            assert k == "sheet" and p["host"] == host
    out["sheets"] = sheets
    out["line_checks"] = [{"index": i, "payload": p, "check": line_check(i, p)} for i, p in
                          ((0, "ABCDEFGHIJKLMNOPQRSTUVWX"), (1, "ABCDEFGHIJKLMNOPQRSTUVWX"), (7, "AAAA"), (0, "A"), (30, "234567234567234567234567"))]

    # legacy key-only form
    lr = det("legacy-root", 32)
    legacy = b32(lr + hashlib.sha256(b"OM-RECOVERY-KEY-DISPLAY-CHECK" + lr).digest()[:3])
    out["legacy"] = {"root": lr.hex(), "text": legacy, "grouped": " ".join(legacy[i:i + 4] for i in range(0, 56, 4)),
                     "bad_check": b32(lr + b"\0\0\0")}

    # negatives: text, expected error ("line_check:N" names the 1-based line)
    good = layout(1, root, scope, None, b"home.example.net")
    neg = []

    def add(id_, text, error):
        try:
            parse_text(text)
        except Bad as e:
            assert str(e) == error, (id_, str(e), error)
        else:
            raise AssertionError(f"{id_} parsed")
        neg.append({"id": id_, "text": text, "error": error})

    add("version-2", b32(layout(1, root, scope, None, b"home.example.net", version=2)), "version")
    add("version-0", b32(layout(1, root, scope, None, b"home.example.net", version=0)), "version")
    add("kind-0", b32(layout(0, root, scope, None, b"home.example.net")), "kind")
    add("kind-3", b32(layout(3, root, scope, None, b"home.example.net")), "kind")
    add("truncated-by-one-byte", b32(good[:-1]), "length")
    add("truncated-to-two-bytes", b32(good[:2]), "length")
    add("trailing-byte", b32(good + b"\0"), "trailing")
    add("host-len-longer-than-host", b32(layout(1, root, scope, None, b"home.example.net", host_len=20)), "length")
    add("host-len-shorter-than-host", b32(layout(1, root, scope, None, b"home.example.net", host_len=10)), "trailing")
    add("host-len-zero", b32(layout(1, root, scope, None, b"", host_len=0)), "host")
    for name, host in (("host-uppercase", b"Home.example.net"), ("host-underscore", b"ho_me.example.net"), ("host-ip-literal", b"192.168.0.1"),
                       ("host-localhost", b"localhost"), ("host-trailing-dot", b"home.example.net."), ("host-empty-label", b"home..example.net"),
                       ("host-leading-hyphen-label", b"-home.example.net"), ("host-trailing-hyphen-label", b"home-.example.net"),
                       ("host-label-64-chars", (b"a" * 64) + b".example.net"), ("host-last-label-digits", b"home.example.123"),
                       ("host-with-port-text", b"home.example.net:8443"), ("host-with-scheme", b"https://home.example.net"),
                       ("host-non-ascii", "höme.example.net".encode()), ("host-with-space", b"ho me.example.net")):
        add(name, b32(layout(1, root, scope, None, host)), "host")
    add("check-all-zero", b32(layout(1, root, scope, None, b"home.example.net", check=False)), "check")
    for i in (0, 1, 5, 33, 40, 70, 80, len(good) - 1):
        bits = bytearray(good)
        bits[i] ^= 0x01
        e = "check"
        if i == 0:
            e = "version"
        elif i == 1:
            e = "kind"
        add(f"flip-bit-at-byte-{i}", b32(bytes(bits)), e)
    add("alphabet-digit-one", b32(good)[:10] + "1" + b32(good)[11:], "alphabet")
    add("alphabet-lowercase-ok-but-symbol", b32(good)[:10] + "$" + b32(good)[11:], "alphabet")
    add("empty", "", "length")
    add("only-separators", " - \n ", "length")
    add("too-long-input", "A" * 2000, "length")
    nonzero_tail = b32(good)[:-1] + "B" if b32(good)[-1] != "B" else b32(good)[:-1] + "C"
    add("non-zero-trailing-bits", nonzero_tail, "length")
    # sheet form: wrong line check, swapped lines
    sf = sheet_form(b32(good)).split("\n")
    assert len(sf) >= 4
    wrong = list(sf)
    lc = wrong[1][-2:]
    wrong[1] = wrong[1][:-2] + ("AA" if lc != "AA" else "BB")
    add("line-2-check-wrong", "\n".join(wrong), "line_check:2")
    swapped = list(sf)
    swapped[0], swapped[1] = swapped[1], swapped[0]
    add("lines-1-and-2-swapped", "\n".join(swapped), "line_check:1")
    typo = list(sf)
    c = typo[2][0]
    typo[2] = ("A" if c != "A" else "B") + typo[2][1:]
    add("line-3-one-character-changed", "\n".join(typo), "line_check:3")
    # QR
    add("qr-without-prefix", qr(good)[len("om:recover/"):], "alphabet")
    add("qr-wrong-prefix", "om:other/" + qr(good)[len("om:recover/"):], "line_check:1")
    add("qr-bad-base64", "om:recover/!!!!", "qr")
    add("qr-flipped-char", qr(good)[:20] + ("B" if qr(good)[20] != "B" else "C") + qr(good)[21:], "check")
    add("legacy-bad-check", out["legacy"]["bad_check"], "check")
    out["negatives"] = neg

    # corrections
    typed = b32(good)
    pos_corr = []
    for ch, rep in (("O", "0"), ("B", "8"), ("I", "1"), ("L", "1")):
        i = typed.find(ch)
        if i >= 0:
            t = typed[:i] + rep + typed[i + 1:]
            pos_corr.append({"id": f"correct-{ch}-typed-as-{rep}", "typed": t, "corrected": correction(t)})
    t0 = typed
    if "O" in typed and "B" in typed:
        t0 = typed.replace("O", "0", 1).replace("B", "8", 1)
    pos_corr.append({"id": "two-confusables-no-single-fix", "typed": t0, "corrected": correction(t0)})
    pos_corr.append({"id": "no-confusable", "typed": typed, "corrected": correction(typed)})
    pos_corr.append({"id": "plain-typo-not-corrected", "typed": typed[:5] + ("A" if typed[5] != "A" else "B") + typed[6:], "corrected": None})
    # a sheet-form correction (line checks pass only for the right letter)
    sf_text = sheet_form(typed)
    j = sf_text.find("O")
    if j >= 0:
        t = sf_text[:j] + "0" + sf_text[j + 1:]
        pos_corr.append({"id": "sheet-form-O-typed-as-0", "typed": t, "corrected": correction(t)})
    out["corrections"] = pos_corr

    # live feedback while typing
    out["first_bad_line"] = [
        {"text": sheet_form(typed), "line": None},
        {"text": "\n".join(typo), "line": 3},
        {"text": "\n".join(wrong), "line": 2},
        {"text": "\n".join(sf[:2]), "line": None},
        {"text": typed, "line": None},
    ]

    # redirect rules (no network part)
    S1, S2 = det("sc1", 32).hex(), det("sc2", 32).hex()
    red = []

    def rd(id_, host, port, scope, visited, hops, expect):
        red.append({"id": id_, "host": host, "port": port, "scope": scope, "visited": visited, "hops": hops, "expect": expect})

    rd("first-hop", "new.example.org", None, S1, [], 0, "ok")
    rd("third-hop", "new.example.org", 8443, S1, [["a.example.org", S2], ["b.example.org", S2]], 2, "ok")
    rd("fourth-hop", "new.example.org", None, S1, [["a.example.org", S2], ["b.example.org", S2], ["c.example.org", S2]], 3, "too_many_hops")
    rd("loop-same-host-and-scope", "new.example.org", None, S1, [["new.example.org", S1]], 1, "visited")
    rd("same-host-other-scope-is-new", "new.example.org", None, S1, [["new.example.org", S2]], 1, "ok")
    rd("scheme-in-host", "https://new.example.org", None, S1, [], 0, "host")
    rd("ip-literal", "10.0.0.1", None, S1, [], 0, "host")
    rd("localhost", "localhost", None, S1, [], 0, "host")
    rd("uppercase-host", "New.example.org", None, S1, [], 0, "host")
    rd("port-0", "new.example.org", 0, S1, [], 0, "host")
    rd("no-scope", "new.example.org", None, None, [], 0, "no_scope")
    out["redirect"] = red
    return out


def main(out):
    Path(out).write_text(json.dumps(build(), indent=1, sort_keys=True) + "\n")
    print(f"wrote {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
