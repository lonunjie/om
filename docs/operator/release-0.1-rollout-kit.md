# Release candidate 0.1.0 and alerts: operator rollout kit

**Status: prepared 2026-11-22; nothing in this kit has been run against production.** The tree carries a release-candidate version, the
checks and dry runs below were run on a build host, and every step that changes what visitors see or what runs in an account is yours.
Commands assume the operator workstation with the repository at `~/dig/lonunjie/om`. Nothing here prints, reads or needs a release key
until step 5, and step 5 is yours.

## Design sources read

[`NOTIFICATIONS_AND_PUSH.md`](../architecture/client/NOTIFICATIONS_AND_PUSH.md), [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md),
[ADR-0210](../../decisions/ADR-0210-push-wake-through-the-persons-own-home.md) (decision and metadata table), the existing
[`deploy/CUTOVER.md`](../../deploy/CUTOVER.md), [`deploy/README.md`](../../deploy/README.md), [`REBUILD-AND-COMPARE.md`](../../deploy/REBUILD-AND-COMPARE.md) and the relay
deploy template report. Not re-read in full: [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md), [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) and [`HOME_WAKE_CHANNEL.md`](../architecture/providers/HOME_WAKE_CHANNEL.md) (their rules are
applied only through the existing tooling and the ADR).

## What is being released

| Piece | From | To | Why |
|---|---|---|---|
| Web app (`apps/web`) | 0.0.12 (live since 2026-11-04) | **0.1.0** | About 170 commits on `main`, including alerts. The service worker embeds the version, so each release needs a new one. Versions are strict `x.y.z`; a pre-release suffix such as `-rc.1` is refused by the update check, so "candidate" is a state of this process, not part of the number |
| Per-user delivery service template (`infra/cloudflare-home`, `lonunjie/om-relay`) | 0.2.0 | **0.3.0** | Adds migrations `v3` (usage counter), `v4` (recovery store), `v5` (push alerts, the `PushHub` class). The digest log refuses a repeated version |
| The operator's own Home (`om-home-lonunjie`) | deployed 2026-10-28 | same source | Needs the same `v5` migration before alerts can work on it |

What the web 0.1.0 build contains is whatever `main` contains with no feature flags set: Rooms are reachable (see the declaration proposal),
calls and expressive media are off because their build switches are refused in a production build. Decide the declaration question in
[`release-declaration-proposal.md`](release-declaration-proposal.md) **before** step 5; it can change what the build must hide.

## Rules for the whole kit

- Never push the site repository's `main` until step 7. A push to its `main` is a production deploy.
- Never put a release key, a token or a password in a command line, a file in the repository, or a log. The commands below read keys
  from the agent vault in memory only.
- Run the checks on a clean checkout of `main`. The relay builder refuses a dirty tree, and the provenance names the commit.
- Stop at the first failure and keep the output; do not retry with different flags.

## Step 0: decisions and evidence you need first

1. The release declaration choice (proposal linked above). If groups should not be visible in 0.1.0, say so now.
2. The real-phone alerts check is **not done** and cannot be done before step 4: no real iPhone or Android phone has run the shipped
   client. Plan to do step 6 before telling anyone alerts work. Until then the support matrix says "built, not released".
3. The delegation expires **2027-01-26** ([`apps/web/release-delegation.json`](../../apps/web/release-delegation.json)); renew about 30 days before. A release signed after expiry is refused by the signer.

## Step 1: confirm the checks on `main` (no account, no network)

On a clean checkout (the build host is `emeth4`; use `bin/lonunjie-heavy` from the control-plane root, never a heavy run on the Mac):

```sh
cd ~/dig/lonunjie/om
make check
pnpm typecheck
pnpm --filter @om/core-wasm build:wasm && pnpm --filter @om/core-wasm build:wasm:groups && pnpm --filter @om/core-wasm build:wasm:expressive
pnpm --filter @om/web check:size-budget        # must print "size budget: ..." and exit 0
pnpm --filter @om/web repro                    # two cold builds must be identical
cargo test --workspace                         # not --all-features; `make core-check` has known formatting drift
pnpm exec vitest run infra/cloudflare-home/test/upgrade-v5.test.ts
```

The size budget is the ADR-0187 limit on how much larger the groups module is than the base module. A failure means the fallback in ADR-0187
(option D) is reconsidered; it is never loosened by editing [`apps/web/build/size-budget.json`](../../apps/web/build/size-budget.json). The expressive module is a separate, flag-gated
artifact in its own dist and is not part of a plain release.

## Step 2: dry runs (safe at any time)

