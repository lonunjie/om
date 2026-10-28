# core_kernel Opus audit (after M1 pass 2)

- **Date:** 2026-10-28
- **Auditor:** Claude Opus 5.5, one operator-approved read-only pass. No code was changed.
- **Scope:** commits after `b879a40` up to `b6e32da`: `crates/om-codec`, `crates/om-core`, `crates/om-wasm`, `packages/om-core-wasm`, the `apps/web` wiring, and the release signing path ([`apps/web/build/sign.mjs`](../../apps/web/build/sign.mjs), [`release.mjs`](../../apps/web/build/release.mjs), [`verify-release.mjs`](../../apps/web/build/verify-release.mjs), `src/update/release-key.ts`, `src/sw/trust-root.ts`). Checked against [`SEALED_SENDER_DELIVERY_PROFILE.md`](../architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md), [`UI_CORE_PLATFORM_CONTRACT.md`](../architecture/ux/UI_CORE_PLATFORM_CONTRACT.md), [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) and the public release gates. Status report: [`docs/reports/2026-10-28-core-kernel-m1-status.md`](2026-10-28-core-kernel-m1-status.md).
- **Independence:** this is still an Anthropic review of Anthropic-written code. It does not satisfy ADR-0167's non-Anthropic crypto review.
- **Commands run:** `cargo test --workspace` and `make check core-check`. Both pass. `pnpm test` and the e2e suites were not run.

## Verdict

- **Public release: no.** B1 and B2 must be fixed first. H1 and H2 must be fixed, or closed by the scope decisions named below.
- **Labelled pre-release: acceptable with conditions.** Production has no provider adapter (`RealApp provider={null}`), so the network paths below cannot be reached from a production build today. A pre-release is acceptable if it is labelled "pre-release, not connected, no security claims", and only after H1 is fixed. H1 is about what gets signed, not about the network.
- **B1 must be fixed before any provider adapter ships, including in a pre-release.**
- **Fail-closed checks that hold up:**
  - Every `Rejected(unsupported)` intent is a pure rejection with no side effect.
  - The sealed-delivery acceptance order follows profile sections 7 to 9: replay lookup before key lookup, the candidate budget checked before `RESERVED`, the bootstrap key destroyed in the winning Hello's own transaction, and no clear-sender or bootstrap fallback on send.
  - The `test-loopback`/`debug_*` code is not compiled into the wasm artifact.
  - Entropy and clock failures trap rather than continue.

## Blocker

### B1. A forged AccountControlState successor verifies

**Where:**
- `crates/om-core/src/control.rs:544-552`: `open_signed` checks each signer against the credential committed by the **child** state.
- `control.rs:149-222` and `469-497`: `deltas`/`model_of` never compare `device_credentials` (ACS key 13).
- `control.rs:369`: signers are counted against the parent's controllers, but by device ID only.

**Failure scenario:**
1. An attacker takes any published chain for account A, for example the one carried in an invite descriptor.
2. They build a child state at generation +1. It replaces controller D's credential digest with a credential for a key the attacker holds and adds an attacker messaging device (`link_messaging`, rule One).
3. They sign it as D with that attacker key.
4. `open_signed` accepts the signature because the child commits to that credential. `authorize` sees D as a voting controller of the parent and returns `Accept("single active controller")`.

Anyone can therefore extend any account's chain. The inviter side calls `verify_chain` on the requester's chain without any external pin (`flows.rs:1630`) and pins it on `AcceptIntroduction`. The attacker can then appear as account A, with A's AccountID, through a device they control. Any future peer-ACS refresh would inherit the same hole.

**Spec:** [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) table row (line 155) says a device rotates its own credential "signing with its current key", meaning the key committed by the parent.

**Fix:**
1. In `verify_successor`, verify each signer against the credential committed by the **parent**.
2. In the model, treat any change to `device_credentials` as a delta. A change for an existing DeviceID is `rotate_own_key` (SelfOnly, and it needs a matching `control_key_epochs` bump). Removing a credential is `unlisted` unless the device is revoked. Adding one is allowed only with `link_messaging` or `promote` of that device.
3. Add negative vectors for credential swap, swap plus link, and swap without an epoch bump. Extend the V-002 enumeration to cover credentials.

