# SPDX-License-Identifier: Apache-2.0
"""Feasibility prototype for V-001. Not product code. Not for production.

Assemble verification/results/v001-crypto-latest.json from run.sh outputs. Deterministic: no
timestamps, no absolute paths, no raw timings (those go to rust/target/v001-perf.json).
Exit status 1 if any executable check failed.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

OUT, HERE, RESULTS, LONG = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]), sys.argv[4] == "1"
REL = "verification/prototypes/v001-crypto"
BUDGET_MSG_US, BUDGET_SETUP_US = 5000.0, 50000.0  # MOTION_AND_PERFORMANCE.md (V-022)
LOW_END_FACTOR = 10  # rough assumed slowdown of a sub-USD-200 Android WebView vs this Apple M3 (see README)


def load(name):
    return json.loads((OUT / name).read_text())


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


failures: list[str] = []


def need(cond: bool, what: str) -> bool:
    if not cond:
        failures.append(what)
    return cond


# --- selftests
runtimes = ["native", "wasi", "node"]
selftests = {r: load(f"selftest-{r}.json") for r in runtimes}
check_names = [c["name"] for c in selftests["native"]["checks"]]
selftest_ok = all(need(s["all_ok"], f"selftest {r}") for r, s in selftests.items())
same_checks = need(all([c["name"] for c in s["checks"]] == check_names for s in selftests.values()), "selftest names differ")

# --- vectors
vec_hashes = {r: sha(OUT / f"vectors-{r}.json") for r in runtimes}
committed = HERE / "vectors/om-v001-proto-vectors.json"
vec_identical = need(len(set(vec_hashes.values())) == 1 and vec_hashes["native"] == sha(committed), "vectors differ across runtimes or from committed file")
py = load("python.json")
need(py["vectors_ok"], "python vectors")
need(py["acvp"]["all_passed"], "python ACVP")
mut = need(load("mutation.json")["mutated_vector_rejected"], "mutated vector accepted")
acvp_rust = {r: load(f"acvp-{r}.json") for r in ("native", "wasi")}
for r, a in acvp_rust.items():
    need(a["all_passed"], f"rust ACVP {r}")

# --- persistence
def last(logname):
    lines = (OUT / logname).read_text().strip().splitlines()
    return [json.loads(x) for x in lines]


chains = {m: last(f"persist-{m}.log") for m in ("native", "wasi", "mixed", "node")}
final = chains["native"][-1]
per_step_identical = need(
    all([s["world_sha256"] for s in chains[m]] == [s["world_sha256"] for s in chains["native"]] for m in chains),
    "persistence chains diverge",
)
max_skipped = max(max(s["alice_skipped_keys"], s["bob_skipped_keys"]) for s in chains["native"])
persist_ok = need(final["queued"] == 0 and final["delivered"] > 300 and final["spqr_max_epoch"] >= 3 and max_skipped >= 1, "persistence coverage")

# --- R-072 sketch
r072 = load("r072.json")
r072_ok = need(all(r072.values()), "R-072 build exclusion sketch")

# --- performance verdicts (deterministic booleans; raw numbers -> target/v001-perf.json)
bench = {r: load(f"bench-{r}.json") for r in runtimes}
(OUT.parent / "v001-perf.json").write_text(json.dumps(bench, indent=2, sort_keys=True))
perf = {}
for r, b in bench.items():
    msg = b["message_encrypt_plus_decrypt"]["median_us"]
    setup = b["session_setup_total"]["median_us"]
    perf[r] = {
        "message_median_within_budget": msg <= BUDGET_MSG_US,
        "message_median_within_budget_after_10x_low_end_factor": msg * LOW_END_FACTOR <= BUDGET_MSG_US,
        "session_setup_median_within_budget": setup <= BUDGET_SETUP_US,
        "session_setup_median_within_budget_after_10x_low_end_factor": setup * LOW_END_FACTOR <= BUDGET_SETUP_US,
        "spqr_epochs_advanced_during_benchmark": b["spqr_max_epoch_reached"] >= 2,
    }

# --- dependency licenses (resolved graph for the host platform)
meta = load("metadata.json")
pk = {p["id"]: p for p in meta["packages"]}
licenses: dict[str, list[str]] = {}
for node in meta["resolve"]["nodes"]:
    p = pk[node["id"]]
    if p["name"].startswith("om-v001"):
        continue
    licenses.setdefault(p.get("license") or "UNKNOWN", []).append(f'{p["name"]} {p["version"]}')
licenses = {k: sorted(v) for k, v in sorted(licenses.items())}
copyleft = {k: v for k, v in licenses.items() if "GPL" in k}

# --- tool versions
tools = dict(line.split("=", 1) for line in (OUT / "tools.txt").read_text().strip().splitlines())
lock = (HERE / "rust/Cargo.lock").read_text()
def lockver(name):
    m = re.search(r'name = "%s"\nversion = "([^"]+)"(?:\nsource = "([^"]+)")?' % re.escape(name), lock)
    return (m.group(1) + (" " + m.group(2).split("#")[-1][:12] if m and m.group(2) and m.group(2).startswith("git") else "")) if m else None
for c in ["spqr", "libcrux-ml-kem", "x25519-dalek", "curve25519-dalek", "aes-gcm-siv", "hkdf", "hmac", "sha2", "rand_chacha"]:
    tools[f"crate:{c}"] = lockver(c)
ulock = (HERE / "python/uv.lock").read_text()
for c in ["cryptography", "kyber-py"]:
    m = re.search(r'name = "%s"\nversion = "([^"]+)"' % re.escape(c), ulock)
    tools[f"pypi:{c}"] = m.group(1) if m else None
tools["acvp_server_commit"] = "975de31eb83d87039ec88934fdc47d8c312b892d"

probes = load("probes.json") if LONG else "not run (use run.sh --long); last recorded results are in README.md"

E = lambda *p: [f"{REL}/{x}" if not x.startswith("verification/") else x for x in p]
AUTHORSHIP = ("written by a Claude agent; the design package was authored by a ChatGPT design agent and Claude agents; "
              "ADR-0117 (proposed) decides whether an AI reviewer counts as independent")
criteria = [
    {"gate": "V-001", "criterion": "Rust feasibility prototype of the PQXDH + Triple Ratchet/SPQR-class state machine runs on native and wasm32 with state persistence/reload",
     "status": "met" if selftest_ok and per_step_identical and persist_ok else "unmet",
     "evidence": E("rust/core/src/pqxdh.rs", "rust/core/src/triple.rs", "rust/core/src/persist.rs", "README.md#results"),
     "note": f"PQXDH rev3 + Triple Ratchet (EC DR + Signal spqr SCKA). {len(chains['native'])} steps, each in a fresh process/instance, "
             f"{final['delivered']} messages incl. out-of-order and skipped keys carried across reloads, SPQR epoch {final['spqr_max_epoch']}; "
             "native, wasm32-wasip1 (wasmtime), alternating native/wasip1 and wasm32-unknown-unknown (Node/V8) produce byte-identical state at every step."},
    {"gate": "V-001", "criterion": "deterministic vectors produced by the prototype are consumed by a second independent implementation or checked against upstream KATs",
     "status": "met" if vec_identical and py["vectors_ok"] and mut and py["acvp"]["all_passed"] and all(a["all_passed"] for a in acvp_rust.values()) else "unmet",
     "evidence": E("vectors/om-v001-proto-vectors.json", "python/check_vectors.py", "rust/core/src/acvp.rs"),
     "note": f"Python (cryptography + kyber-py) reproduces {py['vector_checks_passed']}/{py['vector_checks']} values over {py['vector_messages']} messages "
             "(PQXDH, XEdDSA verify, ML-KEM, DR keys, KDF_HYBRID, headers, bit-exact AES-GCM-SIV ciphertexts) and rejects a mutated vector. "
             f"NIST ACVP ML-KEM-768: {acvp_rust['native']['passed']}/{acvp_rust['native']['total']} on libcrux (native+wasi), "
             f"{py['acvp']['passed']}/{py['acvp']['total']} on kyber-py. Scope limit: SPQR message keys are inputs (Signal's spqr is the only implementation). "
             "Same agent wrote both implementations (different language/libraries); see independence."},
    {"gate": "V-001", "criterion": "one-time key material handling and depletion behavior demonstrated without downgrade",
     "status": "met" if selftest_ok else "unmet",
     "evidence": E("rust/core/src/otk.rs", "rust/core/src/store.rs"),
     "note": f"{len(check_names)} checks pass on native, wasip1 and Node: depletion to signed last-resort ML-KEM key, single-use deletion, replay rejection "
             "(one-time and last-resort), tampered handshake does not consume keys, PQ-less bundle / forged prekey / CT-less message / SPQR V0 all refused. "
             "Single provider pool only; multi-provider disjoint pools and multi-device (V-003) not exercised."},
    {"gate": "V-001", "criterion": "dependency, licensing and update strategy recorded in an ADR",
     "status": "partial", "evidence": E("README.md#draft-adr-dependency-licensing-and-update-strategy-for-the-production-crypto-path"),
     "note": "Draft ADR written; needs an integrator to file it under decisions/ and an operator/legal decision on the AGPL-3.0-only spqr dependency vs the MPL-2.0 core policy."},
    {"gate": "V-001", "criterion": "no reliance on an unsupported external library API contract",
     "status": "partial", "evidence": E("README.md#upstream-library-investigation", "rust/core/src/triple.rs"),
     "note": "libsignal (explicitly unsupported outside Signal) is not used. spqr is used only via its 3-function public API with OM-owned state/wire framing, but it has "
             "no crates.io release, no written external support statement and is AGPL-3.0-only; its wire message and state encodings are spqr-defined."},
    {"gate": "V-001", "criterion": "[pre-release] independent cryptographic review of the integrated production path",
     "status": "unmet", "evidence": [], "note": "Needs an external reviewer; out of scope for an agent prototype."},
    {"gate": "V-001", "criterion": "[pre-release] production build provably excludes dev/test CryptoProvider (R-072)",
     "status": "partial" if r072_ok else "unmet", "evidence": E("rust/core/src/rng.rs", "run.sh"),
     "note": "Sketch only: `production` + `insecure-test-rng` is a compile_error!, the production build succeeds, and its compiled rlib contains no InsecureTestRng (the test build's does, as a control). A release-pipeline proof is still needed."},
    {"gate": "V-022", "criterion": "V-001 and V-005 prototypes record crypto and codec costs on native and wasm32 against the budgets; unattainable budgets revised by ADR",
     "status": "partial", "evidence": E("README.md#performance-v-022", "rust/core/src/bench.rs"),
     "note": "V-001 crypto costs recorded on native, wasm32-wasip1 (wasmtime) and wasm32-unknown-unknown (Node/V8); medians are far inside both budgets even with a 10x "
             "low-end factor. Measured on an Apple M3, not a reference low-end device; V-005 codec costs not covered here."},
]

ok_all = not failures
results = {
    "gates": ["V-001", "V-022"],
    "slug": "v001-crypto",
    "gate_status_effect": "evidence_only_no_gate_passed",
    "criteria": criteria,
    "independence": {
        "authorship": AUTHORSHIP,
        "second_implementation": "Python 3 (cryptography, kyber-py) written from the public specs; different language and different primitive libraries than the Rust side (dalek, libcrux, RustCrypto)",
        "upstream_kat": "NIST ACVP-Server ML-KEM-768 internalProjection.json (keyGen, encapsulation, decapsulation, key checks) at a pinned commit with SHA-256",
        "upstream_code": "SPQR is Signal's own implementation (spqr v1.6.0), not OM-authored",
        "limitations": ["both implementations written by the same AI agent in one session",
                        "no independent reimplementation of SPQR/ML-KEM Braid",
                        "no human cryptographic review"],
    },
    "tool_versions": tools,
    "summary": {
        "all_executable_checks_passed": ok_all,
        "failed_checks": failures,
        "selftest_checks": check_names,
        "selftest_all_ok_per_runtime": {r: s["all_ok"] for r, s in selftests.items()},
        "vectors_sha256": vec_hashes["native"],
        "vectors_identical_native_wasi_node_committed": vec_identical,
        "python_vector_checks": f'{py["vector_checks_passed"]}/{py["vector_checks"]}',
        "acvp_mlkem768": {"libcrux_native": f'{acvp_rust["native"]["passed"]}/{acvp_rust["native"]["total"]}',
                          "libcrux_wasi": f'{acvp_rust["wasi"]["passed"]}/{acvp_rust["wasi"]["total"]}',
                          "kyber_py": f'{py["acvp"]["passed"]}/{py["acvp"]["total"]}'},
        "persistence": {"steps_each_in_fresh_process": len(chains["native"]), "messages_delivered": final["delivered"],
                        "spqr_max_epoch": final["spqr_max_epoch"], "max_skipped_keys_persisted": max_skipped,
                        "final_world_sha256": final["world_sha256"], "schedules": sorted(chains),
                        "byte_identical_at_every_step": per_step_identical},
        "r072_sketch": r072,
        "node_host_rng_reaches_rust": load("node-rng.json")["distinct"],
        "performance_budget_verdicts": perf,
        "copyleft_dependencies": copyleft,
        "dependency_license_counts": {k: len(v) for k, v in licenses.items()},
        "upstream_probes": probes,
    },
}
RESULTS.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
print(json.dumps({"ok": ok_all, "failed": failures}))
sys.exit(0 if ok_all else 1)
