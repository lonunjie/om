# OM Agent Instructions

OM is SeedLife's open-source, operatorless, end-to-end-encrypted messenger. This repository is the **single source of
truth** for its design: documents, decisions, machine ledgers, specs and verification evidence. It is private on GitHub
until the operator makes it public. Write everything as if a stranger will read it, because they will.

**Engineering is authorized for scopes `core_kernel` (ADR-0169), `provider_reference` (ADR-0171), `reference_client_ui` (ADR-0172) and `groups` (ADR-0177) only.** Product code for that scope follows
[`docs/process/ENGINEER_START_HERE.md`](docs/process/ENGINEER_START_HERE.md). Every other scope is still locked:
research, specification, formal models and bounded feasibility prototypes (`verification/`) are allowed for them, and
product code is not. [`machine/repository_contract.json`](machine/repository_contract.json) is the authority.

**Current phase:** architecture remains pre-engineering and is amended through ADR-0166. Two parallel 2026-09-29 OpenAI lines were consolidated by ADR-0166: the independent-review line (a different-vendor review of Anthropic-authored ADR-0154–0156, giving ADR-0157 critical-prefix convergence, the DeviceHistorySnapshot/GroupHistoryBundle split and scheduled-send late binding; ADR-0158 `LanDeliveryProfile-v1`; ADR-0159 UI/UX integration; ADR-0160 V-008 wire freeze) and the Run 2–7 audit line, renumbered as ADR-0161 (terminal DeviceID revocation, mutable-record convergence, GroupHistoryBundle isolation), ADR-0162 (Omnibar/visual/theme hardening on the ADR-0159 shell), ADR-0163 (implementation-readiness and crash/retry transaction contract), ADR-0164 (dangerous algorithms and failure paths) and ADR-0165 (protocol evolution and engineering-agent guardrails). `design_open=0`; every engineering scope remains unauthorized. OpenAI-authored ADR-0157–0165 need a non-OpenAI independent reviewer; Anthropic-authored ADR-0166 needs a non-Anthropic one; the consolidated `core_kernel` brief still needs its independence review. [`verification/CLAUDE_OPUS_5_5_HANDOFF.md`](verification/CLAUDE_OPUS_5_5_HANDOFF.md) is the stable compatibility entry point. For the future engineering boundary start at [`docs/process/ENGINEER_START_HERE.md`](docs/process/ENGINEER_START_HERE.md) and [`docs/process/IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md`](docs/process/IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md). Do not bounce the repository between agents one criterion at a time.

## Part 1: Working in this repository

### Where things live

| Path | Purpose |
|---|---|
| [`SNAPSHOT.md`](SNAPSHOT.md) | current checkpoint state; update it with every checkpoint |
| `docs/constitution/`, `docs/overview/`, `docs/architecture/<domain>/`, `docs/process/`, `docs/threat-model/` | canonical design prose; basenames are unique, so refer to a doc as `` `NAME.md` `` and `tools/om_docs.py fmt` turns it into a link |
| `docs/reviews/`, `docs/checkpoints/`, `docs/research/`, `docs/reports/` | dated records: reviews, checkpoint records, references, working reports |
| `decisions/` | ADRs; every ADR is listed in [`decisions/INDEX.md`](decisions/INDEX.md) |
| `spec/` | normative schemas (`spec/protocol/`, `spec/client/`); frozen CDDL/registries go here too |
| `machine/` | machine truth (contract, file registry, ledgers, traceability, threats) |
| `verification/` | evidence: `tla/`, `bounded/`, `cbor/`, `prototypes/<gate>/`, `vectors/`, `results/` |
| `crates/` | engineering (`core_kernel`): Rust workspace `om-codec`, `om-core`, `om-wasm`; `packages/om-core-wasm` is its TypeScript binding. Status: [`2026-10-28-core-kernel-m1-status.md`](docs/reports/2026-10-28-core-kernel-m1-status.md) |
| `infra/`, `packages/om-home-wire`, `packages/om-provider-http` | engineering (`provider_reference`): the reference Home on Cloudflare (`infra/cloudflare-home`), its Home API wire layer and the Core transport adapter. Status and deploy plan: [`2026-10-28-relay-status.md`](docs/reports/2026-10-28-relay-status.md) |
| `docs/ux-design/` | **UI work starts at [`AGENT_UX_BRIEF.md`](docs/ux-design/AGENT_UX_BRIEF.md)**: the short digest of page, flow, motion, copy and device decisions, with the definition of done for any screen |
| `tools/` | [`validate_architecture.py`](tools/validate_architecture.py), [`om_docs.py`](tools/om_docs.py), [`om_drop.py`](tools/om_drop.py), [`build_manifest.py`](tools/build_manifest.py), [`layout.json`](tools/layout.json) |
| `design-drops/` | **gitignored** drop zone for design zips going out to, or coming in from, other agents |

