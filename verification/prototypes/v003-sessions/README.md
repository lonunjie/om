# V-003 one-to-one session manager model

Feasibility prototype for V-003 (`one_to_one_session_model`). Not product code. Not for production.

Written by a Claude agent; the design package was authored by a ChatGPT design agent and Claude agents; ADR-0117
(proposed) decides whether an AI reviewer counts as independent. **No gate is claimed passed.**

## What it is

The per-device 1:1 session manager is the state machine that sits above the ratchet ([`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../../../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md),
Q-050 in [`ARCHITECTURE_RESOLUTIONS_20260923.md`](../../../docs/reviews/ARCHITECTURE_RESOLUTIONS_20260923.md)). It is modelled twice, independently, from the docs
([`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../../../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md), [`MULTI_DEVICE_SYNC.md`](../../../docs/architecture/crypto/MULTI_DEVICE_SYNC.md), [`ASYNC_KEY_MATERIAL.md`](../../../docs/architecture/crypto/ASYNC_KEY_MATERIAL.md), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md),
[`CRYPTOGRAPHY_AND_AGILITY.md`](../../../docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) "Downgrade floor"):

| Artifact | Role |
|---|---|
| [`verification/tla/OneToOneSessions.tla`](../../tla/OneToOneSessions.tla) | TLA+ model, checked by the pinned TLC 1.7.4 |
| [`verification/tla/models-v003.json`](../../tla/models-v003.json) | every config and its expected outcome (`make tlc` checks them) |
| [`sessions.py`](sessions.py) | Python reference machine, written from the docs, not from the TLA+ |
| [`fuzzer.py`](fuzzer.py) | replays every vector on [`sessions.py`](sessions.py) step by step; hypothesis fuzzing on a larger topology |
| [`tlc_runs.py`](tlc_runs.py) | generates the `OneToOneSessions*.cfg` configs (`tlc_runs.py gen`), runs them, writes vectors |
| [`results.py`](results.py) | writes [`verification/results/v003-sessions-latest.json`](../../results/v003-sessions-latest.json) |

Covered behaviours: crossed initiation (both sides initiate), device link and revoke races, stale sessions and
authenticated reset, duplicate delivery and dedupe, crash with partial persistence, backup restore, weak or depleted
prekey bundles (downgrade floor), and a provider that advertises a rogue device.

Bounds: TLC uses account A = {a1}; account B = {b1 control, b3 linked then revoked}; 2 or 3 messages; at most 4
sessions; at most one crash, restore, duplicate delivery, weak bundle and rogue device. The fuzzer uses 2 + 3 devices,
link/revoke on both accounts, 5 messages, 12 sessions, 2 crashes, 1 restore and 3 duplicates.

## Invariants

S1 `NoDoubleAccept`, S2 `NoKeyReuse` (per session, per direction, per counter), S3 `NoSendToIneligible` (only devices
in the validated AccountControlState), S4 `NoAcceptFromKnownRevoked`, S5 `NoSilentDowngrade`. At quiescence:
C1 `DeliveredAtQuiescence` (unless deliberately refused, or the sender or recipient is no longer authorized) and
C2 `SamePreferredAtQuiescence` (undisturbed authorized pairs prefer the same session). The probe
`NoBlackHoleAtQuiescence` is a design question, so it is not in the passing configs.

## Session-manager rules (K1..K12) and the design gaps they expose

The docs state most of these rules only as outcomes, or not at all. With every rule on, all invariants hold. Turning
off any one rule gives a TLC counterexample, which becomes a regression vector.

