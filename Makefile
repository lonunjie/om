# OM design repository tasks. `make help` lists them.
PY ?= python3
TLA_MODELS := $(wildcard verification/tla/*.tla)

.PHONY: help check verify validate docs docs-check evidence tlc export import reuse

help: ## list targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-12s %s\n", $$1, $$2}'

check: validate docs-check ## fast checks: validator + docs links

verify: evidence tlc check ## everything: evidence runner, TLC on every model, validator, docs

validate: ## architecture contract validator
	$(PY) tools/validate_architecture.py

docs: ## linkify doc mentions and repair moved links
	$(PY) tools/om_docs.py fmt

docs-check: ## fail if docs need formatting or a link is broken
	$(PY) tools/om_docs.py check

evidence: ## run all executable verification evidence (verification/run_phase1.py)
	$(PY) verification/run_phase1.py

tlc: ## model-check every TLA+ model with the pinned TLC
	@for m in $(TLA_MODELS); do echo "== TLC $$m"; tools/tlc.sh $$m || exit 1; done

export: ## package HEAD as design-drops/om-design-<stamp>-<sha>.zip
	$(PY) tools/om_drop.py export

import: ## merge a design drop: make import ZIP=design-drops/<file>.zip
	@test -n "$(ZIP)" || { echo "usage: make import ZIP=design-drops/<file>.zip"; exit 2; }
	$(PY) tools/om_drop.py import "$(ZIP)"

reuse: ## license compliance (REUSE); needs `uv tool install reuse`
	reuse lint
