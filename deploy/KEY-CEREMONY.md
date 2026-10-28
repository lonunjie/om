# OM release key ceremony (2-of-3 root + delegated online key)

Runbook for the operator. Implements audit finding B2 / gate PR-005 in a minimal TUF-style form
([`SECURITY_RELEASE_GATES.md`](../docs/process/SECURITY_RELEASE_GATES.md), ADR-0130, ADR-0149).
Operator decision 2026-10-28: the root is **2-of-3 Ed25519 signers**.

## The model

| Key | Where it lives | What it signs | Lifetime |
|---|---|---|---|
| **Root signer 1: Ledger A** (Nano S+, own recovery phrase) | The device. The private key never leaves it. | One small `delegation` document | Years |
| **Root signer 2: Ledger B** (Nano S+, a different recovery phrase, kept elsewhere) | The device. | same | Years |
| **Root signer 3: software key** (Ed25519 PKCS8 PEM) | ONLY the operator's **personal** Vaultwarden (secure note). Never on disk, never in the agents' vault `vw.agents@lonunjie.org`, never given to an agent. | same | Years |
| **Online release key** (Ed25519) | Agent vault (`om-release-2026-10-28`, unchanged vault path) | Each bundle manifest | Months. Replaced by a root-signed delegation. |

- The client (service worker) and `verify.mjs` pin **only the three root public keys, each with its signing frame**
  (see "Ledger framing"), and `ROOT_THRESHOLD = 2`. The delegation travels inside every signed `bundle-manifest.json`
  (`delegation` field, covered by the release signature and by `SHA256SUMS`).
- A manifest is accepted only if: at least two distinct pinned roots signed the delegation (each over the bytes framed as
  *its pinned descriptor* says, never as the signature says), the delegation is not expired, its `version` is not below
  the highest version the browser ever accepted, the signing key is listed, not in `revoked`, inside its
  `notBefore`/`notAfter` window, and the manifest signature verifies. Anything else fails closed. One public key pinned
  under two ids makes the whole pin unusable (`root_unset`), so one key can never count twice.
- Routine signing uses **Ledger A + Ledger B**. The vault key is the third signer for loss of a Ledger, and a replacement
  signer is added by rotation (below). Whenever the vault key signs, its PEM is in the memory of the machine that runs
  the command (it is piped from `bw` into [`root-ceremony.mjs`](../apps/web/build/root-ceremony.mjs), never written to disk); do that on a trusted machine only.
- Compromise of the vault or the agent host yields an online key that the operator can revoke and replace **without the
  attacker being able to extend its own authority**: only two of the three roots can extend or renew a delegation.

What this does **not** give: a browser replaces `sw.js` itself without checking the manifest, so an origin
compromise still defeats it. Make no "signed updates" or tamper-resistance claim to users until that limit is
documented next to it (audit B2, point 2).

Tool: [`apps/web/build/root-ceremony.mjs`](../apps/web/build/root-ceremony.mjs) (pure node, no dependencies, never
prints private material). Ledger USB helper: [`deploy/ledger/`](ledger/) (isolated package, needs your approval, see
"Ledger helper"). All paths below assume the repo at `~/dig/lonunjie/om`.

## Status

The root has **not** been generated yet, so `ROOT_KEYS` / `PINNED_ROOTS` are still `{}` (the threshold constants are
already 2). Until step 7 is done: production release builds fail ([`release.mjs`](../apps/web/build/release.mjs), and
`vite build` under `OM_RELEASE_BUILD`), [`sign.mjs`](../apps/web/build/sign.mjs) refuses to sign, and any signed bundle is
rejected by clients (`root_unset`). Dev builds are unaffected.

**Not yet verified on hardware** (the ceremony code was tested against a software emulation of the framing and against
the open source of the Ledger Solana app, never against a physical Nano S+): see "What is unverified" at the end. Do a
dry run (steps 2 to 5 with a throwaway delegation, nothing pinned) before relying on it.

## Ledger framing (which app, and exactly which bytes)

**Choice: the Ledger Solana app, "off-chain message" signing** (APDU `E0 07`, app 1.3.0 or later; `hw-app-solana`
`signOffchainMessage`). Why it is the most reliable option:

