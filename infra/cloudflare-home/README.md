# OM reference Home on Cloudflare

The first `provider_reference` implementation (ADR-0171): a Worker plus SQLite Durable Objects that implements the
[OM Home API v1](../../spec/protocol/home-api.md) reference HTTP binding as an **untrusted, operatorless-by-design**
store-and-forward Home. It is replaceable and never a protocol dependency (absolute rule 18).

| Role | Durable Object | Endpoints |
|---|---|---|
| Relay (mailbox) | `MailboxQueue`, one per queue | `POST /v1/relay/{queues,deposit,drain,ack,rotate}`, `POST` / `DELETE /v1/relay/deposit-capabilities` |
| Key material | `KeyPool`, one per DeviceID | `PUT /v1/keys/bundles/{device}`, `POST /v1/keys/fetch`, `DELETE /v1/keys/bundles/{device}/generations/{n}` |
| Descriptor store | `DescriptorStore`, one per id | `PUT` / `GET` / `DELETE /v1/discovery/descriptors/{id}` |
| IntroGate | `IntroGate`, one per recipient service | `POST /v1/intro/{tickets,requests,drain}` |
| Capabilities | none | `GET /.well-known/om-provider-capabilities` |

Not implemented and not advertised: sponsorship, recovery records, blobs (so no R2, KV or Queues binding is needed).

## Properties that matter

- **Sees only sealed objects.** A deposit must be a profile-1 `SealedDeliveryEnvelope` or a `MessageRequest`; anything else is
  `invalid_request`. The Home cannot open either.
- **Capabilities are held as hashes.** Deposit, receive and admin capabilities are separate; none appears in a URL, cookie or log.
- **No logs, no addresses.** `observability` is off in `wrangler.jsonc`, no code reads a network-address header (a test enforces it).
- **Typed errors only**, from the registered HomeError set; a forged capability and a full queue answer identically.
- **Limits are typed**: size, per-capability rate, queue items and bytes, retention, replay set, ticket uses.

## Develop and test (no Cloudflare account, no network)

```sh
cd /path/to/om
pnpm install --frozen-lockfile
pnpm exec vitest run infra/cloudflare-home        # conformance cases against the real Worker bundle in workerd
node infra/cloudflare-home/test/serve.ts 8788     # the Home on http://127.0.0.1:8788 for local manual runs
```

`wrangler` 4.100.0 and `miniflare` 4.20260611.0 are pinned devDependencies (the 4.100 line is the last that uses a stable
miniflare). `pnpm approve-builds` is set to deny the esbuild, sharp and workerd install scripts; nothing needs them.

Deviations from the spec and the deploy plan: [`2026-10-28-relay-status.md`](../../docs/reports/2026-10-28-relay-status.md).
Threat model: [`2026-10-28-relay-threat-model.md`](../../docs/reports/2026-10-28-relay-threat-model.md).
