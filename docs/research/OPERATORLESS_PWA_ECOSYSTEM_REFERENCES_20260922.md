# Operatorless / PWA / Ecosystem References — 2026-09-22

These references informed the 07:34 PHT architecture checkpoint. Provider/store/law details are time-sensitive and must be revalidated before implementation/release decisions.

## Cloudflare

- Deploy to Cloudflare buttons — deploy public Worker applications into a user's own account; supported resources can be automatically provisioned: https://developers.cloudflare.com/workers/platform/deploy-buttons/
- `workers.dev` — each Workers account has a configurable namespace and each Worker receives a route; intended for personal/hobby/getting-started workloads: https://developers.cloudflare.com/workers/configuration/routing/workers-dev/

## PWA / Web Push

- Apple — Web Push for web apps/browsers; Home Screen web apps on iOS 16.4+ can use standards-based Web Push: https://developer.apple.com/documentation/usernotifications/sending-web-push-notifications-in-web-apps-and-browsers
- WebKit — Home Screen web apps have app-like icons/standalone presentation and Web Push does not require Apple Developer Program membership: https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/

## Update trust

- The Update Framework security model, including rollback/freeze/compromise resistance: https://theupdateframework.io/docs/security/

## Philippine legal-role questions

- RA 11930 — includes participative network platform providers/instant messaging in its definitions and duties for covered intermediaries: https://lawphil.net/statutes/repacts/ra2022/ra_11930_2022.html
- RA 10175 — service-provider preservation/disclosure provisions for data in possession/control: https://elibrary.judiciary.gov.ph/thebookshelf/showdocs/2/50264

These are reasons to preserve narrow operational roles, not proof that a source-code publisher has no obligations.

## Store distribution

- Apple App Review Guidelines §1.2 — UGC/social apps require filtering/reporting/blocking/contact processes; current guidelines also cover random/anonymous chat: https://developer.apple.com/app-store/review/guidelines/
- Google Play UGC policy — reporting/blocking and appropriate action requirements for covered UGC/1:1 interactions: https://support.google.com/googleplay/android-developer/answer/9876937

## EU scope distinction

- EU Digital Services Act recital 14 distinguishes finite-recipient private messaging from online-platform dissemination-to-the-public concepts while public groups/channels can differ: https://eur-lex.europa.eu/eli/reg/2022/2065
