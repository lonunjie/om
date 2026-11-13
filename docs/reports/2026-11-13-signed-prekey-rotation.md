# Signed-prekey rotation: design extract, code map and plan (no code yet)

Date: 2026-11-13. Status: **design read and mapped; implementation not started** (the reading used the context that was meant for it, so
this session handed off at 79%). Group E of [`2026-11-04-handoff-next-week.md`](2026-11-04-handoff-next-week.md).

## Design sources read

Read in full this session: every document and ADR that `python3 tools/design_gate.py needs crates/om-core/src/{state,engine,api}.rs` lists for
`crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events`
(all 36 documents and all 47 ADRs named), plus [`AGENTS.md`](../../AGENTS.md) (Design gate) and the group E part of the handoff.
Not read: [`spec/cddl/keys.cddl`](../../spec/cddl/keys.cddl) and the registries (not in the gate list; the code already matches them), [`SEALED_SENDER_DELIVERY_PROFILE.md`](../architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md)
(listed by the map for none of the three paths).

## What the design says about signed-prekey rotation

Sources: [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md) sections 4, 6, 7, 9, 11; ADR-0140 sections 2, 3, 5, 6; [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) (last-resort ledger);
ADR-0118 section 6; ADR-0120 K11.

1. `generation` is global per (DeviceID, CryptoProfile), strictly increasing on every rotation of either signed prekey; rotation makes fresh EC
   and PQ signed-prekey secrets **and fresh key ids**. Relabelling old secrets as a higher generation is non-conformant. One-time replenishment, a
   provider change or a new `provider_pool_id` never changes it.
2. Rotation publishes the new generation to **every configured provider pool**, asks the providers to delete `g` (best effort, `revokeGeneration`),
   synchronises local publication state, and deletes unused private one-time material of `g` **when `g` is retired**.
3. One-time prekey signatures bind the generation, so `g` items cannot be used under `g+1`.
4. A peer that accepted `g+1` rejects `g` for ever (`BundleFreshnessFloor`); the publisher never rolls back its own counter (secrets and counter are
   durable DeviceVault state; restore creates a fresh DeviceID, K11).
5. In-flight first messages: the last-resort handshake ledger "is scoped to DeviceID + signed-prekey epoch ... retained through that prekey epoch's
   acceptance overlap"; cap 4096 per epoch, exhaustion fails closed "until fresh key material is published" (that is: rotate).
6. Prekey secrets never go in backups (K11); time is not authority (generation is the sequence, not a clock).

## Gaps (the design is silent; answer from the constitution, operator may overrule)

- **G1 schedule.** No interval or trigger numbers exist anywhere ("expiry/rotation" is listed as a policy item, [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md) section 9).
  Proposal: rotate when the current signed prekey is older than 7 days (wall clock, a rotation timestamp persisted with the epoch; a clock that goes
  backwards never rotates early) **or** the last-resort ledger passes 3/4 of the cap **or** the bundle is within 7 days of `expires_at`, **or** on
  suspected compromise (explicit intent). The constants are policy, not wire, so changing them later is not a protocol change.
- **G2 overlap length.** "Acceptance overlap" has no duration. Proposal: keep `prev_*` until the old bundle's own `expires_at` has passed (nothing
  can legitimately initiate against `g` after that), then `expire_previous()` and delete the epoch's one-time secrets. Bound by `expires_at`, so a
  device that is offline cannot keep old secrets for ever.
- **G3 stale pool items.** The Home's `publish` appends one-time items and keeps the older ones; items signed for `g` then fail verification under
  `g+1` and `verify_fetch_with_freshness` fails the **whole** fetch (`crates/om-core/src/crypto/binding.rs`, item loop). So a rotation that only
  publishes a new bundle would break first contact until the old items drain. The design's answer is step 2 (`revokeGeneration`). Core has no
  `NetCall` for it, although the Home has the route (`infra/cloudflare-home/src/index.ts:191`, `keys.ts revokeGeneration`) and, today, revoke
  only deletes when the stored `bundle_generation` equals the argument. Options: (a) new `NetCall::RevokeKeyGeneration` (Core, wasm/TS provider,
  Home) sent before the new publish, accepting a short window with no bundle; (b) Home drops one-time items whose generation is below a newly
  published higher generation (an availability aid in `publish`, no wire change; needs bundle-facts for items). Prefer (b) plus the best-effort (a);
  this is deviation territory, so record it in the relay status table.
