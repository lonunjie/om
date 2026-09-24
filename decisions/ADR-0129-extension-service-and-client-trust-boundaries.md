# ADR-0129 — Extension, service and client trust boundaries

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** integrator under operator delegation 2026-09-24 ("do everything and complete it"), from the
  architecture review
- **Source:** [`2026-09-24-claude-opus-architecture-r3.md`](../verification/reviews/2026-09-24-claude-opus-architecture-r3.md) (F-R3-1 .. F-R3-22)
- **Amends:** ADR-0124 (rule 2's scope: rule 2 below), ADR-0111 wording in the client and UX docs,
  [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../docs/architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md), [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../docs/architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md),
  [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](../docs/architecture/ecosystem/SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md), [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../docs/architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md),
  [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../docs/architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md),
  [`CLIENT_AND_EXTENSION_ARCHITECTURE.md`](../docs/architecture/client/CLIENT_AND_EXTENSION_ARCHITECTURE.md), [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md), [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md),
  [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md), [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../docs/architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md), [`NOTIFICATIONS_AND_PUSH.md`](../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md),
  [`PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md`](../docs/architecture/client/PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md), [`PWA_REFERENCE_CLIENT.md`](../docs/architecture/client/PWA_REFERENCE_CLIENT.md), [`ACCESSIBILITY_AND_INPUT.md`](../docs/architecture/ux/ACCESSIBILITY_AND_INPUT.md),
  [`SECURITY_PRIVACY_PROFILES.md`](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md), [`THEMES_FONTS_AND_PERSONALIZATION.md`](../docs/architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md), [`UI_CORE_PLATFORM_CONTRACT.md`](../docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md),
  [`UI_UX_SYSTEM.md`](../docs/architecture/ux/UI_UX_SYSTEM.md)

## Context

The R3 review found that the ecosystem layer gets the big things right (typed principals, selected context by
default, bridges that say where E2EE ends) but leaks at the seams. Local plugins could send as the account or export
Room plaintext on one user's say-so. Two service and bridge disclosure modes had no cryptographic definition.
RepresentationGrants had no named issuer. Revoked service endpoints had no path out of Rooms. On the client side,
nobody owned the shared ClientHost that most T0/T1 PWA users will run, the canonical release root could push code into
self-hosted origins, one push token linked IdentityProfiles, and a theme font could rewrite a safety number. This ADR
records the decisions; the canonical docs carry the normative text.

## Decision

### Extensions and services

1. **No autonomous Account-authored events from extension code.** `conversation.send_message` is send-with-
   confirmation: the host shows the exact content and destination and the user confirms each send. A plugin that
   acts without per-action confirmation runs as a `local_component` ServicePrincipal with its own ServiceID, a service
   endpoint credential sealed in DeviceVault that plugin code cannot read, and ServicePrincipal authorship (or a
   RepresentationGrant).
2. **Taint covers every executable extension (amends ADR-0124 rule 2).** ADR-0124's taint rule applies to local
   plugins, personal-scope service installations, personal `AutomationRule`s and mini-apps, not only Room-installed
   services. A tainted invocation gets no network egress unless the conversation's policy grants `plaintext_egress`:
   Room policy in a Room, the policy of **both** participants in a 1:1. The one exception is a one-shot,
   user-initiated, per-action selected-context disclosure, handled like manual sharing. Continuous or automated egress
   of conversation plaintext is export under Bridge §7. Taint persists through a plugin's private storage.
3. **Two cryptographic positions only.** A remote service or bridge is either **invocation-only** or a **full Room
   member**. Invocation-only means the invoking member's client sends the selected context to the service over a 1:1
   session with an authorized service endpoint, as a one-shot user-initiated disclosure (ADR-0124 rule 1). Any
   continuous access means full membership, and the roster and UI say the service can read all Room content. The
   former "scoped event subscription" is a trigger filter on full membership that limits automation, not reading.
   There is no designated-forwarder construction. Bridges are always full members; "another cryptographically
   explicit authorized disclosure recipient defined by the bridge protocol" is deleted.
4. **RepresentationGrant issuance.** A grant is signed by an active, non-quarantined **control** device (the level of
   linking a messaging device, ADR-0127 rule 2), over the grant and the AccountControlState head it was validated
   against. That device and head are its basis (ADR-0122 rule 2): the grant stops validating once the issuing device
   is quarantined or revoked, unless re-issued. Messaging-only devices can revoke grants but not issue or expand them.
5. **Counterparties are told.** A grant that makes a service a recipient of other people's messages works only by
   adding the service endpoint as a visible conversation recipient. The counterparty's client shows "Alice's messages
   here are handled by Assistant" before encrypting to it and may decline. No hidden forwarding by the represented
   account's device.
6. **Revoked service endpoints leave every Room.** A full-member installation records the ServiceControlState head it
   was admitted under. The service's controllers deliver every new head to each Room where it is installed; members
   also gossip ServiceControlHead observations. A member that validates a head revoking an endpoint stops sending
   application messages until the endpoint's leaf is removed, and proposes the Remove. Removal needs no capability
   (basis: membership) and uses ADR-0128 rule 1 liveness: the coordinator sequences it, and after `sequencing_timeout`
   any member commits it. It is a monotone reduction (ADR-0122 rule 1). Residual staleness is ADR-0122 rule 8.
7. **Personal grants are as loud as device links.** Creating or expanding a personal grant with plaintext read plus
   egress, a RepresentationGrant, a message-reading personal AutomationRule or a `local_component` ServicePrincipal
   produces the non-suppressible notice on the account's other devices. These grants are synced account state and
   appear in **Connected services** next to Devices, with one-tap revoke.
8. **PWA extensions run on a separate site.** Plugins and mini-apps execute in sandboxed frames or workers on a
   different registrable domain from the ClientHost, with cross-origin isolation and exact-origin message binding.
   Where the browser cannot isolate them, plugins with `content.*` capabilities are disabled. Mini-apps sit in
   host-owned chrome that always shows their origin, may not imitate OM security prompts, and have their permission
   prompts rendered outside the frame.
9. **Ecosystem indicators are reserved UI.** Service/automation authorship, bridge-active and E2EE-termination
   labels, foreign-provenance labels, plaintext-recipient roster entries, counterparty notices, and device-link and
   connected-service notices join the reserved list in THEMES and the plugin host's chrome list. They are the named
   exceptions to "no routine badges".
10. **ExpressiveAssetRef is an attachment-ref.** It is the frozen `attachment-ref` plus `kind`, `media_profile_id`,
    `alt_text`, `fallback_text?` and `pack_ref?`, used in the W1 reserved slots under ADR-0128 rule 18. Each send is
    freshly encrypted by default; a shared-blob mode is an explicit, declared policy with a MetadataExposureDescriptor
    line. The digest-bound blob, not the `blob_id` alone, is the rendering authority.
11. **Export honours expiry.** ViewOnce is never exported. Unexpired disappearing items are exported only in secure
    export, carrying the envelope `expiry`; importers drop expired items and enforce the rest. Human-readable export
    excludes disappearing items unless the user opts in per export.

### Client host, vault, push and fonts

12. **ClientHost update root is the owner's.** The ClientHost owner pins the release root (canonical, a fork's or
    their own); release metadata can never change it. Automatic deploy only of releases seen in at least two
    independent transparency monitors, after an owner-configurable delay (default 24 hours). A signed SecurityAdvisory
    from the pinned root may shorten the delay to no less than 1 hour but never skips transparency. Owners choose
    automatic or approval-required updates. "Emergency minimum-version policy" means an owner-local floor plus
    ADR-0104 peer-local floors; there is no remote disable.
