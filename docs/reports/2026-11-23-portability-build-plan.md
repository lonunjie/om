# Portability build plan (scope `portability`, authorized by ADR-0256)

Date: 2026-11-23. Status: scope **authorized** on the operator's explicit instruction, 2026-11-23 ([`ADR-0256`](../../decisions/ADR-0256-authorize-portability-engineering.md), Accepted). This is a plan; no product code for the scope exists yet beyond the pieces named in section 3.

## Design sources read

Read in full: [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) (sections 1 to 20 and the ADR-0125 ruling), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), the `portability` entry of [`engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json), the V-014 gate record, [`ADR-0256`](../../decisions/ADR-0256-authorize-portability-engineering.md), the V-014 prototype README, `criteria.json` and the first third of [`OM_ARCHIVE_V1_SPEC.md`](../../verification/prototypes/v014-archive/OM_ARCHIVE_V1_SPEC.md) (scope, conventions, container).

Read in part: [`RECOVERY_AND_BACKUP_POLICY.md`](../architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) section 8 (history backup and restore sequence) and the header of [`backup.rs`](../../crates/om-core/src/backup.rs).

**Not read, and every slice must read before it builds** (`python3 tools/design_gate.py needs <slice paths>`): the rest of the archive spec (manifest, encryption, reader algorithm, import semantics), [`CANONICAL_ENCODING.md`](../architecture/crypto/CANONICAL_ENCODING.md), ADR-0125 and ADR-0184 in full, the two logical schemas (`om-archive-manifest`, `imported-history-record`), [`om-secret-object-types.json`](../../spec/registries/om-secret-object-types.json), the multi-device sync and account-control documents that define "authorized device", and the UX brief. The V-014 prototype was not re-run for this plan.

## 1. What the scope covers, per the design

`om-archive-v1`: a public, implementation-independent export and import of a person's own data and evidence. It is a way out, never a source of authority.

- **Four operations kept apart:** live provider migration, client or device migration (never by copying a DeviceVault), portable archive export and import, and foreign-messenger history import. Only the last two are this scope.
- **Content:** exact canonical bytes of signed and hashed protocol objects where available plus a decoded projection, blobs by BlobID, portable records, provenance and public authentication material, under a canonical-CBOR `manifest.cbor` (ADR-0125) with an integrity root.
- **Never inside:** raw long-term Account, recovery, DeviceIdentity, MLS, Service or Bridge keys (the secret-object-types registry lists them), drafts, notification tokens, provider secrets, caches, push subscriptions. ViewOnce items are never exported; unexpired disappearing items only in the secure export, with their canonical expiry, and importers enforce it.
- **Profiles:** a portable secure export (encrypted under a public, reviewed profile) and an explicit human-readable export that never becomes the only portable format.
- **Import modes:** rehydrate only into an already authorized or recovered device of the same account, never overriding newer control state, revocations or retention; or read-only historical import by any client with no live authority.
- **Foreign imports** are local, non-routable `ImportedHistoryRecord`s with a `PortabilityReport`; never fabricated native authorship; adapters are untrusted bounded parsers with no keys or network.
- **Verification obligations** (design section 20): two independent implementations, exact preservation of signed objects, stale-control override attempts, path, decompression and resource attacks, hostile foreign formats, missing blobs, spoofed authorship, read-only import that tries to send, wrong-key and corruption handling, and no cloning of device or account authority.

## 2. Blocking gates and prerequisites

| # | Condition | State |
|---|---|---|
| 1 | Accepted ADR names the scope | Met: ADR-0256 |
| 2 | Gate V-014 pre-engineering | **Waived by ADR-0256 for engineering only.** Met repo-locally: 67 hostile cases rejected, 6 positive, reference reader and writer pass. **Open:** the frozen specification (needs an adopting ADR, per-object CDDL from V-005, independent review) and review of the encryption profile |
| 3 | Scope contract prerequisite: a reviewed archive-encryption profile and vectors | **Not met.** `OMA-ENC-1-candidate` is verification-only and must not ship (ADR-0125) |
| 4 | `depends_on` | `core_kernel` authorized |
| 5 | Validator | Passes (12 of 12 scopes) |

Pre-release (never an engineering blocker): two independent implementations round-trip, and real foreign importers pass authority and provenance tests.

## 3. What already exists

| Piece | Where | Gap |
|---|---|---|
| Frozen-candidate spec, vectors, hostile corpus, Rust reference reader and writer | `verification/prototypes/v014-archive` | Prototype, not product; uses an insecure test stand-in for canonical objects because V-005 had not frozen them |
| Chat-history backup `OMB-1` (Argon2id, HKDF, XChaCha20-Poly1305), allow-listed fields, read-only restore | `crates/om-core/src/backup.rs`, ADR-0184 | Explicitly not the Portable Archive; one profile, display fields only, no canonical objects, no blobs |
| Frozen W0 object vectors and CDDL | `spec/cddl`, `verification/vectors/v005-w0-object-vectors.json.gz` | Per-object archive inventory not defined |
| Logical schemas for the manifest and imported records | `spec/protocol` | Logical only; the unlock rules say they never substitute for frozen wire |

## 4. Slices

Each slice reads its design-gate output first, ends with `make check`, the touched crate tests and a commit with `Design-Read` and `Design-Departures`. Heavy runs go through `bin/lonunjie-heavy`.

| Slice | Content | Done when |
|---|---|---|
| 0 | **Freeze the format.** Adopt the container, manifest and integrity rules by an additive ADR; give each archived object type a per-object entry using the frozen V-005 objects instead of the stand-in; register labels and fields; decide the relationship to `OMB-1` (Q2) and the encryption profile (Q1). Regenerate vectors with real objects | Vectors replay byte for byte in the prototype and in a product codec; ADR-0165 checks pass |
| 1 | **Reader (hostile-input boundary).** A bounded, streaming reader in a new crate or module (location per [`ENGINEERING_REPOSITORY_LAYOUT.md`](../process/ENGINEERING_REPOSITORY_LAYOUT.md)), built from the spec and not by copy of the prototype; count and size checks before allocation; no panics | All 67 hostile cases and the 20,000-mutant sweep pass on the product reader; fuzz harness exists before any UI can reach it |
| 2 | **Read-only import.** Build an `ImportedConversation`-style local history view, verify canonical objects exactly, quarantine fabricated authorship, downgrade provenance without `link_evidence`, classify stale control state and never apply it | Import cannot send, cannot add a device, cannot grant Room rights; the nine import checks pass on product code |
| 3 | **Writer and selective export.** Selection by conversation, range, media, contacts, aliases, provenance; exclusion by registry; ViewOnce and disappearing rules; staging so a partial export never publishes a manifest naming objects not durably written; `PortabilityReport` | Interrupted export resumes or discards staging; a secret-type object cannot be written; round trip is byte identical |
| 4 | **Rehydrate into the same account.** Only on an already authorized device; live validation of control state, revocations and floors before anything archived becomes current; fresh sessions, as the restore sequence requires | A stale archive cannot lower a floor, resurrect a revoked device or override a newer control state |
| 5 | **Human-readable export** with the plaintext warning and default exclusion of disappearing items | Explicit per-export choice recorded; never offered as the migration format |
| 6 | **Foreign import framework.** `ImportAdapter` contract (`detect`, `parse_bounded`, `normalize`, `enumerate_blobs`, `emit_warnings`) in a sandboxed worker with no keys or network; `PortabilityReport` fidelity output; which formats to ship is Q3 | Hostile HTML, JSON, SQLite and media fixtures rejected or neutralized; no record gains an OM EventID or signature |
| 7 | **UI.** Export wizard with scope choices and the warnings, import with read-only default, fidelity report display, accessibility and reduced motion per the UX brief | UX brief definition of done; Playwright on three engines |
| 8 | **Evidence.** Continuous fuzzing of the reader and every adapter under the agreed CPU-hour budget; a second independent reader in a different language by a different author; real foreign export fixtures | Recorded; nothing claimed as passed that needs independent review |

Slices 0 to 3 are pure Rust and testable on emeth4 with no UI. The encryption profile (Q1) gates the secure export in slice 3 and 4; until it is reviewed the encrypted path is not offered.

## 5. Constitution rules that bind it

C-12 and C-17 (identity, history and relationships belong to users; no dependency owns them), C-01 (no project-operated migration gate, design section 18), C-20 (no new primitive; floors unchanged), and the ADR-0256 constraints: import never grants authority and restore never overwrites newer live state.

## 6. Questions for the operator or an ADR by delegation

| ID | Question | Safest default |
|---|---|---|
| Q1 | Which reviewed archive-encryption profile does the secure export use? `OMA-ENC-1-candidate` cannot ship | Do not offer encrypted portable export until a profile is reviewed; the only encrypted path stays `OMB-1` for history |
| Q2 | Is `OMB-1` replaced by the Portable Archive, wrapped by it, or kept as a separate lightweight backup? | Keep `OMB-1` as is; the archive is a separate, larger format |
| Q3 | Which foreign formats get adapters first (design names Telegram, Signal, WhatsApp, Matrix, email)? | Ship the adapter framework and one text-only format after the hostile fixtures pass; others later |
| Q4 | Does the archive adopt the OM-ZIP-v1 container as the normative one? | Yes, as specified in the prototype, with ADR adoption in slice 0 |
| Q5 | Disappearing-item export needs expiry enforcement on import | Follow design section 9: importers without enforcement skip those items |

## 7. Routing and budget

Slices 0 and 1 are well specified with checkable vectors and fit a mid tier worker with a complete spec; slices 2, 4 and 6 touch authority and hostile parsing and stay on the session model. Run the usage gate from the lonunjie [`AGENTS.md`](../../AGENTS.md) before dispatching.

## 8. What this plan did not do

It wrote no code, ran no tests, edited no ledger and amended no ADR. The Q-list is for the operator and nothing in it changes audited text.
