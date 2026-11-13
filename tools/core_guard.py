#!/usr/bin/env python3
"""Fast guard for the Rust Core workspace (runs in `make check`, no Rust toolchain needed).

* No production-reachable dependency edge enables a test-only feature of om-core (`insecure-test-rng`,
  `test-loopback`): only [dev-dependencies] may (R-072: a production build provably excludes dev/test providers).
* The `production` feature stays declared and the compile_error! guard that forbids combining it with a test-only
  feature stays in the source.
* The built wasm artifact, if present, carries none of the test-only markers. `--wasm FILE...` scans exactly those
  files instead (the release script passes the wasm asset inside the site folder it is about to sign).
* No `unsafe` outside the single audited ABI module of om-wasm.
* Every `.wasm` scanned imports only `om_host.random_fill` and `om_host.now_ms` (ADR-0187, proposed: the narrow ABI as a
  mechanical check; any `__wbindgen_*` import fails). The OpenMLS plumbing shims under `third_party/openmls-wasm-shims`
  may contain `unsafe` only in the one `om_host.now_ms` import declaration of `web-time/src/lib.rs`.
"""
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEST_FEATURES = {"insecure-test-rng", "test-loopback"}
TEST_MARKERS = [
    b"OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION",
    b"OM-V001-INSECURE-TEST-RNG",
    b"InsecureTestRng",
    b"insecure_test_rng",
    # Items compiled only under `test-loopback` (audit 2026-10-28, H1): their names make a leaked test build auditable.
    b"debug_inject",
    b"debug_app_state",
    b"debug_device_digest",
    b"debug_link_messaging_device",
    b"LoopbackNetwork",
]
UNSAFE = re.compile(r"\bunsafe\b")
ALLOWED_IMPORTS = {("om_host", "random_fill"), ("om_host", "now_ms")}
SHIMS = "third_party/openmls-wasm-shims"
# the only `unsafe` in the shims we wrote: the host-import declaration (the vendored getrandom copy is upstream code,
# compared with upstream by the shim README, and is not scanned line by line)
SHIM_UNSAFE_OK = {f"{SHIMS}/web-time/src/lib.rs": 1}


def wasm_imports(blob: bytes) -> list[tuple[str, str]]:
    """The (module, name) pairs of a wasm binary's import section (LEB128 parse, no tooling needed)."""
    def leb(i: int) -> tuple[int, int]:
        n = shift = 0
        while True:
            b = blob[i]
            i += 1
            n |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                return n, i

    def name(i: int) -> tuple[str, int]:
        n, i = leb(i)
        return blob[i : i + n].decode("utf-8", "replace"), i + n

    out: list[tuple[str, str]] = []
    i = 8
    while i < len(blob):
        sec, i = blob[i], i + 1
        size, i = leb(i)
        if sec == 2:
            j = i
            count, j = leb(j)
            for _ in range(count):
                mod, j = name(j)
                nm, j = name(j)
                kind, j = blob[j], j + 1
                if kind == 0:
                    _, j = leb(j)
                elif kind == 1:
                    j += 1
                    flag, j = leb(j)
                    _, j = leb(j)
                    if flag & 1:
                        _, j = leb(j)
                elif kind == 2:
                    flag, j = leb(j)
                    _, j = leb(j)
                    if flag & 1:
                        _, j = leb(j)
                elif kind == 3:
                    j += 2
                else:
                    _, j = leb(j)
                out.append((mod, nm))
            return out
        i += size
    return out


def main(argv: list[str]) -> int:
    errs: list[str] = []
    crates = ROOT / "crates"
    for manifest in sorted(crates.glob("*/Cargo.toml")):
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        rel = manifest.relative_to(ROOT).as_posix()
        for section in ("dependencies", "build-dependencies"):
            for name, spec in data.get(section, {}).items():
                feats = set(spec.get("features", [])) if isinstance(spec, dict) else set()
                if feats & TEST_FEATURES:
                    errs.append(f"{rel}: [{section}] {name} enables test-only feature(s) {sorted(feats & TEST_FEATURES)}")
    core = crates / "om-core"
    feats = tomllib.loads((core / "Cargo.toml").read_text(encoding="utf-8")).get("features", {})
    if "production" not in feats:
        errs.append("crates/om-core/Cargo.toml: the `production` marker feature is missing")
    if "default" in feats and set(feats["default"]) & (TEST_FEATURES | {"production"}):
        errs.append("crates/om-core/Cargo.toml: default features must not include test-only features")
    lib = (core / "src/lib.rs").read_text(encoding="utf-8")
    if "compile_error!" not in lib or "test-loopback" not in lib or "insecure-test-rng" not in lib:
        errs.append("crates/om-core/src/lib.rs: the production-vs-test-provider compile_error! guard is missing")
    for rs in sorted(crates.rglob("*.rs")):
        rel = rs.relative_to(ROOT).as_posix()
        if rel == "crates/om-wasm/src/abi.rs" or "/tests/" in rel:
            continue
        for n, line in enumerate(rs.read_text(encoding="utf-8").splitlines(), 1):
            code = line.split("//", 1)[0]
            if UNSAFE.search(code) and "forbid(unsafe_code)" not in code and '"unsafe_code"' not in code and "deny(unsafe_code" not in code:
                errs.append(f"{rel}:{n}: unsafe outside the audited ABI module")
    shims = ROOT / SHIMS
    for rs in sorted(shims.rglob("*.rs")) if shims.exists() else []:
        rel = rs.relative_to(ROOT).as_posix()
        if "/getrandom-0.4.3/" in rel:
            continue
        hits = sum(1 for line in rs.read_text(encoding="utf-8").splitlines() if UNSAFE.search(line.split("//", 1)[0]))
        if hits > SHIM_UNSAFE_OK.get(rel, 0):
            errs.append(f"{rel}: {hits} `unsafe` use(s), allowed {SHIM_UNSAFE_OK.get(rel, 0)} (the om_host.now_ms declaration)")
    explicit = argv[argv.index("--wasm") + 1 :] if "--wasm" in argv else None
    if explicit is not None and not explicit:
        errs.append("--wasm needs at least one file")
    wasms = [Path(a) for a in explicit] if explicit else [ROOT / "packages/om-core-wasm/dist/om_wasm.wasm"]
    scanned = 0
    for wasm in wasms:
        if not wasm.exists():
            if explicit:
                errs.append(f"{wasm}: wasm artifact to scan does not exist")
            continue
        scanned += 1
        blob = wasm.read_bytes()
        for m in TEST_MARKERS:
            if m in blob:
                errs.append(f"{wasm}: contains test-only marker {m.decode()}")
        try:
            extra = sorted(set(wasm_imports(blob)) - ALLOWED_IMPORTS)
        except (IndexError, ValueError):
            extra = [("?", "unparseable import section")]
        for mod, nm in extra:
            errs.append(f"{wasm}: import {mod}.{nm} is not on the allow-list (only om_host.random_fill and om_host.now_ms)")
    for e in errs:
        print("ERROR: core guard:", e, file=sys.stderr)
    print(f"core guard: {'FAILED' if errs else 'OK'} ({len(errs)} error(s)){'' if scanned else ' [wasm artifact not built; marker scan skipped]'}")
    return 1 if errs else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
