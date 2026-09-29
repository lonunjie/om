#!/usr/bin/env python3
"""OM Omnibar benchmark harness.

Purpose:
- stress a local FTS5 search plane with synthetic message histories;
- measure candidate-generation latency/storage, not UI paint;
- compare a transparent recency/frequency ranker with a Bayesian-smoothed
  contextual ranker on deterministic synthetic preference traces.

This is design evidence, not production implementation and not human usability
validation. Synthetic outcomes must not be treated as proof of user benefit.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import random
import sqlite3
import statistics
import tempfile
import time
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Sequence, Tuple

VOCAB = [
    "hotel", "address", "meeting", "tomorrow", "family", "photo", "document",
    "airport", "ticket", "coffee", "project", "design", "message", "schedule",
    "invoice", "school", "class", "office", "travel", "lunch", "call", "video",
    "security", "device", "invite", "group", "birthday", "location", "file",
    "weather", "report", "notes", "shopping", "weekend", "morning", "evening",
]
SPECIAL_TERMS = ["hotel address", "airport ticket", "project design", "birthday photo", "security device"]


def percentile(xs: Sequence[float], p: float) -> float:
    if not xs:
        return float("nan")
    ys = sorted(xs)
    idx = min(len(ys) - 1, max(0, math.ceil(p * len(ys)) - 1))
    return ys[idx]


def latency_stats(samples_ms: Sequence[float]) -> Dict[str, float]:
    return {
        "n": len(samples_ms),
        "p50_ms": round(percentile(samples_ms, 0.50), 3),
        "p95_ms": round(percentile(samples_ms, 0.95), 3),
        "p99_ms": round(percentile(samples_ms, 0.99), 3),
        "max_ms": round(max(samples_ms), 3) if samples_ms else float("nan"),
    }


def body_for(i: int, rng: random.Random) -> str:
    words = rng.choices(VOCAB, k=8)
    if i % 997 == 0:
        words.extend(SPECIAL_TERMS[i // 997 % len(SPECIAL_TERMS)].split())
    return " ".join(words)


def build_db(message_count: int, seed: int) -> Tuple[Path, float, int]:
    rng = random.Random(seed)
    fd, path_s = tempfile.mkstemp(prefix=f"om-omnibar-{message_count}-", suffix=".sqlite3")
    os.close(fd)
    path = Path(path_s)
    conn = sqlite3.connect(path)
    try:
        conn.execute("PRAGMA journal_mode=OFF")
        conn.execute("PRAGMA synchronous=OFF")
        conn.execute("PRAGMA temp_store=MEMORY")
        conn.execute("CREATE VIRTUAL TABLE messages USING fts5(body, person UNINDEXED, conversation UNINDEXED, ts UNINDEXED)")
        started = time.perf_counter()
        batch = []
        for i in range(message_count):
            person = f"Person {i % 1000:04d}"
            conversation = f"Conversation {i % 5000:04d}"
            ts = 1_700_000_000 + i
            batch.append((body_for(i, rng), person, conversation, ts))
            if len(batch) >= 5000:
                conn.executemany("INSERT INTO messages(body, person, conversation, ts) VALUES (?,?,?,?)", batch)
                batch.clear()
        if batch:
            conn.executemany("INSERT INTO messages(body, person, conversation, ts) VALUES (?,?,?,?)", batch)
        conn.commit()
        elapsed = time.perf_counter() - started
    finally:
        conn.close()
    return path, elapsed, path.stat().st_size


def benchmark_fts(path: Path, rounds: int) -> Dict[str, object]:
    conn = sqlite3.connect(path)
    try:
        queries = ["hotel", "project design", "airport ticket", "security device", "birthday photo", "meeting"]
        # warm cache/index paths
        for q in queries:
            conn.execute("SELECT rowid FROM messages WHERE messages MATCH ? LIMIT 20", (q,)).fetchall()
        samples = []
        rows_seen = 0
        for i in range(rounds):
            q = queries[i % len(queries)]
            t0 = time.perf_counter()
            rows = conn.execute(
                "SELECT rowid, person, conversation FROM messages WHERE messages MATCH ? ORDER BY bm25(messages) LIMIT 20",
                (q,),
            ).fetchall()
            samples.append((time.perf_counter() - t0) * 1000)
            rows_seen += len(rows)
        return {"latency": latency_stats(samples), "rows_returned_total": rows_seen}
    finally:
        conn.close()


def benchmark_hot_candidates(rounds: int, seed: int) -> Dict[str, object]:
    rng = random.Random(seed)
    people = [f"Person {i:04d}" for i in range(1000)]
    actions = ["Call", "Message", "Invite", "Scan QR", "Settings", "New group", "Search messages", "Show my QR"]
    samples = []
    total = 0
    prefixes = ["p", "pe", "person 0", "person 09", "call", "inv", "set", "scan"]
    for i in range(rounds * 10):
        q = prefixes[i % len(prefixes)].casefold()
        t0 = time.perf_counter()
        p = [x for x in people if q in x.casefold()][:20]
        a = [x for x in actions if q in x.casefold()][:20]
        # small deterministic scoring pass representative of hot candidates
        scored = [(0 if x.casefold().startswith(q) else 1, len(x), x) for x in (p + a)]
        scored.sort()
        total += len(scored[:20])
        samples.append((time.perf_counter() - t0) * 1000)
        if rng.random() < -1:  # keep RNG consumed nowhere; determinism explicit
            raise AssertionError
    return {"latency": latency_stats(samples), "rows_returned_total": total}


@dataclass
class DecayedCounter:
    value: float = 0.0
    last_t: int = 0

    def get(self, now: int, half_life: float) -> float:
        if self.value == 0:
            return 0.0
        age = max(0, now - self.last_t)
        return self.value * (0.5 ** (age / half_life))

    def add(self, now: int, amount: float, half_life: float) -> None:
        self.value = self.get(now, half_life) + amount
        self.last_t = now


class BaselineRanker:
    """Transparent contextual recency/frequency ranker."""
    def __init__(self, targets: Sequence[str], half_life: float = 40.0):
        self.targets = list(targets)
        self.half_life = half_life
        self.counts: Dict[Tuple[str, str], DecayedCounter] = defaultdict(DecayedCounter)
        self.recency: Dict[Tuple[str, str], int] = {}

    def scores(self, context: str, now: int) -> Dict[str, float]:
        out = {}
        for target in self.targets:
            c = self.counts[(context, target)].get(now, self.half_life)
            last = self.recency.get((context, target))
            rec = 0.0 if last is None else math.exp(-(now - last) / 15.0)
            out[target] = math.log1p(c) + 0.45 * rec
        return out

    def update(self, context: str, target: str, now: int) -> None:
        self.counts[(context, target)].add(now, 1.0, self.half_life)
        self.recency[(context, target)] = now


class BayesianRanker:
    """Decayed contextual Dirichlet-style posterior using a global prior.

    It intentionally remains simple enough to explain and reset. Global decayed
    counts provide smoothing for sparse contexts; contextual evidence dominates
    as it accumulates.
    """
    def __init__(self, targets: Sequence[str], half_life: float = 40.0, prior_strength: float = 2.5):
        self.targets = list(targets)
        self.half_life = half_life
        self.prior_strength = prior_strength
        self.ctx: Dict[Tuple[str, str], DecayedCounter] = defaultdict(DecayedCounter)
        self.global_counts: Dict[str, DecayedCounter] = defaultdict(DecayedCounter)

    def scores(self, context: str, now: int) -> Dict[str, float]:
        global_vals = {t: self.global_counts[t].get(now, self.half_life * 2) for t in self.targets}
        global_total = sum(global_vals.values())
        base_denom = global_total + len(self.targets)
        prior = {t: (global_vals[t] + 1.0) / base_denom for t in self.targets}
        ctx_vals = {t: self.ctx[(context, t)].get(now, self.half_life) for t in self.targets}
        ctx_total = sum(ctx_vals.values())
        denom = ctx_total + self.prior_strength
        return {
            t: (ctx_vals[t] + self.prior_strength * prior[t]) / denom
            for t in self.targets
        }

    def update(self, context: str, target: str, now: int) -> None:
        self.ctx[(context, target)].add(now, 1.0, self.half_life)
        self.global_counts[target].add(now, 1.0, self.half_life * 2)


def argmax_stable(scores: Dict[str, float]) -> str:
    return max(sorted(scores), key=lambda k: scores[k])


def synthetic_trace(kind: str, n: int, seed: int) -> List[Tuple[str, str]]:
    rng = random.Random(seed)
    targets = ["Maria", "Ren", "Alex", "Work", "Family", "Nina"]
    out = []
    for i in range(n):
        context = "work" if i % 3 == 0 else "home"
        if kind == "stable":
            probs = [0.58, 0.14, 0.08, 0.08, 0.08, 0.04] if context == "home" else [0.12, 0.08, 0.10, 0.50, 0.12, 0.08]
        elif kind == "shift":
            before = i < n // 2
            if context == "home":
                probs = ([0.62, 0.12, 0.08, 0.05, 0.08, 0.05] if before else [0.12, 0.58, 0.08, 0.05, 0.12, 0.05])
            else:
                probs = ([0.10, 0.08, 0.10, 0.55, 0.10, 0.07] if before else [0.08, 0.08, 0.52, 0.12, 0.12, 0.08])
        elif kind == "sparse-context":
            # many low-sample contexts share a broad target prior
            context = f"ctx-{i % 17}"
            hot = (i % 17) % len(targets)
            probs = [0.50 if j == hot else 0.10 for j in range(len(targets))]
            s = sum(probs)
            probs = [p / s for p in probs]
        else:
            raise ValueError(kind)
        target = rng.choices(targets, weights=probs, k=1)[0]
        out.append((context, target))
    return out


def evaluate_rankers(seed: int, events: int = 1200) -> Dict[str, object]:
    targets = ["Maria", "Ren", "Alex", "Work", "Family", "Nina"]
    result = {}
    for scenario in ("stable", "shift", "sparse-context"):
        trace = synthetic_trace(scenario, events, seed + len(scenario))
        rankers = {"baseline": BaselineRanker(targets), "bayesian": BayesianRanker(targets)}
        per = {}
        for name, ranker in rankers.items():
            correct = 0
            evaluated = 0
            switches = 0
            last_pred_by_ctx: Dict[str, str] = {}
            post_shift_correct = 0
            post_shift_n = 0
            for i, (ctx, chosen) in enumerate(trace):
                pred = argmax_stable(ranker.scores(ctx, i))
                if i >= 20:  # minimal warmup; cold start is separately visible in traces
                    evaluated += 1
                    correct += int(pred == chosen)
                    prev = last_pred_by_ctx.get(ctx)
                    if prev is not None and prev != pred:
                        switches += 1
                    last_pred_by_ctx[ctx] = pred
                    if scenario == "shift" and i >= events // 2:
                        post_shift_n += 1
                        post_shift_correct += int(pred == chosen)
                ranker.update(ctx, chosen, i)
            per[name] = {
                "top1_accuracy": round(correct / evaluated, 4),
                "prediction_switches": switches,
                "post_shift_accuracy": round(post_shift_correct / post_shift_n, 4) if post_shift_n else None,
            }
        result[scenario] = per
    return result


def run_size(message_count: int, rounds: int, seed: int) -> Dict[str, object]:
    path, build_s, bytes_on_disk = build_db(message_count, seed)
    try:
        return {
            "messages": message_count,
            "fts5_build_s": round(build_s, 3),
            "sqlite_bytes": bytes_on_disk,
            "bytes_per_message": round(bytes_on_disk / message_count, 2),
            "hot_candidates": benchmark_hot_candidates(rounds, seed),
            "full_text": benchmark_fts(path, rounds),
        }
    finally:
        try:
            path.unlink()
        except FileNotFoundError:
            pass


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sizes", nargs="+", type=int, default=[100_000, 1_000_000])
    ap.add_argument("--rounds", type=int, default=120)
    ap.add_argument("--seed", type=int, default=28092026)
    ap.add_argument("--output", type=Path)
    args = ap.parse_args()

    if sqlite3.sqlite_version_info < (3, 9, 0):
        raise SystemExit("SQLite version too old for expected FTS5-era behavior")
    try:
        c = sqlite3.connect(":memory:")
        c.execute("CREATE VIRTUAL TABLE t USING fts5(x)")
        c.close()
    except sqlite3.OperationalError as exc:
        raise SystemExit(f"FTS5 unavailable: {exc}")

    report = {
        "schema": "om.omnibar-benchmark.v1",
        "seed": args.seed,
        "environment": {
            "python": os.sys.version.split()[0],
            "sqlite": sqlite3.sqlite_version,
            "note": "Container benchmark: candidate generation and local index only; not browser UI paint or mobile-device evidence.",
        },
        "search": [run_size(n, args.rounds, args.seed + n) for n in args.sizes],
        "ranking_simulation": evaluate_rankers(args.seed),
    }
    out = json.dumps(report, indent=2, sort_keys=True)
    print(out)
    if args.output:
        args.output.write_text(out + "\n")


if __name__ == "__main__":
    main()
