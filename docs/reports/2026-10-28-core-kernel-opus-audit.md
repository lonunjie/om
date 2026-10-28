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

Each fix has a regression test that failed before the change (checked by running the test against the unfixed source). B2 was deliberately not touched (offline root and TUF key custody is an operator decision).

| Finding | Commit | Test (fails before the fix) | Notes |
|---|---|---|---|
| B1 | `83eac94` | `crates/om-core/tests/acs_authorization.rs`: `a_forged_credential_swap_is_not_a_valid_successor`, `credential_changes_are_rotate_own_key_only_and_need_an_epoch_bump`, `credentials_may_only_be_added_with_their_device_and_removed_with_its_revocation`; `crates/om-core/tests/acs_forgery.rs` drives the `flows.rs` inviter path with a forged successor of a published chain (honest chain listed, forged one refused) | Signers are verified against the PARENT's credentials. A changed `device_credentials` entry is `rotate_own_key` (device itself, strictly higher `control_key_epochs`), a removed one is unlisted unless the device is revoked, an added one is unlisted unless the device is added in the same step. The 63 golden vectors pass and the 16,200-case differential still agrees with the V-002 reference (`OM_ACS_EXHAUSTIVE`). The Python enumeration was not extended to credentials (the abstract model has none): coverage is the Rust tests above. |
| H1 | `d8c7e3e` | `apps/web/build/release.test.ts` ("production build hygiene"): polluted environment refused for four variables, vite refuses a production build with a test flag, pre-signing site checks, allowlisted environment | [`release.mjs`](../../apps/web/build/release.mjs) refuses to run when any `VITE_*` or `OM_*` variable is set, builds with an allowlisted environment, rebuilds the wasm with `om-core/production`, and runs `check-no-harness` and `tools/core_guard.py --wasm` on the site before signing. `vite.config.ts` throws on a production-mode build with `VITE_OM_DEV_HARNESS` or `VITE_OM_E2E_CORE` unless `OM_BUNDLE_DEV=1` (so `build:harness` now sets it). `debug_inject`, `debug_app_state`, `debug_device_digest`, `debug_link_messaging_device` and `LoopbackNetwork` joined the marker list. |
| M4 | `dc2efca` | `crates/om-core/tests/first_contact_limits.rs` | Requester bootstrap key capped at `BOOTSTRAP_TTL_S` plus 1 hour of wall-clock skew (`BOOTSTRAP_CLOCK_SKEW_S`). |
| M6 | `2f16ab4` | `sealed_delivery.rs`: `a_rejected_payload_on_an_established_session_does_not_advance_the_ratchet` (uses the test-only `debug_device_digest`) | Decision: a payload that is not accepted never changes cryptographic state, on every path. The device is snapshotted before decrypt and restored on rejection. |
| M3 | `43bd2a9` | `crates/om-core/tests/secrets.rs` | Delivery and request seeds are `SecretHex` (redacted `Debug`, wiped on drop, same JSON shape); working copies are `Zeroizing`. Not done: the profile record handed to the host still contains the seeds as hex (needs a vault-held record key inside Core). |
| M5 | `fc1d32a` | `packages/om-ui/src/conversations/services.test.ts`; `onboarding.test.tsx` ("invite_first without a provider") | `settledOutcome` takes a timeout (default 15 s) and an abort signal, resolves with the latest pending outcome, and unsubscribes. `InviteFirst` shows a "queued, nothing sent yet" status and aborts on unmount. |
| M2 (partial) | `0850762` | `crates/om-core/tests/devices.rs` | A confirmation token must be a JSON object; `false`, `0`, `""`, `[]` and strings are `Rejected(unauthorized)`. Core-issued single-use nonces bound to intent, target and profile are not done (see below). |
| L3 | `3346bdb` | `release.test.ts` ("externally supplied signature") | [`release.mjs`](../../apps/web/build/release.mjs) verifies the signature against the pinned key (or the throwaway key's own public key) before reporting "signed"; deploy docs use `node verify.mjs --require-signed`. |
| L6 | `d8c7e3e` | `release.test.ts` (marker cases) | Marker list extended, see H1. |

### Not changed, with reasons

- **B2:** operator decision (offline root and key custody). Untouched.
- **H2:** `blocked-item`. The specification does not fully define the intro-identity lifecycle, the ticket issuer, the rate-limit numbers or the pre-acceptance disclosure rule. Open questions: [`2026-10-28-h2-first-contact-open-questions.md`](2026-10-28-h2-first-contact-open-questions.md).
- **M1 (route binding):** documented, not changed. The spec defines `route_binding = SHA-256(["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, deposit_capability])` from a `RelayEntry`, but Core has no `RelayEntry`/`RelaySet` and the provider contract exposes no relay URI, so the real value cannot be computed yet. It must land with `RelaySet` and before the first provider adapter; the stand-in `om-core:mailbox/v0` must be versioned if it ever reaches users.
- **M2 (rest):** needs a contract decision. `ConfirmationToken` is documented as minted only by the Action Registry (an opaque object, tracked UI-side). Core-issued nonces change that contract (new query and intent plumbing in `om-platform-api`, the harness and the UI).
- **L1:** intended v1 strictness, recorded here: the ACS parser rejects keys outside 0..=14, so a future non-critical ACS field needs a spec-defined skip rule before it is added.
- **L2:** no change needed today (Core sends no receipts). Condition stays: an unknown inner kind must not yield a `Delivered` receipt once receipts ship.
- **L7:** already covered by `scenarios.rs::a_failed_host_commit_releases_nothing`.
- **L4, L5:** accepted as written (L4 is replaced by TUF metadata under PR-005 with B2).

