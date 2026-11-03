# Delivery service capacity, and where chat backup stands

- **Date:** 2026-11-03
- **Scope:** `provider_reference`, `reference_client_ui`. Answers the operator's 2026-11-03 questions; no deploy of the Home was changed.

## What the default delivery service is

- It is the reference Home (`infra/cloudflare-home`) running as Worker `om-home-lonunjie` on **SeedLife's own Cloudflare account**, free plan, `OM_HARD_FREE_TIER=1`. Its address is baked into every production build (`apps/web/.env.production`), so people can message with no setup. Yes, it spends our Worker and Durable Object quota.
- It is **not a directory**. Nobody can look anyone up. People find each other only through an invite link or QR, which carries what is needed. The Home stores sealed messages until a device collects them, a short-lived invite descriptor, and published key bundles. It cannot read messages.
- Settings > Message delivery only lets a person point their copy of OM at another Home. **Today both people in a chat must use the same Home** (Core refuses to deposit to a route at another Home), so choosing a different one cuts you off from people on the default. Cross-Home delivery is the missing piece for "no single operator", and it is a Core change (RelaySet lineage, D1/D10 in the relay status report).

## Is it sustainable?

Not on the free plan, and not with a fixed 3 s poll. Every open client asks for new mail; each ask is one Worker request plus one Durable Object request.

| Client behaviour | Asks per day, one tab left open | Share of a 100k/day free limit |
|---|---|---|
| Before 0.0.8: every 3 s, always | 28,800 | about 29% (roughly 3 tabs exhaust it) |
| 0.0.8: 3 s in use, 15 s untouched for a minute, 60 s hidden | 1,440 when hidden all day; about 400 per 20 min of use | a typical person is well under 1,500, so on the order of 70 or more people per day |

Free-plan numbers are from memory of Cloudflare's published limits (Workers 100k requests/day, Durable Objects 100k requests/day and 100k SQLite rows written/day, 5 GB storage); they change, so check the dashboard before relying on them. With `OM_HARD_FREE_TIER=1` the Home returns typed failures at the limit, so messaging for everyone pauses until the daily reset. Nothing is lost.

Path, in order of effort:

1. **Done in 0.0.8:** adaptive polling (`apps/web/src/core/pollPace.ts`), plus an immediate check when the app returns to the screen or the network comes back.
2. **Operator decision, small:** move the Worker to the Workers Paid plan (about 5 USD a month; 10M Worker requests and 1M Durable Object requests a month included, then cents per million). That is roughly ten times the free headroom and removes the hard stop. Add the WAF rate-limit rules still listed as not done.
3. **Engineering, medium:** replace polling with a push wake-up (WebSocket with hibernation on the queue Durable Object, or Web Push). Idle users then cost nothing between messages, which is the real scale lever.
4. **Engineering, large:** cross-Home delivery plus a second, vendor-neutral Home (PR-013), so the default is one choice among several and anyone can run their own.

## Chat backup to R2 or Google Drive

Not built, and not a quick add. Facts:

- Core has no history export yet; `EncryptedBackup` is only a recovery-custodian kind in the API. The policy is [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) (portable secure export under a documented archive-encryption profile) and [`RECOVERY_AND_BACKUP_POLICY.md`](../architecture/identity/RECOVERY_AND_BACKUP_POLICY.md). The archive format, its key, and restoring it to a new device (device linking is also unbuilt, see the multi-device plan) all come before any storage destination.
- Doing it on our R2 would make SeedLife a custodian of everyone's encrypted history: cost grows with users, it is an operator in the middle, and it conflicts with "operatorless" (Where did authority go? To us). Not recommended as the default.
- Google Drive is workable later as a *user-owned* destination (the app-data folder scope, OAuth in the browser, ciphertext only), but it needs a Google Cloud project, a consent screen and verification, and it ties a feature to one vendor; keep it behind a provider-neutral "backup destination" interface.

Recommended order: (1) Core encrypted history archive with a documented format and independent review; (2) "Save a backup file" and "Restore from file" in Settings > Recovery, which works with any storage the person already has (Drive, iCloud, a USB stick) with zero infrastructure of ours; (3) optional destinations the *person* connects (their own S3-compatible bucket or R2, or Drive), never ours by default. Step 1 is security-critical Core work and needs the operator's go-ahead on scope.

## Changes in OM 0.0.8 (this pass)

- Unsent drafts survive Lock now, idle lock, reload and closing the tab. Each draft is sealed by the identity's vault in its encrypted store (key `ui/draft/<chat>`; Core reads only `core/`), written about 0.6 s after typing stops, written first when locking, and removed when the message is sent. `apps/web/src/core/draftStore.ts`.
- Landing footer no longer links to the site source (GitHub is in "Follow along").
- Settings > Help and feedback (`/settings/help`): report a problem, suggest an idea, browse reports, report a security problem privately. All open GitHub issue forms in a new tab; the only thing added is the version number, shown on the page. Discussions are **not enabled** on `lonunjie/om`, so ideas now go to Discussions (enabled 2026-11-03, category "ideas"); bugs use the Issues form in `.github/ISSUE_TEMPLATE`.
- Adaptive polling (above); Block on a message request is now a quiet button so Accept leads; the Recovery "not set up yet" line now says to save the safety code to finish.
