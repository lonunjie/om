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
need(py["hpke_pq_05_kats"]["all_passed"], "python HPKE-PQ-05 KATs")
mut = need(all(load("mutation.json").values()), "mutated vector accepted")
hpke_rust = {r: load(f"hpke-{r}.json") for r in runtimes}
hpke_ok = all(need(h["all_passed"], f"rust hpke kat {r}") for r, h in hpke_rust.items())
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
persist_ok = need(final["queued"] == 0 and final["delivered"] > 300 and max_skipped >= 1, "persistence coverage")

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
for c in ["libcrux-ml-kem", "libcrux-ml-dsa", "ed25519-dalek", "x25519-dalek", "curve25519-dalek", "aes-gcm-siv", "chacha20poly1305", "sha3", "shake", "hkdf", "hmac", "sha2", "rand_chacha"]:
    tools[f"crate:{c}"] = lockver(c)
ulock = (HERE / "python/uv.lock").read_text()
for c in ["cryptography", "kyber-py"]:
    m = re.search(r'name = "%s"\nversion = "([^"]+)"' % re.escape(c), ulock)
    tools[f"pypi:{c}"] = m.group(1) if m else None
tools["acvp_server_commit"] = "975de31eb83d87039ec88934fdc47d8c312b892d"
tools["hpke_pq_vectors_commit"] = "6433c8fce0b8b749dfc86c1095081a88698ccfab (tag draft-ietf-hpke-pq-05)"

E = lambda *p: [f"{REL}/{x}" if not x.startswith("verification/") else x for x in p]
AUTHORSHIP = ("written by a Claude agent; the design package was authored by a ChatGPT design agent and Claude agents; "
              "ADR-0117 (proposed) decides whether an AI reviewer counts as independent")
names = set(check_names)
def group(prefix):
    return [n for n in check_names if n.startswith(prefix)]
