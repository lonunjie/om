# OM release key ceremony (offline root + delegated online key)

Runbook for the operator. Implements audit finding B2 / gate PR-005 in a minimal TUF-style form
([`SECURITY_RELEASE_GATES.md`](../docs/process/SECURITY_RELEASE_GATES.md), ADR-0130, ADR-0149).

## The model

| Key | Where it lives | What it signs | Lifetime |
|---|---|---|---|
| **Root** (Ed25519) | Offline media only, held by the operator. Never on a networked machine, never in the vault, never given to an agent. | One small `delegation` document | Years. Rotated only deliberately. |
| **Online release key** (Ed25519) | Agent vault (`om-release-2026-10-28`, unchanged vault path) | Each bundle manifest | Months. Replaced by a root-signed delegation. |

- The client (service worker) and `verify.mjs` pin **only the root public key(s)**. The delegation travels inside every
  signed `bundle-manifest.json` (`delegation` field, covered by the release signature and by `SHA256SUMS`).
- A manifest is accepted only if: root signature on the delegation verifies (threshold met), the delegation is not
  expired, its `version` is not below the highest version the browser ever accepted (persisted, like the bundle
  floor), the signing key is listed, not in `revoked`, inside its `notBefore`/`notAfter` window, and the manifest
  signature verifies. Anything else fails closed.
- Compromise of the vault or the agent host now yields a key that the operator can revoke and replace **without the
  attacker being able to extend its own authority**: only the offline root can extend or renew a delegation.

What this does **not** give: a browser replaces `sw.js` itself without checking the manifest, so an origin
compromise still defeats it. Make no "signed updates" or tamper-resistance claim to users until that limit is
documented next to it (audit B2, point 2).

Tool: [`apps/web/build/root-ceremony.mjs`](../apps/web/build/root-ceremony.mjs) (pure node, no dependencies, never
prints private material). All paths below assume the repo at `~/dig/lonunjie/om`.

## Status

The root has **not** been generated yet. Until `ROOT_KEYS` is set (step 3):
production release builds fail ([`release.mjs`](../apps/web/build/release.mjs), and `vite build` under `OM_RELEASE_BUILD`), [`sign.mjs`](../apps/web/build/sign.mjs) refuses to
sign, and any signed bundle is rejected by clients (`root_unset`). Dev builds are unaffected.

## 1. Prepare (once)

You need: an offline computer (or a laptop with Wi-Fi off and no network cable), Node 24, a checkout of this repo on
it, and **three** removable drives: `OM-ROOT-A` (working), `OM-ROOT-B` (backup), and a place for B that is not where A lives.

```sh
cd ~/dig/lonunjie/om && git pull --ff-only && node --version
# now disconnect from the network (Wi-Fi off, cable out) before generating anything
```

## 2. Generate the root key (offline)

Plug in `OM-ROOT-A` (macOS mounts it at `/Volumes/OM-ROOT-A`).

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs gen-root --out /Volumes/OM-ROOT-A/om-root.pem
```

It writes the private key to that path only (mode 600, never overwritten, refuses any path inside a git work tree)
plus `om-root.pem.pub`, and prints **only** the root key id (`root-<16 hex>`), the public key (64 hex chars) and the
SHA-256 fingerprint. Write the id, public key and fingerprint down (paper or the `.pub` file). The private key is not
shown anywhere.

## 3. Back it up (2 copies, separate places)

The root key cannot be recovered if lost; losing it means shipping a new client that pins a new root.

```sh
cp -p /Volumes/OM-ROOT-A/om-root.pem /Volumes/OM-ROOT-B/om-root.pem && chmod 600 /Volumes/OM-ROOT-B/om-root.pem
cd ~/dig/lonunjie/om
node apps/web/build/root-ceremony.mjs show-root --root-key /Volumes/OM-ROOT-A/om-root.pem
node apps/web/build/root-ceremony.mjs show-root --root-key /Volumes/OM-ROOT-B/om-root.pem   # identical id, public key, fingerprint
```

Store `OM-ROOT-A` and `OM-ROOT-B` in **different physical locations** (for example a home safe and a deposit box or a
trusted relative). Re-run `show-root` against each copy once a year to prove it still reads. Never copy the key to
cloud storage, the vault, a password manager, email or a chat.

## 4. Pin the public root in the code (the placeholder)

On a normal machine, with the three public values from step 2 (these are public; they may travel by USB or by typing):

1. `apps/web/src/update/release-key.ts`: set `ROOT_KEYS = { "<root key id>": "<64 hex public key>" }` (leave `ROOT_THRESHOLD = 1`).
2. [`apps/web/build/verify-release.mjs`](../apps/web/build/verify-release.mjs): set `PINNED_ROOTS` to the same object.
3. `~/dig/lonunjie/registry/signing.toml`: set the root entry's `public_raw_hex`, `fingerprint_sha256`, `status = "active"`.
4. Run the checks and commit (the test `pins the same offline root...` fails if 1 and 2 differ):

```sh
cd ~/dig/lonunjie/om && pnpm exec vitest run apps/web/build && make web-release-check
```

## 5. Delegate the online release key

The existing online key `om-release-2026-10-28` has public key
`fad8cae9155ea0b7943c0b3f141a20f85b35c88ca5eacfe576a9bf578f28ebeb` (also in `registry/signing.toml`). On the offline
machine, with `OM-ROOT-A` plugged in:

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs sign-delegation \
  --root-key /Volumes/OM-ROOT-A/om-root.pem \
  --online-pub fad8cae9155ea0b7943c0b3f141a20f85b35c88ca5eacfe576a9bf578f28ebeb \
  --key-id om-release-2026-10-28 --expires +90d \
  --out /Volumes/OM-ROOT-A/release-delegation.json
```

