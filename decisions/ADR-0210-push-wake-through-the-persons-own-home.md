# ADR-0210 — Push wake through the person's own Home (`om.relay.push.v1`)

## Design sources read

[`NOTIFICATIONS_AND_PUSH.md`](../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md) (sections 1 to 6 and the 2026-09-22 operatorless refinement, including the per-profile subscription rule),
[`HOME_WAKE_CHANNEL.md`](../docs/architecture/providers/HOME_WAKE_CHANNEL.md) (all; section 5 is the text this ADR answers), ADR-0038, ADR-0178, ADR-0183 (the PR-006 waiver), ADR-0185,
[`PLATFORM_CAPABILITY_MATRIX.md`](../docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md) (the push, notification-removal and payload rows) and the 2026-11-03 delivery report (section 4 and the "Web Push" paragraph). The other documents
the design gate lists for these paths were consulted for the points below and not read in full: [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../docs/architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md), [`SECURITY_PRIVACY_PROFILES.md`](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md), [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md), ADR-0129.

- **Status:** Accepted by operator delegation, 2026-11-18.
- **Scope:** `provider_reference` and `reference_client_ui`. Fills a silence; amends no audited text.
- **Follows:** [`HOME_WAKE_CHANNEL.md`](../docs/architecture/providers/HOME_WAKE_CHANNEL.md) section 5 ("If built later, it is a separate optional feature (`om.relay.push.v1`: register an opaque subscription for a queue, per-profile, revocable, expiring), a user-visible opt-in with a plain disclosure, and a declared linkage point"). This ADR is that feature, nothing larger.
- **Does not amend:** any wire object, key, route or cryptographic rule; ADR-0038 (push stays an opaque optional wakeup); ADR-0185 (the open-app channel is unchanged and still preferred); ADR-0183 (PR-006 stays waived; see "What stays unclaimed").

## Context

The beta cannot alert a person while the app is closed. Section 5 of the wake-channel document listed what Web Push would add (a third party on the path, a stable per-install identifier at the Home, a signing key held by whoever runs the Home, a mandatory visible notification on iOS, a locked vault that cannot show anything but a generic notice) and said it needs an ADR, a plain disclosure, a per-profile subscription and a declared linkage point. A per-person Home removes the main objection: the signing key is held by the person's own Worker, so no SeedLife or other third-party OM server exists.

## Decision