### B2. Release trust reduces to one online key (public release gate PR-005)

**Where:** `apps/web/build/sign.mjs:26-32` (key read from the agent vault), `src/update/release-key.ts:4-5`, `build/verify-release.mjs:14`.

**Failure scenario:**
- One Ed25519 key, readable by any agent with the `lonunjie-agents` vault profile, signs every release. There is no threshold, no delegation, no rotation and no revocation path.
- Whoever compromises the vault or that agent host can ship a release that every installed service worker accepts.
- The browser also replaces `sw.js` itself without checking the manifest signature, so an origin compromise bypasses the signature entirely. That is a known PWA limit, but it rules out any "signed updates" claim.

AGENTS.md forbids shipping a public binary "whose trust reduces to ... one online update key". `PR-005` (TUF-style metadata, 2-of-3 root custody) is `open`.

**Fix:**
1. Implement PR-005: an offline root, a delegated online targets key and a key-rotation manifest field.
2. Until then, the web release must not make signed-update or tamper-resistance claims.
3. Move the private key out of the agent vault into operator-held custody.

## High

### H1. The production release path can sign a test, harness or loopback build

**Where:**
- `apps/web/build/release.mjs:44-46`: `vite build` inherits `process.env` as-is.
- `src/main.tsx:14-17`: the bundle content is chosen by `VITE_OM_DEV_HARNESS` / `VITE_OM_E2E_CORE`.
- [`release.mjs`](../../apps/web/build/release.mjs) does not run [`check-no-harness.mjs`](../../apps/web/build/check-no-harness.mjs), [`ensure-core-wasm.mjs`](../../apps/web/build/ensure-core-wasm.mjs), [`tools/core_guard.py`](../../tools/core_guard.py) or a fresh wasm build.
- `packages/om-core-wasm/build.mjs:12` never enables `om-core/production`, so the `compile_error!` guard in `crates/om-core/src/lib.rs:10-16` is never exercised by the shipped artifact.

**Failure scenario:**
- A shell or CI environment with `VITE_OM_DEV_HARNESS=1` (the `build:harness` script sets no `OM_BUNDLE_DEV`) or `VITE_OM_E2E_CORE=1` runs `release.mjs --sign-from-vault`.
- The output is not `dev`, so the release is signed as production and contains the harness or the in-page TEST loopback provider.
- A stale `dist/om_wasm.wasm` is also reused without a rebuild or marker scan.

The "impossible to ship in a production configuration" rule (R-072) therefore holds only by convention.

**Fix:**
1. [`release.mjs`](../../apps/web/build/release.mjs) builds with an allowlisted environment that strips every `VITE_OM_*` variable and `OM_BUNDLE_DEV`.
2. It rebuilds the wasm with `--features om-core/production`.
3. Before signing, it runs `check-no-harness.mjs <site>` and the `core_guard` marker scan on the wasm asset in `site/`.
4. `vite.config.ts` throws when `mode === "production"`, a test flag is set and `OM_BUNDLE_DEV` is unset.
5. Add `debug_inject` and `LoopbackNetwork` to the marker list in [`tools/core_guard.py`](../../tools/core_guard.py).

### H2. First contact is not the specified IntroGate object set, and the inviter side has no anchor

**Where:** `flows.rs:1295` (the request key is trusted through the link digest), `flows.rs:1570-1684` (inviter side).

**What holds:**
- The invitee side does not fail open: the link pins the ACS head hash and the request-key digest, and both are checked (`flows.rs:1288-1297`).

**What fails:**
- **Conformance:** [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) section 5.1 requires the request key to come only from a ContactDescriptor signed by the intro identity. That is unbuilt, along with `IntroductionTicket`, the signed `MessageRequestPayload` and `IntroIdentityBinding`.
- **Inviter side:** the requester's identity rests only on its self-presented chain. Combined with B1, the requester's AccountID is forgeable.
- **Abuse (absolute rule 10):** each request costs an HPKE decapsulation plus up to 65 chain links × 16 dual-signature verifications before any admission control (`flows.rs:1599-1630`). The invite is marked used only after that work, so anyone holding the link can make an inviter's device burn CPU repeatedly.

