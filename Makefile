# OM design repository tasks. `make help` lists them.
PY ?= python3

.PHONY: help check verify verify-drop validate docs docs-check evidence evidence-drop tlc export import reuse registries manifest bundle preflight preflight-drop preflight-full

help: ## list targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-14s %s\n", $$1, $$2}'

check: validate docs-check ## fast checks: validator + docs links

verify-drop: preflight-drop evidence-drop check manifest ## fresh design-drop checks; no Git history or heavyweight toolchain required

verify: preflight-full evidence tlc check manifest registries ## full Git-repo verification including Rust/TLC and history immutability

preflight: ## report verification-tool availability without changing evidence
	$(PY) tools/verify_preflight.py --profile report

preflight-drop: ## require the portable tools used by verify-drop
	$(PY) tools/verify_preflight.py --profile drop

preflight-full: ## require Git history + Rust/Java/bootstrap tools used by full verify
	$(PY) tools/verify_preflight.py --profile full

validate: ## architecture contract validator
	$(PY) tools/validate_architecture.py

docs: ## linkify doc mentions and repair moved links
	$(PY) tools/om_docs.py fmt

docs-check: ## fail if docs need formatting or a link is broken
	$(PY) tools/om_docs.py check

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