1. **An optional feature `om.relay.push.v1`**, advertised in `ProviderCapabilities.features` with its limits. A Home that omits it is simply not push-capable; the client says so and nothing else changes. It is offered only where the Home has owner admission and the deployment bound it (an open Home never becomes a free push sender). `OM_PUSH=0` turns it off.
2. **The Home holds its own key.** A P-256 VAPID key is made by the Home's Worker on first use and lives only in its Durable Object storage (no secret in the repository or in any deployment variable). The public half is read at `GET /v1/push/key`; there is no rotation in v1 (replacing it would invalidate every registration; devices re-register at each start).
3. **Registration is per device and per mailbox.** `PUT /v1/push/subscription` with the mailbox's receive capability as bearer, the owner's admission secret in the existing header, and the body `{"endpoint": "<https address>"}`. `DELETE` on the same path needs the receive capability only, so a device can always leave. Stored per (mailbox, endpoint), at most 4 per mailbox and 64 per Home, lapsing after 30 days unless the device registers again (it does at each start). The Home keeps the endpoint and nothing else of the subscription: **no p256dh and no auth secret**, because it never encrypts a payload.
4. **The Home sends only where it may.** Endpoints must be https on the default port, a name (not an address), without credentials or fragment, on a known browser push service (Google, Mozilla, Apple, Microsoft), or an origin the owner lists in `OM_PUSH_HOSTS`. A registration can therefore never aim the Home at an arbitrary address. Redirects are not followed.
5. **The wake is empty.** When a new item arrives for a mailbox that has a registration and **no client is connected on the wake channel**, the Home sends each registered endpoint one request with no body, a TTL, normal urgency and a VAPID signature whose subject is the Home's own origin (no operator identity). No content, no item id, no sender, no capability, no AccountID. Paced: at most one push per registration per 30 seconds (an arrival inside the window is covered by the notice already sent) and 100 per UTC day; a registration the push service calls gone is deleted, and five failed sends in a row delete it too. A push can never fail or slow a deposit. While the app is open the wake channel (ADR-0185) does the job and no push is sent.
6. **The client, in the service worker.** On a push event the worker looks for an open OM window. If one is visible it posts it an opaque wake (the page drains as ADR-0185 already does) and shows nothing. Otherwise it shows **one generic local notification** ("New message", never a name, count or text; one tag, so a second push replaces the first). The worker does **not** open the vault, derive a capability or fetch: section 5 of the wake-channel document says a push-time fetch would widen what a locked device exposes and must not be added. Opening the notification opens the app, which fetches normally and decrypts there. (Where a window exists but is hidden, it is told to drain, which keeps its state current without the vault being opened by the worker.)
7. **Opt-in, per device, off by default.** Settings has an "Alerts" page with plain copy, the iOS Add-to-Home-Screen condition, and one switch. Nothing registers until the person turns it on and the browser grants notification permission. **At the Maximum privacy profile it stays off unless the person confirms a stated cost** (the time of each wake is visible to the browser's push service) on the Alerts page; no profile change ever turns it on, and if the profile becomes Maximum later, the next start does not renew the registration (it lapses within 30 days, and the page offers to switch off at once) until the person confirms again. With more than one identity profile on the device the switch is not offered (see "Open question"), because one browser subscription is one endpoint shared by every identity.
8. **One Home per subscription.** A browser subscription is made for one application server key, so it can wake only the Home whose key it holds: the person's active Home. A backup Home does not push in v1.
9. **Absence is graceful.** No Home support, no browser support (no service worker, Push API or Notification API), a denied permission, a vendor service that is down or a lost push: the app behaves as before and the person sees one plain sentence saying why alerts are unavailable. Delivery never depends on a push; opening the app drains everything.

## Metadata seen, by whom

| Party | Sees | Does not see |
|---|---|---|
| Browser push service (Google, Mozilla, Apple, Microsoft) | the endpoint (an opaque per-install address) and the time of each wake, from the Home's address | any content, sender, mailbox, queue, AccountID, count or key; the push is empty |
| The person's own Home | the endpoint, the mailbox it is registered for, when it was registered and when it was last sent to (it already sees every deposit) | nothing new about content or people. New: **which mailboxes one device holds** (they share an endpoint), a link it could not state before except by network address, which it does not read |
| A network observer of the device | a connection to the push service (as for any web app that uses push) | content |
| Another OM user | nothing | everything |

This is the declared linkage point: the endpoint is a stable per-install identifier known to the Home and to the vendor push service. It is not shared with any other Home or any SeedLife service. A per-person Home makes the Home and the owner the same party; the cost that remains is the vendor's view of timing.

## What stays unclaimed

PR-006 is still waived. Background delivery on a phone is best effort: iOS delivers Web Push only to an installed Home Screen app and may delay or drop it; browsers may suspend an idle worker; the person's own Worker free-plan quota applies (each wake while no one is connected costs one extra Durable Object request plus one outbound request, and deposits are already shed at the usage `hard` state, which therefore also stops pushes). No claim of reliable background delivery is made in copy, and the `background_push_none` limit sentence stays accurate until the person has opted in on a device that supports it.

## Where did authority go?

Nowhere new. Registering needs two things the owner already holds (admission and a receive capability); a push confers no authority and carries no data; a lost, spurious or late push costs a notification or a drain at most (the receive capability already authorises `drain`). A hostile Home can send a spurious generic notice; it cannot cause anything else.

## Floors checked

No cryptographic floor is touched (no new key shares a purpose with any OM key; the VAPID key signs push requests only). No metadata floor is weakened: the empty payload keeps content and identity out; the Maximum-profile default is off; the per-profile rule is honoured by not offering the switch where it cannot be kept (below).

## Single-profile alerts: accepted for v1 (operator ruling, 2026-11-18)

NOTIFICATIONS_AND_PUSH requires a distinct service-worker scope and subscription per identity profile. The reference web client has one worker at the origin root, so two profiles on one device would present one endpoint to their Homes. Reading chosen: **the switch is offered only on a device with exactly one identity profile.** The operator accepted this for v1; per-profile service-worker scopes (one subscription per profile) are a later change. The operator also confirmed that "wake, then fetch" means what decision 6 says: the worker shows the generic notice and never fetches; the app fetches when it is opened.

## Consequences

- One new Durable Object class (`PushHub`, migration `v5`, an addition only) and one new binding (`PUSH`) in the Worker and the release template; existing deployments keep working and do not offer the feature until redeployed.
- The service worker gains `push` and `notificationclick` handlers; the content security policy is unchanged (the browser, not the page, talks to the push service).
- Size: no Rust or wasm change.

## Verification

`PC-P01` to `PC-P06` against the real Worker bundle in workerd (advertising and absence, key, registration and its failure shapes, an empty signed push with a verifiable signature, suppression while a socket is connected, pacing and the daily cap, gone endpoints, an unreachable push service never failing a deposit); wire tests; provider tests; service-worker and Settings UI tests; a Chromium end-to-end of the opt-in flow. **Real delivery through a vendor push service cannot be tested in CI** (it needs a vendor account, a real browser profile and, on iOS, an installed app); it is a manual check recorded in the relay-status ledger.