**Fix:**
1. Build the IntroGate set, or record an ADR that allows a link-committed request key.
2. Cap the chain length and signature count accepted at first contact (for example genesis plus a few links).
3. Rate-limit per invite before HPKE.
4. Display the requester as unverified until a binding exists.

## Medium

- **M1. The route binding is a stand-in** (`crates/om-core/src/crypto/delivery.rs:225,275-277`, `flows.rs:1747,1758`).
  - Problem: `route_binding` uses the fixed string `om-core:mailbox/v0` instead of the spec's `relay_uri`. A delivery key is therefore bound to a mailbox ID at *any* relay, which contradicts the "never reuse a capability across relays" rule. Changing this later alters a signed, wire-visible commitment, which breaks deployed relationships.
  - Fix: bind to the real `RelayEntry.relay_uri` before the first provider adapter ships, and version the binding if a stand-in ever reaches users.
- **M2. Confirmation tokens are not checked** (`flows.rs:336`, `api.rs:146`).
  - Problem: any non-null JSON value (`false`, `0`, `""`) satisfies UI-11 for `RevokeDevice`, `SuspendDevice` and `BlockPerson`.
  - Fix: issue single-use confirmation nonces from Core, bound to `(intent kind, target, profile)`, and reject anything else as `Rejected(unauthorized)`.
- **M3. Secret lifetime.**
  - Problem: request and delivery-key private seeds are serde `String` hex (`state.rs:37,72`) and are never zeroized. Every commit hands the whole profile record, device secrets included, to JavaScript as a hex string (`state.rs:328-331`, `host.rs:54-58`), where it cannot be wiped. Bootstrap snapshots (`flows.rs:1822`) and the `dev.clone()` copies are not wiped either.
  - Fix: use `Zeroizing<Vec<u8>>` or `[u8; 32]` with a zeroizing serde wrapper. Longer term, encrypt the record inside Core with a vault-supplied key handle so plaintext secrets never cross the ABI.
- **M4. Bootstrap key lifetime is not capped** (`flows.rs:1638`).
  - Problem: a requester's reply/bootstrap key is accepted with an expiry of up to `DELIVERY_KEY_TTL_S` (90 days), not `BOOTSTRAP_TTL_S` (7 days). Profile section 7 makes the request deadline `min(route, key)`, so a hostile requester can stretch the bootstrap window 13 times.
  - Fix: cap the expiry at `now + BOOTSTRAP_TTL_S`.
- **M5. `settledOutcome` never settles while an operation is pending** (`packages/om-ui/src/conversations/services.ts:27-42`; used at `onboarding/InviteFirst.tsx:42,63`).
  - Problem: in production with no provider, `CreateInvite` and `AcceptInvitation` stay `PendingNetwork`, so the UI stays busy forever and the observer leaks. This is safe (it never reports success) but is a dead end the user cannot leave.
  - Fix: add a timeout or abort signal, show a "queued, will send when connected" state, and unsubscribe on unmount.
- **M6. A rejected inner payload still advances the established ratchet** (`flows.rs:1822,1845-1848`).
  - Problem: the device snapshot is restored only on the bootstrap path. On an established session, an authenticated but invalid event consumes ratchet state and is marked `REJECTED`. That is acceptable, but nothing in the code documents that choice, and it differs from the bootstrap path.
  - Fix: decide which behavior is intended, document it, and add a test.

## Low