- It is a first-party, actively maintained app available on the Nano S+ through Ledger Live, and its source is public
  ([`LedgerHQ/app-solana`](https://github.com/LedgerHQ/app-solana), read for this decision: `src/handle_sign_offchain_message.c`,
  `src/utils.c`, `libsol/parser.c`, `doc/api.md`).
- The app signs the whole serialized envelope with **plain Ed25519** (`bip32_derive_with_seed_eddsa_sign_hash_256`, SHA-512
  inside, SLIP-0010 ed25519 derivation), so the result verifies with any RFC 8032 Ed25519 verifier (WebCrypto,
  `node:crypto`). Nothing but the envelope is hashed or prefixed by the app.
- ASCII text is shown **in clear on the device** and needs **no blind signing** (blind signing is required only for
  non-ASCII/UTF-8 messages, which we never use).
- Considered and not chosen: the Stellar app's hash signing (shows only a hash, needs the hash-signing mode, blind) and
  Ledger's SSH/PGP agent app (searched; no current Nano S+ support statement found, third-party agent protocol, not
  verified). Other Ed25519 apps sign chain-specific transaction structures, not arbitrary bytes.

**Envelope** (header version 0, message format 0, one signer; the `frame` id is `solana-offchain-v0`):

| Offset | Size | Value |
|---|---|---|
| 0 | 16 | `ff 73 6f 6c 61 6e 61 20 6f 66 66 63 68 61 69 6e` (`\xff` + `solana offchain`) |
| 16 | 1 | header version `00` |
| 17 | 32 | application domain: ASCII `om.release-delegation/1` padded with `00` |
| 49 | 1 | message format `00` (restricted ASCII, 0x20..0x7e) |
| 50 | 1 | signer count `01` |
| 51 | 32 | the signing Ledger's own Ed25519 public key |
| 83 | 2 | text length, unsigned little endian |
| 85 | n | the text |

The text (restricted ASCII, about 140 bytes, well under the 1212-byte format-0 limit):

```
OM release delegation | version <n> | expires <YYYY-MM-DDTHH:MM:SSZ> | sha256 <64 hex>
```

where the hex is the SHA-256 of the delegation's canonical payload (`delegationPayload`: canonical JSON of the whole
delegation with `signatures` set to `null`). The signature covers the envelope above, byte for byte. The text therefore
commits to the full delegation (every key, window and revocation) through that hash, and shows the version and expiry
in plain words. A golden vector (built by an independent implementation) is in
[`delegation.test.ts`](../apps/web/build/delegation.test.ts); both verifiers must reproduce it byte for byte.

**What the device shows.** The Solana app's UI step is "Review message" with the text paged across the screen, then
"Sign message?". In the app's **Expert** display mode (Settings) it additionally shows signer, application domain,
version, format, size and the SHA-256 of the message **text** (not of the envelope). Our text has no newline, so it
scrolls as one string; read the version, the expiry and the 64-hex digest page by page. Blind signing may stay
**disabled**. `ledger-sign.mjs check` reports the setting and the app version.

**How verification works.** Each pinned root key has a descriptor `{ pub, frame }` (a bare hex string means frame
`raw`: the signature covers `delegationPayload` itself, used by the software key). For a signature entry the verifier
looks up the pinned descriptor, requires the entry's declared `frame` (absent means `raw`) to equal the pinned one,
rebuilds the framed bytes from the delegation and the pinned public key, and verifies Ed25519 over them. Unknown
frames (pinned or declared) fail closed; a differing declaration is ignored, not trusted.

## 0. Prerequisites (once, on the Mac that will hold the Ledgers)

```sh
cd ~/dig/lonunjie/om && git pull --ff-only && node --version   # Node 24
mkdir -p -m 700 ~/om-root-work   # public working files only: delegations, payloads, signatures
```

### Ledger firmware and app checks

1. Ledger Live, My Ledger: both Nano S+ run the **latest firmware** offered, and the **Solana app is installed and up to
   date** (the open source head is 1.15.x; 1.3.0 is the minimum for off-chain signing). Install and update only through
   Ledger Live, which performs the genuine-device check. Use the **same Solana app version on both** and write it down.
2. Ledger A and Ledger B are initialised with **different recovery phrases**, stored in **different places**. The same
   phrase on both would make the pin a 1-of-2 in disguise; `pin-snippet` also rejects two equal public keys.
