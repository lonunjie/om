#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Budgeted, resumable fuzz runner for the expressive-media parsers (V-012, ADR-0251 ruling 1).

ADR-0251 asks for 24 CPU-hours per ADR target on a current commit, four targets, so 96 CPU-hours:

    (a) webp, facts            WebP RIFF walker with the profile and budget validator
    (b) asset_ref, event       ExpressiveAssetRef and the inline and reaction event decoders
    (c) release                ContentPack manifest and ContentPackRelease parser
    (d) control_state          ContentPackControlState parser and lineage verifier
    (+) pack_file              portable pack-file codec and predecessor chain (ADR-0254); a new target adds 24

`--budget-hours H` is the total for the four ADR targets (default 96): each gets H/4 split over its binaries, and the
pack-file target gets the same H/4 on top (`--no-pack-file` omits it). Hours are CPU-hours: wall-clock seconds times
`--jobs` (libFuzzer fork workers), which is an upper bound on the CPU actually used.

Engines. `libfuzzer` (default) runs the `cargo-fuzz` targets in this directory on a nightly toolchain with
AddressSanitizer; only its hours count toward the budget. `mutate` runs the in-tree deterministic mutation driver
(`crates/om-core/tests/expressive_fuzz.rs`, offline, stable toolchain); it is smoke evidence and its hours are recorded
but never counted. `afl` runs the independent second tool of ADR-0251, AFL++ (`cargo-afl`, driver crate in `afl/`,
README "Second fuzzer"), over the same entry points. It has its own budget (`--afl-budget-hours`, default 30, split
evenly over the five target groups), its own ledger (work/ledger-afl.json) and its own summary
(verification/results/expressive-fuzz-afl-latest.json); its hours are never added to the libFuzzer count.

Resumable. State lives in the ledger (default work/ledger.json, never committed). Every slice is written to it as
soon as it ends, so an interrupted run loses at most one slice and the next invocation continues from the recorded
hours. A finding (crash, timeout, out-of-memory, sanitizer report) stops that target, is copied under
work/artifacts/, and the runner exits 3 once the other targets are done or with --stop-on-finding at once.

    run_fuzz.py --status
    run_fuzz.py --dry-run
    run_fuzz.py --engine mutate --budget-hours 0.004 --slice-seconds 5      # offline smoke
    run_fuzz.py --jobs 8 --slice-minutes 20                                # the 96 CPU-hour run on the build host
