# OM design repository tasks. `make help` lists them.
PY ?= python3

.PHONY: help check verify validate docs docs-check evidence tlc export import reuse registries bundle

help: ## list targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-12s %s\n", $$1, $$2}'

check: validate docs-check ## fast checks: validator + docs links

verify: evidence tlc check registries ## everything: evidence runner, TLC on every model, validator, docs

validate: ## architecture contract validator
	$(PY) tools/validate_architecture.py

docs: ## linkify doc mentions and repair moved links
	$(PY) tools/om_docs.py fmt

docs-check: ## fail if docs need formatting or a link is broken
	$(PY) tools/om_docs.py check

evidence: ## run all executable verification evidence (verification/run_phase1.py)
	$(PY) verification/run_phase1.py

tlc: ## model-check every config in verification/tla/models.json with the pinned TLC
	$(PY) tools/run_tlc.py

export: ## package HEAD as design-drops/om-design-<stamp>-<sha>.zip
	$(PY) tools/om_drop.py export

import: ## merge a design drop: make import ZIP=design-drops/<file>.zip
	@test -n "$(ZIP)" || { echo "usage: make import ZIP=design-drops/<file>.zip"; exit 2; }
	$(PY) tools/om_drop.py import "$(ZIP)"

registries: ## assigned ids/registry values are never removed or reused
	$(PY) tools/continuity_bundle.py check-registries

bundle: ## build + verify a Protocol Continuity Bundle for HEAD
	$(PY) tools/continuity_bundle.py build && $(PY) tools/continuity_bundle.py verify $$(ls -t design-drops/om-continuity-bundle-*.tar.gz | head -1)

reuse: ## license compliance (REUSE); needs `uv tool install reuse`
	reuse lint