3. On each device, in the Solana app, open Settings: leave **blind signing disabled**; set **Display mode: Expert** for the
   ceremony (it shows the signer and the digest of the text).
4. After the helper is installed (next section), with the device unlocked and the Solana app open:

```sh
cd ~/dig/lonunjie/om/deploy/ledger && node ledger-sign.mjs check
```

   It prints `{"solanaAppVersion":"1.15.x","blindSigningEnabled":false,"ok":true}`; `ok:false` means the app is older than 1.3.0.

### Ledger helper (dependency approval needed)

[`deploy/ledger/`](ledger/) is its own pnpm root (`pnpm-workspace.yaml` with `packages: []`), **not** a member of the
repository workspace, with its own lockfile. Nothing in `apps/web` imports it, the root `pnpm install` never fetches it
and it is never bundled. Pinned exactly (all licences permissive, `pnpm licenses list` in that folder):

| Package | Version | Licence | Why |
|---|---|---|---|
| `@ledgerhq/hw-app-solana` | 7.12.0 | Apache-2.0 | `getAddress`, `signOffchainMessage` |
| `@ledgerhq/hw-transport-node-hid` | 6.33.5 | Apache-2.0 | USB HID transport |
| transitive (56 packages) | see `deploy/ledger/pnpm-lock.yaml` | MIT, ISC, BSD-3-Clause, Apache-2.0, `(MIT OR X11)` (`node-hid`), `(MIT OR WTFPL)` (`expand-template`), `(BSD-2-Clause OR MIT OR Apache-2.0)` (`rc`) | |

**Your approval is required** for this dependency set, in particular for `node-hid` 2.1.2, a native module whose install
script (`prebuild-install` / `node-gyp`) downloads or compiles code. The lockfile was generated with scripts disabled and
`pnpm-workspace.yaml` keeps `allowBuilds: node-hid: false` until you approve. Whether `node-hid` 2.1.2 builds on Node 24 on
your Mac has **not** been tested (no device, no build was run). After approving:

```sh
cd ~/dig/lonunjie/om/deploy/ledger && pnpm install --frozen-lockfile && pnpm approve-builds && pnpm rebuild node-hid
node --test lib.test.mjs   # the device-free tests of the helper
```

If `node-hid` will not build, stop and report the error; do not substitute another package unreviewed.

## 1. Personal vault (separate data dir; never the agents' profile)

The root PEM goes only into **your personal** Vaultwarden account. Use a dedicated `bw` data directory so this cannot touch
the agents' profile (`lonunjie-agents`) or any other login. Replace the two placeholders with your personal server URL and
account email (the agents' account lives on `https://passwords.majlovesreg.one`; use your own personal account there or
wherever it is):

```sh
export BITWARDENCLI_APPDATA_DIR="$HOME/.config/bw-om-root" && mkdir -p -m 700 "$BITWARDENCLI_APPDATA_DIR"
bw config server <PERSONAL_VAULT_URL>
bw login <PERSONAL_EMAIL>
export BW_SESSION="$(bw unlock --raw)"
bw status   # must say "unlocked" and show YOUR personal email, not vw.agents@lonunjie.org
```

Keep that shell for steps 2 and 5, then `bw lock; unset BW_SESSION BITWARDENCLI_APPDATA_DIR`. Never run `bw-agent` with any
profile for the root PEM.

## 2. Generate the software root key straight into the personal vault

The PEM is written to stdout as a Bitwarden secure-note item and piped into `bw create item`; no file, no log. `bw create
item` echoes the item it created (with the PEM), so its stdout goes to `/dev/null`. Stderr shows only the public data.

```sh
cd ~/dig/lonunjie/om && set -o pipefail && \
node apps/web/build/root-ceremony.mjs gen-root --to-stdin-for-vault --name "OM release root (software signer) 2026-10-28" \
  | bw encode | bw create item > /dev/null && echo "stored in the personal vault"
```

Write down the printed **root key id, public key and fingerprint** (public). Then prove the vault copy is the key, with
the same public key printed back:

```sh
cd ~/dig/lonunjie/om && bw sync && bw get notes "OM release root (software signer) 2026-10-28" \
  | node apps/web/build/root-ceremony.mjs show-root --root-key-stdin
```

