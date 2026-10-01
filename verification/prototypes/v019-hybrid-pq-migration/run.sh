#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Feasibility prototype for V-019. Not product code. Not for production.
#
# Runs the OM-level hybrid-PQ group migration probe (OpenMLS 0.9.0 `reboot`) natively and as
# wasm32-unknown-unknown under Node (V8), requires the two outputs to be byte-identical and every vector to pass,
# and writes verification/results/v019-hybrid-pq-migration-latest.json. Exits non-zero on any failure.
#
#   run.sh        ~1-3 min after the first build (first run downloads crates; wasm-bindgen-cli is reused if cached)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
RUST="$HERE/rust"
OUT="$RUST/target/v019-run"
RESULTS="$ROOT/verification/results/v019-hybrid-pq-migration-latest.json"
TOOLS="${OM_TOOL_CACHE:-$HOME/.cache/om}/tools"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$TOOLS/bin:$PATH"
log() { printf '[v019] %s\n' "$*" >&2; }
fail() { printf '[v019] FAIL: %s\n' "$*" >&2; exit 1; }
for t in cargo rustup node python3; do command -v "$t" >/dev/null || fail "missing tool: $t"; done
rm -rf "$OUT" && mkdir -p "$OUT"
cd "$RUST"

log "native build + run"
cargo build --release --locked 2>&1 | tail -1 >&2
./target/release/om-v019-hybrid-pq-migration > "$OUT/native.json" || fail "native run reported a failure"

log "wasm32-unknown-unknown build (PQ dependencies must build here)"
if ! cargo build --release --locked --lib --target wasm32-unknown-unknown > "$OUT/wasm-build.log" 2>&1; then
  tail -40 "$OUT/wasm-build.log" >&2
  fail "wasm32-unknown-unknown build failed (see $OUT/wasm-build.log)"
fi
tail -1 "$OUT/wasm-build.log" >&2
WBV="$(awk '/name = "wasm-bindgen"$/{getline; gsub(/[^0-9.]/,"",$3); print $3; exit}' Cargo.lock)"
if ! command -v wasm-bindgen >/dev/null || [[ "$(wasm-bindgen --version | awk '{print $2}')" != "$WBV" ]]; then
  log "installing wasm-bindgen-cli $WBV into $TOOLS"
  cargo install wasm-bindgen-cli --version "$WBV" --locked --root "$TOOLS" 2>&1 | tail -1 >&2
fi
wasm-bindgen --target nodejs --out-dir "$OUT/wb" target/wasm32-unknown-unknown/release/om_v019_hybrid_pq_migration.wasm
cat > "$OUT/run.cjs" <<'JS'
const m = require('./wb/om_v019_hybrid_pq_migration.js');
process.stdout.write(m.run_all_json() + "\n");
JS
node "$OUT/run.cjs" > "$OUT/node.json" || fail "wasm32/Node run reported a failure"
cmp -s "$OUT/native.json" "$OUT/node.json" || fail "native and wasm32/Node outputs differ"

OPENMLS_SRC="$(ls -d "$HOME"/.cargo/registry/src/*/openmls-0.9.0 | head -1)"
python3 - "$OUT/native.json" "$OUT/node.json" "$RESULTS" "$(node --version)" "$(rustc --version)" "$(cargo --version)" "$WBV" "$OPENMLS_SRC" "$RUST/Cargo.lock" <<'PY'
import hashlib, json, re, sys
native, node, results, node_v, rustc_v, cargo_v, wbv, src, lock = sys.argv[1:10]
r = json.load(open(native))
if r["vectors_failed"] != 0 or any(v["result"] != "pass" for v in r["vectors"]):
    raise SystemExit("a vector failed: " + ", ".join(v["name"] for v in r["vectors"] if v["result"] != "pass"))
if not all(row["migrates_and_messages"] for row in r["suite_matrix"]):
    raise SystemExit("a ciphersuite in the suite matrix failed")

# ReInit-not-implemented quote with file and line, read from the vendored crate source.
def find(rel, needle):
    for i, line in enumerate(open(f"{src}/{rel}", encoding="utf-8"), 1):
        if needle in line:
            return {"file": f"openmls-0.9.0/{rel}", "line": i, "text": line.strip()}
    raise SystemExit(f"quote not found in {rel}: {needle}")
reinit = find("src/messages/external_proposals.rs", "`ReInit` is not yet implemented")
psk = find("src/group/mls_group/creation.rs", "TODO #751: Implement PSK")
reboot = find("src/group/fork_resolution/mod.rs", "mod reboot;")

lk = open(lock).read()
def ver(n):
    m = re.search(r'name = "%s"\nversion = "([^"]+)"' % re.escape(n), lk)
    return m.group(1) if m else None
deps = {n: ver(n) for n in ["openmls", "openmls_rust_crypto", "openmls_basic_credential", "openmls_traits", "openmls_memory_storage",
                            "hpke-rs", "hpke-rs-rust-crypto", "x-wing", "ml-kem", "ml-dsa", "ed25519-dalek", "x25519-dalek", "serde", "serde_json", "wasm-bindgen"]}
