# ADR-0182 — Post-quantum (ML-DSA-65) co-signature on the release-root delegation

- **Status:** Accepted by operator delegation 2026-10-28
- **Date:** 2026-10-28
- **Amends (release trust only, no wire change):** [`ADR-0149`](ADR-0149-promoted-v1-adversarial-hardening.md) and public release gate PR-005 as built in `apps/web` (the root-signed `delegation` of audit finding B2, [`2026-10-28-core-kernel-opus-audit.md`](../docs/reports/2026-10-28-core-kernel-opus-audit.md)), and the 2-of-3 root custody decision of 2026-10-28 (the Ed25519 roots are unchanged)
- **Does not amend:** OM wire formats, OM-Crypto-v1, any CryptoProfile, the Ed25519 verification of a chain that carries no PQ material, [`KEY-CEREMONY.md`](../deploy/KEY-CEREMONY.md) (this ADR is an addendum to it, [`KEY-CEREMONY-PQ.md`](../deploy/KEY-CEREMONY-PQ.md)), the `ROOT_KEYS` / `PINNED_ROOTS` pins, or `apps/web/release-delegation.json`

## Context

The release root is three Ed25519 signers (two Ledger Nano S+, one software key in the operator's personal vault), 2 of 3 must sign a delegation that names the online release key. The three public keys are pinned in every client and live for years. A cryptographically relevant quantum computer would let an attacker derive an Ed25519 private key from its public key, so a root key pinned today could be forged later and the attacker could delegate to a key of their own and ship a malicious bundle. The harvest-now-decrypt-later argument does not apply to signatures, but root keys outlive that horizon and clients cannot be re-pinned out of band.

The operator decided on 2026-10-28 to add a post-quantum co-signature to the delegation, held by a **software** key kept only in the operator's **personal** Vaultwarden (never the agents' vault, never on disk), because no ML-DSA signing app for a hardware wallet is confirmed. As of 2026-10-28 no ML-DSA app is confirmed for the Nano S+; the Ledger Solana app used for the Ed25519 roots signs Ed25519 only. Re-verify against Ledger's current documentation before acting on the future path in section 10.

OM already uses ML-DSA-65 (FIPS 204) in composite device and intro-identity keys (ADR-0131, ADR-0181), so this adds no new primitive (absolute rule 3), only a second use of a reviewed standard.

## Decision

### 1. Authority: a required additional signer, not a replacement

A delegation is valid only if the Ed25519 threshold (2 of the 3 pinned roots) is met **and** a valid ML-DSA-65 signature from the one pinned PQ root key is present. The PQ root cannot authorise anything alone, and the Ed25519 roots cannot authorise anything without it once it is required (section 5). Neither side can switch the other off.

### 2. Algorithm tag, pin descriptor and signed bytes

Pin descriptor (a new constant, `PQ_ROOT` in `apps/web/src/update/pq-root.ts`, mirrored as `PINNED_PQ_ROOT` in `build/verify-release.mjs`; `null` until the PQ ceremony):

```json
{ "keyId": "pq-root-<first 16 hex of SHA-256(pub)>", "alg": "ml-dsa-65", "pub": "<1952 bytes, 3904 lowercase hex>", "frame": "om-pq-v1" }
```