Back up the vault itself per your normal personal-vault practice (the root key is only as safe as that account and its
recovery); losing it is survivable (2-of-3), see the runbook.

## 3. Read the two Ledger public keys

One device at a time, Solana app open, Expert mode. `--display` shows the address on the device; compare it with the
`solanaAddress` printed and with Ledger Live's Solana account 1 address (path `44'/501'/0'`).

```sh
cd ~/dig/lonunjie/om/deploy/ledger && node ledger-sign.mjs pubkey --display | tee ~/om-root-work/ledger-a.json
# unplug A, plug in B
cd ~/dig/lonunjie/om/deploy/ledger && node ledger-sign.mjs pubkey --display | tee ~/om-root-work/ledger-b.json
```

The two `pub` values must differ. Then print the three public descriptors and the paste-ready pin text (`<SW_PUB>` is the
public key from step 2):

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs pin-snippet \
  --root "$(node -p "require('$HOME/om-root-work/ledger-a.json').pub")=solana-offchain-v0" \
  --root "$(node -p "require('$HOME/om-root-work/ledger-b.json').pub")=solana-offchain-v0" \
  --root <SW_PUB>=raw | tee ~/om-root-work/pin.txt
```

The last line of the output is a roots file; save it for the checks below:

```sh
tail -n 1 ~/om-root-work/pin.txt > ~/om-root-work/roots.json
```

## 4. Draft the first delegation (unsigned)

For the existing online key `om-release-2026-10-28` (public key `fad8cae9155ea0b7943c0b3f141a20f85b35c88ca5eacfe576a9bf578f28ebeb`,
also in `registry/signing.toml`):

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs draft \
  --online-pub fad8cae9155ea0b7943c0b3f141a20f85b35c88ca5eacfe576a9bf578f28ebeb \
  --key-id om-release-2026-10-28 --expires +90d --out ~/om-root-work/release-delegation.json
```

Check the printed summary (version 1, expiry, key id, key window). The key window defaults to now..`--expires`.

## 5. Sign with two signers (Ledger A + Ledger B)

For each Ledger, in turn. Make the payload (this prints what you must see on the device), sign on the device, attach.
Ledger A:

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs external-payload \
  --in ~/om-root-work/release-delegation.json \
  --signer-pub "$(node -p "require('$HOME/om-root-work/ledger-a.json').pub")" --out ~/om-root-work/payload-a.bin
```

Read the printed lines: the delegation summary and the line `text the Ledger will show: OM release delegation | version 1 |
expires ... | sha256 ...`. Then (Ledger A plugged in, unlocked, Solana app open):

```sh
cd ~/dig/lonunjie/om/deploy/ledger && node ledger-sign.mjs sign --payload ~/om-root-work/payload-a.bin --out ~/om-root-work/sig-a.json
```

On the device approve **only if** the version, the expiry and the digest (all 64 hex characters, page by page) equal the
printed line. Then attach (the signature is verified against the delegation and the key before anything is written):

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs external-attach \
  --in ~/om-root-work/release-delegation.json --from-helper ~/om-root-work/sig-a.json
```

Ledger B: repeat all three commands with `ledger-b.json`, `payload-b.bin`, `sig-b.json` (unplug A, plug in B first). Then
check against the three descriptors:

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs verify \
  --in ~/om-root-work/release-delegation.json --roots-file ~/om-root-work/roots.json   # expect: OK: 2-of-3 ...
```

**Using the vault key instead of (or besides) a Ledger** (same shell as step 1, `BW_SESSION` set; the PEM is piped, never
written to a file):

```sh
cd ~/dig/lonunjie/om && bw get notes "OM release root (software signer) 2026-10-28" \
  | node apps/web/build/root-ceremony.mjs cosign --root-key-stdin --in ~/om-root-work/release-delegation.json
```

## 6. Pin the three roots, commit the delegation

Open `~/om-root-work/pin.txt`: it has the exact `ROOT_KEYS` and `PINNED_ROOTS` literals. On the Mac:

1. `apps/web/src/update/release-key.ts`: replace `ROOT_KEYS = {}` with the printed object (`ROOT_THRESHOLD = 2` and
   `ROOT_KEY_COUNT = 3` are already set).
