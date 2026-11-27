export PATH=$HOME/.cargo/bin:$PATH
cd packages/om-core-wasm && pnpm repro:scope 2>&1 | grep -a "scope-engines:\|Error"