```sh
# The Worker exactly as a deploy would upload it: no account, token or network.
cd ~/dig/lonunjie/om/infra/cloudflare-home && pnpm run bundle     # expect bindings QUEUES KEYS DESCRIPTORS INTRO BLOBS RECOVERY USAGE PUSH and migration v5

# A full signed release with a THROWAWAY key and root (generated in memory, deleted afterwards).
cd ~/dig/lonunjie/om/apps/web && pnpm run release:dry-run            # web bundle
cd ~/dig/lonunjie/om/apps/web && pnpm run release:dry-run relay      # delivery service template
```

`release:dry-run` verifies the result under the throwaway root and then checks that the pinned production roots **refuse** it; if they ever
accepted it, the script fails loudly. It writes nothing under `apps/web/release/`, appends to no log and never reads the vault.

The v5 upgrade test (`infra/cloudflare-home/test/upgrade-v5.test.ts`) boots the last Worker before alerts (migrations up to `v4`), stores a
queue, messages and a blob, restarts the same storage under the current Worker, and checks that everything is still served byte for byte, that
the old queue takes new messages and an alert registration, and that a further restart loses nothing. It does not prove the real service
applies `v5` to a real account; step 3 is that proof.

## Step 3: deliver the Worker first (yours; changes a real account)

Alerts need both ends, so the Home goes first and the web app second. A web build that meets a Home without alerts simply says
"Your delivery service does not offer alerts" in Settings > Alerts.

**A. Your own Home** (`om-home-lonunjie`, the SeedLife Cloudflare account). Follow the deploy plan in
[`2026-10-28-relay-status.md`](../reports/2026-10-28-relay-status.md) section "Deploy plan" (short-lived token from the account minter, never printed):

```sh
cd ~/dig/lonunjie/om/infra/cloudflare-home
# export CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID as that plan describes, then:
pnpm exec wrangler deploy            # applies migration v5 (adds the PushHub class; nothing is dropped or renamed)
unset CLOUDFLARE_API_TOKEN
```

Check afterwards (no credential needed): `curl -s https://om-home-lonunjie.majal.workers.dev/.well-known/om-provider-capabilities`. The feature
`om.relay.push.v1` is listed only if the Home has owner admission configured (`OM_ADMISSION`); I could not verify whether that Home has it
set. A Home without admission never offers alerts by design.

**B. The per-user template** (what other people deploy). From a clean checkout of `main` after the web and relay versions are committed:

```sh
cd ~/dig/lonunjie/om
node infra/cloudflare-home/release/build.mjs --sign-from-vault --append-digest-log   # reads the online key in memory
node infra/cloudflare-home/release/verify-relay.mjs infra/cloudflare-home/dist/relay-template/om-relay-0.3.0
node infra/cloudflare-home/release/build.mjs --compare infra/cloudflare-home/dist/relay-template/om-relay-0.3.0
```

Commit the new line in [`deploy/release-log/relay-digests.jsonl`](../../deploy/release-log/relay-digests.jsonl) (and any new `delegations/` file), then publish the folder's content to the
template repository `lonunjie/om-relay` (public, outward-facing: your go-ahead). People who already deployed update by replacing the release
files and running `node verify.mjs`, as the template README says; their data is kept. Untested against the real service: that Workers Builds applies migrations `v3` to `v5`
in one step on an account still at `v2`.

## Step 4: build and sign the web release (yours: uses the online key)

```sh
cd ~/dig/lonunjie/om/apps/web
node build/release.mjs --sign-from-vault --append-digest-log
cd release/om-web-0.1.0 && node verify.mjs --require-signed --require-provenance
```

Commit the digest-log line. Rebuild-and-compare as a third party would: [`REBUILD-AND-COMPARE.md`](../../deploy/REBUILD-AND-COMPARE.md).

## Step 5: stage on a branch of the site repository and verify the preview

Exactly as [`deploy/CUTOVER.md`](../../deploy/CUTOVER.md) steps 2 to 4: stage with `node deploy/site-stage.mjs <release folder> <website repo>` on a
**branch** of `om.lonunjie.org`, push the branch, open the preview `https://<hash>.om-lonunjie.pages.dev`, and check headers equal `apps/web/csp.ts`,
the service worker registers, the manifest and icons load, the page installs, offline reload works. Also reload once **under the worker**: Pages
answers `/index.html` with a redirect, and the worker must still serve a 200 (the 0.0.2 failure mode).

## Step 6: real-device alerts check (before you tell anyone)

On the preview or the live site, with a Home that offers alerts. Record each result in the V-006 and V-007 device matrices, not in chat.