binding_checks, ledger_checks, aead_checks, hpke_checks = group("binding_"), group("ledger_"), group("aead_"), group("hpke_")
delivery_checks, freshness_checks = group("sealed_delivery_"), group("adr0140_")
freshness_checks = group("adr0140_")
otk_checks = [n for n in check_names if not n.startswith(("binding_", "ledger_", "aead_", "hpke_", "dr_header", "initiator_", "sealed_delivery_", "adr0140_"))]
lock_has_spqr = "spqr" in lock or any("GPL" in k for k in licenses)
C = "verification/prototypes/v001-crypto"
criteria = [
    {"gate": "V-001", "criterion": "isolated verification prototype (not product code) of the OM-Crypto-v1 PQXDH + Double Ratchet state machine (ADR-0118; SPQR is v2) runs on native and wasm32 with state persistence/reload",
     "status": "met" if selftest_ok and per_step_identical and persist_ok else "unmet",
     "evidence": E("rust/core/src/pqxdh.rs", "rust/core/src/session.rs", "rust/core/src/persist.rs", "README.md#results"),
     "note": f"PQXDH rev3 + Double Ratchet rev4 (no SPQR). {len(chains['native'])} steps, each in a fresh process/instance, {final['delivered']} messages incl. "
             "out-of-order and skipped keys carried across reloads; native, wasm32-wasip1 (wasmtime), alternating native/wasip1 and wasm32-unknown-unknown "
             "(Node/V8) produce byte-identical state at every step."},
    {"gate": "V-001", "criterion": "deterministic vectors produced by the prototype are consumed by a second independent implementation or checked against upstream KATs",
     "status": "met" if vec_identical and py["vectors_ok"] and mut and py["acvp"]["all_passed"] and all(a["all_passed"] for a in acvp_rust.values()) else "unmet",
     "evidence": E("vectors/om-v001-proto-vectors.json", "python/check_vectors.py", "rust/core/src/acvp.rs"),
     "note": f"Python (cryptography/OpenSSL + kyber-py) reproduces or verifies {py['vector_checks_passed']}/{py['vector_checks']} values: binding chain and "
             f"{py['binding_negatives']} negative binding vectors, PQXDH, handshake_id, ML-KEM, DR keys, AEAD inputs, bit-exact AES-GCM-SIV ciphertexts and the "
             "HPKE envelope; it rejects a mutated ciphertext and a mutated ML-DSA-65 signature. "
             f"NIST ACVP ML-KEM-768: {acvp_rust['native']['passed']}/{acvp_rust['native']['total']} on libcrux (native+wasi), {py['acvp']['passed']}/{py['acvp']['total']} on kyber-py."},
    {"gate": "V-001", "criterion": "one-time key material handling and depletion behavior demonstrated without downgrade",
     "status": "met" if selftest_ok else "unmet", "evidence": E("rust/core/src/otk.rs", "rust/core/src/store.rs"),
     "note": f"{len(otk_checks)} checks pass on native, wasip1 and Node: depletion to the signed ML-KEM prekey, single-use deletion, replay rejection, "
             "tampered handshake consumes nothing, PQ-less bundle and CT-less initial message refused, epoch rotation with grace and expiry."},
    {"gate": "V-001", "criterion": "dependency, licensing and update strategy recorded in an ADR",
     "status": "met" if not copyleft else "unmet", "evidence": ["decisions/ADR-0118-production-crypto-dependency-licensing-and-update-strategy.md", "decisions/ADR-0135-om-crypto-v1-construction-pinning.md"],
     "note": "ADR-0118 (strategy; v1 without SPQR) and ADR-0135 §6 (the v1 dependency set: libcrux ML-KEM/ML-DSA, dalek, RustCrypto; all MIT/Apache-2.0/BSD). "
             f"Resolved graph licence classes: {sorted(licenses)}."},
    {"gate": "V-001", "criterion": "no reliance on an unsupported external library API contract",
     "status": "met" if not lock_has_spqr else "unmet", "evidence": E("rust/core/Cargo.toml", "rust/Cargo.lock"),
     "note": "The v1 path depends only on crates.io releases used through their documented public APIs (FIPS 203/204/RFC primitives); spqr and libsignal are gone "
             "from the graph. Wire and state encodings are OM-defined (frozen CDDL + ADR-0135), not library-defined."},
    {"gate": "V-001", "criterion": "PQXDH DeviceIdentity binding vectors require both Ed25519 and ML-DSA-65 over the PQXDH identity/prekey binding and reject either-signature substitution (ADR-0131 rule 1)",
     "status": "met" if selftest_ok and py["vectors_ok"] and mut else "unmet", "evidence": E("rust/core/src/sig.rs", "rust/core/src/binding.rs", "vectors/om-v001-proto-vectors.json", "python/check_vectors.py"),
     "note": f"{len(binding_checks)} Rust checks on three runtimes and {py['binding_negatives']} negative vectors the Python verifier (OpenSSL Ed25519/ML-DSA-65) also rejects: either half "
             "substituted, either half alone, halves split into two entries, prekey/one-time-item/identity-key/signing-key substitution, quarantined device, stale or untrusted ACS head."},
    {"gate": "V-001", "criterion": "last-resort PQXDH replay prototype persists an exact per-signed-prekey transcript ledger before acknowledgement, rejects duplicates across crash, and fails closed at the 4096 cap without eviction (ADR-0131 rule 2)",
     "status": "met" if selftest_ok else "unmet", "evidence": E("rust/core/src/ledger.rs", "rust/core/src/store.rs", "rust/core/src/otk.rs"),
     "note": f"{len(ledger_checks)} checks: vault commit precedes plaintext release; a failed commit changes and releases nothing; replay after crash+reload rejected; 4096 real "
             "handshakes fill an epoch, the 4097th is LastResortExhausted, the first id is still rejected (no eviction), one-time initiations still work, a fresh epoch accepts."},
    {"gate": "V-001", "criterion": "AES-256-GCM-SIV payload vectors verify the OM-DR-AEAD-KEY-v1 / OM-DR-AEAD-NONCE-v1 derivation and authenticated header context (ADR-0131 rule 3)",
     "status": "met" if selftest_ok and py["vectors_ok"] else "unmet", "evidence": E("rust/core/src/kdf.rs", "rust/core/src/session.rs", "vectors/om-v001-proto-vectors.json"),
     "note": f"Python recomputes header_hash, K, N and AD for all {py['vector_messages']} vector messages and matches every GCM-SIV ciphertext bit-exactly; header and "
             "envelope-context tampering are rejected in Rust and Python."},
    {"gate": "V-001", "criterion": "MessageRequest HPKE consumes draft-ietf-hpke-pq-05 MLKEM768-X25519/HKDF-SHA256/ChaCha20Poly1305 KATs plus OM context vectors; no ad-hoc hybrid implementation (ADR-0131 rule 9)",
     "status": "met" if hpke_ok and py["hpke_pq_05_kats"]["all_passed"] and py["vectors_ok"] else "unmet",
     "evidence": E("rust/core/src/hpke.rs", "python/hpke_pq.py", "vectors/om-v001-proto-vectors.json"),
     "note": f"Official draft-05 vectors (pinned commit + SHA-256): Rust {hpke_rust['native']['checks']} checks on native/wasi/node and Python "
             f"{py['hpke_pq_05_kats']['checks']} checks, all pass; the OM info/ticket binding vector is reproduced by Python. The combiner is the draft's CG "
             "construction, verified by its KATs, not an OM design."},
    {"gate": "V-001", "criterion": "responder binds the inbound PQXDH initiator IK_A to exactly one active, non-quarantined DeviceCredential committed by the trusted relationship ACS head before any secret-key operation, with negative vectors for unknown, route-mismatched, revoked, quarantined, stale, substituted and ambiguous keys checked by the independent implementation (ADR-0136 rule 4, review F-9)",
     "status": "met" if selftest_ok and py["vectors_ok"] else "unmet",
     "evidence": E("rust/core/src/binding.rs", "rust/core/src/store.rs", "rust/core/src/otk.rs", "vectors/om-v001-proto-vectors.json", "python/check_vectors.py", "verification/reviews/2026-09-27-gpt-5-6-sol-v001-f9-re-review.md"),
     "note": "Responder IK_A is checked against the trusted relationship ACS/DeviceCredential before secret-key work. Current selftests and independent Python vectors include route mismatch, stale/revoked/quarantined/substituted/ambiguous cases; focused non-Anthropic GPT-5.6 Sol re-review accepted the remediation."},
    {"gate": "V-001", "criterion": "ADR-0135 §7 / ADR-0136 rule 4 responder binding accepted on re-review by a non-Anthropic reviewer (review F-9 reopened V-001)",
     "status": "met", "evidence": ["verification/reviews/2026-09-27-gpt-5-6-sol-v001-f9-re-review.md"],
     "note": "GPT-5.6 Sol independently accepted the responder-side initiator binding remediation for F-9."},
    {"gate": "V-001", "criterion": "ADR-0140 AsyncKeyBundle v1 prototype persists a recipient high-water mark across reload/provider migration, rejects lower-generation replay and equal-generation core equivocation, and rejects one-time-prekey transplantation across device/profile/generation/provider-pool context",
     "status": "met" if selftest_ok and py["vectors_ok"] and len(freshness_checks) >= 8 else "unmet",
     "evidence": E("rust/core/src/binding.rs", "rust/core/src/store.rs", "rust/core/src/otk.rs", "vectors/om-v001-proto-vectors.json", "python/check_vectors.py"),
     "note": f"{len(freshness_checks)} focused native/wasip1/Node checks cover same-generation migration across disjoint pools, generation-floor persistence through Device serialization/reload, lower-generation rollback, equal-generation core equivocation, and item transplantation across pool/device/profile/generation. Independent Python consumes refreshed current-tree vectors."},
    {"gate": "V-001", "criterion": "SealedDeliveryEnvelope HPKE consumes the OM-Crypto-v1 pinned draft-ietf-hpke-pq-05 suite under ADR-0142 route-bound info/AAD and padding buckets, rejects wrong route/key/expiry, and never treats outer open as sender authentication",
     "status": "met" if selftest_ok and py["vectors_ok"] and py["sealed_delivery_vectors"] == 1 and len(delivery_checks) >= 14 else "unmet",
     "evidence": E("rust/core/src/delivery.rs", "rust/core/src/hpke.rs", "rust/core/src/otk.rs", "vectors/om-v001-proto-vectors.json", "python/check_vectors.py", "python/hpke_pq.py"),
     "note": f"{len(delivery_checks)} focused native/wasip1/Node checks plus an independent Python seal/open vector exercise the exact 1216-byte public key, 1120-byte hybrid encapsulation, draft-05 suite, route-bound info, wrapper AAD, all five padding buckets, expiry, wrong route/key, tampering and the sender-authentication boundary. No clear-sender fallback is accepted."},
    {"gate": "V-001", "criterion": "[pre-release] independent cryptographic review of the integrated production path",
     "status": "unmet", "evidence": [], "note": "External reviewer required (external_review)."},
    {"gate": "V-001", "criterion": "[pre-release] production build provably excludes dev/test CryptoProvider (R-072)",
     "status": "partial" if r072_ok else "unmet", "evidence": E("rust/core/src/rng.rs", "run.sh"),
     "note": "Sketch only: `production` + `insecure-test-rng` is a compile_error!, the production build succeeds, and its rlib contains no InsecureTestRng (the test build's does, as a control)."},
    {"gate": "V-022", "criterion": "V-001 and V-005 prototypes record crypto and codec costs on native and wasm32 against the budgets; unattainable budgets revised by ADR",
     "status": "partial", "evidence": E("README.md#performance-v-022", "rust/core/src/bench.rs"),
     "note": "V-001 costs recorded on native, wasm32-wasip1 and wasm32-unknown-unknown (Node/V8) on an Apple M3; no reference low-end device."},
]