- **G4 no new crypto.** None needed. Rotation reuses `Device::rotate_signed_prekeys`, `OM-PREKEY-*` labels and the existing signer. Stop and ask if
  that changes.

## Code map (what exists)

- `crates/om-core/src/crypto/store.rs`: `Device` already holds `epoch` (= generation), `spk`/`prev_spk`, `last_resort`/`prev_last_resort`,
  `rotate_signed_prekeys()` (fresh secrets + fresh ids, epoch+1, returns a bundle-only `Upload`), `expire_previous()` (drops the previous ledger),
  `accept_initial` accepts the previous epoch's spk and last-resort. Gaps: `opks`/`pqopks` are **not tagged with an epoch**, so retiring `g` cannot
  delete its one-time secrets; `signed_bundle()` uses the fixed pool `[0x11;16]`; no rotation timestamp is persisted (persistence keys 0..18 used).
- `crates/om-core/src/flows.rs` `maintain_prekeys` (~line 507) does one-time replenishment only; its doc comment says rotation is "not built".
  `st.app.keys_published`, `Purpose::PublishKeys` and the outbox give crash-retry; the Home PUT is idempotent by `request_id` fingerprint, and
  rejects a lower generation (`stale_generation`).
- `NetCall::PublishKeyMaterial` (host.rs), loopback in `runtime.rs` (bundle replaced, items appended: same as the Home), TS in
  `packages/om-provider-http/src/provider.ts publishKeys`.
- Tests today: `tests/prekeys.rs` (replenishment), `tests/secrets.rs`, `tests/w0_conformance.rs`, `crypto/selftest.rs` (rotation at crypto level).

## Plan (slices)

1. **Store:** tag one-time items with their epoch (new persistence key, absent = current epoch for old state), `rotation_at` (wall seconds) next to
   the epoch, `rotate_signed_prekeys` returns a full `Upload` (new bundle + a fresh one-time batch under `g+1`), `retire_previous(now)` deletes
   `prev_*`, the old ledger and every one-time secret of an epoch below the current one once `now > old bundle expires_at`.
2. **Flow:** `maintain_prekeys` gains the schedule (G1). Rotation is one durable state commit (new epoch secrets + the `PublishKeys` outbox entry
   in the same commit) before anything is sent; a crash replays the outbox; a retry re-sends the identical bytes (same request id); a failed
   publish never lowers the epoch and never lets a second rotation run while one is pending. Retirement (G2) runs from the same tick.
3. **Provider:** G3 (Home-side drop of lower-generation items, loopback mirrored, optional `RevokeKeyGeneration`), TS provider and wasm types if a
   `NetCall` is added.
4. **Tests (Rust):** generation strictly increases and ids are fresh; old-generation first message still accepted inside the overlap and rejected
   after retirement; last-resort ledger resets per epoch and the 4096 cap triggers rotation; crash after commit before publish replays;
   crash/failed publish twice does not double-rotate; clock going backwards does not rotate and never lowers the epoch; peer that saw `g+1`
   rejects a replayed `g` bundle; stale `g` items cannot break a `g+1` fetch (G3); restore creates a fresh DeviceID (existing K11 test stays green).
5. **Verify:** `cargo test -p om-core`; wasm conformance in `packages/om-core-wasm`; `pnpm exec vitest run src` in touched TS packages;
   `pnpm exec tsc -b apps/web`. Commit with `Design-Read: crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy,
   relay_reachability, time_ordering_events` only if the committing session read them itself, `Design-Departures:` citing the relay-status deviation
   for G3 if the Home behaviour changes.

## What remains

Everything above: no code was changed or committed except this report. The next session must read the six domains itself (about 140k tokens;
that is why this one ran out) or, if the operator accepts it, rely on this extract for the design questions and read the domains the hook demands
before committing. The operator's yes is wanted on G1 (numbers) and G3 (Home-side behaviour) before slice 3.
