#!/usr/bin/env bash
# Feasibility prototype for V-009. Not product code. Not for production.
# Builds the Rust materializer, regenerates the corpus, checks permutation/duplication convergence,
# diffs against the Python reference, writes verification/results/v009-materializer-latest.json.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH
mkdir -p "$here/work"
(cd "$here/materializer" && cargo build --release --locked --quiet)
python3 "$here/gen_corpus.py" "$here/work/corpus.json"
rc=0
"$here/materializer/target/release/om-v009-materializer" check "$here/work/corpus.json" "$here/work/rust.json" || rc=1
rustv=$(cd "$here/materializer" && rustc --version | cut -d' ' -f2)
python3 "$here/compare.py" "$here/work/corpus.json" "$here/work/rust.json" "$here/work/compare.json" \
  "$root/verification/results/v009-materializer-latest.json" \
  "rustc=$rustv" "serde_json=1.0.145" "python=$(python3 -c 'import platform;print(platform.python_version())')" || rc=1
echo "v009-materializer: $( [[ $rc == 0 ]] && echo PASS || echo FAIL )"
exit $rc
