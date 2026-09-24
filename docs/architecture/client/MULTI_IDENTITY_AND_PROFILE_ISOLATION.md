# Multiple Identities and Profile Isolation

## Decision

OM Core is multi-identity from its first storage schema, even if v1 UI exposes a single identity. A later account/profile switcher must be a UI feature, not a database rewrite.

Internally call these **IdentityProfiles** to avoid confusing a local profile with an AccountID.

## Isolation boundary

Each profile has independent:

- AccountID and credentials;
- DeviceIdentity/session state;
- AccountControlState and RecoveryPolicy;
- contacts/PersonRecords;
- conversations/search/drafts;
- RelaySet/IntroGate/discovery aliases;
- backups/blob references;
- notification/privacy policy;
- ProfileVaultKey and derived local keys.

No public stable `ClientInstallationID` may link profiles. A platform push token is such an identifier: see [`NOTIFICATIONS_AND_PUSH.md`](NOTIFICATIONS_AND_PUSH.md) §Multiple IdentityProfiles (per-profile Web Push registrations; a native token is a declared linkage point, ADR-0129).

## Shared resources allowed

The application executable, design assets and user-approved aesthetic preferences MAY be shared locally. Shared resources must never create protocol identifiers or shared cryptographic keys.

Cross-profile blob deduplication based on plaintext/content hashes is forbidden by default because it can create a correlation oracle.

## Provider linkage warning

Different Workers in one Cloudflare account share the account's `workers.dev` subdomain, so profiles deployed to the same provider account are not promised infrastructure unlinkability.

Future UX can offer:

- **Normal profile** — convenient, may use same provider account;
- **Keep separate** — recommends a separate provider account/provider and stronger routing isolation.

OM must not silently fingerprint a physical device to tie profiles together for abuse control.

## Later account switcher UX

Account switching should be Telegram-simple:

- tap avatar/profile chip;
- select identity;
- preserve each identity's draft/navigation state;
- notifications indicate destination identity according to privacy policy;
- no logout/reload/rekey just to switch views.

## Theme suggestion per identity

When a second or later profile is created, OM MAY ask:

> Give this identity a different look?

Offer several curated, calm built-in themes. It is optional and local; the selection does not go into public identity/discovery state.

Every theme supports:

- authored light + dark semantic palettes;
- **Auto** mode following system light/dark preference;
- accessibility/high-contrast overrides.

A custom theme missing a valid light or dark variant may be imported only after the theme compiler derives/validates a safe counterpart and the user previews it. Security-state tokens remain protected.

## Deep links with several identities

If an `om:` link does not already belong to a known relationship/profile, the UI asks which identity should use it. Do not guess based on hidden cross-profile heuristics.

## V1 engineering requirement

All persistence tables/keys/cache namespaces take a `profile_id` or are explicitly global. Tests MUST prove no query/cache/notification can accidentally return another profile's data.
