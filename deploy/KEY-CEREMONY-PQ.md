# OM release key ceremony, addendum: post-quantum (ML-DSA-65) co-signature

Addendum to [`KEY-CEREMONY.md`](KEY-CEREMONY.md), which it does not change. Decision and rules:
[ADR-0182](../decisions/ADR-0182-post-quantum-co-signature-on-the-release-root-delegation.md) (operator decision 2026-10-28).
Do the original ceremony first (Ed25519 roots, first delegation, pins committed); this adds one more **required** signer on top.

## The model

| Key | Where it lives | What it signs | Lifetime |
|---|---|---|---|
| Ed25519 roots (Ledger A, Ledger B, vault PEM), 2 of 3 | unchanged, see [`KEY-CEREMONY.md`](KEY-CEREMONY.md) | the delegation | years |
| **PQ root** (ML-DSA-65, FIPS 204, one software key) | ONLY your **personal** Vaultwarden, as a secure note holding a 32-byte seed. Never on disk, never in the agents' vault `vw.agents@lonunjie.org`, never given to an agent. | the same delegation, framed with its own label (ADR-0182 section 2) | years |
| Online release key (Ed25519) | unchanged | each bundle manifest | months |

A delegation is valid only if **2 of 3 Ed25519 roots AND the PQ root** signed it. Until a delegation carries `pqRequired`
(the "epoch", phase C) an Ed25519-only chain is still accepted, so phases A and B cannot strand any browser.

Tool: [`apps/web/build/root-ceremony.mjs`](../apps/web/build/root-ceremony.mjs) (the new commands live in
[`pq-ceremony.mjs`](../apps/web/build/pq-ceremony.mjs)). It needs the workspace install (`@noble/post-quantum` 0.7.1) and Node 24 with ML-DSA in
`node:crypto` (tested on 24.14.0). Every signature is verified by two independent ML-DSA implementations before it is written.

## 0. Prerequisites

```sh
cd ~/dig/lonunjie/om && git pull --ff-only && pnpm install --frozen-lockfile && node --version   # Node 24
mkdir -p -m 700 ~/om-root-work
```

Use the same personal-vault shell as [`KEY-CEREMONY.md`](KEY-CEREMONY.md) step 1 (`BITWARDENCLI_APPDATA_DIR="$HOME/.config/bw-om-root"`, `bw login`, `BW_SESSION`);
`bw status` must show **your personal** email.

## Phase A. Generate the PQ root (any time after `KEY-CEREMONY.md` step 2)

### A1. Generate it straight into the personal vault

The seed goes to stdout as a Bitwarden secure-note item and into `bw create item`; no file, no log. Stderr shows only the public key id
and fingerprint. The command refuses to print to a terminal.

```sh
cd ~/dig/lonunjie/om && set -o pipefail && \
node apps/web/build/root-ceremony.mjs gen-pq-root --to-stdin-for-vault --name "OM release PQ root (ML-DSA-65) 2026-10-28" \
  --pub-out ~/om-root-work/pq-root.json \
  | bw encode | bw create item > /dev/null && echo "stored in the personal vault"
```

Write down the printed **pq root key id and fingerprint** (public). `~/om-root-work/pq-root.json` is the public descriptor (not secret).

### A2. Prove the vault copy is the key, and print the pin literals

```sh
cd ~/dig/lonunjie/om && bw sync && bw get notes "OM release PQ root (ML-DSA-65) 2026-10-28" \
  | node apps/web/build/root-ceremony.mjs show-pq-root --pq-root-stdin --pub-out ~/om-root-work/pq-root-check.json
cmp ~/om-root-work/pq-root.json ~/om-root-work/pq-root-check.json && echo "vault copy matches"
```

The output ends with two paste-ready literals (`PQ_ROOT` for the client, `PINNED_PQ_ROOT` for [`verify-release.mjs`](../apps/web/build/verify-release.mjs)).

### A3. Keep an offline backup (recommended before phase C)

If this seed is lost after browsers have set the PQ floor, those browsers can never accept another delegation (they must reinstall, ADR-0182
section 9). Print the seed once on a trusted machine, put it on paper in a sealed envelope stored apart from both Ledgers' recovery phrases, then
clear the terminal:

```sh
bw get notes "OM release PQ root (ML-DSA-65) 2026-10-28" | grep '^seed:'
```

## Phase B. Co-sign the current delegation and pin the PQ root

Do this once the Ed25519 ceremony has produced a 2-of-3 delegation ([`KEY-CEREMONY.md`](KEY-CEREMONY.md) step 5; PQ can also be attached before the Ledger
signatures, the order does not matter because the PQ entry is outside the Ed25519 signed bytes).

### B1. PQ-sign the delegation

Works on `~/om-root-work/release-delegation.json` (or [`apps/web/release-delegation.json`](../apps/web/release-delegation.json)). `roots.json` is the Ed25519 roots file from
[`KEY-CEREMONY.md`](KEY-CEREMONY.md) step 3; add the PQ descriptor to it so `verify` checks everything:

```sh
cd ~/dig/lonunjie/om && node -e '
const fs=require("fs"),h=process.env.HOME+"/om-root-work/";
const r=JSON.parse(fs.readFileSync(h+"roots.json","utf8"));
r.pq=JSON.parse(fs.readFileSync(h+"pq-root.json","utf8"));
fs.writeFileSync(h+"roots-pq.json",JSON.stringify(r));'
bw get notes "OM release PQ root (ML-DSA-65) 2026-10-28" \
  | node apps/web/build/root-ceremony.mjs pq-sign-delegation --pq-root-stdin \
      --in ~/om-root-work/release-delegation.json --roots-file ~/om-root-work/roots-pq.json
```

