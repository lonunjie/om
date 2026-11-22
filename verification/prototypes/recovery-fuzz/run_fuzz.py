#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Budgeted, resumable libFuzzer runner for two suites (V-012).

    recovery    the owner-recovery codecs and the link-ceremony frames of this directory (8 targets). Default budget 24 CPU-hours.
    expressive  the existing expressive-media targets of ../expressive-fuzz, built sanitized (AddressSanitizer plus debug
                assertions and overflow checks, the Rust counterpart of UBSan, which rustc does not offer). Default budget 12 CPU-hours.

Hours are CPU-hours: wall-clock seconds times `--jobs` (libFuzzer fork workers), an upper bound on the CPU used. The budget
is split evenly over a suite's targets. State lives in a ledger (work/ledger-<suite>.json, never committed) written after
every slice, so an interrupted run resumes where it stopped. A finding (crash, timeout, out-of-memory, sanitizer report)
stops that target, copies the input under work/artifacts/<suite>/<target>/, and makes the runner exit 3; inputs from a
finding are kept locally and are not committed.

    run_fuzz.py --suite recovery --status
    run_fuzz.py --suite recovery --dry-run
    run_fuzz.py --suite recovery --budget-hours 0.01 --slice-seconds 10 --jobs 2      # smoke
    run_fuzz.py --suite recovery --jobs 12 --slice-minutes 15 --write-summary        # the 24 CPU-hour run on the build host