2. [`apps/web/build/verify-release.mjs`](../apps/web/build/verify-release.mjs): replace `PINNED_ROOTS = {}` with the same object.
3. `~/dig/lonunjie/registry/signing.toml`: fill the three `om-release-root-*` entries (`root_key_id`, `public_raw_hex`,
   `fingerprint_sha256`, `frame`), set `status = "active"` and the `created` date.
4. Copy the delegation and run the checks (the test `pins the same offline root...` fails if 1 and 2 differ, and checks
   exactly three distinct public keys):

```sh
cp ~/om-root-work/release-delegation.json ~/dig/lonunjie/om/apps/web/release-delegation.json
cd ~/dig/lonunjie/om && pnpm exec vitest run apps/web/build && make web-release-check
```

5. Commit `release-key.ts`, [`verify-release.mjs`](../apps/web/build/verify-release.mjs), the delegation and the registry change; push.

## 7. Sign a release (online machine, unchanged vault path)

```sh
cd ~/dig/lonunjie/om/apps/web && node build/release.mjs --sign-from-vault   # embeds apps/web/release-delegation.json
```

[`sign.mjs`](../apps/web/build/sign.mjs) refuses to sign if the delegation is missing, not signed by two pinned roots,
expired, does not list the key id, lists a different public key than the vault key, or the key is revoked or outside its
window. After the build it verifies the whole chain. Operators check a release with
`node verify.mjs --require-signed` (see [`README.md`](README.md)).

## 8. Renew (before every expiry, at least every 60 days if you use `+90d`)

An expired delegation stops all updates, so renew early. Renewal bumps `version` and keeps the keys and revocations. Every
change below is "draft, then two signers" exactly as in steps 4 and 5; `draft` takes `--prev`:

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs draft \
  --prev apps/web/release-delegation.json --expires +90d --out ~/om-root-work/release-delegation.json
```

then step 5 (payload, sign, attach for two signers, `verify`), then copy to `apps/web/release-delegation.json` and ship it
with the next release. Put a calendar reminder 30 days before expiry. (Software-only shortcut: `sign-delegation
--root-key-stdin` with a second `cosign`, as in step 5.)

## 9. Rotate the online key (planned)

1. Generate a new Ed25519 key on the signing host, store the PKCS8 PEM in the agent vault under the agreed note, and take
   its raw public key (64 hex). Pick a **new** key id (ids are never reused), for example `om-release-2027-04-01`.
2. Draft it onto the current delegation (the old key stays valid until you revoke it), then sign with two roots (step 5):

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs draft \
  --prev apps/web/release-delegation.json --online-pub <NEW 64-HEX PUBLIC KEY> --key-id om-release-2027-04-01 \
  --expires +90d --out ~/om-root-work/release-delegation.json
```

3. Commit the new delegation, release once with the new key (`release.mjs --sign-from-vault --key-id om-release-2027-04-01`), then revoke the old key (step 10).

## 10. Revoke a key

Draft the revocation (`--revoke-key-id` moves the id to `revoked`, never reused, and bumps `version`), then two signers:

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs draft \
  --prev apps/web/release-delegation.json --revoke-key-id om-release-2026-10-28 --expires +90d \
  --out ~/om-root-work/release-delegation.json