- **L1.** The ACS parser rejects every key outside 0..=14 (`binding.rs:194-199`). This is correct for key 23 (critical extensions) and fails closed. It also rejects any future *non-critical* field, so a later ACS extension would cut contacts off rather than degrade. Record this as an intended v1 strictness in the spec, or skip unknown non-critical keys.
- **L2.** Unknown inner payload kinds are accepted as `true` and marked `COMPLETE` (`flows.rs:1843`). That is harmless today because Core sends no receipts. Before receipts ship, an unknown kind must not produce a `Delivered` receipt.
- **L3.** `release.mjs --signature <file>` puts an external signature into the manifest without verifying it against the pinned key, and the release still reports "signed". The `verify.mjs` README step does not pass `--require-signed`. Fix: run `verifyRelease(dir, {requireSigned: true})` inside [`release.mjs`](../../apps/web/build/release.mjs), and make require-signed the default for non-dev releases.
- **L4.** The release manifest signature covers canonical JSON, not OM-CBOR. Producer and verifier canonicalize the same way and the payload is distribution metadata, not a wire object, so it is acceptable. Replace it with TUF metadata under PR-005 (B2).
- **L5.** `connect-src 'self' https: wss:` is broad by design (providers are chosen by the user). Trusted Types (`require-trusted-types-for 'script'`) and `script-src 'self' 'wasm-unsafe-eval'` limit the damage. Keep the CSP test pinned.
- **L6.** `debug_*` methods are gated only by `feature = "test-loopback"`, not by `cfg(test)`. That is correct given the dependency guard. The H1 marker additions make it auditable in the artifact.
- **L7.** `inbound` advances the mailbox cursor even for items that fail (`flows.rs:1520-1526`). This is intended, since replay and failures are recorded, but a refused host commit must re-deliver from the old cursor. Today it does, because the cursor lives in the same staged record. Keep a test for that.

## Not findings (checked)

- **Bootstrap budget.** At most 8 distinct candidate fingerprints are admitted. The ninth is rejected before `RESERVED`. Slots are never replenished while the bootstrap is open, because candidates and `REJECTED` replay records persist with the staged transaction. Exact retries do not consume a slot.
- **Key destruction.** `rel.bootstrap = None` runs in the same staged transaction as the winning Hello's route and key install (`flows.rs:1941-1947`). A later exact retry of the winning wrapper is answered from the replay `COMPLETE` record without the key. Non-winning wrappers are never left `RESERVED` across commits.
- **Peer delivery-key records.** They are verified against the pinned device credential and an active device in the pinned ACS. They are route-bound and their TTL is capped (`flows.rs:1866-1887`).
- **Send path.** There is no clear or bootstrap fallback (`flows.rs:298-309`).
- **ACS signer checks.** Signers must be active, non-quarantined parent controllers, apart from the B1 credential source. Duplicate signers fail. The signature count is capped at 16 and the chain at 64 links.
- **Wasm ABI.** `unsafe` code is confined to `abi.rs`. A missing or failing `random_fill` throws through wasm and traps. It never yields zero entropy.
- **Service worker update path.** It fails closed: an unsigned manifest is accepted only for `dev` under a dev trust root, and the version floor refuses unsigned rollback (`update/signature.ts:26-41`, `update/floor.ts`).

## Applied (2026-10-28, Sonnet 5.5 follow-up)

Each fix has a regression test that failed before the change (checked by running the test against the unfixed source). B2 was first left untouched (offline root and TUF key custody is an operator decision); its implementation followed the same day (see the B2 section below).