- `alg` is the **algorithm tag** (`ml-dsa-65`, FIPS 204 parameter set ML-DSA-65). It and `frame` are pinned with the key. A signature entry declares them too, but the declaration only has to **equal** the pin; it never selects anything. A descriptor with an unknown `alg`, an unknown `frame`, a malformed `keyId` or a public key that is not exactly 1952 bytes is **unusable**, and an unusable pin fails closed (`pq_root_unset`) for every delegation, required or not. Unknown tags are never "skipped".
- The signature entry lives in the delegation's existing `signatures` array: `{ "keyId": "<pin keyId>", "alg": "ml-dsa-65", "frame": "om-pq-v1", "sig": "<base64, 3309 bytes>" }`. That array is outside the Ed25519 signed bytes, so the PQ signature can be added **after** the Ed25519 ones without invalidating them, and an older Ed25519-only verifier ignores the entry (it is not a pinned Ed25519 id). The `frame` of a PQ entry is mandatory.
- **Signed bytes** (frame `om-pq-v1`; both verifiers build them byte for byte, golden vector from an independent Python implementation in `pq-cosign.test.ts`):

  ```
  "om.release-delegation.pq/1" 00 | "ml-dsa-65" 00 | SHA-256(raw PQ public key) (32 bytes) | delegationPayload
  ```

  `delegationPayload` is the canonical JSON of the whole delegation with `signatures` set to null, exactly the bytes the Ed25519 roots sign, so the PQ signature commits to every key, window, revocation, version, expiry and the `pqRequired` marker. The label separates it from every Ed25519 frame (those start with `0xff` or `{`); the algorithm tag and the key fingerprint bind it to this algorithm and this pinned key, so a signature cannot be replayed under another key or tag. ML-DSA's own context string is left empty (pure `ML-DSA.Sign`); the domain separation is in the message, so any conforming implementation interoperates. Signing may be hedged (random); verification does not depend on it.

### 3. Sizes

