# OM

**OM is an open-source, end-to-end-encrypted messenger that no single company, server or project, including its own authors, can control.**
It is meant to be fast and simple for people who have never configured anything, while keeping identity, conversations
and security independent of any one provider. The name carries six meanings (Open-source, Open, Own, Our, One, Oh
Messenger) plus **0M: zero total control**; see [`NAMING.md`](docs/constitution/NAMING.md).

OM is stewarded by SeedLife ("Protect life. Seed it.") under the [lonunjie](https://github.com/lonunjie) organization.

## Try it

OM is in **beta** (a label, like early Gmail: it is usable and still changing). Open **<https://om.lonunjie.org>** in your
browser and install it as an app from the page. Setup takes about a minute and needs no technical knowledge; advanced
controls are there for people who want them.

OM has not been independently audited by a human cryptographer, makes no security claims, and may lose data during the
beta. Details: [`SECURITY.md`](SECURITY.md) and the privacy notice in
[`docs/process/BETA_PRIVACY_NOTICE.md`](docs/process/BETA_PRIVACY_NOTICE.md).

## Run your own copy

Anyone can run their own copy of OM, on their own address, with their own Home (the small online mailbox that holds
messages until the other person opens OM). Start with [`deploy/README.md`](deploy/README.md). Release folders are signed and
checked before use (`node verify.mjs --require-signed`); [`deploy/REBUILD-AND-COMPARE.md`](deploy/REBUILD-AND-COMPARE.md) shows how
to rebuild the app yourself and compare it with the published release.

## Status

**Open beta.** The web app, the Rust core, the reference Home on Cloudflare and the deploy kit are built and in use. The
release gates and which of them are waived for the beta are recorded in
[`ADR-0183`](decisions/ADR-0183-open-beta-release-waivers.md); the live checklist is
[`docs/reports/2026-10-29-open-beta-checklist.md`](docs/reports/2026-10-29-open-beta-checklist.md). Still to come: linking
more devices, account recovery flows beyond the safety code, per-conversation records and a second independent Home. The
protocol design is documented below and is the source of truth.

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
apps/web/      the web app (PWA) served at om.lonunjie.org
crates/        the Rust core (om-codec, om-core, om-wasm)
packages/      shared TypeScript packages (UI, platform, core bindings)
infra/         the reference Home (Cloudflare Workers + Durable Objects)
deploy/        signed release tooling, key ceremony runbooks, deploy kits
docs/          human-readable design (constitution, overview, architecture/<domain>, process, reviews, ...)
decisions/     architecture decision records (ADRs), indexed in decisions/INDEX.md
spec/          protocol/client models plus normative frozen CDDL/registries; for frozen W0 wire objects, CDDL/registries are the canonical bytes
machine/       machine-checked project state: decisions, requirements, gates, threats, traceability
verification/  formal models (TLA+), bounded models, prototypes, test vectors and recorded results
tools/         validator, docs formatter, design-drop import/export
LICENSES/      full license texts (see LICENSE.md and REUSE.toml for which applies where)
```

## Checking the project locally

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