1. **Android, Chrome:** install OM, open Settings > Alerts, turn alerts on (grant the permission), close OM completely, send a message from another identity. Expect one notice that says only "New message".
2. **iPhone, Safari:** open the site in Safari: Settings > Alerts must say to add OM to the Home Screen first. Add it (Share, Add to Home Screen), open it **from the icon**, turn alerts on, close it, send a message. iOS or iPadOS 16.4 or later is required.
3. Also try with the phone locked, in power saving mode, and after force-quitting OM. The copy says alerts are best effort; write down what really happens. If any result contradicts that copy, fix the copy before release.
4. Turn alerts off on each phone and send again: expect nothing.

## Step 7: go live (yours only)

Merge the staged branch to the site repository's `main`. That push is the production deploy. Then:

1. Fetch `https://om.lonunjie.org/bundle-manifest.json` and confirm version 0.1.0 and the manifest digest equal the digest-log line.
2. Update the support matrix labels to what step 6 showed, and the SNAPSHOT.
3. Tell no one that alerts work until step 6 has results.

## Rolling back

| What | How | Limits |
|---|---|---|
| Alerts only, on a Home | Set the variable `OM_PUSH` to `0` on that Worker and redeploy (a configuration change, no code revert). The Home stops offering alerts and answers 404 on the push paths; the app then says the delivery service does not offer alerts. No data is touched. | Devices stay registered inside the Home until they lapse (30 days) but nothing is sent |
| Worker code | Redeploy the previous template release (0.2.0) or the previous Worker version. The `v5` migration only added a class, which older code ignores, so nothing is lost. | Cloudflare may refuse to roll back across a Durable Object migration; I could not check the current rule, so confirm it in the Cloudflare documentation before relying on the dashboard rollback and prefer the redeploy |
| Web app, before step 7 | Delete the staged branch. Nothing live changed. | None |
| Web app, after step 7 | Revert the merge commit on the site repository's `main`, or use the previous deployment in the Pages dashboard ([`CUTOVER.md`](../../deploy/CUTOVER.md) step 5). New visitors get the old bytes. | **Anyone who already updated keeps 0.1.0**: the update check refuses any version lower than the highest one a device has accepted, and the release tool has no rollback marker option. To take 0.1.0 away from installed copies, ship **0.1.1** with the fix (or the older code) through steps 4 to 7 |
| A bad key or delegation | Follow [`KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md): revoke the online key in a new delegation version | An expired delegation stops all updates |

## Measured on 2026-11-22 (build host, tree of commit 3c86ea1 plus this change)

| Check | Result |
|---|---|
| Core wasm, cold build twice (`pnpm --filter @om/web repro`) | Identical: core `7975ccf7ea90`, groups `4508412a3d01`; two clean bundle builds give an identical `bundle-manifest.json` |
| Size budget (groups module over base Core) | base 7,527,415 raw / 2,113,589 gzip; groups 9,432,336 / 2,728,007; delta **+1,904,921 raw / +614,418 gzip** against limits +2,169,546 / +652,950 (margin 264,625 raw, 12.2 percent; 38,532 gzip, 5.9 percent). The budget file's tolerance is 0 percent, so it is the ADR-0187 baseline itself; nothing was trimmed or re-baselined |
| Expressive Core (flag-gated, own dist, not in a plain release) | 9,831,336 raw, which is +2,303,921 over the base Core; no size budget covers it (ADR-0187 is about the groups module). Decide a budget before it ships |
| Signed-release dry run, web | `om-web-0.1.0`, 21 files, 18,105,103 bytes in `site/`; verifies under the throwaway root; refused by the pinned production roots |
| Signed-release dry run, relay template | `om-relay-0.3.0`, Worker 161,770 bytes (sha256 `10753ea3...`, the same on two hosts); verifies under the throwaway root; refused by the pinned production roots |
| `wrangler deploy --dry-run` of the Home | Worker 157.98 KiB (37.95 KiB gzip), bindings include `PUSH (PushHub)`; the 2026-11 pre-alerts Worker was 130.89 KiB |
| v5 upgrade test | Passes: queue, messages, acknowledgement state and a blob survive; the old queue takes new messages and an alert registration; nothing is lost on a further restart |

`build/release.test.ts` has four tests that run the whole release build (including both wasm rebuilds) under a 120 second limit; on the
build host they hit that limit (the other 22 pass), so run them on a fast machine or with a longer limit before relying on them. The dry
runs above cover the same path.

## What I did not do

No deploy, no tag, no release, no push to the site repository, no use of any release key, no change to [`machine/repository_contract.json`](../../machine/repository_contract.json).
Verified in the build host dry runs only; the dates, sizes and test results are in the commit message and in the memory note for this slice.