```

It takes effect for a browser once that browser accepts any manifest carrying the new delegation; from then on the
browser's persisted delegation floor also refuses the older delegation. Revoke only after a release signed by a
still-valid key is ready to ship, or you strand updates.

## 11. Compromise, loss and root rotation runbook

**Online release key (vault or agent host) compromised**
1. Generate a new online key on a clean host (step 9.1) and store it in a fresh vault note; delete the old note.
2. Draft one delegation that adds the new key and revokes the old one (`draft --prev apps/web/release-delegation.json
   --online-pub <NEW> --key-id <NEW ID> --revoke-key-id <OLD ID> --expires +30d`), then two root signers (step 5).
3. Commit the delegation, ship a release signed by the new key, deploy it.
4. Browsers that have not yet fetched the new delegation still trust the old key until the old delegation expires
   (that is why delegations are short-lived); the attacker's reach ends at that expiry at the latest. Rotate other
   secrets that shared the host. Tell users per [`SECURITY.md`](../SECURITY.md) if a malicious release was served.

**One root signer lost** (a Ledger broken or the vault account gone): the other two still sign; nothing is urgent, but you
are now 2-of-2. Replace it (below) soon. Recover a Ledger from its recovery phrase on a new device (same Solana path gives
the same public key, no change of pin); the vault key cannot be recovered without the vault.

**One root signer compromised or suspected** (stolen unlocked Ledger plus PIN, vault account breach): the attacker alone
cannot sign (threshold 2), but a second compromise would be fatal. Replace it now:
1. Generate or initialise the replacement (a new Ledger C: step 3; a new vault key: step 2, with a new name), and compute
   the new pin with `pin-snippet` (still exactly three distinct roots).
2. Draft a renewal (step 8) and sign it with **two roots that are in both the old and the new set** (the two you trust):
   signatures from keys not in a pin are ignored, so one delegation is valid under both pins.
3. Ship a release whose client pins the NEW set (`ROOT_KEYS`, `PINNED_ROOTS`, `registry/signing.toml`, `status =
   "revoked"` on the replaced entry). Old clients verify that release under the old pin (two honest signatures) and then
   run the new pin. Until every client updated, the old compromised key remains a valid pinned signer for old clients.
4. Wipe the lost/compromised signer: reset the Ledger, delete the vault item (and rotate the vault password).

**Two or more root signers compromised, or two or more lost** (below threshold): the pinned root cannot sign or cannot be
trusted. Generate a full new set (steps 2 to 4) and ship a client that pins it. Running clients trust only the old pin, so if
two old keys are still held (not compromised), do the single-replacement flow twice in turn; if not, publish the new root
out of band and have users reinstall (state is lost only if they did not back up per the recovery design).

## Quick reference

```
gen-root         --out <abs path>                         # software key on removable media (legacy, offline)
gen-root         --to-stdin-for-vault [--name <n>]        # software key straight into the PERSONAL vault, no file
show-root        --root-key <file> | --root-key-stdin
draft            --expires <t> [--online-pub <hex> --key-id <id>] [--prev <file>] [--revoke-key-id <id>] [--out <file>]
sign-delegation  (--root-key <file> | --root-key-stdin)... --expires <t> [--online-pub <hex> --key-id <id>] [--prev <file>] [--out <file>]
revoke           (--root-key <file> | --root-key-stdin)... --prev <file> --key-id <id> [--expires <t>] [--out <file>]
cosign           (--root-key <file> | --root-key-stdin) --in <file> [--out <file>]
external-payload --in <file> --signer-pub <hex> [--frame solana-offchain-v0|raw] --out <file>
external-attach  --in <file> (--from-helper <file> | --pub <hex> --sig <b64|hex>) [--frame ...] [--out <file>]
pin-snippet      --root <pub>=<frame> x3
verify           --in <file> --roots-file <file>
inspect          --in <file>
<t> = YYYY-MM-DD | YYYY-MM-DDTHH:MM:SSZ | +Nd (max 400 days)

ledger-sign.mjs  check | pubkey [--path P] [--display] | sign --payload <file> [--path P] [--out <file>]
```

## What is unverified (no physical Ledger was available)

Verified in software: the verifier logic (TS and `verify.mjs` agree on every case, golden envelope vector from an
independent implementation), the ceremony commands end to end with a software stand-in that signs the same bytes a Ledger
would, and the helper's envelope parsing, signer check and local signature verification against a fake device object.

Taken from the Ledger Solana app source and docs, **not exercised on a device**:
- that the Nano S+ app (1.15.x) accepts this ~223-byte format-0 message and signs it (path `44'/501'/0'`), with blind
  signing off;
- that the signature is plain Ed25519 over the whole envelope (read in `set_result_sign_message`), so it verifies in
  WebCrypto and `node:crypto`. The helper refuses any signature that does not verify locally, and `external-attach` refuses
  it again, so a wrong assumption fails loudly at the ceremony, not in production;
- the on-device paging of the text (and Expert mode fields), so the exact screens may differ from the description above;
- that `hw-app-solana` 7.12.0 and `node-hid` 2.1.2 work together on Node 24 and macOS (native build, USB permissions).

If the first dry run shows a different signed byte sequence, the fix is local: change `frameDelegationPayload` in both
verifiers and the golden vector, and keep the pin's `frame` id, before anything is pinned.