ok_all = not failures
results = {
    "gates": ["V-001", "V-022"],
    "slug": "v001-crypto",
    "gate_status_effect": "evidence_only_no_gate_passed",
    "criteria": criteria,
    "independence": {
        "authorship": AUTHORSHIP,
        "second_implementation": "Python 3 (cryptography/OpenSSL, kyber-py) written from the public specs; different language and different primitive libraries than the Rust side (dalek, libcrux, RustCrypto)",
        "upstream_kat": ["NIST ACVP-Server ML-KEM-768 internalProjection.json at a pinned commit with SHA-256",
                         "draft-ietf-hpke-pq-05 test-vectors.json (hpkewg/hpke-pq tag draft-ietf-hpke-pq-05) with SHA-256"],
        "limitations": ["both implementations written by the same AI agent (ADR-0134 §5 accepts language/runtime diversity for pre-engineering)",
                        "no human cryptographic review (pre-release external_review)"],
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
        "hpke_pq_05_kats": {r: f'{h["checks"] - len(h["failed"])}/{h["checks"]}' for r, h in hpke_rust.items()} | {"python": f'{py["hpke_pq_05_kats"]["checks"] - len(py["hpke_pq_05_kats"]["failed"])}/{py["hpke_pq_05_kats"]["checks"]}'},
        "persistence": {"steps_each_in_fresh_process": len(chains["native"]), "messages_delivered": final["delivered"],
                        "max_skipped_keys_persisted": max_skipped,
                        "final_world_sha256": final["world_sha256"], "schedules": sorted(chains),
                        "byte_identical_at_every_step": per_step_identical},
        "r072_sketch": r072,
        "node_host_rng_reaches_rust": load("node-rng.json")["distinct"],
        "performance_budget_verdicts": perf,
        "copyleft_dependencies": copyleft,
        "dependency_license_counts": {k: len(v) for k, v in licenses.items()},
    },
}
RESULTS.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
print(json.dumps({"ok": ok_all, "failed": failures}))
sys.exit(0 if ok_all else 1)
