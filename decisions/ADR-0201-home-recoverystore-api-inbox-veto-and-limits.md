# ADR-0201 — Home RecoveryStore API: capability binding, request inbox, veto board and limits

- **Status:** Accepted
- **Date:** 2026-11-18
- **Decided by:** operator delegation 2026-11-18 for the owner-side recovery question Q-REC-4 ("normal users never handle keys or type URLs; easy by default; fail closed; additive only, no amendment of audited text"; the Home stays a dumb store with no new read of account state). This ADR fills silence only and amends no audited text (AGENTS "Design gate" rule 6).
- **Class:** `additive_noncritical_extension` ([`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) section 2, ADR-0165). Home API v1 section 1 says additive behavior is advertised through `ProviderCapabilities.features`; this adds the feature `om.recovery.store.v1`, one new request-authentication class and new operations, and changes no existing operation, body or error. A Home that does not advertise the feature is unaffected, and a client treats a missing feature as "no RecoveryStore here".
- **Answers:** Q-REC-4. Companions: ADR-0199 (the record), ADR-0200 (the sheet). Normative text: [`home-api.md`](../spec/protocol/home-api.md) section 8 and the additions to [`home-api.cddl`](../spec/cddl/home-api.cddl).
- **Amends:** nothing. **Does not amend:** [`home-api.md`](../spec/protocol/home-api.md) sections 1 to 7 (a new section 8 and a status-line mention only), ADR-0133, ADR-0138 sections 5 and 6, ADR-0141 section 2, [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) 10.3, ADR-0190 (the contact notice and in-session veto paths stand as written).

## Design sources read, and not read

Read in full: AGENTS "Design gate"; [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md); [`home-api.md`](../spec/protocol/home-api.md); [`home-api.cddl`](../spec/cddl/home-api.cddl); ADR-0138; ADR-0141; ADR-0190; ADR-0199 and ADR-0200 (this session); ADR-0165; `infra/cloudflare-home/src/limits.ts` and `capabilities.ts` (every number below is taken from, or scaled against, `limits.ts`); the registry files named in ADR-0199; the [`w0-metadata-exposure-descriptors.json`](../spec/metadata/w0-metadata-exposure-descriptors.json) roles and notes (the `recovery_record_store` entry and the notes array).

Read in part: ADR-0186 (decision and consequences only); `infra/cloudflare-home/src/http.ts` (the `bearer()` helper and the CORS header list, by search; no routing code was read); `packages/om-home-wire/src/index.ts` (`FEATURES` only); [`om-home-errors.json`](../spec/registries/om-home-errors.json); [`IMPLEMENTATION_INTERFACES.md`](../docs/architecture/providers/IMPLEMENTATION_INTERFACES.md) (the recovery lines found by search).

**Not read:** ADR-0120, [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../docs/architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md), [`HOME_WAKE_CHANNEL.md`](../docs/architecture/providers/HOME_WAKE_CHANNEL.md), [`PROVIDER_CONFORMANCE_SUITE.md`](../docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md), [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md), `mailbox.ts`, `queue.ts`, `rules.ts`, `usage.ts` (so the token-bucket mechanics and the Durable Object cost accounting are not checked against the code), and the other documents of the eight design-gate domains named in ADR-0199. The numbers of section 4 are conservative starting points, not measurements; the Home builder checks them against `usage.ts` and the free-plan request ceilings in `limits.ts` before any deployment. If an unread document fixes a Home rule differently, it wins.

## Context

ADR-0138 froze `RecoveryRecordPut`, `RecoveryRecordGet` and `RecoveryRequestFanout` bodies, and the Home API row for `recovery_auth` says the proof is "never provider discretion". No Home implements them. The Get body has no capability field and no binding of the capability to the record is defined; there is no veto endpoint (A-045); a controller has no way to pull a request; and RECOVERY 10.3 asks Homes to rate-limit requests per account and drop duplicates without giving numbers. Some of this is needed only in a lost-some-devices recovery (a live controller must see the request to veto it); the lost-all case needs only the record read.

## Decision

### 1. Binding of the record capabilities (the auth binding)

The read and write capabilities derive from the RecoveryRoot and the store's `recovery_scope_id` (ADR-0199 section 3). They are bearer secrets, carried only in `Authorization: Bearer <base64url>` (Home API 3.1) and never in a URL, query, cookie or log. The Home stores the record under `opaque_locator` and, at create, binds `SHA-256(write_cap)` to it. **Get** proves the read capability by `hash(read_cap) = locator`; **put** and **delete** prove the write capability by its stored hash. Creating needs the Home's owner admission as every resource creation here does (`om.admission.v1`), so an open Home never becomes free storage and a stranger cannot squat a locator on a personal Home. The Home cannot link a read to a write capability and does not need to. A replace uses `If-Match` with the stored envelope's `ETag` (a hash the Home computes over the bytes it holds), which gives writers compare-and-swap without the Home reading the generation inside the ciphertext (ADR-0199 section 6). A wrong capability and an unknown locator get the same `not_found`.

### 2. The recovery inbox: how a live controller sees a request

A request goes to the account's Homes (RECOVERY 10.3), and in a lost-some case a surviving controller must learn of it. The Home does not know accounts, so a controller **registers an inbox** keyed by `inbox_id = DS-HASH(OM-RECOVERY-INBOX-ID-v1, account_id)`, proving a `recovery_inbox_cap` that is 32 random bytes minted by a controller, held in every controller's RecoveryKit (ADR-0199 section 5) and **not** derived from the RecoveryRoot (so the holder of the root alone cannot read the inbox). Registration is owner-admitted, idempotent, and refreshed by controllers about monthly. A controller **pulls** with drain and ack: on app start, on foreground, and about every 5 minutes while foreground, hooking the Home wake channel later; a device that stays closed learns at its next open, and the 72-hour or longer delay is what protects the person in that case (verifier-local, ADR-0131/0190).

### 3. The Home stays a dumb store; fan-out and veto are shape checks

On `RecoveryRequestFanout` and on a veto post the Home decodes only the outer envelope and the body's type, version, size and `account_id` (and, for a veto, `request_digest`). It **never** verifies a signature and never acts on one, and it reads no account state: it holds no recovery key and no policy. A forged request can fill space but cannot authorize anything, because every consumer (a controller, a contact) verifies the request against its own pinned `AccountControlState` and `RecoveryPolicy` and runs its own interval (ADR-0190 section 1). The Home:

- appends a request to the inbox of that `account_id` if one is registered, de-duplicated by `SHA-256` of the request body inside the envelope; an identical request changes nothing;
- accepts a veto only when the inbox holds a request with that digest; otherwise it drops it, answered like a success;
- answers **identically** whether or not an inbox exists, and also when a per-inbox limit or a full inbox drops the item, so a poster cannot enumerate accounts that have an inbox; only Home-wide limits answer `rate_limited`;
- never evicts an item it holds: a full inbox keeps its items and drops the new one, so a later flood cannot push out an earlier genuine request;
- returns **no timestamp** in any response (drain items are `{item_id, kind, bytes}`): a Home is never a time or veto authority (ADR-0131);
- offers the vetoes stored for one request digest to anyone who knows the digest (`GET .../vetoes`, `public_read`), so a recovering device can show "stopped by your other device" without a session; a digest is a hash of the request, so it is not guessable by a stranger.

**Residual, stated:** an adversary who can post can fill an inbox with junk (8 pending requests, 3 distinct per rolling day) and so keep a genuine request out of the inbox of the account for as long as it keeps posting within the limits, or fill the veto board of one digest. This costs availability of the **convenience path** only: a contact or controller that never sees an item keeps its pinned head and waits its own interval (fail closed), the contact notice (ADR-0190 3.1) and the in-session veto (3.3) do not use the Home, and a genuine request posted first is never evicted. A Home that can verify the recovery signature against a registered key would remove this residual; it is left to a later additive feature (`om.recovery.verified.v1`) and is not required here, so the Home remains a dumb store as the ruling asks.

### 4. Limits (implementation parameters; advertised; scaled from `limits.ts`)

The table is in [`home-api.md`](../spec/protocol/home-api.md) section 8.4. In short: one record of at most 1,048,576 bytes per locator, 16 records and 16 inboxes on the default profile and 4 and 4 on the personal profile (the same 4:1 scaling `limits.ts` applies to deposit capabilities); a put burst of 6 with refill 1 per 600 s (the `fetchRefillSeconds` pace, sized so linking several devices in a row is not refused while a runaway writer is); a get burst of 10 refill 1 per 30 s per locator and a Home-wide failed-get bucket of burst 30 refill 1 per 10 s; per inbox, 3 distinct requests per rolling 24 hours, 8 pending requests, 32 pending vetoes (16 per digest) and 8 vetoes per rolling hour (a genuine recovery posts one request, then resends the same digest, which costs nothing, and "repeated vetoed requests trigger custody rotation, not an endless veto loop", RECOVERY 10.3); Home-wide fan-out and veto posts burst 10 refill 1 per 60 s; drain burst 20 refill 1 per 5 s (the `descriptorRead` pace); item retention 14 days (`maxRetentionSeconds`); record and inbox retention 400 days without a replace or a register, which the steward refresh of 90 days (ADR-0199) never lets lapse. A signed request or veto is at most 16,384 bytes (the registry maximum of types 4 and 5). Every limit fails with a typed `quota`, `payload_too_large` or `rate_limited`.

### 5. Metadata minimization

Compared with an ordinary mailbox the Home additionally learns: the record locator and capability hashes (neither contains or derives from the AccountID), one of four size buckets (ADR-0199 pads inside the AEAD), the times of writes, `inbox_id` (a hash), and, **only at fan-out and veto post**, the `account_id` and that a recovery was attempted (declared `recovery_attempt`, never `no_by_design`; ADR-0138 section 6, RECOVERY 10.3). It stores no network address, no `DeviceID`, no contact, no relationship and no plaintext, and returns no timestamp. A Home that is both mailbox and RecoveryStore is one observer and may join timing (the descriptor's non-collusion line). The [`w0-metadata-exposure-descriptors.json`](../spec/metadata/w0-metadata-exposure-descriptors.json) notes carry this; the descriptor values are unchanged (see "Not decided here").

### 6. Error and version behavior

Unknown major versions fail `unsupported_version`; the feature id is listed in `ProviderCapabilities.features` with `recovery_scope_id` and the section 8.4 limits in `limits`. Idempotency follows Home API v1 section 5 (a repeated `request_id` with the same bytes returns the determined outcome across process restart; the same id with different bytes is `invalid_request`). Required failures added to the frozen rows: put also `stale_generation`, `payload_too_large`, `rate_limited` and `admission_denied`; get also `rate_limited`.

## Not decided here

1. **`ciphertext_size` of the `recovery_record_store` descriptor.** The frozen descriptor says `no_by_design`; with four padding buckets the Home sees a coarse size. This ADR discloses the bucket in a notes entry and leaves the audited value unchanged; if the operator reads `no_by_design` as unmet, the value should become `sometimes`.
2. **GET without a body.** The frozen row names `RecoveryRecordGetRequest` as the request of an HTTP GET. A GET has no body in practice, so this ADR carries its only field (`request-id`) in an optional `OM-Request-Id` header. If a binding with a body is wanted, a `POST .../get` alternative can be added later; it would not change the semantic body.
3. **Home-side signature verification** (section 3 residual) is a later additive feature, by the ruling that the Home stays dumb.

## Where did authority go?

Nowhere new. The record capabilities prove only ciphertext read or write of one record and are not a vote. The inbox capability proves only a controller's right to pull a mailbox of signed objects that anyone could have posted. The Home never judges a request, a veto, a policy or a time; the verifiers do, against their own pinned state, exactly as ADR-0190 requires.

## Consequences

A fresh device can fetch its record and post a request knowing only the string on its sheet; a live controller can see a request without a session with the recovering device; no Home learns more than it does for a mailbox plus the declared fan-out fact. The cost is a new store and inbox to build and operate (about 1 MiB per record, 16 KiB items), and the stated junk-flood residual. Residual: the numbers are untested against the free-plan request budget; polling every 5 minutes costs about 288 Durable Object requests per controller device per day, which the builder weighs against the 100,000 per day ceiling in `limits.ts`.

## Work list for a builder

1. **Builder 2 (Home, `infra/cloudflare-home` and `packages/om-home-wire`).** `recovery.ts` (record, inbox, veto board in the existing Durable Object style), the `FEATURES` entry `om.recovery.store.v1`, bodies and bounds in `om-home-wire` (decoders reject unknown keys, check CDDL size bounds), `limits.ts` entries and their personal-profile values, `capabilities.ts` (feature, `recovery_scope_id` stable for the store's life, `limits`), the bearer, `If-Match`, `If-None-Match` and `ETag` handling, identical answers for unknown and wrong capability, identical answers for existing and absent inbox, no timestamps, no eviction, retention sweeps. Tests: each limit (a typed failure at exactly the bound), duplicate request, veto without a request, full inbox keeps items, crash and restart idempotency, capability never in a URL or log, record put and get round trip of a 1 MiB body, create without admission refused, `If-Match` mismatch, delete idempotent.
2. **Builder 1 (Core).** The transport adapter (`NetCall` for the operations), fan-out to every Home of the provider snapshot, drain and ack cadence, the controller's "was it you?" input from the inbox, veto post and veto read for a restricted device (ADR-0199 and ADR-0190 for the rest).
3. **Builder 3 (UI).** The restricted recovery state reads the veto board and shows "stopped by your other device"; nothing else new.
4. Conformance: add the cases to the provider conformance suite and vectors for the new bodies (`home-api` CDDL additions).

## UX line

If a thief starts a recovery, your other device hears about it and can stop it with one tap.

## Traces

Requirement R-440, threat T-184, decision D-218. Related: ADR-0133, ADR-0138, ADR-0141, ADR-0190, ADR-0199, ADR-0200.
