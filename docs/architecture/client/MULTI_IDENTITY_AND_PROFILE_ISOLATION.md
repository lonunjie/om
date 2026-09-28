# Multiple Identities and Profile Isolation

## Decision

OM Core is multi-identity from its first storage schema. The V1 reference client exposes an account/profile switcher whenever two or more IdentityProfiles are configured; this remains a UI feature over already-isolated state, not a database/key rewrite (ADR-0148).

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

## V1 account switcher UX

Account switching should be Telegram-simple:

- tap avatar/profile chip;
- select identity;
- preserve each identity's draft/navigation state;
- notifications indicate destination identity according to privacy policy;
- no logout/reload/rekey just to switch views;
- the active-profile selector is local installation state and is not synchronized as an account preference;
- switching purges/rebinds profile-scoped decrypted view caches before rendering the destination profile;
- unknown deep links ask which profile to use rather than choosing by hidden cross-profile heuristics.

## V1 ProfileExecutionContext / async isolation (ADR-0149)

Namespace isolation is not sufficient if asynchronous work resolves after the visible profile changes. Every profile-sensitive intent/task captures an immutable `ProfileExecutionContext` when dispatched. It includes the originating `profile_id` and the profile-scoped vault/storage/session namespace needed by that operation. Continuations never consult a mutable process-global `active_profile` to decide where results belong.

This applies to attachment transfer/media processing, composer drafts/send preparation, search/index queries, file/media picker callbacks, OS share/import intents, notification opens/quick replies, deep links, background account-private sync, clipboard-triggered actions and desktop/multi-window navigation. Switching A -> B does not rebind an A task to B: its result stays in A, is presented only by explicitly navigating to A, or is cancelled/discarded if A is no longer eligible.

Each independent window/navigation surface is explicitly profile-bound. External intents without an unambiguous profile require user choice. A notification action retains the profile/conversation/notification instance that created it and cannot be retargeted by a later active-profile change. OS clipboard contents are an explicitly shared platform boundary, not a profile-private namespace; OM never auto-imports clipboard data on switch and may use timed sensitive-clipboard clearing where the platform supports it.

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

All persistence tables/keys/cache namespaces take a `profile_id` or are explicitly global. Tests MUST prove no query/cache/draft/sync/notification or deep-link path can accidentally return or mutate another profile's data. Run-6B evidence is tracked under V-021 (ADR-0148).
