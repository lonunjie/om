# OM relay (your own delivery service)

This repository deploys one thing to **your own Cloudflare account**: the OM delivery service (relay) as a Worker with SQLite Durable
Objects. It needs no payment method on the free plan. Nobody else runs it, sees into it, or can switch it off for you. It stores only
sealed (end-to-end encrypted) objects it cannot open.

You normally reach this page from the OM app's setup. If you came here directly, start in the app: it creates the one value you paste below.

## What happens when you deploy

1. Cloudflare copies this repository into your GitHub or GitLab account and asks for one value, `OM_ADMISSION`: the setup value OM showed you.
2. Cloudflare builds and deploys. The build step is `node verify.mjs`: it refuses unless `relay/worker.js` is the signed OM relay release
   named in `release.lock` (pinned root key, delegation, release signature, file hashes, provenance), and unless [`wrangler.json`](wrangler.json) and
   [`package.json`](../../../../package.json) carry nothing but the signed configuration. If anything differs, **nothing is deployed**.
3. `wrangler deploy` uploads `relay/worker.js` as it is (no bundling, no build from source) with the signed [`wrangler.json`](wrangler.json).
4. Your relay answers at `https://<worker name>.<your account>.workers.dev`. Type that address into OM.

## Check it yourself

The verifier in this repository cannot vouch for itself. To check the template with an independent copy, use a trusted checkout of
[lonunjie/om](https://github.com/lonunjie/om) (Node 24, `pnpm install --frozen-lockfile` once):

```sh
cd /path/to/om
node infra/cloudflare-home/release/verify-relay.mjs /path/to/your/clone
```

To check that the Worker is what the public source produces, check out the commit named in `relay-manifest.json` (`createdFrom.gitCommit`) and run:

```sh
cd /path/to/om && node infra/cloudflare-home/release/build.mjs --compare /path/to/your/clone
```

## Without GitHub (command line)

```sh
git clone <this repository> om-relay && cd om-relay
node verify.mjs && npx wrangler deploy      # you will be asked to sign in to Cloudflare once
```

Then set the setup value as a secret: `npx wrangler secret put OM_ADMISSION`.

## Updating

The updater is manual in this version. A new signed release is a new version of this template; replace `relay/`, `relay-manifest.json`,
`SHA256SUMS`, `provenance.intoto.jsonl`, `release.lock`, `verify*.mjs`, [`wrangler.json`](wrangler.json), [`package.json`](../../../../package.json) and `pnpm-lock.yaml` with the new ones, run
`node verify.mjs`, commit. `release.lock` also takes a `minVersion` floor that refuses older releases. An automatic, staged updater
that checks independent transparency monitors is specified (CLIENTHOST_UPDATE_LIFECYCLE section 4) and not built yet.

## Limits you should know

- **You are the operator of this relay.** It runs in your Cloudflare account, under your account's terms. OM and SeedLife give no warranty
  or service level, cannot see into it and cannot switch it off. The OM project is in open beta and has had no human security audit.
- **One account is one observer.** Your relay, your contact page and your mailbox live in the same Cloudflare account, so whoever controls that
  account (and Cloudflare) could link them. Protect it with a passkey and a unique password. Your relay address is trusted on first use:
  a contact who first reaches it cannot yet prove it is yours. A single relay is also a single point of failure; OM can use more than one.
- If the setup value is empty or malformed, your relay **refuses to create anything** (it fails closed) and does not advertise
  `om.admission.v1`; the OM app checks for that when you enter the address and tells you to fix the value.
- The release signature has one human custodian (2-of-3 offline root, equal to one key against coercion of that person) and is not yet
  watched by independent monitors.
- A new release (including a security fix) only reaches you when you take it; nothing tells a stale clone that one exists, so watch
  [lonunjie/om](https://github.com/lonunjie/om) releases and security advisories. The release delegation expires on **2027-01-26**: after that
  a *rebuild* of an old clone fails closed until you take a newer release; a relay that is already running is not affected.
- Cloudflare's free plan limits apply; hitting a limit makes operations fail, never bills you. The wake channel can be turned off with
  `OM_WAKE=0`, but that changes the signed configuration, so it needs a release that ships it.
