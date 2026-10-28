#!/usr/bin/env python3
"""Fast guard for the Rust Core workspace (runs in `make check`, no Rust toolchain needed).

* No production-reachable dependency edge enables a test-only feature of om-core (`insecure-test-rng`,
  `test-loopback`): only [dev-dependencies] may (R-072: a production build provably excludes dev/test providers).
* The `production` feature stays declared and the compile_error! guard that forbids combining it with a test-only
  feature stays in the source.
* The built wasm artifact, if present, carries none of the test-only markers.
* No `unsafe` outside the single audited ABI module of om-wasm.
"""
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEST_FEATURES = {"insecure-test-rng", "test-loopback"}
TEST_MARKERS = [b"OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION", b"OM-V001-INSECURE-TEST-RNG", b"InsecureTestRng", b"insecure_test_rng"]
UNSAFE = re.compile(r"\bunsafe\b")


def main() -> int:
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
    wasm = ROOT / "packages/om-core-wasm/dist/om_wasm.wasm"
    if wasm.exists():
        blob = wasm.read_bytes()
        for m in TEST_MARKERS:
            if m in blob:
                errs.append(f"{wasm.relative_to(ROOT).as_posix()}: contains test-only marker {m.decode()}")
    for e in errs:
        print("ERROR: core guard:", e, file=sys.stderr)
    print(f"core guard: {'FAILED' if errs else 'OK'} ({len(errs)} error(s)){'' if wasm.exists() else ' [wasm artifact not built; marker scan skipped]'}")
    return 1 if errs else 0


if __name__ == "__main__":
    raise SystemExit(main())