A new file must be added to [`machine/file_registry.json`](machine/file_registry.json), or the validator fails. New doc folders need a line in the table
in [`docs/README.md`](docs/README.md).

### Checks (run before every commit)

```sh
make verify      # validator + docs check + phase-1 evidence + TLC
make check       # fast subset: validator + docs link check
```

### Design gate (read the design before you build; operator rule, 2026-11-03)

The design is practically complete, so the usual way to go wrong is not a missing design but building from memory of it.
On 2026-11-03 a delivery plan was written from the tiers document alone and missed that the Discovery Fabric makes the
invite carry the inviter's introduction service. That cost a day of rework. The rules that stop it:

1. **Code in `crates/`, `infra/`, `packages/`, `apps/`, `spec/cddl/`, `deploy/` is design-bound.** Before you plan or change it, run
   `python3 tools/design_gate.py needs <paths>` and read the docs and ADRs it prints for those paths' domains (the map is
   [`machine/architecture_map.json`](machine/architecture_map.json), `code_paths` over `domains`). Read them; a grep is not reading. Then also read any ADR
   that amends them ([`decisions/INDEX.md`](decisions/INDEX.md), newest first).
2. **Every code commit carries two trailers**, enforced by the `commit-msg` hook (`make hooks` installs it once per clone;
   `--no-verify` is a violation of this rule):
   `Design-Read: <domain ids>` and `Design-Departures: none | <ADR/waiver/requirement ids>`. A departure is anything that
   ships or is planned differently from a normative doc; it must cite the ADR or waiver that accepts it. If none exists,
   stop and ask the operator, do not ship the difference quietly. The trailers are an audit trail; the operator reviews them.
3. **Plans and reports about design-bound work start with "Design sources read" and say which relevant documents were not
   read.** Not having read something is acceptable if stated; claiming a design without having read it is not.
4. **When code and design disagree, the design wins until an ADR says otherwise.** Do not invent product architecture
   (new tiers, paid plans, spending, a default service, a directory) to fill a gap; record the gap in a report and ask.
5. **SeedLife's own accounts are not a product dependency.** Nothing may be designed or sized so that OM relies on money or
   settings on SeedLife's Cloudflare account; each person owns their own Home ([`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](docs/architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md),
   ADR-0043). The default Home is a labelled beta convenience (ADR-0183).
6. **No ADR changes audited design.** An "accepted by operator delegation" ADR cannot amend audited text in design-bound areas. If the
   text looks wrong or incomplete, write the question in a report and stop. What ships differently is a deviation (see the
   D1 to D12 table in [`docs/reports/2026-10-28-relay-status.md`](docs/reports/2026-10-28-relay-status.md)) citing the passage it departs from. The 2026-11-03 audit of what
   drifted is [`docs/reports/2026-11-03-return-to-the-audited-design.md`](docs/reports/2026-11-03-return-to-the-audited-design.md).

### Design drops (zip <-> repository)

External design agents (for example the ChatGPT design agent) work on zips; this repository is canonical.

```sh
python3 tools/om_drop.py export                   # HEAD -> design-drops/om-design-<stamp>-<sha>.zip
python3 tools/om_drop.py import design-drops/X.zip # merge a returned drop into the current branch
```