"""
import argparse
import datetime
import json
import os
import platform
import re
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
WORK = HERE / "work"
EXPRESSIVE = HERE.parent / "expressive-fuzz"

SUITES = {
    "recovery": {
        "fuzz_dir": HERE,
        "targets": ["sheet", "record", "kit", "approval", "notice", "link", "frame", "continuation"],
        "budget": 24.0,
        "max_len": {"sheet": 1200, "record": 70000, "kit": 5000, "approval": 100000, "notice": 140000,
                    "link": 600, "frame": 16400, "continuation": 600},
        "seed_cmd": [sys.executable, str(HERE / "gen_seed_corpus.py"), "--unpack"],
        "summary": ROOT / "verification/results/recovery-fuzz-latest.json",
        "schema": "om-recovery-fuzz-summary-v1",
        "extra": [],
    },
    "expressive": {
        "fuzz_dir": EXPRESSIVE,
        "targets": ["webp", "facts", "asset_ref", "event", "release", "control_state", "pack_file"],
        "budget": 12.0,
        "max_len": {"pack_file": 262144},
        "seed_cmd": [sys.executable, str(EXPRESSIVE / "gen_seed_corpus.py"), "--unpack"],
        "summary": ROOT / "verification/results/expressive-fuzz-sanitized-latest.json",
        "schema": "om-expressive-fuzz-sanitized-summary-v1",
        "extra": [],
    },
}
DEFAULT_MAX_LEN = 65536
FINDING_PREFIXES = ("crash-", "timeout-", "oom-", "leak-", "slow-unit-")
stop = False


def on_signal(_sig, _frame):
    global stop
    stop = True


def now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def git_head() -> str:
    try:
        return subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def load(path: Path, suite: str) -> dict:
    if path.exists():
        return json.loads(path.read_text())
    return {"schema": "om-recovery-fuzz-ledger-v1", "suite": suite, "targets": {}, "history": []}


def save(path: Path, ledger: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(ledger, indent=2, sort_keys=True) + "\n")
    os.replace(tmp, path)


def finding_files(directory: Path) -> list[Path]:
    if not directory.exists():
        return []
    return sorted(p for p in directory.iterdir() if p.name.startswith(FINDING_PREFIXES))


def toolchain_env(args) -> dict:
    env = dict(os.environ, RUSTUP_TOOLCHAIN=args.toolchain)
    try:
        bindir = subprocess.run(["rustup", "which", "--toolchain", args.toolchain, "cargo"], capture_output=True, text=True, check=True).stdout.strip()
        env["PATH"] = str(Path(bindir).parent) + os.pathsep + env.get("PATH", "")
    except (OSError, subprocess.CalledProcessError):
        pass
    return env


def unpack_seeds(suite: str) -> Path:
    seeds = WORK / suite / "seeds"
    if not seeds.exists():
        subprocess.run([*SUITES[suite]["seed_cmd"], str(seeds)], check=True)
    return seeds


def run_slice(args, suite: str, name: str, seconds: int) -> tuple[int, int | None, float]:
    """One slice. The returned seconds are the fuzzing wall time only: a build is done first and never counted."""
    cfg = SUITES[suite]
    seeds = unpack_seeds(suite)
    corpus = WORK / suite / "corpus" / name
    artifacts = WORK / suite / "artifacts" / name
    corpus.mkdir(parents=True, exist_ok=True)
    artifacts.mkdir(parents=True, exist_ok=True)
    env = toolchain_env(args)
    build = subprocess.run(["cargo", "fuzz", "build", "--fuzz-dir", str(cfg["fuzz_dir"]), "-s", "address", "--debug-assertions", name],
                           cwd=ROOT, env=env, capture_output=True, text=True)
    if build.returncode != 0:
        sys.exit(f"build of {name} failed:\n{build.stdout}{build.stderr}")
    cmd = [
        "cargo", "fuzz", "run", "--fuzz-dir", str(cfg["fuzz_dir"]), "-s", "address", "--debug-assertions", name,
        str(corpus), str(seeds / name), "--",
        f"-max_total_time={seconds}", "-timeout=1", "-print_final_stats=1", f"-rss_limit_mb={args.rss_mb}",
        f"-max_len={cfg['max_len'].get(name, DEFAULT_MAX_LEN)}", f"-artifact_prefix={artifacts}/",
    ]
    if args.jobs > 1:
        cmd += [f"-fork={args.jobs}", "-ignore_crashes=0"]
    log = WORK / suite / "logs" / f"{name}-{int(time.time())}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    started = time.time()
    with log.open("w") as out:
        code = subprocess.run(cmd, cwd=ROOT, env=env, stdout=out, stderr=subprocess.STDOUT).returncode
    elapsed = time.time() - started
    # In fork mode libFuzzer reports per finished job: the sum of each job's reported count is the execution total.
    per_job: dict[str, int] = {}
    for line in log.read_text(errors="replace").splitlines():
        m = re.match(r"#(\d+):.* job: (\d+)", line)
        if m:
            per_job[m.group(2)] = int(m.group(1))
    executions = sum(per_job.values()) if per_job else None
    return code, executions, elapsed


def plan(args) -> list[tuple[str, float]]:
    cfg = SUITES[args.suite]
    targets = [t for t in cfg["targets"] if not args.target or t == args.target]
    per = args.budget_hours / len(cfg["targets"]) * 3600
    return [(t, per) for t in targets]


def spent(ledger: dict, name: str) -> float:
    return ledger["targets"].get(name, {}).get("cpu_seconds", 0.0)


def status(ledger: dict, rows) -> None:
    print(f"{'target':14} {'spent h':>9} {'budget h':>9}  state")
    for name, budget in rows:
        e = ledger["targets"].get(name, {})
        print(f"{name:14} {spent(ledger, name) / 3600:9.3f} {budget / 3600:9.3f}  {e.get('state', 'pending')}")
    print(f"counted CPU-hours: {sum(spent(ledger, n) for n, _ in rows) / 3600:.3f} of {sum(b for _, b in rows) / 3600:.3f}")


def write_summary(ledger: dict, rows, args) -> None:
    cfg = SUITES[args.suite]
    summary = {
        "schema": cfg["schema"],
        "generated": now(),
        "commit": git_head(),
        "suite": args.suite,
        "engine": "libFuzzer (cargo-fuzz) with AddressSanitizer, debug assertions and overflow checks",
        "toolchain": args.versions,
        "jobs": args.jobs,
        "budget_cpu_hours_total": args.budget_hours,
        "counted_cpu_hours": round(sum(spent(ledger, n) for n, _ in rows) / 3600, 6),
        "budget_met": all(spent(ledger, n) >= b - 1e-6 and ledger["targets"].get(n, {}).get("state") != "finding" for n, b in rows),
        "findings_open": sorted(n for n, _ in rows if ledger["targets"].get(n, {}).get("state") == "finding"),
        "note": "CPU-hours are wall seconds times libFuzzer fork workers, an upper bound on CPU used. Inputs of any finding are kept locally and are not part of this file.",
        "targets": {n: {"budget_cpu_hours": round(b / 3600, 6), "run": ledger["targets"].get(n, {})} for n, b in rows},
    }
    out = cfg["summary"]
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out.relative_to(ROOT)}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--suite", choices=sorted(SUITES), default="recovery")
    ap.add_argument("--budget-hours", type=float, help="total CPU-hours for the suite (default 24 recovery, 12 expressive)")
    ap.add_argument("--jobs", type=int, default=1, help="libFuzzer fork workers; CPU time is wall time x jobs")
    ap.add_argument("--slice-minutes", type=float, default=15.0)
    ap.add_argument("--slice-seconds", type=float, help="overrides --slice-minutes (smoke runs)")
    ap.add_argument("--rss-mb", type=int, default=768, help="libFuzzer -rss_limit_mb per worker")
    ap.add_argument("--target", help="run only this target")
    ap.add_argument("--toolchain", default="nightly")
    ap.add_argument("--stop-on-finding", action="store_true")
    ap.add_argument("--status", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--write-summary", action="store_true")
    ap.add_argument("--summary-only", action="store_true", help="write the summary from the ledger of every target and run nothing (ignores --target)")
    args = ap.parse_args()
    if args.budget_hours is None:
        args.budget_hours = SUITES[args.suite]["budget"]
    args.versions = {}
    ledger_path = WORK / f"ledger-{args.suite}.json"
    ledger = load(ledger_path, args.suite)
    if args.summary_only:
        args.target = None
    rows = plan(args)
    if args.summary_only:
        for tool in (["cargo", "fuzz", "--version"], ["rustc", "--version"]):
            args.versions[" ".join(tool[:2])] = subprocess.run(tool, env=toolchain_env(args), capture_output=True, text=True).stdout.strip()
        write_summary(ledger, rows, args)
        return 0
    if args.status:
        status(ledger, rows)
        return 0
    slice_s = max(1, int(args.slice_seconds if args.slice_seconds else args.slice_minutes * 60))
    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)
    if not args.dry_run:
        for tool in (["cargo", "fuzz", "--version"], ["rustc", "--version"]):
            args.versions[" ".join(tool[:2])] = subprocess.run(tool, env=toolchain_env(args), capture_output=True, text=True).stdout.strip()
    findings = 0
    for name, budget in rows:
        entry = ledger["targets"].setdefault(name, {"cpu_seconds": 0.0, "slices": 0, "executions": 0, "state": "pending"})
        if entry["state"] == "finding":
            print(f"{name}: open finding recorded, skipping (resolve it, clear the state in the ledger, rerun)")
            findings += 1
            continue
        while not stop:
            remaining = budget - entry["cpu_seconds"]
            if remaining <= 1e-6:
                entry["state"] = "complete"
                break
            wall = min(slice_s, max(1, int(remaining / args.jobs + 0.999)))
            if args.dry_run:
                print(f"[dry-run] {args.suite}/{name}: {wall}s wall x {args.jobs} jobs, {remaining / 3600:.3f} CPU-hours remaining")
                break
            code, executions, elapsed = run_slice(args, args.suite, name, wall)
            entry["cpu_seconds"] += elapsed * args.jobs
            entry["slices"] += 1
            entry["executions"] += executions or 0
            entry["state"] = "running"
            entry["last_run"] = now()
            ledger["history"].append({"time": now(), "target": name, "wall_seconds": round(elapsed, 1), "jobs": args.jobs,
                                      "executions": executions, "exit": code, "commit": git_head(), "host": platform.node()})
            found = finding_files(WORK / args.suite / "artifacts" / name)
            if code != 0 or found:
                entry["state"] = "finding"
                entry["artifacts"] = [p.name for p in found]
                findings += 1
                save(ledger_path, ledger)
                print(f"{name}: FINDING (exit {code}) {[p.name for p in found]}", file=sys.stderr)
                if args.stop_on_finding:
                    if args.write_summary:
                        write_summary(ledger, rows, args)
                    return 3
                break
            save(ledger_path, ledger)
            print(f"{name}: {entry['cpu_seconds'] / 3600:.4f} of {budget / 3600:.4f} CPU-hours ({executions} executions this slice)", flush=True)
        if stop:
            break
        if not args.dry_run:
            save(ledger_path, ledger)
    if not args.dry_run:
        save(ledger_path, ledger)
        if args.write_summary:
            write_summary(ledger, rows, args)
    status(ledger, rows)
    if stop:
        print("interrupted; rerun to resume")
        return 130
    return 3 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