| Rule | What the manager must do | Violated without it | Doc status |
|---|---|---|---|
| K1 PersistBeforeSend | ratchet/outbox state durable before an envelope leaves | S2 (crash → key reuse) | gap: mechanism unstated |
| K2 DedupeDurable | dedupe record + session state durable before the relay ack | S1 | gap: mechanism unstated |
| K3 FloorFromACS | the downgrade floor also derives from the ACS minimum profile | S5 (restore rolls the floor back) | gap |
| K4 RetryOnReset | re-encrypt the message named by an authenticated reset | C1 (message silently lost) | gap |
| K5 RecheckOnEncrypt | re-check eligibility at encrypt time (fanout, retries) | S3 | partially stated |
| K6 ValidateAgainstACS | recipients come from ACS, never a provider list | S3 | stated (negative control) |
| K7 ReconcileControlHead | a peer's validated control head is durable across crash/restore | S3 (revoked peer re-authorized) | partially stated |
| K8 RestoreDropsSessions | restore never restores ratchet state | S2 | stated (negative control) |
| K9 RejectFromRevoked | reject traffic from a device known to be revoked | S4 | partially stated |
| K10 PrekeyUntilConfirmed | every message on an unconfirmed session carries the prekey header | C2 | gap |
| K11 RestoreRotatesPrekeys | backups exclude prekey secrets; a restored device publishes fresh prekeys | S2 | gap (**new, found by the fuzzer**) |
| K12 BindInitiator | accept a new inbound session only if the initiation's identity key is the sender's committed credential key (ADR-0136 rule 4) | S6 (session attributed to a real device on an attacker's key) | gap (**found by review F-4**, now normative) |
| P1 probe | a peer that lost a session is repaired only on the next send | black hole until then | gap (lazy repair) |

## Failing traces found (all captured as vectors)

- **FOUND-01..03** (see [`found_traces.json`](found_traces.json)): model bugs fixed during development (key log keyed
  without direction; a newly linked device not holding the state that linked it; a prekey message overtaken by a
  later message, which led to K10).
- **K11, found by hypothesis with K1..K10 on:** b2 restores (its ratchet state is dropped, per K8). The relay then
  re-delivers b1's original prekey message. b2 re-derives the same session from its pre-restore prekey secrets and
  sends at counter 1 again, so a key is reused. TLC confirms it ([`OneToOneSessions_K11.cfg`](../../tla/OneToOneSessions_K11.cfg), 11 steps).
- **Below-floor rejection is silent** (fuzzer, K3 off): a2 initiates a weak session before learning the pair's
  floor, and a1 rejects it with no signal to the sender, so the message is lost. K3 hides this only because the ACS
  minimum equals the strongest modelled profile. If a pair has established a profile above the ACS minimum, the
  race comes back.
- Two model artifacts were fixed in this run: C1 and C2 now exempt revoked devices, because a revoked sender's unsent
  outbox is rejected by K9 by design.

Vectors: [`verification/vectors/one-to-one-session-regression-vectors.json`](../../vectors/one-to-one-session-regression-vectors.json)
(found + TLC traces, each replayed step for step on [`sessions.py`](sessions.py)) and [`fuzzer_vectors.json`](fuzzer_vectors.json)
(hypothesis-shrunk traces per disabled rule).

## ADR-0142/ADR-0143 sealed-delivery state pass

[`sealed_delivery_state.py`](sealed_delivery_state.py) is a focused, dependency-free V-003 model for the established-relationship sealed-delivery state added by ADR-0142 and corrected by ADR-0143. It is deliberately **not** HPKE evidence; V-001 owns primitive/KAT execution.

The pass exercises 15 deterministic scenarios covering per-device/per-route delivery-key selection and overlap, expiry, route mismatch, no clear-sender fallback, exact same-route retry, route migration with a fresh delivery id, EventID dedupe, invalid inner authentication, and crash/persistence boundaries around the replay ledger. It also runs five mutation controls.

The pass found one real ambiguity in the original ADR-0142 prose: treating every durable replay reservation as terminal loses a message if the receiver crashes after reservation but before inner durable acceptance. ADR-0143 resolves this by making the local replay record `RESERVED` / `COMPLETE` / `REJECTED`: only the exact same wrapper may resume `RESERVED`; `COMPLETE` is idempotent; a same-key/id different-wrapper collision is rejected; and inner K2 session/dedupe acceptance plus `COMPLETE` commit crash-consistently before application release/ack. The M1 mutation deliberately restores the lossy behavior and is caught.

Current focused result: **15/15 scenarios pass; 5/5 deliberate mutations are caught**. Canonical evidence is [`verification/results/v003-sealed-delivery-latest.json`](../../results/v003-sealed-delivery-latest.json) with compact cases in [`verification/vectors/sealed-delivery-session-vectors.json`](../../vectors/sealed-delivery-session-vectors.json).

### ADR-0144 Run-2 composition evidence

[`../../bounded/sealed_delivery_adr0144_composition.py`](../../bounded/sealed_delivery_adr0144_composition.py) is the focused closure harness for the request-scoped acceptance-reply bootstrap key and the stale-ACS × replay-state composition added by ADR-0144. It covers valid request registration plus wrong route binding, CryptoProfile mismatch, key expiry beyond the reply route, delivery-key-id reuse, key-pair reuse, cross-request transplant, stable-authority promotion and retirement after bootstrap completion. It also composes `RESERVED` with stale authenticated peer-head refresh for both eventual accept and eventual reject, and verifies that a same-key/id different-wrapper collision cannot displace the original resumable wrapper.

Current Run-2 result: **12/12 scenarios pass; 7/7 deliberate security mutations are caught**. In particular, the mutation that maps temporarily stale/unknown relationship state to terminal `REJECTED` makes the byte-identical retry terminal and is detected. Canonical result: [`verification/results/sealed-delivery-adr0144-composition-latest.json`](../../results/sealed-delivery-adr0144-composition-latest.json). This satisfies the new repo-local ADR-0144 V-003 criterion; independent review remains separate.

## Criteria

| Criterion | Status | Evidence |
|---|---|---|
| executable model/fuzzer covering crossed initiation, add/revoke races, stale/reset, dedupe, persistence | met (bounded) | TLA+ + [`sessions.py`](sessions.py) + [`fuzzer.py`](fuzzer.py); results JSON |
| every discovered failing trace captured as a vector | met | vectors files above |
| ADR-0142/0143 sealed-delivery key/replay/retry/no-downgrade state semantics | met (bounded) | [`sealed_delivery_state.py`](sealed_delivery_state.py); 15/15 scenarios, 5/5 mutation controls |
| ADR-0144 request-scoped bootstrap + stale-ACS/`RESERVED` composition | met (bounded) | [`../../bounded/sealed_delivery_adr0144_composition.py`](../../bounded/sealed_delivery_adr0144_composition.py); 12/12 scenarios, 7/7 mutation controls |
| model reviewed by someone other than its author | unmet | needs an independent reviewer |

## Run

```sh
verification/prototypes/v003-sessions/run.sh          # ~4 min on 8 cores (TLC ~3.5 min, fuzz 15 s)
verification/prototypes/v003-sessions/run.sh --long   # 20,000 all-rules schedules; +30 s
```

Needs Java 21, `uv` (Python 3.14, hypothesis 6.140.2 pinned in `uv.lock`). Output is deterministic: TLC gap runs use
one worker (BFS, shortest trace), hypothesis is derandomized, and the results JSON contains no timings or paths.

Limits: the backup-restore pass config is bounded to backups on a1 with 3 sessions. With backups on all devices it
did not terminate and filled the disk. A pass config that combines restore with redelivery and 3 messages
(`pass-restore-redelivery`, MaxSid 2) did not finish in 3 minutes and was dropped. K11 is therefore covered by its
TLC gap config and by the fuzzer (restore + 3 duplicates, 20,000 schedules clean in `--long`), not by an exhaustive
TLC pass run.

## Proposed design updates

1. [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../../../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md): make K1, K2, K4, K5, K7, K9, K10 and K11 normative rules (they are currently
   outcomes or unstated), each with its vector as the conformance test.
2. Backup/restore ([`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../../../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) §7, [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md)): backups must not
   contain handshake prekey secrets, and a restored device must publish fresh prekeys and answer prekey messages for
   old prekeys with an authenticated reset (K11).
3. [`CRYPTOGRAPHY_AND_AGILITY.md`](../../../docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) "Downgrade floor": say where the floor survives restore (K3). A receiver that rejects
   a session below the floor must send an authenticated reject/reset naming the message so the sender can retry.
4. Decide whether lazy repair of a lost session (P1) is acceptable, or whether a restored/crashed device must
   proactively announce lost sessions.
5. Register the new files in [`machine/file_registry.json`](../../../machine/file_registry.json) (not done here: this prototype does not edit `machine/`).

## 2026-09-25: restore + redelivery and the R-206 floor

- [`restore_redelivery.py`](restore_redelivery.py) is the dedicated restore scenario. It uses ADR-0131 semantics
  (`World.restore_as`): a restore enrolls a **fresh DeviceID** through the account's control chain, and delivery checks
  get no exemption. The older same-DeviceID `restore` exempted every intended delivery. The script explores every
  bounded interleaving of backup, loss, restore, late learning, redelivery of the old device's queue to the restored
  installation, duplicate delivery and a crash: 13,378 states, no violation. A scripted golden trace delivers the
  post-restore message exactly once.
- **Finding (for review):** the fresh installation can already be receiving when the backup import runs. The import
  must therefore **merge**: union of the dedupe history, max of floors and control heads. An overwrite loses a message
  the installation had already accepted. The mutation config catches this. No document states it yet.
- **Residual:** messages sent only to the lost device between its backup and the sender learning of the restore are
  not re-encrypted to the new DeviceID. They reach it only through history sync from another device of the account.
- [`floor.py`](floor.py) writes [`one-to-one-floor-vectors.json`](../../vectors/one-to-one-floor-vectors.json): 14 vectors
  driven by the real CryptoProfile registry, 6 of them negative. They include a peer profile below the relationship's
  security_generation, and a SecurityAdvisory move to a lower generation (R-206).