- `export` packages a committed ref (never uncommitted work). It adds `OM_EXPORT.json` (the base commit) and a
  `MANIFEST.json`, and validates the result before zipping.
- `import` applies the drop on a branch cut from the drop's base, commits it, tags it `checkpoint/<stamp>`, and
  **merges** it into the current branch, so concurrent repository work is kept and real conflicts are surfaced. It
  accepts both this layout and the legacy flat layout (drops up to 2026-09-24 02:29), mapped by [`tools/layout.json`](tools/layout.json).
  It then runs `om_docs.py fmt` and the validator.

**If you are an agent editing an exported drop:** edit files in place, keeping this layout. Do not create `history/`
or nest zips; git is the history. Leave `OM_EXPORT.json` unchanged. Record the checkpoint in [`SNAPSHOT.md`](SNAPSHOT.md),
[`CHANGELOG.md`](CHANGELOG.md) and a new `docs/checkpoints/CHECKPOINT_YYYYMMDD_HHMM.md`. Register new files, run
`python3 tools/validate_architecture.py` and `python3 tools/om_docs.py check`, then zip the folder as
`om-design-YYYYMMDD-HHMM-PHT.zip`.

### Workspace rules (lonunjie control plane)

This repository lives at `~/dig/lonunjie/om`, inside the SeedLife control plane (`~/dig/lonunjie`, repo `lonunjie/hq`).
When working there, read `../AGENTS.md` first for org-wide rules and registry.

- **Push cadence: auto-push.** Push to `origin` after each verified commit. Never force-push without explicit approval.
- **Memory:** there is no repo-local `memory/`. The shared store is `../memory/`; OM facts go in `../memory/topics/om-*.md`.
- **Reports:** dated writeups go in `docs/reports/YYYY-MM-DD-<slug>.md`, and move to `docs/reports/completed/` once
  executed (see [`docs/reports/README.md`](docs/reports/README.md)).
- **Artifacts stay local:** no hosted artifacts, gists or pastebins unless the operator asks for a shareable link.
- **Secrets:** never commit secrets or credentials.
- CLI tools before GUI tools; small single-purpose tools; shared logic in `tools/omlib/`.

## Part 2: Architecture rules

The architecture rules (mission, absolute rules, core interfaces, change discipline, every dated hardening, resolution and
protocol-evolution rule) are in [`AGENT_ARCHITECTURE_RULES.md`](docs/process/AGENT_ARCHITECTURE_RULES.md). **Read it before touching
`spec/`, `decisions/`, `machine/`, `crates/`, `infra/`, `packages/`, `apps/` (design-bound code: see "Design gate" above), or anything about protocol, cryptography,
security, delivery, discovery, groups or the design.** Docs-only and release-notes work does not need it, but these always apply:

- Provider names never become wire-protocol semantics; Cloudflare is a reference implementation, never a dependency.
- Never silently downgrade security; crypto upgrades are automatic and invisible; no user-facing cipher choice.
- Normal users must not need infrastructure or crypto vocabulary: could someone who cannot configure Wi-Fi still use this correctly and securely?
- Every architecture addition must answer: **Where did authority go?**
- Run `python3 tools/validate_architecture.py` before accepting a checkpoint.

### 2026-09-22 shared UI constraints

- Official PWA/iOS/Android/desktop product screens belong in shared `om-ui`; do not create independent product/security semantics in platform shells.
- Platform adapters expose capabilities/system surfaces, not messaging/security business logic.
- UI components send typed intents to Core and render returned state; never mutate cryptographic/control/provider state directly.
- Motion must be semantic, interruptible where interactive, and reduced-motion compatible; never delay core actions for animation.
- Themes are declarative data-only; no arbitrary CSS/HTML/script/WASM/native code or remote tracking assets.
- Themes cannot suppress/spoof critical security UI, focus, accessibility or external-link destination semantics.
- Accessibility regressions in shared primitives are release-blocking.
- Default UI prioritizes ergonomics/functionality and low visual noise; advanced capabilities use progressive disclosure.
