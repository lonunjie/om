#!/usr/bin/env python3
"""Report or enforce verification-environment prerequisites.

Profiles deliberately separate checkpoint-portable verification from checks that need
Git history or heavyweight external toolchains. This script does not download tools.
TLC's pinned JAR is bootstrapped once by tools/tlc.sh when `make tlc` actually runs.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PIN_VERSION = "1.7.4"
PIN_SHA1 = "bee4a54f3ee3d4afc347c3240ec2d9e93b075104"
CACHE = Path(os.environ.get("OM_TOOL_CACHE", str(Path.home() / ".cache/om")))
TLC_JAR = CACHE / f"tla2tools-{PIN_VERSION}.jar"


def tool(name: str) -> str | None:
    return shutil.which(name)


def git_worktree() -> bool:
    if not tool("git"):
        return False
    cp = subprocess.run(["git", "rev-parse", "--is-inside-work-tree"], cwd=ROOT,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return cp.returncode == 0


def uv_hypothesis_status() -> str:
    if not tool("uv"):
        return "uv missing"
    # Offline/frozen on purpose: preflight must not hide a network dependency.
    cp = subprocess.run(
        ["uv", "run", "--offline", "--frozen", "--quiet", "python", "-c",
         "import hypothesis; print(hypothesis.__version__)"],
        cwd=ROOT / "verification/prototypes/v003-sessions", text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if cp.returncode == 0:
        return f"cached ({cp.stdout.strip()})"
    return "not cached; V-003 full fuzz run needs uv package availability/network"


def status() -> dict:
    tools = {name: tool(name) for name in
             ("python3", "node", "git", "cargo", "rustc", "rustup", "java", "curl", "shasum", "uv")}
    return {
        "tools": tools,
        "git_worktree": git_worktree(),
        "tlc": {
            "jar": str(TLC_JAR),
            "cached": TLC_JAR.is_file(),
            "if_missing": "tools/tlc.sh will attempt exactly one pinned download; network is then required",
            "pin": {"version": PIN_VERSION, "sha1": PIN_SHA1},
        },
        "v003_hypothesis": uv_hypothesis_status(),
        "v005_note": "full phase-1 cross-wave evidence requires cargo/rustc; the complete V-005 prototype additionally requires rustup, wasm32-wasip1, wasmtime, Ruby cddl and Rust cddl as documented in verification/prototypes/v005-encoding/README.md",
    }


def require(profile: str, s: dict) -> list[str]:
    missing: list[str] = []
    t = s["tools"]
    if profile in ("drop", "full"):
        for name in ("python3", "node"):
            if not t[name]:
                missing.append(f"missing required portable tool: {name}")
    if profile == "full":
        if not s["git_worktree"]:
            missing.append("full verification requires a Git worktree/history; use `make verify-drop` in an exported ZIP")
        for name in ("cargo", "rustc", "java", "shasum"):
            if not t[name]:
                missing.append(f"full verification requires {name}")
        if not s["tlc"]["cached"] and not t["curl"]:
            missing.append("pinned TLC JAR is not cached and curl is unavailable for the one-time bootstrap")
    return missing


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", choices=("report", "drop", "full", "v003", "v005"), default="report")
    a = ap.parse_args()
    s = status()
    problems: list[str] = []
    if a.profile in ("drop", "full"):
        problems = require(a.profile, s)
    elif a.profile == "v003":
        if not s["tools"]["uv"]:
            problems.append("V-003 fuzzing requires uv")
        if not s["v003_hypothesis"].startswith("cached"):
            problems.append(s["v003_hypothesis"])
    elif a.profile == "v005":
        for name in ("python3", "node", "cargo", "rustc", "rustup"):
            if not s["tools"][name]:
                problems.append(f"V-005 prototype requires {name}")

    print(json.dumps({"profile": a.profile, **s, "problems": problems}, indent=2))
    if problems:
        for p in problems:
            print(f"PRECHECK: {p}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
