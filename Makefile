# OM design repository tasks. `make help` lists them.
PY ?= python3
# The repo pins its Rust toolchain in rust-toolchain.toml; Homebrew's cargo ignores it, so prefer rustup's shims.
ifneq ($(wildcard /opt/homebrew/opt/rustup/bin),)
export PATH := /opt/homebrew/opt/rustup/bin:$(PATH)
endif

.PHONY: help core-check core-wasm core-guard web-guard web-check web-release-check web-e2e check verify verify-drop validate docs docs-check file-registry evidence evidence-drop tlc export import reuse registries manifest bundle preflight preflight-drop preflight-full

help: ## list targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-14s %s\n", $$1, $$2}'

check: validate docs-check file-registry evolution web-guard core-guard ## fast checks: validator + docs links + current file registry + protocol-evolution policy

verify-drop: preflight-drop evidence-drop check manifest ## fresh design-drop checks; no Git history or heavyweight toolchain required

verify: preflight-full evidence tlc check core-check core-wasm web-check web-release-check manifest registries ## full Git-repo verification including Rust/TLC and history immutability

preflight: ## report verification-tool availability without changing evidence
	$(PY) tools/verify_preflight.py --profile report

preflight-drop: ## require the portable tools used by verify-drop
	$(PY) tools/verify_preflight.py --profile drop

preflight-full: ## require Git history + Rust/Java/bootstrap tools used by full verify
	$(PY) tools/verify_preflight.py --profile full

core-guard: ## Rust Core guard: no test-only provider reachable from production, no stray unsafe (no toolchain needed)
	$(PY) tools/core_guard.py

core-check: ## core_kernel: cargo fmt + clippy (native and wasm32) + tests (replays the W0 corpus and V-001 vectors)
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy -p om-wasm --target wasm32-unknown-unknown -- -D warnings
	cargo test --workspace

core-wasm: ## core_kernel: build packages/om-core-wasm/dist/om_wasm.wasm (release, wasm32-unknown-unknown) and re-run the marker guard
	pnpm --filter @om/core-wasm build:wasm
	$(PY) tools/core_guard.py

web-guard: ## web workspace guard: no remote subresources, inline scripts, or harness import in app code (no Node needed)
	$(PY) tools/web_guard.py

web-e2e: ## web workspace: real-browser e2e (Chromium/Firefox/WebKit from the shared Playwright cache; no downloads)
	pnpm e2e

web-check: ## web workspace: typecheck, lint, tests, production build (needs pnpm install)
	pnpm install --frozen-lockfile && pnpm web:check

web-release-check: ## deploy kit: refuse-unsigned + dev release build + verify.mjs + tamper detection (needs pnpm install)
	pnpm exec vitest run apps/web/build/release.test.ts apps/web/build/delegation.test.ts
	node --test deploy/ledger/lib.test.mjs

validate: ## architecture contract validator
	$(PY) tools/validate_architecture.py

docs: ## linkify doc mentions and repair moved links
	$(PY) tools/om_docs.py fmt

docs-check: ## fail if docs need formatting or a link is broken
	$(PY) tools/om_docs.py check

file-registry: ## fail if machine/file_registry.json has missing/stale files or license drift
	$(PY) tools/om_registry.py check

evolution: ## protocol evolution/change-control guardrails
	$(PY) tools/validate_protocol_evolution.py

evidence: ## run all executable phase-1 evidence, including the Rust-dependent aggregate
	$(PY) verification/run_phase1.py --profile full

evidence-drop: ## run portable phase-1 evidence that is safe in a fresh exported ZIP
	$(PY) verification/run_phase1.py --profile drop

tlc: ## model-check every config in verification/tla/models*.json with the pinned TLC
	$(PY) tools/run_tlc.py

manifest: ## check continuity normative-doc manifest against this working tree (Git not required)
	$(PY) tools/continuity_bundle.py check-manifest

export: ## package HEAD as design-drops/om-design-<stamp>-<sha>.zip
	$(PY) tools/om_drop.py export

import: ## merge a design drop: make import ZIP=design-drops/<file>.zip
	@test -n "$(ZIP)" || { echo "usage: make import ZIP=design-drops/<file>.zip"; exit 2; }
	$(PY) tools/om_drop.py import "$(ZIP)"

registries: ## assigned ids/registry values are never removed or reused; requires Git history
	$(PY) tools/continuity_bundle.py check-registries

bundle: ## build + verify a Protocol Continuity Bundle for HEAD; requires Git history
	$(PY) tools/continuity_bundle.py build && $(PY) tools/continuity_bundle.py verify $$(ls -t design-drops/om-continuity-bundle-*.tar.gz | head -1)

reuse: ## license compliance (REUSE); needs `uv tool install reuse`
	reuse lint