Check the summary it prints (version, expiry, key id, key window). Carry **only** `release-delegation.json` (public)
to the networked machine and commit it as `apps/web/release-delegation.json`. The key window defaults to
now..`--expires`; pass `--not-before`/`--not-after` to change it.

## 6. Sign a release (online machine, unchanged vault path)

```sh
cd ~/dig/lonunjie/om/apps/web && node build/release.mjs --sign-from-vault   # embeds apps/web/release-delegation.json
```

[`sign.mjs`](../apps/web/build/sign.mjs) refuses to sign if the delegation is missing, not signed by the pinned root, expired, does not list the
key id, lists a different public key than the vault key, or the key is revoked or outside its window. After the build
it verifies the whole chain. Operators check a release with
`node verify.mjs --require-signed` (see [`README.md`](README.md)).

## 7. Renew (before every expiry, at least every 60 days if you use `+90d`)

An expired delegation stops all updates, so renew early. Renewal bumps `version` and keeps the keys and revocations.

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs sign-delegation \
  --root-key /Volumes/OM-ROOT-A/om-root.pem --prev apps/web/release-delegation.json \
  --expires +90d --out /Volumes/OM-ROOT-A/release-delegation.json
```

Copy it to `apps/web/release-delegation.json` and ship it with the next release. Put a calendar reminder 30 days before expiry.

## 8. Rotate the online key (planned)

1. Generate a new Ed25519 key on the signing host, store the PKCS8 PEM in the vault under the agreed note, and take
   its raw public key (64 hex). Pick a **new** key id (ids are never reused), for example `om-release-2027-04-01`.
2. Offline, add it (the old key stays valid until you revoke it):

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs sign-delegation \
  --root-key /Volumes/OM-ROOT-A/om-root.pem --prev apps/web/release-delegation.json \
  --online-pub <NEW 64-HEX PUBLIC KEY> --key-id om-release-2027-04-01 --expires +90d \
  --out /Volumes/OM-ROOT-A/release-delegation.json
```

3. Commit the new delegation, release once with the new key (`release.mjs --sign-from-vault --key-id om-release-2027-04-01`), then revoke the old key (step 9).

## 9. Revoke a key

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs revoke \
  --root-key /Volumes/OM-ROOT-A/om-root.pem --prev apps/web/release-delegation.json \
  --key-id om-release-2026-10-28 --out /Volumes/OM-ROOT-A/release-delegation.json
```

This moves the id to `revoked` (never reused) and bumps `version`. It takes effect for a browser once that browser
accepts any manifest carrying the new delegation; from then on the browser's persisted delegation floor also refuses
the older delegation. Revoke only after a release signed by a still-valid key is ready to ship, or you strand updates.

## 10. Compromise response

**Online release key (vault or agent host) compromised**
1. Generate a new online key on a clean host (step 8.1) and store it in a fresh vault note; delete the old note.
2. Offline: run `sign-delegation` to add the new key **and** `revoke` the old one (run `revoke` on the result), expiry `+30d`.
3. Commit the delegation, ship a release signed by the new key, deploy it.
4. Browsers that have not yet fetched the new delegation still trust the old key until the old delegation expires
   (that is why delegations are short-lived); the attacker's reach ends at that expiry at the latest. Rotate other
   secrets that shared the host. Tell users per [`SECURITY.md`](../SECURITY.md) if a malicious release was served.

**Root key compromised or lost**
- Lost with a surviving backup: restore from the other drive, verify with `show-root`, re-create the lost copy.
- Compromised, or all copies lost: the pinned root cannot be trusted or used. Generate a new root (steps 2 to 4),
  and ship a client that pins it. Because running clients only trust the old root, the transition release must carry a
  delegation signed by **both** old and new roots (only possible if the old key is still held; use `cosign` with
  each key). If the old root is lost or stolen, existing installs cannot be moved safely: publish the new root out of
  band and have users reinstall.

## 11. Upgrade to 2-of-N root custody (when more custodians exist)

The verifiers already count distinct pinned roots against `threshold`. To move to 2-of-3:
1. Generate two more roots on separate media held by separate people (step 2), keep a backup of each.
2. Ship a transition release whose delegation is signed by the old root **and** all new roots (`sign-delegation`
   with each `--root-key`, or `cosign` per key), with a client that pins the three roots and `ROOT_THRESHOLD = 2`
   (set in `release-key.ts` and [`verify-release.mjs`](../apps/web/build/verify-release.mjs); the test checks they agree).
3. From then on every delegation needs two signatures: first holder runs `sign-delegation ... --out d.json`, the
   second runs `cosign --root-key <their key> --in d.json`. Each prints the summary so the signer sees what they approve.

## Quick reference

```
gen-root         --out <abs path>
show-root        --root-key <file>
sign-delegation  --root-key <file>... --expires <t> [--online-pub <hex> --key-id <id>] [--prev <file>] [--out <file>]
revoke           --root-key <file>... --prev <file> --key-id <id> [--expires <t>] [--out <file>]
cosign           --root-key <file> --in <file> [--out <file>]
inspect          --in <file>
<t> = YYYY-MM-DD | YYYY-MM-DDTHH:MM:SSZ | +Nd (max 400 days)
```