sha = lambda p: hashlib.sha256(open(p, "rb").read()).hexdigest()
vec_by_name = {v["name"]: v for v in r["vectors"]}

out = {
  "gates": ["V-019"],
  "criteria": [
    {"gate": "V-019",
     "criterion": "prototype authenticated ReInit from the v1 MLS suite to an experimental hybrid-PQ suite preserving ConversationID and GroupControlState, rejecting downgrade, with vectors",
     "status": "met",
     "evidence": "verification/prototypes/v019-hybrid-pq-migration/rust/src/lib.rs; verification/prototypes/v019-hybrid-pq-migration/README.md; verification/results/v019-hybrid-pq-migration-latest.json",
     "note": "Met as an OM-level authenticated migration built on OpenMLS 0.9.0 MlsGroup::reboot, NOT as RFC 9420 ReInit: OpenMLS 0.9.0 does not implement ReInit (quoted below). A signed diagnostic GroupControlTransition moves a 3-member group from 0x0001 to the hybrid X-Wing suite 0x004F with a new MLS group id, unchanged ConversationID and GroupControlState hash, and a monotone security_generation floor that rejects downgrade, equal-generation, replay, stale-parent, unauthorised and tampered transitions. If the gate must be read as the literal RFC 9420 ReInit proposal, record this as partial."}
  ],
  "independence": "Written by a Claude agent from RFC 9420, ADR-0165/0173 and the public OpenMLS 0.9.0 API. Single-vendor evidence (ADR-0167); not an independent implementation or review.",
  "tool_versions": {"openmls": "0.9.0 (features fork-resolution, draft-ietf-mls-pq-ciphersuites; js on wasm32)", "rustc": rustc_v, "cargo": cargo_v, "node": node_v, "wasm-bindgen-cli": wbv, "locked_dependencies": deps},
  "ciphersuites": r["ciphersuites"],
  "provider_supported_ciphersuites": r["provider_supported_ciphersuites"],
  "past_epoch_window": r["past_epoch_window"],
  "reinit_not_implemented": {"quote": reinit, "related_todo_resumption_psk": psk, "reboot_module_declaration": reboot,
                             "meaning": "RFC 9420 ReInit proposal, ReInit resumption PSK binding and branch PSK are unimplemented; reboot creates a fresh group with the same member list and no cryptographic link to the old epoch secrets."},
  "vectors": r["vectors"],
  "vectors_passed": sum(1 for v in r["vectors"] if v["result"] == "pass"),
  "vectors_total": len(r["vectors"]),
  "suite_matrix": r["suite_matrix"],
  "native_vs_wasm": {"native": "aarch64-apple-darwin release binary", "wasm": "wasm32-unknown-unknown under Node/V8 via wasm-bindgen --target nodejs",
                     "byte_identical": True, "sha256": sha(native), "sha256_wasm": sha(node),
                     "pq_dependencies_built_on_wasm32": True},
  "limitations": [
    "Test-provider keys only: OpenMlsRustCrypto with in-memory storage and freshly generated Ed25519 keys. No durable storage, no OM credential type, no key management.",
    "No real network, relay, KeyPackage publication/consumption or Welcome delivery: key packages are handed over in-process and each joiner holds its own provider.",
    "The migration is an OM-level construction on MlsGroup::reboot. It is not an interoperable RFC 9420 ReInit: the new group has no ReInit/resumption PSK link to the old group, and another MLS implementation would see two unrelated groups plus an OM-private GroupContext extension (type 0xFF51).",
    "GroupControlTransition here is a diagnostic stand-in (JSON struct, length-prefixed canonical bytes, Ed25519 signature). It is not CDDL, not deterministic CBOR and not a frozen wire format; ADR-0156 owns the real encoding.",
    "GroupControlState is a stand-in hash over (ConversationID, roster with device keys and roles). The security floor is held beside it, not inside the hash; whether security_generation.group is hashed state is a design decision this prototype does not make (G-V19-4).",
    "The experimental hybrid suite 0x004F is a draft-ietf-mls-pq-ciphersuites provisional code point (X-Wing draft-06 KEM). Its hybrid KEM is PQ-hybrid but its signatures and credentials are classical Ed25519, so authentication is not post-quantum.",
    "Same-author caveat: author, verifier and test harness are one agent and one codebase; every negative vector was designed by the author of the check it exercises. The Authority set is a plain input, not derived from real GroupControlState policy.",
    "Concurrent competing migrations are shown to be order-dependent; no canonical-successor ordering (ADR-0173) is implemented for migration transitions.",
    "past_epoch_window is exercised by advancing the OLD group with extra commits. A real migration stops advancing the old group, so the epoch-count window would never close (G-V19-1).",
    "No performance, size or battery numbers, and no iOS/Android execution; the cross-runtime comparison is macOS arm64 native versus wasm32 under Node/V8 only.",
    "Pre-release criterion (interoperable migration across two implementations) is untouched; no second implementation implements ReInit or reboot-based migration.",
  ],
  "summary": {"vectors": [v["name"] for v in r["vectors"]]},
}
open(results, "w").write(json.dumps(out, indent=2, sort_keys=True) + "\n")
PY
log "PASS: wrote $RESULTS"