Expect `OK: 2-of-3 root signatures verify ... ; PQ co-signature pq-root-... verifies`. The Ed25519 signatures are untouched.

### B2. Pin the PQ root and commit

On the Mac, on top of the commit from [`KEY-CEREMONY.md`](KEY-CEREMONY.md) step 6:

1. `apps/web/src/update/pq-root.ts`: replace `PQ_ROOT = null` with the `PQ_ROOT` literal printed in A2 (leave `PQ_REQUIRED_BY_BUILD = false`).
2. [`apps/web/build/verify-release.mjs`](../apps/web/build/verify-release.mjs): replace `PINNED_PQ_ROOT = null` with the `PINNED_PQ_ROOT` literal.
3. `~/dig/lonunjie/registry/signing.toml`: add the `om-release-root-pq` entry (key id, fingerprint, algorithm `ml-dsa-65`, frame `om-pq-v1`, status `active`).
4. Copy the delegation and run the checks (`pq-cosign.test.ts` fails if 1 and 2 differ):

```sh
cp ~/om-root-work/release-delegation.json ~/dig/lonunjie/om/apps/web/release-delegation.json
cd ~/dig/lonunjie/om && pnpm exec vitest run apps/web/build && make web-release-check
```

5. Commit `pq-root.ts`, [`verify-release.mjs`](../apps/web/build/verify-release.mjs), the delegation and the registry change; push.

From now on [`sign.mjs`](../apps/web/build/sign.mjs) and [`release.mjs`](../apps/web/build/release.mjs) **refuse** a delegation without a valid PQ
signature, so a release cannot ship under an Ed25519-only chain. Release as in [`KEY-CEREMONY.md`](KEY-CEREMONY.md) step 7.

## Phase C. The PQ epoch (after PQ-aware clients are live)

Do this after a release that pins the PQ root has been deployed (phase B) and you hold the offline backup (A3). It is a normal delegation
renewal with `--pq-required`, so it needs **fresh Ed25519 signatures** (the marker is part of the signed payload; the Ledger text shows
` | pq required | `) and then the PQ signature.

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs draft \
  --prev apps/web/release-delegation.json --pq-required --expires +90d --out ~/om-root-work/release-delegation.json
```

Then [`KEY-CEREMONY.md`](KEY-CEREMONY.md) step 5 (payload, device, attach for two signers), then B1 (PQ sign + verify), then copy to
[`apps/web/release-delegation.json`](../apps/web/release-delegation.json), commit, release. A browser that accepts this delegation stores the PQ floor and from then on refuses any
chain without a valid PQ signature. The marker is carried by every later `draft --prev`.

Optional hardening in the next client release: set `PQ_REQUIRED_BY_BUILD = true` in `pq-root.ts` and `PINNED_PQ_REQUIRED = true` in
[`verify-release.mjs`](../apps/web/build/verify-release.mjs), so browsers that never saw the epoch also require PQ.

## Renew, rotate the online key, revoke (steps 8 to 10 of `KEY-CEREMONY.md`)

Each of those produces a new delegation, so after the two Ed25519 signers (step 5) **always run B1** before copying the file. Check the result:

```sh
cd ~/dig/lonunjie/om && node apps/web/build/root-ceremony.mjs verify \
  --in ~/om-root-work/release-delegation.json --roots-file ~/om-root-work/roots-pq.json --require-pq   # expect: OK ... PQ co-signature ... verifies
```

## Loss, compromise and rotation of the PQ root

- **PQ key compromised or suspected, Ed25519 roots fine:** not urgent (the attacker still needs 2 of 3 Ed25519 roots). Generate a new PQ root (A1, new
  vault name), sign **one** delegation with both PQ keys (run `pq-sign-delegation` twice with each vault note), ship a client release pinning the new key
  (B2 for the new descriptor, keep both keys in the delegation until old clients are gone), then delete the old vault note and rotate the vault password.
- **PQ key lost:** restore from the offline backup (A3): rebuild a vault note in the same format (`-----BEGIN OM PQ ROOT SEED-----` block, see
  `show-pq-root` for the id/fingerprint it must reproduce). With no backup and browsers that hold the floor, they must reinstall from a verified
  out-of-band release that pins a new PQ key; there is intentionally no way to clear the floor (ADR-0182 section 9).
- **Moving the key onto a Ledger** is a rotation (ADR-0182 section 10) once an ML-DSA signing app exists and is verified; as of 2026-10-28 none is
  confirmed for the Nano S+. The commands `pq-payload` (bytes for a device) and `pq-attach` (ingest and verify its signature) already exist.

## Quick reference

```
gen-pq-root         --to-stdin-for-vault [--name <n>] [--pub-out <file>]   PQ root straight into the PERSONAL vault, no file
show-pq-root        --pq-root-stdin [--pub-out <file>]                      id, fingerprint, paste-ready pins (pipe the vault note in)
pq-sign-delegation  --pq-root-stdin --in <file> [--out <file>] [--roots-file <file>]   sign + attach (then verifies if a roots file with "pq" is given)
pq-payload          --in <file> --pq-root-file <descriptor.json> --out <file>           exact bytes for an external PQ signer
pq-attach           --in <file> --pq-root-file <descriptor.json> --sig <b64|hex> [--out <file>]
draft ...           --pq-required                                            make this delegation the PQ epoch (sticky across --prev)
verify              --in <file> --roots-file <file with "pq"> [--require-pq]
```

## What is unverified

Verified in software only: both verifiers on shared fixtures, the signed bytes against an independent Python golden vector, noble/OpenSSL
interop, the ceremony commands end to end, the floor in the service-worker shell and two full release builds with throwaway keys. **Not done:** a real
ceremony with the personal vault and `bw`; a real browser updating across the epoch; any hardware ML-DSA signer (none confirmed).
