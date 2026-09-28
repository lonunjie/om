# OM

**OM is an open-source, end-to-end-encrypted messenger that no single company, server or project, including its own authors, can control.**
It is meant to be fast and simple for people who have never configured anything, while keeping identity, conversations
and security independent of any one provider. The name carries six meanings (Open-source, Open, Own, Our, One, Oh
Messenger) plus **0M: zero total control**; see [`NAMING.md`](docs/constitution/NAMING.md).

OM is stewarded by SeedLife ("Protect life. Seed it.") under the [lonunjie](https://github.com/lonunjie) organization.

## Status

**Architecture verification, design-frozen for every W0 scope. Product engineering is not yet authorized.** The design covers every planned
delivery wave: final architecture closure is recorded in ADR-0139, and one tracked item (A-055, the LAN wire binding) remains open for the later `local_delivery` scope (ADR-0156). It is now being checked by formal models, prototypes and reviews, called verification gates, before any
product code is written. The current state is always in [`SNAPSHOT.md`](SNAPSHOT.md). The gate list and what each gate
still needs is in [`verification/GATE_PASS_CRITERIA.md`](verification/GATE_PASS_CRITERIA.md).

## Reading the design

| If you want… | Start with |
|---|---|
| why OM exists and what it refuses to become | [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) |
| the whole system on one page | [`BLUEPRINT.md`](docs/overview/BLUEPRINT.md), then [`SYSTEM_ARCHITECTURE.md`](docs/overview/SYSTEM_ARCHITECTURE.md) |
| a guided reading order through every domain | [`docs/README.md`](docs/README.md) |
| **why** a decision was made | [`decisions/INDEX.md`](decisions/INDEX.md): one ADR per decision, with the alternatives considered |
| what reviewers found and what changed | [`docs/reviews/`](docs/reviews/) and [`docs/checkpoints/`](docs/checkpoints/README.md) |
| how threats are handled | [`docs/threat-model/`](docs/threat-model/README.md) and [`machine/threat_register.jsonl`](machine/threat_register.jsonl) |
| the evidence behind verification claims | [`verification/`](verification/README.md) |

## Repository map

```text
README.md  SNAPSHOT.md  AGENTS.md  CHANGELOG.md  CONTRIBUTING.md  SECURITY.md  LICENSE.md
docs/          human-readable design (constitution, overview, architecture/<domain>, process, reviews, ...)
decisions/     architecture decision records (ADRs), indexed in decisions/INDEX.md
spec/          protocol/client models plus normative frozen CDDL/registries; for frozen W0 wire objects, CDDL/registries are the canonical bytes
machine/       machine-checked project state: decisions, requirements, gates, threats, traceability
verification/  formal models (TLA+), bounded models, prototypes, test vectors and recorded results
tools/         validator, docs formatter, design-drop import/export
LICENSES/      full license texts (see LICENSE.md and REUSE.toml for which applies where)
```

## Verifying the design locally

```sh
make verify-drop     # fresh exported ZIP: portable evidence + validator/docs + continuity manifest
make verify          # Git repo: full evidence + TLC + validator/docs + registry-history immutability
```

`make help` lists every target. The validator enforces the architecture contract. It checks that engineering stays locked
until the gates allow it, that every file is classified, and that every requirement is traced to a gate.

## Contributing, licensing and security

Contributions use DCO sign-off and inbound-equals-outbound licensing; see [`CONTRIBUTING.md`](CONTRIBUTING.md).
Documentation is CC-BY-SA-4.0, and specifications, schemas and tooling are Apache-2.0 ([`LICENSE.md`](LICENSE.md),
[`REUSE.toml`](REUSE.toml)). To report a security problem in the design, see [`SECURITY.md`](SECURITY.md).