Public key 1952 bytes (3904 hex, pinned in the client source), signature 3309 bytes (4412 base64 characters, carried in every release's delegation, so each `bundle-manifest.json` grows by about 4.4 KB), seed (the private key) 32 bytes.

### 4. The two verifiers are independent implementations

The client (`src/update/pq.ts`) uses `@noble/post-quantum` (section 11). `build/verify-release.mjs`, which ships inside every release and must stay dependency-free, uses `node:crypto` ML-DSA-65 (OpenSSL; needs Node 24 with ML-DSA, tested on 24.14.0; a runtime without it verifies nothing, so it fails closed). Two unrelated implementations agree on every shared fixture, which is the practical check on the noble library's audit status. The ceremony tool signs with noble and refuses to write a signature that does not also verify under `node:crypto` and the production verifier.

### 5. Verifier rule and order

`verifyDelegation` order: Ed25519 pin usable (`root_unset`), PQ pin usable (`pq_root_unset`), structure (`malformed_delegation`), Ed25519 threshold (`root_threshold`), expiry (`delegation_expired`), delegation-version floor (`delegation_rollback`), then the PQ checks:

- PQ is **required** when any of: the build says so (`PQ_REQUIRED_BY_BUILD` / `PINNED_PQ_REQUIRED`, section 6), the browser's persisted PQ floor is set, or the delegation carries `pqRequired: true`.
- Required and no usable PQ pin: `pq_root_unset`. Required and no entry for the pinned `keyId`: `pq_missing`. An entry for the pinned `keyId` exists but none verifies (wrong alg tag or frame, wrong key, tampered payload, truncated, corrupt): `pq_invalid`. This last case also applies when PQ is **not** required: a PQ signature from the pinned key that is present must verify.
- Entries under other key ids are ignored (which is how rotation works, section 9). The PQ signature never counts toward the Ed25519 threshold.
- `pqRequired` is optional and, if present, exactly `true`; any other value is `malformed_delegation`. It is part of the signed payload, so only the Ed25519 roots can set it, and the PQ root cannot be talked out of it by stripping it (the Ed25519 signatures would break).
- A chain without `pqRequired` and without a PQ pin, floor or build flag behaves exactly as before this ADR (`delegation.test.ts` and `verify.test.ts` pass unchanged apart from the new `FloorStore` methods).

### 6. Transition without a downgrade hole

1. **Until the epoch**, an Ed25519-only chain is accepted (clients that have not been updated cannot know about PQ, and the operator cannot sign a PQ-required delegation before the PQ key and pin exist).
2. **The epoch** is a root-signed delegation with `pqRequired: true` (a normal 2-of-3 Ed25519 delegation plus the PQ signature; its Ledger text shows ` | pq required | ` so the operator sees it on the device). The marker is sticky: `root-ceremony.mjs draft --prev` carries it forward and nothing in the tooling can drop it.
3. **The persisted floor.** A client that accepts a delegation for which PQ was required, **after** the whole chain including the PQ signature verified and the bundle staged (`commitFloor`), persists a PQ floor (`pq-required-floor`, next to the bundle and delegation floors). It only ever rises: `FloorStore` has `setPqRequired()` and no way to clear it, and any stored value, including a corrupt one, counts as set. From then on an Ed25519-only chain is refused whatever its version, so neither a root-signed delegation without the marker nor a replay of an older one can roll it back. A delegation below the delegation-version floor is refused as before.
4. **The build floor.** `PQ_REQUIRED_BY_BUILD` flips to true in a later client release (together with `PINNED_PQ_REQUIRED`), so a browser that updates to that build requires PQ without ever having seen the epoch. The signing tools are stricter than a client from the first PQ-pinned release on: [`sign.mjs`](../apps/web/build/sign.mjs) and [`release.mjs`](../apps/web/build/release.mjs) refuse to sign or report a release unless the delegation carries a valid PQ signature, so no release ships under an Ed25519-only chain.
5. **Fail closed.** Unknown algorithm tags and frames (pinned or declared), an unusable pin, a PQ-required delegation seen by a build with no PQ pin (`pq_root_unset`), and a runtime without ML-DSA all refuse.

**Residual risk, stated plainly.** Between now and the moment a given browser has accepted the epoch (or runs a build that sets the build floor), an attacker who can already forge Ed25519 root signatures could still serve that browser an Ed25519-only delegation. That is the price of an upgrade path for installed clients; the window is closed per browser by item 3 or 4, and is the reason the epoch and the build floor should follow the PQ-pinned release quickly.

### 7. Custody

The PQ root is the 32-byte FIPS 204 seed, stored **only** as a secure note in the operator's **personal** Vaultwarden (separate `BITWARDENCLI_APPDATA_DIR`, as in [`KEY-CEREMONY.md`](../deploy/KEY-CEREMONY.md)), never in the agents' vault (`vw.agents@lonunjie.org`), never on disk, never given to an agent. `gen-pq-root --to-stdin-for-vault` refuses a terminal, writes the note only to stdout (a Bitwarden item) and prints only the public key id and fingerprint; `pq-sign-delegation --pq-root-stdin` reads it from a pipe. Whenever it signs, the seed is in the memory of the machine that runs the command: use a trusted machine only. The seed is short enough for an offline paper copy; keeping one is recommended because loss has a hard consequence (section 9).

### 8. Compromise and rotation of Ed25519 roots

Unchanged ([`KEY-CEREMONY.md`](../deploy/KEY-CEREMONY.md) section 11). A renewal changes the payload, so every renewal, revocation or rotation of the online key needs a fresh PQ signature as well (the runbook adds the one command).

### 9. Rotation, loss and compromise of the PQ root

- **Planned rotation or compromise of the PQ key alone.** The attacker still needs 2 of 3 Ed25519 roots, so nothing is urgent. Generate a new PQ key; sign one delegation with **both** PQ keys (entries are matched by `keyId`, so it verifies under either pin); ship a client release pinning the new key; then retire the old key. Old clients keep verifying under the old pin until they update.
- **Loss of the PQ key after a client has the floor.** No delegation can be produced that such a client accepts, and by design there is no floor-clearing path (any such path is a downgrade path). Those clients must reinstall from a verified out-of-band release that pins a new PQ key, the same class of event as losing two of three Ed25519 roots. This is why the paper backup of section 7 is recommended before the epoch.
- **PQ and Ed25519 compromise together** is the full-root compromise case of [`KEY-CEREMONY.md`](../deploy/KEY-CEREMONY.md).

### 10. Future: the PQ key on a Ledger

If an ML-DSA-65 signing app for the Nano S+ (or another device) appears and is verified, the key moves by the rotation in section 9: a new descriptor with a new `keyId` and, if the device frames the message differently, a new frame id (for example `om-pq-ledger-v1`) pinned with the key, an ADR for that frame, a dual-signed delegation, a client release, then retirement of the software key and deletion of the vault note. The algorithm tag stays `ml-dsa-65`. The external-signer commands (`pq-payload`, `pq-attach`) already exist and verify before anything is written.

### 11. Dependency: `@noble/post-quantum` 0.7.1

Approved by the operator 2026-10-28. Pinned exactly in [`apps/web/package.json`](../apps/web/package.json) and `pnpm-lock.yaml` (integrity `sha512-+P9981IiAnVh+rmcubozzVwrEy3XsN/tMhTnvsjV9VDaYpOnNCqWqKo2FLWxbu92YHfjGIlE5XnW175UK+ln+Q==`). Licence **MIT**; dependencies `@noble/curves`, `@noble/hashes` (already in the lockfile as a transitive dependency) and `@noble/ciphers`, all 2.4.0, MIT, no install scripts. Audit status per its README at review time: **not independently audited**; self-audited at 0.6.1 (April 2026) with later changes since; an independent ACVP-based reproducibility study (not an audit) exists for 0.7.0. Mitigations: it is imported only by the client verifier (`src/update/pq.ts`) and the ceremony tool (`build/pq-ceremony.mjs`), never for message crypto; the client only verifies (no secret material); the second, independent `node:crypto` implementation cross-checks it (section 4); and the version is exact so an update is a reviewed change. Re-check the audit status before any public release.

## Consequences

- New files: `src/update/pq.ts`, `src/update/pq-root.ts`, `build/pq-ceremony.mjs`, `build/pq-cosign.test.ts`, [`KEY-CEREMONY-PQ.md`](../deploy/KEY-CEREMONY-PQ.md). Changed: `delegation.ts`, `signature.ts`, `floor.ts` (verification gains the PQ step and the floor), `sw/core.ts` (floor storage), `release-key.ts` (`rootPin` passes the PQ pin), [`verify-release.mjs`](../apps/web/build/verify-release.mjs), [`sign.mjs`](../apps/web/build/sign.mjs), [`release.mjs`](../apps/web/build/release.mjs), [`root-ceremony.mjs`](../apps/web/build/root-ceremony.mjs) (new subcommands, `--pq-required`).
- `make web-release-check` and `pnpm web:check` cover it. 56 PQ tests: 37 shared-fixture verdicts through both verifiers (valid, missing PQ, wrong PQ key, tampered payload, downgrade and rollback, wrong alg tag, 2-of-3 Ed25519 without PQ, unknown tags and pins, floor), the golden signed bytes, OpenSSL/noble interop, the floor in the service-worker shell, the signing tool, the ceremony commands and two full release builds.
- **Not verified:** no real browser end-to-end of a PQ-required update; the first real PQ ceremony; the noble library has no independent audit; Node versions other than 24.14.0 for `node:crypto` ML-DSA.
- The service worker bundle (`sw.js`, which carries the update verifier) grows from 10.2 KB to 31.3 KB (3.8 KB to 11.7 KB gzip) for the ML-DSA-65 verifier; the main app chunk is unchanged. Every release delegation grows by about 4.4 KB.

## Where did authority go?

Nowhere new. The PQ root key is a second, mandatory signer held by the same operator, in the same personal vault as the software Ed25519 root; it is not an OM-operated service and grants no protocol privilege. It makes a future forgery of the Ed25519 roots insufficient; it does not make the operator or the vault a protocol authority, and an origin compromise that replaces `sw.js` still bypasses the manifest (audit B2, point 2), so no "signed updates" claim is made yet.
