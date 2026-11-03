# Chat backup file (OMB-1): design, threats, what remains

Date: 2026-11-03. Decision: [`ADR-0184`](../../decisions/ADR-0184-chat-history-backup-file.md). Builds steps 1 and 2 of the capacity report's backup order; step 3 is not built.

## What exists

A person can save one file from Settings > Backup file, keep it anywhere (a drive, a mail, a USB stick), and restore it in a fresh install. The file holds their conversations and messages, sealed under a password only they know. SeedLife operates nothing in this path.

Restoring shows the old conversations read-only, marked "Restored from a backup file". It does **not** bring back the ability to message those people: a restored chat has no relationship, no keys and no outbox, so there is no composer and no retry. To talk again, both people start from an invitation. This is the conservative reading of "importing never grants authority" (IMPORT_EXPORT sections 7-10), and it needs no operator decision.

## Format (normative copy in `crates/om-core/src/backup.rs`)

84-byte header, then the AEAD ciphertext. Header: magic `OMBACKUP`, version 1, profile 1 (`OMB-ENC-1`), flags 0, Argon2id m/t/p, 16-byte salt, 24-byte nonce, 16-byte key check, ciphertext length. The whole header is the associated data.

- Argon2id (host, `@noble/hashes`): 64 MiB, 3 passes, 1 lane; password NFKC-normalised. Core receives the 32-byte output.
- `prk = HKDF-Extract(salt, master)`; AEAD key and key check are two Expand outputs under labels `OM-BACKUP-v1 aead key` / `key check` (registered).
- XChaCha20-Poly1305 over a deterministic CBOR body (`om.history-backup` v1).
- Floor 64 MiB / 3 passes and ceiling 256 MiB / 10 passes / 4 lanes are enforced before any key work, in both directions. Builds with the insecure test RNG (never production, R-072) use token costs so tests stay fast.
- Limits: 6 MiB file, 10,000 chats, 250,000 messages, 64 KiB per text. Text only, no compression, no attachments.

Typed refusals: `not_a_backup`, `unsupported_version`, `unsupported_profile`, `unsupported_parameters`, `truncated`, `malformed`, `wrong_password`, `tampered`, `invalid_content`, `too_large`. An import is a single staged transaction: nothing is half imported, and a failed commit leaves the profile as it was.

## Verification