"""
import argparse
import datetime
import json
import os
import platform
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
WORK = HERE / "work"
SEEDS = WORK / "seeds"
SUMMARY = ROOT / "verification/results/expressive-fuzz-latest.json"
SUMMARY_AFL = ROOT / "verification/results/expressive-fuzz-afl-latest.json"
AFL_DIR = HERE / "afl"

# ADR-0251 target -> libFuzzer binaries
GROUPS = {
    "a_webp_walker_and_budget": ["webp", "facts"],
    "b_asset_ref_and_event_decoders": ["asset_ref", "event"],
    "c_content_pack_release": ["release"],
    "d_content_pack_control_state_and_lineage": ["control_state"],
}
EXTRA = {"e_pack_file_codec": ["pack_file"]}
MAX_LEN = {"pack_file": 262144}
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
        return subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def load(path: Path) -> dict:
    if path.exists():
        return json.loads(path.read_text())
    return {"schema": "om-expressive-fuzz-ledger-v1", "targets": {}, "history": []}


def save(path: Path, ledger: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(ledger, indent=2, sort_keys=True) + "\n")
    os.replace(tmp, path)


def plan(args) -> list[tuple[str, str, float]]:
    """(group, binary, per-binary CPU-seconds budget)."""
    groups = dict(GROUPS)
    if not args.no_pack_file:
        groups.update(EXTRA)
    if args.engine == "afl":
        per_group = args.afl_budget_hours / len(groups) * 3600
    else:
        per_group = args.budget_hours / 4 * 3600
    rows = []
    for group, bins in groups.items():
        for name in bins:
            if args.target and name != args.target:
                continue
            rows.append((group, name, per_group / len(bins)))
    return rows


def counted_engine(args) -> str:
    return "afl" if args.engine == "afl" else "libfuzzer"


def spent(ledger: dict, name: str, engine: str) -> float:
    return ledger["targets"].get(name, {}).get(engine, {}).get("cpu_seconds", 0.0)


def finding_files(directory: Path) -> list[Path]:
    if not directory.exists():
        return []
    return sorted(p for p in directory.iterdir() if p.name.startswith(FINDING_PREFIXES))


def unpack_seeds() -> None:
    """libFuzzer and AFL++ read directories; the repository carries one archive (gen_seed_corpus.py)."""
    if not SEEDS.exists():
        subprocess.run([sys.executable, str(HERE / "gen_seed_corpus.py"), "--unpack", str(SEEDS)], check=True)


def run_libfuzzer(args, name: str, seconds: int) -> tuple[int, int | None]:
    unpack_seeds()
    corpus = WORK / "corpus" / name
    artifacts = WORK / "artifacts" / name
    corpus.mkdir(parents=True, exist_ok=True)
    artifacts.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cargo", "fuzz", "run", "--fuzz-dir", str(HERE), name,
        str(corpus), str(SEEDS / name), "--",
        f"-max_total_time={seconds}",
        "-timeout=1",
        "-rss_limit_mb=2048",
        f"-max_len={MAX_LEN.get(name, DEFAULT_MAX_LEN)}",
        f"-artifact_prefix={artifacts}/",
    ]
    if args.jobs > 1:
        cmd += [f"-fork={args.jobs}", "-ignore_crashes=0"]
    env = dict(os.environ, RUSTUP_TOOLCHAIN=args.toolchain)
    try:
        bindir = subprocess.run(
            ["rustup", "which", "--toolchain", args.toolchain, "cargo"], capture_output=True, text=True, check=True
        ).stdout.strip()
        env["PATH"] = str(Path(bindir).parent) + os.pathsep + env.get("PATH", "")
    except (OSError, subprocess.CalledProcessError):
        pass
    log = WORK / "logs" / f"{name}-{int(time.time())}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as out:
        code = subprocess.run(cmd, cwd=ROOT, env=env, stdout=out, stderr=subprocess.STDOUT).returncode
    executions = None
    for line in reversed(log.read_text(errors="replace").splitlines()):
        if "stat::number_of_executed_units:" in line:
            executions = int(line.rsplit(":", 1)[1])
            break
    return code, executions


def afl_env(args) -> dict:
    env = dict(os.environ, RUSTUP_TOOLCHAIN=args.toolchain)
    try:
        bindir = subprocess.run(
            ["rustup", "which", "--toolchain", args.toolchain, "cargo"], capture_output=True, text=True, check=True
        ).stdout.strip()
        env["PATH"] = str(Path(bindir).parent) + os.pathsep + env.get("PATH", "")
    except (OSError, subprocess.CalledProcessError):
        pass
    return env


def build_afl(args) -> Path:
    """Build the instrumented driver once per invocation (`cargo afl config --build` is done once per host)."""
    env = afl_env(args)
    config = subprocess.run(["cargo", "afl", "config", "--build"], cwd=AFL_DIR, env=env, capture_output=True, text=True)
    if config.returncode != 0 and "already built" not in (config.stdout + config.stderr):
        sys.exit(f"cargo afl config --build failed:\n{config.stdout}{config.stderr}")
    subprocess.run(["cargo", "afl", "build", "--release"], cwd=AFL_DIR, env=env, check=True)
    for tool in (["cargo", "afl", "--version"], ["cargo", "--version"], ["rustc", "--version"]):
        out = subprocess.run(tool, cwd=AFL_DIR, env=env, capture_output=True, text=True).stdout.strip()
        args.afl_versions[" ".join(tool[:2])] = out
    return AFL_DIR / "target" / "release" / "om-afl"


def afl_stats(directory: Path) -> dict[str, dict[str, str]]:
    stats = {}
    for f in sorted(directory.glob("*/fuzzer_stats")):
        stats[f.parent.name] = dict(
            (k.strip(), v.strip()) for k, _, v in (line.partition(":") for line in f.read_text().splitlines()) if k
        )
    return stats


def run_afl(args, name: str, seconds: int) -> tuple[int, int | None]:
    """One slice: `--jobs` AFL++ instances (one -M main, the rest -S) syncing through one output directory."""
    unpack_seeds()
    out = WORK / "afl" / name
    artifacts = WORK / "artifacts" / name
    artifacts.mkdir(parents=True, exist_ok=True)
    out.mkdir(parents=True, exist_ok=True)
    resume = (out / "main" / "queue").exists()
    before = sum(int(v.get("execs_done", 0)) for v in afl_stats(out).values())
    env = dict(
        afl_env(args),
        OM_AFL_TARGET=name,
        AFL_NO_UI="1",
        AFL_SKIP_CPUFREQ="1",
        AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES="1",
        AFL_TRY_AFFINITY="1",
        AFL_AUTORESUME="1",
        AFL_NO_CRASH_README="1",
    )
    log = WORK / "logs" / f"afl-{name}-{int(time.time())}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    procs = []
    with log.open("w") as logf:
        for i in range(args.jobs):
            role = ["-M", "main"] if i == 0 else ["-S", f"s{i}"]
            cmd = [
                "cargo", "afl", "fuzz", "-i", "-" if resume else str(SEEDS / name), "-o", str(out), *role,
                "-V", str(seconds), "-t", "1000", "-m", "2048", "--", str(args.afl_binary),
            ]
            procs.append(subprocess.Popen(cmd, cwd=AFL_DIR, env=env, stdout=logf, stderr=subprocess.STDOUT))
            time.sleep(0.5)
        codes = []
        for pr in procs:
            try:
                codes.append(pr.wait())
            except KeyboardInterrupt:
                pr.send_signal(signal.SIGTERM)
                codes.append(pr.wait())
    stats = afl_stats(out)
    if len(stats) < args.jobs:
        sys.exit(f"{name}: only {len(stats)} of {args.jobs} AFL++ instances wrote stats; see {log}")
    executions = sum(int(v.get("execs_done", 0)) for v in stats.values()) - before
    # Copy every crash and hang to the artifacts directory under the runner's finding prefixes.
    for inst in stats:
        for kind, prefix in (("crashes", "crash-"), ("hangs", "timeout-")):
            for f in sorted((out / inst / kind).glob("id:*")):
                dest = artifacts / f"{prefix}{inst}-{f.name.split(',')[0].replace(':', '')}"
                if not dest.exists():
                    shutil.copy2(f, dest)
    stats_main = stats.get("main", {})
    print(
        f"  afl {name}: {len(stats)} instances, paths {sum(int(v.get('corpus_count', 0)) for v in stats.values())}, "
        f"main edges {stats_main.get('edges_found', '?')}/{stats_main.get('total_edges', '?')}, "
        f"saved crashes {sum(int(v.get('saved_crashes', 0)) for v in stats.values())}, "
        f"saved hangs {sum(int(v.get('saved_hangs', 0)) for v in stats.values())}"
    )
    return (1 if any(c not in (0, None) for c in codes) and not any(artifacts.iterdir()) else 0), executions


def run_mutate(args, name: str, seconds: int) -> tuple[int, int | None]:
    artifacts = WORK / "artifacts" / name
    artifacts.mkdir(parents=True, exist_ok=True)
    env = dict(
        os.environ,
        OM_FUZZ_TARGET=name,
        OM_FUZZ_SECONDS=str(seconds),
        OM_FUZZ_SEED=str(int(time.time())),
        OM_FUZZ_ARTIFACTS=str(artifacts),
    )
    cmd = [
        "cargo", "test", "--offline", "-p", "om-core", "--test", "expressive_fuzz",
        "timed_mutation_run", "--", "--ignored", "--nocapture",
    ]
    proc = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
    executions = None
    for line in proc.stdout.splitlines():
        if line.startswith("OM_FUZZ_RESULT"):
            executions = int(dict(kv.split("=") for kv in line.split()[1:])["executed"])
    return proc.returncode, executions


def status(ledger: dict, rows, args) -> None:
    print(f"{'target':14} {'engine':9} {'spent h':>9} {'budget h':>9}  state")
    for group, name, budget in rows:
        for engine in ("libfuzzer", "afl", "mutate"):
            entry = ledger["targets"].get(name, {}).get(engine)
            if not entry and engine != counted_engine(args):
                continue
            s = (entry or {}).get("cpu_seconds", 0.0)
            state = (entry or {}).get("state", "pending")
            b = f"{budget / 3600:9.3f}" if engine == counted_engine(args) else "        -"
            print(f"{name:14} {engine:9} {s / 3600:9.3f} {b}  {state}")
    eng = counted_engine(args)
    counted = sum(spent(ledger, n, eng) for _, n, _ in rows) / 3600
    total = sum(b for _, _, b in rows) / 3600
    print(f"counted CPU-hours ({eng}): {counted:.3f} of {total:.3f}")


def write_summary(ledger: dict, rows, args) -> None:
    eng = counted_engine(args)
    afl = eng == "afl"
    summary = {
        "schema": "om-expressive-fuzz-afl-summary-v1" if afl else "om-expressive-fuzz-summary-v1",
        "generated": now(),
        "adr": "ADR-0251",
        "commit": git_head(),
        "budget_cpu_hours_total": args.afl_budget_hours if afl else args.budget_hours,
        "counted_engine": eng,
        "targets": {},
        "counted_cpu_hours": round(sum(spent(ledger, n, eng) for _, n, _ in rows) / 3600, 6),
        "budget_met": all(
            spent(ledger, n, eng) >= b - 1e-6 and ledger["targets"].get(n, {}).get(eng, {}).get("state") != "finding"
            for _, n, b in rows
        ),
        "note": (
            "Independent second fuzzer (AFL++ via cargo-afl). Its CPU-hours are a separate account from the libFuzzer budget "
            "and are not added to it."
            if afl
            else "Hours recorded by engines other than libfuzzer are smoke evidence and are not counted."
        ),
    }
    if not afl:
        summary["budget_cpu_hours_total_adr_targets"] = summary.pop("budget_cpu_hours_total")
    if afl:
        summary["aflplusplus"] = args.afl_versions
    for group, name, budget in rows:
        summary["targets"][name] = {
            "adr_target": group,
            "budget_cpu_hours": round(budget / 3600, 6),
            "engines": ledger["targets"].get(name, {}),
        }
    out = SUMMARY_AFL if afl else SUMMARY
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out.relative_to(ROOT)}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--budget-hours", type=float, default=96.0, help="total CPU-hours for the four ADR targets (default 96)")
    ap.add_argument("--engine", choices=("libfuzzer", "afl", "mutate"), default="libfuzzer")
    ap.add_argument("--afl-budget-hours", type=float, default=30.0, help="total CPU-hours for --engine afl, split evenly over the target groups (default 30)")
    ap.add_argument("--jobs", type=int, default=1, help="libFuzzer fork workers or AFL++ instances; CPU time is wall time x jobs")
    ap.add_argument("--slice-minutes", type=float, default=15.0)
    ap.add_argument("--slice-seconds", type=float, help="overrides --slice-minutes (smoke runs)")
    ap.add_argument("--target", help="run only this binary")
    ap.add_argument("--no-pack-file", action="store_true", help="omit the pack-file target (an extra 24 CPU-hours)")
    ap.add_argument("--toolchain", default="nightly", help="toolchain for cargo-fuzz (needs -Z sanitizer) and cargo-afl")
    ap.add_argument("--ledger", type=Path, help="default work/ledger.json, work/ledger-afl.json for --engine afl")
    ap.add_argument("--stop-on-finding", action="store_true")
    ap.add_argument("--status", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--write-summary", action="store_true", help=f"write {SUMMARY.relative_to(ROOT)} at the end")
    args = ap.parse_args()
    args.afl_versions = {}
    if args.ledger is None:
        args.ledger = WORK / ("ledger-afl.json" if args.engine == "afl" else "ledger.json")

    rows = plan(args)
    ledger = load(args.ledger)
    if args.status:
        status(ledger, rows, args)
        return 0
    slice_s = max(1, int(args.slice_seconds if args.slice_seconds else args.slice_minutes * 60))
    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)
    runner = {"libfuzzer": run_libfuzzer, "afl": run_afl, "mutate": run_mutate}[args.engine]
    workers = args.jobs if args.engine in ("libfuzzer", "afl") else 1
    if args.engine == "afl" and not args.dry_run:
        args.afl_binary = build_afl(args)
    findings = 0

    for group, name, budget in rows:
        entry = ledger["targets"].setdefault(name, {}).setdefault(
            args.engine, {"cpu_seconds": 0.0, "slices": 0, "executions": 0, "state": "pending"}
        )
        if entry["state"] == "finding":
            print(f"{name}: open finding recorded, skipping (fix it, move its artifact to a regression seed, clear the state)")
            findings += 1
            continue
        while not stop:
            remaining = budget - entry["cpu_seconds"]
            if remaining <= 1e-6:
                entry["state"] = "complete"
                break
            wall = min(slice_s, max(1, int(remaining / workers + 0.999)))
            if args.dry_run:
                print(f"[dry-run] {args.engine} {name}: {wall}s wall x {workers} jobs, {remaining / 3600:.3f} CPU-hours remaining")
                break
            started = time.time()
            code, executions = runner(args, name, wall)
            elapsed = time.time() - started
            entry["cpu_seconds"] += elapsed * workers
            entry["slices"] += 1
            entry["executions"] += executions or 0
            entry["state"] = "running"
            entry["last_run"] = now()
            ledger["history"].append(
                {"time": now(), "engine": args.engine, "target": name, "wall_seconds": round(elapsed, 1),
                 "jobs": workers, "executions": executions, "exit": code, "commit": git_head(),
                 "host": platform.node()}
            )
            found = finding_files(WORK / "artifacts" / name)
            if code != 0 or found:
                entry["state"] = "finding"
                entry["artifacts"] = [p.name for p in found]
                findings += 1
                save(args.ledger, ledger)
                print(f"{name}: FINDING (exit {code}) {[p.name for p in found]}", file=sys.stderr)
                if args.stop_on_finding:
                    write_summary(ledger, rows, args) if args.write_summary else None
                    return 3
                break
            save(args.ledger, ledger)
            print(f"{name}: {entry['cpu_seconds'] / 3600:.4f} of {budget / 3600:.4f} CPU-hours ({executions} executions this slice)")
        if stop:
            break
        if not args.dry_run:
            save(args.ledger, ledger)

    if not args.dry_run:
        save(args.ledger, ledger)
        if args.write_summary:
            write_summary(ledger, rows, args)
    status(ledger, rows, args)
    if stop:
        print("interrupted; rerun to resume")
        return 130
    return 3 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
