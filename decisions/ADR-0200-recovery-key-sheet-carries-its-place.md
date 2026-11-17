# ADR-0200 — The recovery key sheet carries its own place: one checksummed string, no URL ever typed

- **Status:** Accepted
- **Date:** 2026-11-18
- **Decided by:** operator delegation 2026-11-18 for the owner-side recovery question Q-REC-3 ("normal users never handle keys or type URLs; easy by default; fail closed; additive only, no amendment of audited text"). This ADR fills silence only and amends no audited text (AGENTS "Design gate" rule 6).
- **Class:** `additive_noncritical_extension` ([`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) section 2, ADR-0165). A new transcription and scanning format and one new plaintext kind; no frozen object, label or key changes. The earlier key-only text keeps working for entry.
- **Answers:** Q-REC-3. Companions: ADR-0199 (the record), ADR-0201 (the Home side).
- **Amends:** nothing. **Does not amend:** [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) section 4 (representations) and section 10.2 (custodian 2: "256-bit root, checksummed, plus locator"), ADR-0138 section 6, [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) section 3.4 (migration with overlap), [`INVITE_QR_MEDIA.md`](../docs/ux-design/specs/INVITE_QR_MEDIA.md) (QR rules, followed not changed).

## Design sources read, and not read

Read in full: AGENTS "Design gate"; [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md); ADR-0199 (this session); ADR-0141; ADR-0138; ADR-0129 (items 15 and 18 on vault wrapping and sensitive-write controls); [`ACCESSIBILITY_AND_INPUT.md`](../docs/architecture/ux/ACCESSIBILITY_AND_INPUT.md) and its `ux-design/specs` counterpart; [`INVITE_QR_MEDIA.md`](../docs/ux-design/specs/INVITE_QR_MEDIA.md); [`home-api.md`](../spec/protocol/home-api.md); the display and parse code of the current recovery key (`recovery_groups`, `parse_recovery_key` in `crates/om-core/src/flows.rs`) and `BeginRecovery` in `api.rs` and `flows/recovery_watch.rs`.

Read in part: [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) (section 3.4 only); [`SECURITY_PRIVACY_PROFILES.md`](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md) and [`PRIVACY_SECURITY_INTERACTION_UX.md`](../docs/ux-design/specs/PRIVACY_SECURITY_INTERACTION_UX.md) (the recovery lines found by search, not the documents).

**Not read:** [`CONTENT_AND_MICROCOPY_VOICE.md`](../docs/ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md) (the copy keys below are drafted for the builder to check against it), [`LOCALIZATION_AND_RTL_READINESS.md`](../docs/ux-design/specs/LOCALIZATION_AND_RTL_READINESS.md), [`ACCESSIBILITY_PLATFORM_MAPPING.md`](../docs/ux-design/specs/ACCESSIBILITY_PLATFORM_MAPPING.md), [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) and [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) (the secure-surface and clipboard rules of section 7 are taken from ADR-0129 item 18 only), [`SCREEN_MAP_AND_JOURNEYS.md`](../docs/ux-design/SCREEN_MAP_AND_JOURNEYS.md), and the other documents of the eight design-gate domains named in ADR-0199. If any of them fixes a recovery-entry rule differently, it wins.

## Context

RECOVERY 10.2 says the recovery key is "256-bit root, checksummed, plus locator". Today the shown key is the root only (14 groups of 4 base32 characters, a 3-byte check) and `BeginRecovery` takes a Home URL separately. A person who lost everything cannot be asked to type a URL, and the `recovery_scope_id` that capability derivation needs (ADR-0138) is not on the sheet. The audited text names no encoding and no fallback when the Home has moved.

## Decision

### 1. One string carries root, place and scope

The string is the layout of [`recovery-sheet.md`](../spec/protocol/recovery-sheet.md): version, kind, the 256-bit RecoveryRoot (full kind only), the 32-byte `recovery_scope_id`, the Home port and host, and a 4-byte checksum (`OM-RECOVERY-SHEET-CHECK-v1`). The locator is **inside** the checksummed bytes, so a mistyped or altered host fails the same check as a mistyped key. The Home is named by its origin only (`https://host[:port]/`), which is what every reference Home is.

### 2. Three ways in, none of them a URL

- **Scan** the QR (`om:recover/` plus base64url of the layout), the normal path for a printed sheet.
- **Paste** the compact text (a password manager entry, a saved file), the normal path for a digital copy.
- **Type** the sheet form: groups of 4 base32 characters, 6 groups per line, each line closed by `:` and a 2-character line check (an aid: "line 3 does not match" appears while the person types). A 103-byte string is about 165 characters in 7 lines. Case, spaces and hyphens are ignored.

Typing is the last resort and is allowed: no step needs a pointer, camera or clipboard, no essential state is color-only or motion-only, and every control has a keyboard and screen-reader path (ACCESSIBILITY AX-01, AX-02, AX-04). The recovery key stays a deliberate secret display: its QR has an accessible description of its purpose (not the value) and its text form is reachable by an explicit "show" action that a screen reader can read.

### 3. Checksums

`OM-RECOVERY-SHEET-CHECK-v1` (whole string, 32 bits, constant-time compare) and `OM-RECOVERY-SHEET-LINE-CHECK-v1` (per line, 10 bits, includes the line index so swapped lines are caught). Both labels are registered `documented` (the vectors, built by the builder, freeze them). 32 bits detect an accidental error with probability 1 - 2^-32; they are an integrity aid and carry no security claim. An entry screen may offer a one-character confusable correction only when exactly one substitution passes both checks.

### 4. The sheet at creation, for every custodian

When the recovery key is the custodian, the sheet shows one block: the QR, the sheet-form text with the line checks, and a **Copy** action using the clipboard's sensitive-write flag. It prints the Home's name in plain words beside the QR (not secret, so a person can tell sheets apart), and offers "also save a copy in a password manager" (compact text). When a synced passkey is the custodian no sheet is shown, and the recovery key is offered as a second custodian after the first conversation as RECOVERY 10.2 says; the same format is used. Existing sheets in the key-only form stay valid and are completed by the place string of section 5.

### 5. The place alone

A `kind = 0x02` string holds the scope and host and **no secret**. Settings, Recovery, "Recovery address" shows it as a QR, text and copy on any controller device (it is not secret-handled), and it is shown again whenever it changes (section 6). It is what completes a key-only sheet, and what repairs a sheet whose Home has moved. A person is never asked for it unless one of those two cases applies.

### 6. When the Home has moved (fail closed, two ways)

The sheet names a Home and its scope. A person may later move their Home (HOME_ONBOARDING 3.4, "move to my own home" with overlap). Then:

1. **A redirect record, left by the owner.** While the move runs, a controller device holding the RecoveryKit (ADR-0199 section 5) writes at the **old** locator a `RecoveryRedirect` (type 37), sealed like a record under the old scope's keys: it names the new host, port and scope. It also writes a full RecoveryRecord at the new Home (new scope, so new capabilities and locator, same RecoveryRoot). A recovering device that fetches a record at the old place and opens a type 37 plaintext follows it **only** when: the AEAD opened (so a RecoveryRoot or kit holder wrote it); the new host and port obey the host rules of the sheet; the new place's capabilities document advertises the redirect's `recovery_scope_id` and `om.recovery.store.v1`; the pair (host, scope) was not visited; and at most **3** hops were taken. A Home's own `moved` error without such a record is treated as `not_found`: a Home cannot produce the verifiable replacement information for a record it cannot read, so the redirect record is that information for this object type (Home API v1 section 4 and 6).
2. **The place string.** After a move every controller device shows the new place string (section 5) with the line "Your recovery address changed. Save the new one next to your recovery key." The person scans or pastes it at recovery next to the old key (the root part is unchanged). If the sheet's Home is gone with no redirect and the person has no updated place, the record cannot be found: the restricted state says so plainly, and recovery continues through the contact path of ADR-0190 (re-introduction), which needs no Home. This is the stated residual, as RECOVERY 10.7 already says for a lost Home.

A capabilities document whose `recovery_scope_id` differs from the sheet's stops the attempt with "this address is out of date" and goes to rule 1 or 2; the Home's value never overrides the sheet's.

### 7. Handling

A `kind = 0x01` string, its QR and the entered key are class `om.recovery_secret`: secure-surface display (ADR-0129 item 18), clipboard sensitive-write, excluded from backups, never in a log, crash report or analytics, zeroized after use. A scan or paste that does not parse is rejected without echoing it. Entry holds the root only in the restricted recovery state until completion (RECOVERY 10.3).

### 8. Entry flow and states (for the UI builder)

`BeginRecovery` takes **one** string (`sheet`) and an optional second (`place`), replacing the key plus Home URL pair; Core parses, checks, fetches `/.well-known/om-provider-capabilities`, compares the scope and fetches the record (ADR-0199 section 4). States, none with a protocol word: `idle`, `needsPlace` (key-only sheet), `needsKey` (place only), `checking`, `found` (restricted recovery, the record read), `outOfDate`, `notFound` ("we can't find a saved account for this key at this address"; the same answer for a wrong key and an unknown record, which is what the Home gives), `unreachable` (retry, the Home is down) and `moved` (following a redirect). Three consecutive `notFound` for the same input stop automatic retries. Draft copy keys, group `recoveryEntry` (the builder checks them against the voice rules): `scan`, `paste`, `type`, `lineMismatch` ("Line {n} doesn't match. Check it against your sheet."), `keyMismatch` ("That doesn't look right. Check each line."), `needsPlace` ("This key doesn't say where your account is saved. Scan or paste your recovery address."), `outOfDate` ("This address is out of date. Scan or paste your newest recovery address."), `notFound`, `unreachable` ("Can't reach it right now. Try again."), `addressChanged` (the Settings line above).

### 9. Test-vector plan (vectors by the builder)

`verification/vectors/recovery-sheet-vectors.json.gz`: binary layout round trips for kinds 1 and 2 (shortest and longest host, port 0 and a non-default port); the check and each line check over fixed inputs; the text forms (compact, sheet, with and without separators and case) and the QR form; negatives for every rejection of the layout (version, kind, length, host rules, IP literal, trailing bytes, a flipped bit anywhere, a swapped line, a wrong line check); the legacy 35-byte form (accepted for entry, never produced); the one-substitution correction (unique and ambiguous cases); the redirect rules of section 6 (a loop, 4 hops, a scope mismatch, a non-https host, a `moved` error without a record).

## Where did authority go?

Nowhere new. The string is a secret of the same class as the RecoveryRoot it contains (plus two public values), and it confers exactly what the root confers (RECOVERY 10.3 still applies: delay, veto, co-signature). The Home named on it is a place to look, never an authority: its capabilities document cannot override the sheet, and a record found there is advisory (ADR-0199).

## Consequences

A person recovers by scanning, pasting or, at worst, typing one string; no URL or protocol word appears. The string is longer than the 56-character key (about 165 characters for a typical Home), which is the price of carrying the place and scope; the QR and paste paths remove it for nearly everyone, and the typed form has line-level feedback. A moved Home is survivable with a redirect or an updated address, not otherwise. Residual: 32 bits of checksum do not protect against a deliberate substitution (the string is not authenticated, and a substituted string yields a different root, so recovery finds nothing); the labels have no vectors until the builder's commit.

## Work list for a builder

1. **Builder 1 (Core).** `recovery_sheet.rs`: encode and parse the layout and both checks, the legacy 35-byte parse, `RecoveryRedirect` seal and open, the redirect follow rules, `BeginRecovery { sheet, place }`, the restricted-state fields, writing the redirect and the new-scope record during a Home move.
2. **Builder 3 (UI).** The fresh-install entry screen and states of section 8 (scan, paste, type with line feedback), the creation sheet block and the "Recovery address" Settings item, QR rendering per INVITE_QR_MEDIA, accessibility cases AX-01, AX-02, AX-03.
3. Vectors of section 9; freeze the two sheet labels with them.

## UX line

Scan it, paste it, or type it: one string brings the account back, and nobody types a web address.

## Traces

Requirement R-439, threat T-183, decision D-217. Related: ADR-0138, ADR-0141, ADR-0199, ADR-0201.
