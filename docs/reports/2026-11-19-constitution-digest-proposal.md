# Constitution digest and `constitution.py`: what was built and the proposed reading-duty change

- **Date:** 2026-11-19
- **Status:** built and checked; the AGENTS.md / ENGINEER_START_HERE wording change below **needs operator acceptance**. No AGENTS.md or constitution text was edited.
- **Why:** reading the constitution in full for almost every task costs about 20k tokens, because `python3 tools/design_gate.py needs` lists all nine constitution documents (plus about 19 ADRs) for `constitution_governance`, and [`decisions/INDEX.md`](../../decisions/INDEX.md) adds another 35 KB.
- **Design sources read for this report:** all nine documents in `docs/constitution/`, in full. Not read: the 19 ADRs in the `constitution_governance` domain beyond the ids the constitution itself cites.

## 1. What was built (all derived, non-normative)

| File | Role |
|---|---|
| [`docs/constitution/CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md) | 95 numbered rules `C-01..C-95`, each one sentence with a title, grouped by theme, each ending `[CODE §heading; ADR-nnnn]`. Banner: derived, the original wins, the design-gate reading duty is unchanged. |
| [`machine/constitution_index.json`](../../machine/constitution_index.json) | one entry per rule: `id, title, text, tags[], source[{file, heading, anchor, lines}], adrs[]`. Generated from the digest. |
| [`machine/adr_index.json`](../../machine/adr_index.json) | one entry per ADR: `id, title, status, date, path, amends[], amended_by[]` (plus `index_title` when INDEX.md words it differently). Generated from `decisions/ADR-*.md` and [`INDEX.md`](../../decisions/INDEX.md). |
| [`tools/constitution.py`](../../tools/constitution.py) | `digest`, `find`, `show`, `adr`, `gen`, `check` (stdlib only). |
| `tools/design_gate.py needs --brief` | for `constitution_governance` only, prints `read docs/constitution/CONSTITUTION_DIGEST.md (or: tools/constitution.py find <topic>)` in place of the nine constitution files. Other docs and ADRs, the default output, the `Design-Read` domain list and the commit-msg hook are unchanged (verified by diff and a test). |
| [`tests/test_constitution.py`](../../tests/test_constitution.py), `make tests` | 12 unit tests (resolution, ambiguity, drift, CLI, `--brief`). |
| `make constitution` (part of `make check`) | runs `constitution.py check`. |

`check` fails when a cited heading no longer exists or is ambiguous, an id is duplicated, a rule has no source, the digest exceeds 3,500 words, or either JSON file differs from what its source regenerates. After editing the digest, an original document's headings, or any ADR title/status, run `python3 tools/constitution.py gen`.

## 2. Sizes (before and after)

| Material | Bytes | Words | Tokens (chars/4) |
|---|---|---|---|
| Nine constitution documents (what `needs` demands today) | 78,827 | 10,170 | about 19,700 |
| [`CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md) | 25,176 | 3,467 | about 6,300 (about 4,500 by words x 1.3) |
| `constitution.py find <topic> -n 5` (typical) | under 3,000 | | under 750 |
| `constitution.py adr <keywords> -n 5` (typical) | under 2,000 | | under 500 |
| [`decisions/INDEX.md`](../../decisions/INDEX.md) (35 KB) | 34,625 | | about 8,700 |

Reading the digest instead of the nine documents saves roughly two thirds of the constitution load; a targeted `find` saves more than 95%. The digest is above the 2,500-word ideal and under the 3,500 ceiling. Fewer, shorter rules would lose traceability.

## 3. How an agent uses it

1. `python3 tools/design_gate.py needs --brief <paths>` for the working list; read [`CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md) once (or only what the task touches).
2. `python3 tools/constitution.py find <topic or tag>` (tags include operatorless, authority, identity, security, privacy, metadata, recovery, extensibility, longevity, anti-capture, licensing, stewardship, legal, naming, ux); `--full` adds the cited original section, bounded to 60 lines.
3. `show C-nn --source` for one rule and its original passage; `adr <keywords>` to find the controlling ADR without opening INDEX.md.
4. Read an original in full when a rule is cited as ambiguous, when the digest and an original seem to disagree (the original wins; report the digest error), or when the work changes constitutional text.

## 4. Proposed wording change (needs operator acceptance)

Today AGENTS.md "Design gate" rule 1 says to read every document and ADR `needs` lists, in full. Proposal, for `constitution_governance` only:

> For the `constitution_governance` domain, read [`docs/constitution/CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md) first (run `design_gate.py needs --brief`), then use `python3 tools/constitution.py find <topic>` for detail. Read an original constitution document in full only when a digest rule is cited as ambiguous, when the digest and the original disagree, or when the task edits constitutional text. Every other domain's documents and the listed ADRs are still read in full. The digest is derived and the originals win.

The same sentence would go in [`docs/process/ENGINEER_START_HERE.md`](../process/ENGINEER_START_HERE.md) where it repeats the read-in-full duty, and in the plan/report rule 3 ("say which documents you read": the digest counts as read for the constitution only under this wording). Until accepted, the existing duty stands and the digest is an optional aid.

## 5. Things to know

- The constitution says little about user-held keys and recovery mechanics (those live in the architecture docs); those digest groups hold only what the constitution states, with tags `identity` and `recovery`.
- Where the constitution cites an ADR, that id is in the rule. ADR-0009 is also cited on rule C-08 as the decision that records it; this is the only id added beyond the originals' own citations.
- `make check` now fails if an ADR is added or retitled without `constitution.py gen`; the same is true of the file registry today.