13. **Shared ClientHost is a named distribution role** under ADR-0049 and ADR-0082. It must be one of several
    selectable independent operators, serve only unmodified transparency-logged releases, be named to the user at
    onboarding and in Settings, sit in a different hosting account from the user's Home, and never be technically
    privileged. R-164's digest check applies. Moving away is new-device enrollment.
14. **Push tokens are linkage points.** On web, each IdentityProfile has its own service-worker registration and
    Web Push subscription. Where the platform gives one token per app (native), the token is a declared linkage point
    in the push provider's MetadataExposureDescriptor, and "Keep separate" profiles offer disabling vendor push for
    that profile or one user-chosen gateway declared as the linkage point.
15. **Vault wrapping is not the recovery custodian.** The vault uses a device-bound credential where available and,
    in every case, a credential or PRF salt and label distinct from the RecoveryRoot's. Where only a synced passkey
    exists, the reduced at-rest property is stated. A passphrase may add a wrapping factor but is never the sole
    wrapper of a production DeviceIdentity.
16. **DeviceVault KDF labels are implementation-private.** The `om:v1:*` vault subkey labels are local, "not
    registered, not interoperable", and are not added to the frozen [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) registry.
17. **Reserved surfaces use built-in fonts.** Every security-reserved surface renders with built-in fonts regardless
    of theme. Theme and imported fonts are hostile content (ADR-0062) and pass an OTS-class sanitizer before use.
18. **Platform leakage controls are adapter families.** `SecureSurface`, `BackupExclusion`, `InputPrivacy` and
    `NotificationRemoval` join the PlatformAdapter, `Clipboard` gains a sensitive-write flag, and each reports
    `available | partial | unavailable`.
19. **Stale text corrected.** Recovery is on by default in the client and UX docs (ADR-0111); v1 groups are classical
    in the distribution strategy (ADR-0107); themes and renderers never receive plaintext (ADR-0054, ADR-0061); bridge
    origin fingerprints are Room-scoped (ADR-0124 rule 3); New Chat names no vendor; remote transcription is a visible
    service participant.

## Consequences

- Requirements R-240..R-259 and threats T-070..T-079 are added. V-006, V-010, V-011, V-013, V-015, V-018 and V-021
  gain pre-engineering criteria.
- The `service-installation` logical schema drops `scoped_subscription` as a data-access mode in favour of a
  `trigger_filter` on `full_room_member`; the `representation-grant`, `expressive-asset` and `clienthost-release-state`
  logical schemas, the push-provider MetadataExposureDescriptor and the room-capabilities registry need additions.
  These are listed for the spec owner in the integrator's apply notes for this review; nothing in the frozen W0 set changes.
- `plaintext_egress` becomes a Room policy field changed under a protected capability; in a 1:1 it is a per-participant
  policy.
