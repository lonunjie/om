# Research Notes — Volatile External Facts

These facts were researched during planning and must be re-verified before implementation because platform policies, quotas, and APIs change.

## Cloudflare / bring-your-own relay

Planning research found that Cloudflare user-account authorization and Workers/Durable Objects make a one-click bring-your-own relay technically plausible. Free-tier limits appeared sufficient for ordinary text relay workloads in the planning stage. R2 has a free allowance but is a separately enabled usage-billed product.

Architecture consequence: Cloudflare is only one `RelayProvider`/`BlobStore` implementation. It is never the protocol.

## Google Drive

Drive is better suited to encrypted backup/archive than to real-time primary delivery. App-private storage has sharing limitations; normal Drive sharing/permissions are too awkward to make a clean universal relay.

Architecture consequence: Drive-class systems implement storage/backup capabilities, not assumed relay capabilities.

## Browser/PWA

Planning research supports PWA as a useful early universal reference client and Web Push wake path where supported. Browser background execution and key-storage guarantees differ from native platforms.

Architecture consequence: web is a first-class DeviceIdentity, but native clients remain necessary for final Android SMS/default-app integration and stronger platform lifecycle/key facilities.

## Android publication

Planning research found current Play publication to have identity/testing requirements for new developer accounts, making the web reference path faster for protocol iteration. These policies must be checked again before release planning.

## External messaging systems studied

- Signal — device/session security, message requests, key transparency concepts, user-friendly verification.
- SimpleX — capability-based queues, metadata minimization, replaceable relays, public contact address separate from private relationships.
- Matrix — open/federated architecture lessons, aliases/state complexity/moderation costs.
- XMPP — long-running open messaging/federation/spam-extension lessons.
- Telegram — fast UX, group/bot product patterns; not used as OM's E2EE architecture reference.
- Bluesky/Mastodon — domain-backed/decentralized human handles and provenance concepts.

Links/source details should be refreshed in implementation-time ADRs rather than treated as timeless truth.
