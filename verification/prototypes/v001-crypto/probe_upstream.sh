#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Feasibility prototype for V-001. Not product code. Not for production.
# Clone Signal's SparsePostQuantumRatchet and libsignal at pinned commits and record whether they
# build/test natively and for wasm32 (wasip1, unknown-unknown with default features). JSON on stdout.
set -uo pipefail
W="$1"; mkdir -p "$W"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
SPQR_TAG=v1.6.0; SPQR_COMMIT=06959b4708f9b7b1e94d0f8cc835f3958c077e94
LIBSIGNAL_COMMIT=e8cc2dddd578859b4a029c9c94670b24ce2b616a

clone() { # url dir commit
  if [[ ! -d "$2/.git" ]]; then git clone -q "$1" "$2" >&2 || return 1; fi
  git -C "$2" fetch -q --depth 1 origin "$3" >&2 2>/dev/null || true
  git -C "$2" checkout -q "$3" >&2
}
ok() { if "$@" >>"$W/probe.log" 2>&1; then echo true; else echo false; fi; }

clone https://github.com/signalapp/SparsePostQuantumRatchet.git "$W/spqr" "$SPQR_COMMIT" || exit 1
cd "$W/spqr"
S_NATIVE=$(ok cargo build --release)
S_TESTS=$(cargo test --release 2>&1 | grep -E '^test result' | head -1 | sed -E 's/.*ok\. ([0-9]+) passed; ([0-9]+) failed.*/\1 \2/')
rustup target add wasm32-wasip1 wasm32-unknown-unknown >/dev/null 2>&1
S_WASI=$(ok cargo build --release --target wasm32-wasip1)
S_WASMU=$(ok cargo build --release --target wasm32-unknown-unknown)

clone https://github.com/signalapp/libsignal.git "$W/libsignal" "$LIBSIGNAL_COMMIT" || exit 1
cd "$W/libsignal"
rustup target add --toolchain "$(cat rust-toolchain* 2>/dev/null | tr -d '[:space:]')" wasm32-wasip1 wasm32-unknown-unknown >/dev/null 2>&1
L_NATIVE=$(ok cargo build --release -p libsignal-protocol)
L_WASI=$(ok cargo build --release -p libsignal-protocol --target wasm32-wasip1)
L_WASMU=$(ok cargo build --release -p libsignal-protocol --target wasm32-unknown-unknown)
L_LICENSE=$(grep -m1 '^license' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')
grep -q "Use outside of Signal is unsupported" README.md && L_UNSUPPORTED=true || L_UNSUPPORTED=false

cat <<EOF
{
  "spqr": {"repo": "github.com/signalapp/SparsePostQuantumRatchet", "tag": "$SPQR_TAG", "commit": "$SPQR_COMMIT",
           "license": "$(grep -m1 '^license' "$W/spqr/Cargo.toml" | sed -E 's/.*"(.*)".*/\1/')",
           "native_build": $S_NATIVE, "upstream_tests_passed_failed": "$S_TESTS",
           "wasm32_wasip1_build": $S_WASI, "wasm32_unknown_unknown_default_build": $S_WASMU},
  "libsignal_protocol": {"repo": "github.com/signalapp/libsignal", "commit": "$LIBSIGNAL_COMMIT", "license": "$L_LICENSE",
           "readme_says_use_outside_signal_unsupported": $L_UNSUPPORTED,
           "native_build": $L_NATIVE, "wasm32_wasip1_build": $L_WASI, "wasm32_unknown_unknown_default_build": $L_WASMU}
}
EOF