Rust (`crates/om-core/tests/backup.rs`, vectors in [`verification/vectors/om-backup-file-vectors.json`](../../verification/vectors/om-backup-file-vectors.json), `crates/om-wasm/tests/bridge.rs`): fixed vector; round trip; wrong key; every truncation and every byte flip classified; trailing byte; version/profile/flag/cost refusals; secrets absent (every long hex string in the exporter's state is searched for in the file); fresh device reads but cannot send; restore twice and incremental restores; restart persistence; refusals import nothing; failed commit; export leaves state unchanged; deleted messages not exported.

TypeScript: pinned Argon2id vector and NFKC equivalence (`om-web-platform`), services and screen with axe (`apps/web`), real-wasm round trip (`om-core-wasm`).

Playwright `apps/web/e2e/backup.spec.ts` (Chromium, Firefox, WebKit, production wasm at full Argon2 cost): Alice and Bob chat, Alice saves a file, a fresh browser context with a new identity restores it, reads the history, has no composer; a junk file, wrong password, changed file and cut file are each named and import nothing; restoring again says nothing is new; the history survives a reload.

## Threat notes (T-169..T-171)

- **Leaked file:** offline guessing, no rate limit possible. Argon2id at 64 MiB / 3 passes slows it; the password strength decides. The UI applies the existing passphrase checks. Size and header reveal that it is an OM backup and roughly how large the history is.
- **Forged or swapped file:** an unsigned archive proves integrity only to someone who knows the password. Restored chats are labelled an unverified copy and carry no authority. A stale file can omit newer history.
- **Accidental leakage via state growth:** the body is built from an allow-list, so a new profile field is not exported until `collect` changes. A test scans for the profile's secrets, but a secret of a new shape needs a new test.
- **Wrong password versus damaged salt or cost:** indistinguishable by construction (the key differs), reported as wrong password. Damage to nonce or ciphertext is reported as tampered.
- **Host memory:** the derived key and file bytes pass through JS (hex across the JSON bridge) and are zeroed where possible, not guaranteed. Same class of exposure as the vault passphrase path.

## Limits of this release

- Not the Portable Archive (still locked, `portability`): no human-readable export, attachments, selective export, foreign imports or provenance classes. `OMB-1` is a legacy-readable object if that scope opens.
- ViewOnce and expiring content do not exist in Core yet; when they do, ADR-0129 applies and the allow-list must exclude ViewOnce and carry expiry.
- A device lost without recovery still loses the identity; this file restores history only.

## What remains (handoff for the next agent)

Ordered by what the operator asked for next. State at handoff: OMB-1 and Settings > Backup file are merged, pushed and live as om-web 0.0.10; `make core-check`, `pnpm web:check`, `make check` and the Chromium/Firefox/WebKit backup spec are green.

1. **Filterable export and import (operator request 2026-11-03).** "All-or-nothing" today means integrity (a refused file imports nothing); it does not mean the person cannot choose. IMPORT_EXPORT section 9 already lists what people may choose (conversations, date ranges, attachments/media, contact records, aliases, provenance, preferences; drafts, tokens, provider secrets, caches and device keys excluded by default). Plan:
   - *Export filter* (easy): extend the plan passed to `Core::export_history` with a `Filter { chats: Option<set of conversation ids>, from_ms, to_ms, direction, kinds }`. `backup::collect` already builds from an allow-list, so filtering is a pure pre-step; the header and AEAD are unchanged. Record the filter inside the body (not the header) so a restored copy says "partial: these chats, these dates".
   - *Import filter* (moderate): add `Core::inspect_history(file, key)` that authenticates and decodes the body and returns a summary (per chat: name, message count, first/last date) without staging a transaction; then `import_history(.., ImportSelection)` applies only the chosen chats/dates in one staged transaction. Atomicity stays: either the whole selection lands or nothing does. UI: after the password, list chats with checkboxes and a date range, then "Restore selected".
   - *Types*: only text messages exist in Core today. Add a `kind` per message when attachments, calls or system events exist, then filter by kind; unknown kinds in a file must be skipped, not refused (version-gate with `FORMAT_VERSION`).
   - *Gate*: "selective export" is part of the locked `portability` scope. Ask the operator for a narrow go-ahead for filters on OMB-1 (core_kernel + reference_client_ui) and record it in an ADR-0184 amendment or ADR-0185 before building. Add R-/T- entries (a filtered file must not be mistaken for a full backup; a date range can hide the messages that matter).
2. **Step 3: user-connected destinations** (the person's own S3/R2 or Drive) storing the same sealed file. Needs its own go-ahead and a provider-neutral destination interface; nothing in OMB-1 changes. Scheduled or automatic backups depend on this.
3. **Onboarding path "Have a backup file?"** so a fresh install can restore before the first chat (today restore lives in Settings, after setup).
4. **ViewOnce/expiry**: when Core gains them, exclude ViewOnce and carry expiry in the body (ADR-0129); extend `collect` and the secrets-absence test.
5. **Portable Archive** (`portability` scope, still locked): human-readable export, attachments, foreign imports, provenance classes. Keep an OMB-1 reader as a legacy importer.
6. **Hardening, optional**: pass the Argon2 output to Core as bytes (not hex) if the bridge gains a binary channel; a recovery-assisted wrong-password hint is deliberately not offered.
7. **Operator-only, not an agent job**: nothing pending from this work. A larger real-device pass of Save/Restore on phones (V-006 matrix) is still open.

Pointers: format `crates/om-core/src/backup.rs`; engine `crates/om-core/src/engine.rs` (`export_history`, `import_history`); bridge `crates/om-wasm/src/bridge.rs`; services `apps/web/src/core/backupServices.ts`; UI `apps/web/src/core/BackupScreen.tsx`; tests `crates/om-core/tests/backup.rs`, `apps/web/e2e/backup.spec.ts`; memory `lonunjie/memory/topics/om-core-kernel.md`.
