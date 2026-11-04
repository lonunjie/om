# Attachments, Home side: the bounded blob spool (WP3a)

- **Date:** 2026-11-04. **Scope:** `provider_reference`, `infra/cloudflare-home` and `packages/om-home-wire`. **Status:** built and tested in workerd; **not deployed** (it needs migration `v2` on a live Home, which is the operator's call), adapter and Core not started.
- **Result in one line:** a Home with owner admission can hold up to 32 MiB of client-side-encrypted blobs (4 MiB each, 64 at most, 7 days by default) in one SQLite Durable Object, with no R2 and no card. Feature id `om.blob.spool.v1`.

## Design sources read

Read in full: the eight constitution documents (`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE`, `LICENSING_AND_CONTRIBUTION_POLICY`, `PROJECT_STEWARDSHIP_AND_CONTINUITY`, `OPERATORLESS_CORE_AND_ECOSYSTEM`, `PRIVACY_GOVERNANCE_AUTHORITY`, `LEGAL_AND_OPERATIONAL_BOUNDARIES`, `PROTOCOL_EXTENSIBILITY`, `NAMING`), `ARCHITECTURE_RESOLUTIONS_20260923`, `MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE`, `SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY`, `PROTOCOL_LONGEVITY_AND_0M_CONTINUITY`, the five discovery documents, `MULTI_RELAY_AND_DELIVERY_RESILIENCE`, `NOTIFICATIONS_AND_PUSH`, `STORAGE_AND_BLOB_ARCHITECTURE`, `SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION`, `METADATA_THREAT_MODEL`, `PROVISIONING_AND_BOOTSTRAP`, `CLOUDFLARE_REFERENCE_DEPLOYMENT`, `SOFTWARE_SUPPLY_CHAIN_AND_UPDATES`, `IMPLEMENTATION_INTERFACES`, `HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY`, `VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES`, `HOME_WAKE_CHANNEL`, `RELAY_AND_DELIVERY_ARCHITECTURE`, `REACHABILITY_AND_OBJECT_REPAIR`, [`spec/protocol/home-api.md`](../../spec/protocol/home-api.md), the v1 completion plan and the relay deploy template report; plus the spec files [`home-api.cddl`](../../spec/cddl/home-api.cddl), [`events.cddl`](../../spec/cddl/events.cddl) (`attachment-ref`), [`lan.cddl`](../../spec/cddl/lan.cddl) (`fetch_blob`), the numeric-fields, domain-separation and home-errors registries and the ProviderCapabilities schema.

ADRs: 0004, 0012, 0014, 0018 to 0022, 0025, 0031 to 0034, 0036, 0040, 0042, 0043, 0046, 0048 to 0050, 0069 to 0071, 0075, 0078 to 0083, 0094, 0098, 0100, 0104, 0114 read in full (each is a few lines). **Skimmed, not read in full:** 0102, 0112, 0122, 0124 (decision sections only), 0126 (decision list), 0128, 0129, 0140, 0159 (headings and the lines that mention blobs, quotas, features or admission), 0130 and 0185 (read for their decisions, consequence sections skimmed), 0150 (decision 1, offline-local delivery and direct file transfer; the rich-list and native-target decisions were not read). **Not read:** [`PROVIDER_CONFORMANCE_SUITE.md`](../architecture/providers/PROVIDER_CONFORMANCE_SUITE.md) beyond PC-C01 (the quota case), the relay threat model, and the ADRs' own consequence and verification sections.

What the reading changed in the build:

| Source | Effect on the build |
|---|---|
| STORAGE, CLOUDFLARE_REFERENCE, PROVISIONING 5 to 6, ADR-0034 | A bounded DO spool, not R2; thresholds are benchmarks, not protocol constants, so they live in `limits.ts` and are advertised, never put on the wire. A Free-plan DO limit is a hard stop, so a full spool answers `quota` with the `hard` flag. |
| STORAGE, REACHABILITY, ADR-0150, ADR-0075 | BlobID stays random and E2EE-only: the Home never sees it, addresses an object by a capability prefix and does not re-encrypt or rewrite anything. Integrity is the event's `ciphertext_digest`, so the Home verifies and stores that digest and never alters bytes. Local or spool receipt is not durability; the client must not claim it. No `BlobLocationRecord` repair endpoint is built (separate operation). |
| home-api 3.1 and 3.2, ADR-0141 | Capabilities only in the `Authorization` header or the CBOR slot, never in the URL; at least 256 bits; object-scoped; `no-store`, `no-referrer`, no cookie, no log. |
| METADATA_THREAT_MODEL 7, 14 | The Home sees ciphertext size, timing and a locator; no filename, MIME type, conversation or account id ever reaches it. Padding is the client's job and the UI must not say size is hidden. |
| INVITES_INTROGATE 5 | A ticket allows "no arbitrary attachment upload", so a stranger can never use the spool. Only the owner's admission creates blobs. |
| OPERATORLESS, LEGAL, 0M, ADR-0043, ADR-0183 rule | The spool exists on a person's own Home, behind their admission, so an open Home never becomes free public storage and the operator of a Home answers for what it holds. Nothing depends on SeedLife's account. |
| PROTOCOL_EXTENSIBILITY, home-api 1 and 3 | An optional, advertised feature id; unknown ids are ignored; no frozen CDDL changed. |
| SUPPLY_CHAIN, the template report (D13, D15 to D17) | A new Durable Object class needs migration `v2` in both `wrangler.jsonc` and the template's [`wrangler.json`](../../infra/cloudflare-home/release/template/wrangler.json); the next signed relay release carries it. Admission semantics (D13/D17) are reused unchanged. |
| HOME_WAKE_CHANNEL, ADR-0185 | Pattern for an optional feature: kill switch (`OM_BLOB_SPOOL=0`), absent means `404`, tests named PC-S. |
| V1 completion plan 3 | Small files use the spool, larger ones go direct or to an opt-in BlobStore; the refusal "before sending, in plain words" is a client duty that needs the advertised limits. |

## What was built

Commits: `81101b8` (wire layer), `d428beb` (Home), and this report.

| Piece | Where |
|---|---|
| Bodies, DS-HASH ciphertext digest, locator, digest header, feature id | `packages/om-home-wire/src/blob.ts`, `index.ts` (`FEATURES.blobSpool`) |
| The spool (one SQLite Durable Object class `BlobSpool`, 1 MiB chunks, derived read capability) | `infra/cloudflare-home/src/blobs.ts` |
| Routing, admission gate, capability advertisement | `src/index.ts`, `src/capabilities.ts`, `src/env.ts`, `src/limits.ts` |
| Migration `v2` and binding `BLOBS` | `wrangler.jsonc`, `release/template/wrangler.json` (the no-drift test still holds) |
| Tests | `test/blobs.test.ts` (PC-S01 to S14), wire tests in `wire.test.ts`, template test now expects five Durable Objects |

### Operations (reference HTTP binding)

| Operation | Binding | Authority | Notes |
|---|---|---|---|
| put | `POST /v1/blobs`, CBOR `BlobPutRequest` | owner admission header (`om-admission`) plus the owner `blob_cap` in the CBOR slot | the sender's digest travels in header `om-ciphertext-digest` (base64url, 32 bytes, required); the Home recomputes and refuses a mismatch with `invalid_request`; answer `BlobPutResponse` |
| get | `GET /v1/blobs/{locator}` | read `blob_cap` or owner `blob_cap` in `Authorization: Bearer` | raw ciphertext, `application/octet-stream`; the Home re-hashes before serving |
| delete | `DELETE /v1/blobs/{locator}` | owner `blob_cap` only | idempotent; absent is success-equivalent; a valid read capability gets `unauthorized` |

`locator` is the lowercase hex of the 16-byte routing prefix of the capability (same idea as the wake channel's `om.q.` label): it names the object and grants nothing. A wrong capability, an absent locator and another blob's capability all answer `not_found` (no existence oracle).

Errors: per-object over `max_blob_bytes` is `payload_too_large` (also when the body exceeds the read bound); the spool full by bytes or count is `quota` (class 4, hard flag); an unreadable retention is `invalid_request`; after expiry and before purge `expired`; a corrupt object is deleted and reported `not_found`.

### The cap and why

| Value | Default | Why |
|---|---|---|
| `max_blob_spool_bytes` (registered field `max_blob_spool_bytes` of the numeric-fields registry; a conservative Home-wide value here) | 32 MiB | a few photos handed over for a week on a free-plan Home; far below the 5 GB SQLite storage a Free account has, so storage is not the binding limit, request and row quotas are |
| `max_blob_bytes` | 4 MiB | one put, decoded and hashed, is about three copies in memory, inside a Worker's 128 MB and a Durable Object's row limit (stored as 1 MiB chunks, below the 2 MB row bound) |
| `max_blobs` | 64 | bounds rows and the cost of a purge |
| retention | 7 days default, 14 maximum (the queue's values) | aligned with queue retention; a longer or zero request is refused, never silently shortened, as for sealed wrappers |

All four are operational values in `limits.ts`, advertised in `ProviderCapabilities.limits` (`max_blob_spool_bytes`, `max_blob_bytes`, `max_blobs`, `blob_default_retention_seconds`); the feature id appears in `features` and `blob` in `provider_kinds` only when the spool is on.

### When the spool is offered

Only when `BLOBS` is bound, `OM_BLOB_SPOOL` is not `0`, **and** the Home has owner admission (`om.admission.v1` configured). Otherwise it is neither advertised nor served (`404`). Consequences: a Worker updated without the new migration, a shared or test Home and a Home without admission keep working exactly as before (tested: queue create, deposit and drain on a Home with no `BLOBS` binding).

## Decisions I took where the design is silent (please review)

| ID | Decision | Basis | If the operator disagrees |
|---|---|---|---|
| A | **Field meaning of the frozen bodies.** `BlobPutRequest` key 2 is the owner capability the sender chose; `BlobPutResponse` key 1 is a read capability the Home derives, key 2 is the digest the Home computed, key 3 the expiry. | The CDDL names shapes, not meanings; this reading gives the sender two authorities (the recipient who gets the read capability cannot delete) and needs no new field. | Swap the meanings in `blob.ts`; the Home logic is unchanged. |
| B | **Digest carrier.** "Verify the ciphertext digest on put" needs the digest in the request, and the frozen `BlobPutRequest` has no slot. It travels in the transport header `om-ciphertext-digest`. The CDDL comment says transport headers are outside the CBOR bodies. | home-api section 3: additional behaviour only as an advertised optional capability; precedent D7 and D13 (`om-admission`). | An ADR could add a body field in a new feature version; the header is then dropped. |
| C | **Derived, not stored, read capability.** The Home derives it by HMAC from a per-spool secret and the owner capability hash, so an exact retry returns the same capability without keeping it. | Idempotency across restart (home-api 5) without storing a capability. | Store the hash and return the capability once from the idempotency table (stores a bearer secret for 7 days). |
| D | **One spool per Home.** The quota is the Home's, not per member. | A personal Home has one owner. | A hosted multi-tenant Home (ADR-0112) needs a spool per member; the capability prefix would then carry the shard. Not built. |
| E | **`quota` versus `payload_too_large`.** One object over `max_blob_bytes` is `payload_too_large`; the spool being full is `quota`. | home-api lists both for put; PC-C01 asks for a typed `quota` for the spool limit. The brief said "quota becomes `payload_too_large`"; I read it as the per-object case. | One-line change in `blobs.ts` if the spool-full case should also be `payload_too_large`. |
| F | **Blob delete is an extra operation** (the table has put, get and repair only; `StoragePool.delete_ciphertext` exists in IMPLEMENTATION_INTERFACES). | Needed for expiry-on-demand (Q-026) and so a sender can free its own space. | Remove the route. |
| G | **Licence header.** New files carry MPL-2.0 like their neighbours, although LICENSING section 1 says AGPL for the relay. | Same open follow-up as finding 1 of the template report; changing headers piecemeal would be inconsistent. | Part of the coordinated header change. |

I shipped the commits with `Design-Departures: none` because B, E and F are additions under the home-api permission for advertised optional capabilities (as D7, D13 and the wake channel were); the operator may prefer to record them as D23 to D25 in the relay status report.

## What the adapter and Core must add (not done)

Adapter (`packages/om-provider-http`, owned by another agent):

1. Read `om.blob.spool.v1` and the four limits from the capabilities document; never assume the spool.
2. `putBlob(ciphertext)`: generate `newBlobCap()`, compute `blobCiphertextDigest`, send the owner admission header and `om-ciphertext-digest`, then **compare the response digest with its own** and the read capability's prefix with the locator; delete on mismatch. Retry with the same request id.
3. `getBlob(locator, readCap)`: fetch, recompute the digest and compare with the `AttachmentRef.ciphertext_digest` before handing bytes to Core; map `not_found`, `expired`, `quota`, `payload_too_large` to typed results.
4. `deleteBlob(locator, ownerCap)`.

Core and UI:

1. `AttachmentRef` path: random 32-byte `blob_id`, content encrypted before upload, `ciphertext_digest` per `OM-BLOB-CIPHERTEXT-HASH-v1`, the Home URL in `locations`, the **read capability in the `repair_capability` slot** (the only capability slot of the frozen `attachment-ref`; the LAN `fetch_blob` also calls key 10 "blob/read capability"); keep the owner capability locally to delete.
2. Refuse **before sending** when the file exceeds `max_blob_bytes` (or the remaining spool, which a put reports as `quota`), in plain words, and offer to send when the other person is online (completion plan section 3); the direct online and LAN paths carry the same `BlobID` and digest.
3. Show "delivered" separately from "stored for later" (ADR-0150): spool receipt is a seven-day hand-over, not configured durability.
4. Fetch from the **sender's** Home with the read capability; the recipient never needs admission.
5. Pad large objects only through a profile that says so (METADATA 7); never imply size is hidden.

## Not verified, not built

- Not deployed. The live default Home needs migration `v2` and an admission value before the feature appears; a new signed relay release (`build.mjs`) is needed for per-user template deployments. Neither was done.
- Unmeasured on the real service: Durable Object RPC with a 4 MiB body, request and row quotas of a Free account under attachment load. The limits are conservative guesses, to be checked by the first live test.
- `POST /v1/blobs/repair` (BlobLocationRecord set) and the byte-range or resumable transfer for larger files are not built (not in the spool's remit).
- A spool per hosted member (decision D) and a `usage_state` figure for spool bytes (PC-C03) are not built.
- The rest of the repository validator still reports unregistered files that belong to other agents' uncommitted work (`packages/om-ui/src/home/*`); my files are registered.
