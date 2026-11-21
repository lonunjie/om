#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Run the browser-path fuzz (V-012) at a chosen size and record the tallies in verification/results/expressive-fuzz-browser-latest.json.

Runs packages/om-web-platform/src/expressive/fuzz.test.ts and packages/om-core-wasm/src/expressive-fuzz.test.ts under vitest with
OM_BROWSER_FUZZ_ITERATIONS and OM_BROWSER_FUZZ_SEED, parses their one-line tallies and fails on any test failure. The wasm test needs
packages/om-core-wasm/dist/om_wasm_expressive.wasm (pnpm --filter @om/core-wasm build:wasm:expressive).

    python3 run_browser_fuzz.py --iterations 20000 --seeds 2830 2831 2832 --write-summary
"""
import argparse
import datetime
import json
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SUMMARY = ROOT / "verification/results/expressive-fuzz-browser-latest.json"
TESTS = ["packages/om-web-platform/src/expressive/fuzz.test.ts", "packages/om-core-wasm/src/expressive-fuzz.test.ts"]
TAG = re.compile(r"^(OM_BROWSER_FUZZ_(?:WEB|PACK|COMMANDS)) (\{.*\})$", re.M)

ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
ap.add_argument("--iterations", type=int, default=20000)
ap.add_argument("--seeds", type=int, nargs="+", default=[2830])
ap.add_argument("--write-summary", action="store_true")
args = ap.parse_args()

wasm = ROOT / "packages/om-core-wasm/dist/om_wasm_expressive.wasm"
runs, failed = [], False
for seed in args.seeds:
    env = dict(os.environ, OM_BROWSER_FUZZ_ITERATIONS=str(args.iterations), OM_BROWSER_FUZZ_SEED=str(seed), OM_BROWSER_FUZZ_REPORT="1")
    proc = subprocess.run(["pnpm", "exec", "vitest", "run", "--silent=false", *TESTS], cwd=ROOT, env=env, capture_output=True, text=True)
    sys.stdout.write(proc.stdout[-1500:] if proc.returncode else "")
    failed |= proc.returncode != 0
    for tag, body in TAG.findall(proc.stdout):
        runs.append({"seed": seed, "probe": tag.removeprefix("OM_BROWSER_FUZZ_").lower(), **json.loads(body)})
    print(f"seed {seed}: exit {proc.returncode}")
summary = {
    "schema": "om-expressive-fuzz-browser-summary-v1",
    "generated": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "adr": "ADR-0251",
    "iterations_per_probe": args.iterations,
    "seeds": args.seeds,
    "real_wasm_artifact_sha256": (wasm.with_name("om_wasm_expressive.wasm.sha256").read_text().split()[0] if wasm.exists() else None),
    "all_tests_passed": not failed,
    "runs": runs,
    "note": "Deterministic per (seed, iteration index). Node has no WebP codec: pixels are not decoded; budgets, refusals, timeouts and Core traps are.",
}
if args.write_summary:
    SUMMARY.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(f"wrote {SUMMARY.relative_to(ROOT)}")
sys.exit(1 if failed else 0)