| Finding | Commit | Test (fails before the fix) | Notes |
|---|---|---|---|
| B1 | `83eac94` | `crates/om-core/tests/acs_authorization.rs`: `a_forged_credential_swap_is_not_a_valid_successor`, `credential_changes_are_rotate_own_key_only_and_need_an_epoch_bump`, `credentials_may_only_be_added_with_their_device_and_removed_with_its_revocation`; `crates/om-core/tests/acs_forgery.rs` drives the `flows.rs` inviter path with a forged successor of a published chain (honest chain listed, forged one refused) | Signers are verified against the PARENT's credentials. A changed `device_credentials` entry is `rotate_own_key` (device itself, strictly higher `control_key_epochs`), a removed one is unlisted unless the device is revoked, an added one is unlisted unless the device is added in the same step. The 63 golden vectors pass and the 16,200-case differential still agrees with the V-002 reference (`OM_ACS_EXHAUSTIVE`). The Python enumeration was not extended to credentials (the abstract model has none): coverage is the Rust tests above. |
| H1 | `d8c7e3e` | `apps/web/build/release.test.ts` ("production build hygiene"): polluted environment refused for four variables, vite refuses a production build with a test flag, pre-signing site checks, allowlisted environment | [`release.mjs`](../../apps/web/build/release.mjs) refuses to run when any `VITE_*` or `OM_*` variable is set, builds with an allowlisted environment, rebuilds the wasm with `om-core/production`, and runs `check-no-harness` and `tools/core_guard.py --wasm` on the site before signing. `vite.config.ts` throws on a production-mode build with `VITE_OM_DEV_HARNESS` or `VITE_OM_E2E_CORE` unless `OM_BUNDLE_DEV=1` (so `build:harness` now sets it). `debug_inject`, `debug_app_state`, `debug_device_digest`, `debug_link_messaging_device` and `LoopbackNetwork` joined the marker list. |
| M4 | `dc2efca` | `crates/om-core/tests/first_contact_limits.rs` | Requester bootstrap key capped at `BOOTSTRAP_TTL_S` plus 1 hour of wall-clock skew (`BOOTSTRAP_CLOCK_SKEW_S`). |
| M6 | `2f16ab4` | `sealed_delivery.rs`: `a_rejected_payload_on_an_established_session_does_not_advance_the_ratchet` (uses the test-only `debug_device_digest`) | Decision: a payload that is not accepted never changes cryptographic state, on every path. The device is snapshotted before decrypt and restored on rejection. |
| M3 | `43bd2a9` | `crates/om-core/tests/secrets.rs` | Delivery and request seeds are `SecretHex` (redacted `Debug`, wiped on drop, same JSON shape); working copies are `Zeroizing`. Not done: the profile record handed to the host still contains the seeds as hex (needs a vault-held record key inside Core). |
| M5 | `fc1d32a` | `packages/om-ui/src/conversations/services.test.ts`; `onboarding.test.tsx` ("invite_first without a provider") | `settledOutcome` takes a timeout (default 15 s) and an abort signal, resolves with the latest pending outcome, and unsubscribes. `InviteFirst` shows a "queued, nothing sent yet" status and aborts on unmount. |
| M2 (partial, superseded) | `0850762` | `crates/om-core/tests/devices.rs` | A confirmation token must be a JSON object. Superseded by the Core-issued nonces below (ADR-0181). |
| L3 | `3346bdb` | `release.test.ts` ("externally supplied signature") | [`release.mjs`](../../apps/web/build/release.mjs) verifies the signature against the pinned key (or the throwaway key's own public key) before reporting "signed"; deploy docs use `node verify.mjs --require-signed`. |
| L6 | `d8c7e3e` | `release.test.ts` (marker cases) | Marker list extended, see H1. |

### B2 applied (offline root + delegated online key)

Implemented the minimal TUF-style chain of PR-005 / ADR-0149. **Not closed until the operator runs the key ceremony** ([`KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md)): the root public key is a clearly marked, currently empty constant, so production releases fail and clients would reject any signed bundle (`root_unset`) by design. Dev builds are unaffected.

- **Client and `verify.mjs`:** pin only the offline root key(s) (`ROOT_KEYS`/`ROOT_THRESHOLD` in `src/update/release-key.ts`, `PINNED_ROOTS` in `build/verify-release.mjs`; a 2-of-N threshold is already honoured, distinct root key ids only). Every signed manifest embeds a root-signed canonical `delegation` (`src/update/delegation.ts`): `version` (monotonic), `expires`, `keys[{keyId,alg,pub,notBefore,notAfter}]`, `revoked[]`. Fail-closed order: root threshold, expiry, persisted delegation-version floor (`delegation-floor` in the worker's IndexedDB, next to the bundle floor), revoked, unlisted, algorithm, key window, manifest signature.
- **Tests (new; the old single-key tests were rewritten for the chain):** `apps/web/build/delegation.test.ts` runs 32 shared cases through both verifiers and requires identical verdicts and reasons (tamper, wrong root, below-threshold, duplicate roots, expiry, rollback, revoked, unlisted, window, alg); `src/update/verify.test.ts` covers the TS chain, floor persistence and 2-of-3; `build/release.test.ts` covers sign-refusal (missing/expired/unlisted/revoked/mismatched delegation), the unset-root refusal in [`release.mjs`](../../apps/web/build/release.mjs) and under `vite build` with `OM_RELEASE_BUILD`, and a full throwaway-root release. Tests use throwaway roots only.
- **Signing:** [`sign.mjs`](../../apps/web/build/sign.mjs) refuses without a valid delegation; [`release.mjs`](../../apps/web/build/release.mjs) embeds [`apps/web/release-delegation.json`](../../apps/web/release-delegation.json) (or `--delegation`) and verifies the whole chain before reporting "signed". The vault path for the online key is unchanged.
- **Tool:** [`apps/web/build/root-ceremony.mjs`](../../apps/web/build/root-ceremony.mjs) (`gen-root`, `show-root`, `sign-delegation`, `revoke`, `cosign`, `inspect`): pure node, key file must be mode 600, `gen-root` writes only to the named path outside any git work tree and prints only public data.
- **Registry:** `registry/signing.toml` marks `om-release-2026-10-28` as the delegated online key and adds `om-release-root` with `status = "pending-ceremony"`.
- **Still open (operator):** run the ceremony; point 3 of the finding (move custody of the signing key out of the agent vault) is unchanged, now bounded by revocation and the delegation expiry. Plain `pnpm build` (used by `web:check`) still succeeds without a root; only release builds fail. The SW-replacement limit (an origin compromise bypasses the manifest) is unchanged, so no "signed updates" claim yet.

### H2 and M2 applied (ADR-0181)

The operator delegated the seven open questions of H2 and the M2 contract change to the engineering agents on 2026-10-28; the decisions are [ADR-0181](../../decisions/ADR-0181-first-contact-intro-identity-and-confirmation-nonces.md) (D-202, R-411..R-417, T-164..T-166). Commits: ADR and ledgers `47098d9`; Core, codec and Rust tests `318d6bb`; platform API, harness, Action Registry, UI and the queue-while-unbound fix `ee42d2c`. Regression tests: `crates/om-core/tests/{first_contact,first_contact_limits,acs_forgery,confirmations}.rs`, `w0_conformance.rs`, `secrets.rs`; `packages/om-ui/src/actions/{registry,confirm}.test.ts`, `conversations.test.tsx`; `packages/om-core-harness/src/devices.test.ts`; `packages/om-core-wasm/src/core.test.ts` (real Core). Mutation checks (disabling the limiter, the chain cap, the nonce match, single use, the link digest check) each fail named tests. The 63 golden vectors and the 16,200-case differential are unaffected (`acs_authorization.rs`, run with `OM_ACS_EXHAUSTIVE`).

| Question | Built |
|---|---|
| Q1 intro identity | Per-invite composite Ed25519 + ML-DSA-65 key (role 4), HKDF-derived from a profile `intro_root` and the invite id, signs one descriptor and ticket (inviter) or one payload and descriptor (requester), expires with the invite. |
| Q2 request key | Link = `invite_id \|\| SHA-256(intro key)` (48 bytes). The request key is trusted only from a signed ContactDescriptor under that key, plus a ticket under the same key; substituted or partial records are `IntegrityFailure`. The descriptor carries no account material. |
| Q3 hidden account | The sealed payload carries only the intro key and an unverified label. `ACCEPTED_UNBOUND` until Accept, Bind and both `IntroIdentityBinding`s verify; Hello after both; messages composed meanwhile are queued and sent when ACTIVE. |
| Q4 ticket | Core issues and signs it (`max_uses` 1, 32 KiB payload limit, empty `admission_proof`). |
| Q5 limiter | Cheap checks (invite state, size, W0 shape, ticket and digest) then a persisted limiter, 20/hour and 100/day, all before any HPKE decapsulation; refused requests are silently dropped and aggregated in `GetIntroductionLoad`. |
| Q6 chain cap | 16 entries for a not-yet-bound peer's chain, refused before any signature check; local policy (`FIRST_CONTACT_MAX_CHAIN`). |
| Q7 verification | `IntroductionRequest.verification`, `ConversationSummary.relationship`, the `unverified_contact` notice, an "Unverified" label and a stranger-pressure notice in the UI. |
| M2 | `IssueConfirmation` / `GetConfirmation`: single-use, 5-minute nonces bound to (intent kind, target, profile); anything else is `Rejected(unauthorized)` and the nonce is spent even on a mismatch. The Action Registry asks Core in two steps and only for a direct user action. |

Findings and decisions to record:

- **Codec defect found and fixed.** Accept and Bind carry an ACS chain, which needs the 64 KiB sealed bucket, but `om-codec` applied the profile-wide 65536-byte string limit before the CDDL's own per-field bounds, so a 64 KiB bucket (ciphertext 65552) and any `MessageRequest` ciphertext above 65536 bytes could never decode. The string limit now follows the object's registered `max_bytes`. No registry or vector changed; the W0 corpus replays identically.
- **Conservative readings.** The intro identity signs no binding itself (the spec has a device sign it); the Accept is authenticated by the request-scoped key plus the device-signed binding bound to the intro key and request digest. The account the inviter's binding names is trusted on first use, as in any first contact. The limiter budget is per profile, not per invite, so a flood on one link also uses the budget for others (reserved invite-bound capacity is IntroGate work). A chain above the cap is refused even for a legitimate account until chain compaction exists. The limiter clock is the wall clock with a persisted non-decreasing high-water mark; an OS monotonic reading is not plumbed through the wasm ABI.
- **Migration.** App state version 3; version 2 profiles open and drop pending introductions, unused invites, non-active relationships and their outbox entries (`state.rs`, tested).
- **Still open:** blind-RSA referral tokens (IntroGate adapter), `ReferralAttestation`, a real `relay_uri` for the reply route (M1), per-invite reserved capacity.

### B2 root custody applied (2-of-3: two Ledgers + a personal-vault key), operator decision 2026-10-28

Still **not closed**: the operator's ceremony and the dependency approval are pending, and nothing was tested on a physical Ledger.

- **Policy:** `ROOT_THRESHOLD = 2` and `ROOT_KEY_COUNT = 3` (`release-key.ts`, `PINNED_ROOT_THRESHOLD = 2` in [`verify-release.mjs`](../../apps/web/build/verify-release.mjs)); `ROOT_KEYS` / `PINNED_ROOTS` stay `{}` until the ceremony. A pinned value is a hex key (frame `raw`) or `{ pub, frame }`; one public key pinned under two ids makes the pin unusable (`root_unset`).
- **Framing (pinned per key, never chosen by the signature):** `raw`, or `solana-offchain-v0`, the Ledger Solana app's off-chain message (header v0, format 0, restricted ASCII shown in clear, no blind signing) around `OM release delegation | version n | expires t | sha256 <delegation payload hash>`. A signature entry may declare `frame`; a declaration that differs from the pinned frame, an absent one for a framed key, and an unknown frame all count for nothing, and an unknown pinned frame fails closed. Exact layout, the source it was read from (`LedgerHQ/app-solana`) and the rejected alternatives: [`KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md) "Ledger framing".
- **Tests:** 18 new shared-fixture cases run through both verifiers (`delegation.test.ts`: wrong frame, mixed frames, 2-of-3, 1-of-3, duplicate entries counted once, unknown frames, duplicate pinned public key, tamper of version/expiry) plus a golden envelope vector from an independent Python implementation, and CLI end-to-end tests of the ceremony with a software stand-in for a Ledger.
- **Tooling:** [`root-ceremony.mjs`](../../apps/web/build/root-ceremony.mjs) gained `gen-root --to-stdin-for-vault` (PEM only as a Bitwarden item on stdout; refuses a terminal; public data on stderr), `--root-key-stdin`, `draft`, `external-payload`, `external-attach` (verifies before accepting), `pin-snippet`, `verify`. The isolated USB helper is [`deploy/ledger/`](../../deploy/ledger/) (`@ledgerhq/hw-app-solana` 7.12.0 and `@ledgerhq/hw-transport-node-hid` 6.33.5, Apache-2.0, `node-hid` 2.1.2 native; own pnpm root, not in the workspace, not bundled; build scripts off until the operator approves).
- **Runbook:** copy-pasteable steps with working directories, personal-vault login in a separate `BITWARDENCLI_APPDATA_DIR`, firmware/app checks, rotation, revocation and compromise cases: [`KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md). `registry/signing.toml` lists the three signers as `pending-ceremony`.
- **Unverified on hardware:** device acceptance and paging of the message, plain Ed25519 over the whole envelope (read from the app source; the ceremony verifies every signature locally and fails loudly otherwise), and the `node-hid` build on Node 24.

### B2 post-quantum co-signature applied (ADR-0182), operator decision 2026-10-28

Still **not closed**: neither ceremony has run. Adds a **required** ML-DSA-65 (FIPS 204) co-signature from one software PQ root key (seed only in the operator's personal Vaultwarden) to the delegation; the Ed25519 2-of-3 roots, `ROOT_KEYS`, `PINNED_ROOTS`, [`release-delegation.json`](../../apps/web/release-delegation.json) and [`KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md) are unchanged.

- **Rule:** valid only if the Ed25519 threshold is met AND, once required, a valid PQ signature from the pinned PQ root (algorithm tag `ml-dsa-65`, frame `om-pq-v1`, over `label | alg | SHA-256(pub) | delegationPayload`). Required = a root-signed `pqRequired` delegation, the persisted client floor, or the build flag. Until the epoch an Ed25519-only chain is accepted; the floor only rises. Reasons `pq_root_unset`, `pq_missing`, `pq_invalid`.
- **Code:** `src/update/{pq,pq-root}.ts` (new), `delegation.ts`, `signature.ts`, `floor.ts`, `sw/core.ts`, [`verify-release.mjs`](../../apps/web/build/verify-release.mjs) (on `node:crypto`, independent of the client's `@noble/post-quantum` 0.7.1), [`sign.mjs`](../../apps/web/build/sign.mjs), [`release.mjs`](../../apps/web/build/release.mjs); ceremony commands in [`root-ceremony.mjs`](../../apps/web/build/root-ceremony.mjs) and [`pq-ceremony.mjs`](../../apps/web/build/pq-ceremony.mjs); runbook [`KEY-CEREMONY-PQ.md`](../../deploy/KEY-CEREMONY-PQ.md).
- **Tests:** `apps/web/build/pq-cosign.test.ts` (56): 37 shared-fixture cases through both verifiers (valid, missing PQ, wrong PQ key, tampered payload, downgrade and rollback, wrong alg tag, 2-of-3 Ed25519 without PQ, unknown tags and pins), golden signed bytes from an independent Python build, noble/OpenSSL interop, the floor in the service-worker shell, the signing tool, ceremony commands, two full release builds. Mutation checks (dropping the epoch trigger or the floor write) fail named tests.
- **Dependency:** `@noble/post-quantum` 0.7.1, MIT (plus `@noble/curves`, `@noble/ciphers` 2.4.0), not independently audited (self-audit at 0.6.1); mitigated by the second implementation. Re-check before any public release.

### Not changed, with reasons

- **B2:** implemented, but not closed: the operator's key ceremony is still pending (see "B2 applied" above).
- **H2:** applied, see "H2 and M2 applied (ADR-0181)" below.
- **M1 (route binding):** documented, not changed. The spec defines `route_binding = SHA-256(["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, deposit_capability])` from a `RelayEntry`, but Core has no `RelayEntry`/`RelaySet` and the provider contract exposes no relay URI, so the real value cannot be computed yet. It must land with `RelaySet` and before the first provider adapter; the stand-in `om-core:mailbox/v0` must be versioned if it ever reaches users. **Applied 2026-10-29:** Core now binds to the adapter's allocated `RelayEntry`; the stand-in survives only as an explicit legacy entry written by the app-state 3 -> 4 migration (see "Route binding adopted" in [`2026-10-28-core-kernel-m1-status.md`](2026-10-28-core-kernel-m1-status.md)).
- **M2 (rest):** applied, see below.
- **L1:** intended v1 strictness, recorded here: the ACS parser rejects keys outside 0..=14, so a future non-critical ACS field needs a spec-defined skip rule before it is added.
- **L2:** no change needed today (Core sends no receipts). Condition stays: an unknown inner kind must not yield a `Delivered` receipt once receipts ship.
- **L7:** already covered by `scenarios.rs::a_failed_host_commit_releases_nothing`.
- **L4, L5:** accepted as written (L4 is replaced by TUF metadata under PR-005 with B2).
