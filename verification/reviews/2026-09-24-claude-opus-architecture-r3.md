# Architecture review R3: ecosystem, client, UX

- **Reviewer:** Claude Opus (claude-opus-5-5), independent reviewer under ADR-0117
- **Date:** 2026-09-24
- **Base commit:** b33c518
- **Scope:** `docs/architecture/ecosystem/` (6 docs), `docs/architecture/client/` (9 docs), `docs/architecture/ux/` (6 docs), read in full. ADRs, `machine/`, `spec/cddl`, `spec/registries` and the threat model were read only to check consistency. Claude-authored material (ADR-0110 and later, sections headed "2026-09-24 ...", [`PLATFORM_CAPABILITY_MATRIX.md`](../../docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md), the V-022 budget table in [`MOTION_AND_PERFORMANCE.md`](../../docs/architecture/ux/MOTION_AND_PERFORMANCE.md)) was used as context and is not the subject of findings.

**Verdict:** The ecosystem layer gets the big things right. Principals are typed, selected context is the default, creation is permissionless and admission is scoped, bridges state where E2EE ends, and archives restore data but not authority. The weak points are the seams. Local plugins and personal services can still send as the account or export Room plaintext on one user's say-so. Two service and bridge disclosure modes have no cryptographic definition. The client docs predate ADR-0107, ADR-0111 and ADR-0112, so they still claim things that are now wrong: recovery off by default, PQ for groups, and a user-owned origin. Three things are missing outright: a push design that keeps IdentityProfiles unlinkable, fonts treated as a spoofing channel, and ClientHost update-root selection.

## Findings

### F-R3-1 — Local plugins can send autonomously as the AccountPrincipal

Severity: major
Where: [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../../docs/architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) §OM host capabilities (lines 104, 115); compare [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](../../docs/architecture/ecosystem/SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md) §9.2, §15 invariant 7, and [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../../docs/architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) line 107.
Problem: `conversation.send_message` is a plugin capability that needs only "prominent install/runtime disclosure", not per-action confirmation. A local plugin (or a local LLM running as one) holding this grant can emit events that the account authors with no human review of each action. The service docs say that any action taken without per-action Account confirmation must be ServicePrincipal-authored and labelled as automation. The plugin model reopens the masquerade that typed principals were built to close. Recipients see "Alice" when the true label is "Alice · via X". The `local_component` execution mode in `ServiceDefinition` (SERVICE_PRINCIPALS line 122) is also never tied to the plugin sandbox, so nothing defines where a local service's endpoint credential lives.
Proposed fix: In the plugin doc, split the capability. `conversation.send_message` becomes send-with-user-confirmation: the host shows the exact outgoing content and the user confirms each send, and only then is the event Account-authored. Autonomous sending requires the plugin to act as a `local_component` ServicePrincipal. That means its own ServiceID and a service endpoint credential sealed in DeviceVault, with events authored under the service or a RepresentationGrant. Add an invariant ("no autonomous Account-authored events from extension code") and a V-010/V-011 negative test.
Resolution: accepted — `conversation.send_message` is send-with-confirmation; autonomous plugins are `local_component` ServicePrincipals with a DeviceVault-sealed endpoint credential (PLUGIN §OM host capabilities, SERVICE_PRINCIPALS §ServiceDefinition; ADR-0129 rule 1; R-240, T-070, V-010/V-011).

### F-R3-2 — Personal plugins and services can egress Room plaintext on one user's grant, contradicting ADR-0124 and Bridge §7

Severity: major
Where: [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../../docs/architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) §Four data-access modes, mode 2 (lines 175–179); [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../../docs/architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) capability families (`content.read_current_conversation` + `net.fetch_origin_scoped`, lines 98–109, 115–117); [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../../docs/architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) §7.
Problem: Under mode 2, the host "warns/grants" network egress to a local service, and the grant comes from the local user. ADR-0124 rule 2 says a tainted invocation (one that has read Room plaintext) gets no egress unless the **Room policy** grants `plaintext_egress`. Bridge §7 says continuous export "cannot hide behind an ordinary local plugin's network permission". As written, one member can install a personal plugin or automation that reads the current Room and holds origin-scoped network access. That is continuous export which other members cannot see. The docs never say whether ADR-0124's taint rule covers local plugins, personal-scope installations and personal `AutomationRule`s, or only Room-installed services. Nor do they say what "Room policy" means in a 1:1.
Proposed fix: Amend SERVICE_PRINCIPALS mode 2 and the plugin doc. Taint tracking applies to every executable extension that reads conversation plaintext. Egress from a tainted plugin or personal service needs Room-level `plaintext_egress` (in a 1:1, both participants' policy), except for one-shot, user-initiated, per-action selected-context disclosure, which stays like manual sharing. Continuous or automated egress of Room plaintext is classified as export under Bridge §7. Record the rule in the ADR-0124 amendments list and add a V-010 case: a plugin with read and egress in a Room without `plaintext_egress` must be blocked.
Resolution: accepted — taint covers every executable extension; tainted egress needs Room `plaintext_egress` (both participants in a 1:1); continuous egress is export (PLUGIN, SERVICE_PRINCIPALS mode 2; ADR-0129 rule 2 amends ADR-0124 rule 2; R-241, T-071, V-010).

### F-R3-3 — The "selected event subscription" and non-MLS bridge recipient have no cryptographic definition

Severity: major
Where: [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../../docs/architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) mode 3 (lines 181–187: "the implementation must still define how ..."); [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../../docs/architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) §6 line 111 ("or another cryptographically explicit authorized disclosure recipient defined by the bridge protocol").
Problem: A private Room has two ways to deliver a subset of events to a non-member. (a) Make the endpoint an MLS member, which gives it keys for **all** traffic; [`GROUP_CHAT_ARCHITECTURE.md`](../../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md) line 304 already says "mentions only" is then not a cryptographic boundary. (b) Have some member device re-encrypt and forward. Option (b) makes that device a disclosure agent, and the doc does not say which device, under what authorization, or how other members audit it. Two honest implementations will diverge: one tells users "selected event classes" while the service holds full Room keys. ADR-0124 rule 6 ("membership intervals") implicitly assumes (a) for bridges.
Proposed fix: Decide by ADR. Either (1) remove mode 3 as a distinct cryptographic mode: subscription becomes UI or trigger filtering on top of full membership, and the roster and UI must say the service can read all Room content. Or (2) specify a forwarding construction: a designated forwarder endpoint named in Room state, per-event re-encryption to the service endpoint, disclosure receipts, and failure behaviour. Apply the same choice to Bridge §6 and delete "defined by the bridge protocol".
Resolution: accepted, option (1) refined — invocation-only (selected context over a 1:1 session) or full member; subscription is a trigger filter on full membership; bridges always full members, "defined by the bridge protocol" deleted (SERVICE_PRINCIPALS modes, SERVICE_CREATION §13, BRIDGES §6; ADR-0129 rule 3; R-242).

### F-R3-4 — RepresentationGrant: issuing authority and counterparty visibility undefined

Severity: major
Where: [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](../../docs/architecture/ecosystem/SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md) §9.3 (`issued_by_account_authority`, lines 188–202), §10 ("answer direct messages from an allowlist", line 229).
Problem: (1) The doc does not say which Account authority may issue a grant: any messaging device, a control-capable device, or the AccountControlState quorum. It also does not say whether a grant dies when the issuing device is quarantined or revoked (ADR-0122 rule 2 needs a named "basis"). A briefly linked or stolen messaging device could otherwise mint grants that let a service post as "Alice · via X" in every scope. (2) For a service to answer Bob's DMs to Alice, it must receive Bob's 1:1 plaintext. That happens either by Bob's client encrypting to service endpoints, or by Alice's device forwarding. Either way Bob's messages reach a third party, and nothing requires Bob to be told.
Proposed fix: Specify that RepresentationGrants are authorized by the AccountControlState authority, at the same level as device enrollment, or name the minimum. Record the basis per ADR-0122 so grants cascade when the basis is revoked. For any grant that makes a service a recipient of counterparties' messages, require a visible conversation-state notice to those counterparties ("Alice's messages here are handled by Assistant"), on the same pattern as Bridge §8. Add both cases to V-011.
Resolution: accepted — grants signed by a non-quarantined control device with ACS-head basis that cascades; counterparty notice before encrypting to a representing service (SERVICE_CREATION §9.3; ADR-0129 rules 4–5; R-243, R-244, T-072, V-011).

### F-R3-5 — ServiceControlState distribution and endpoint-revocation propagation into Rooms unspecified

Severity: major
Where: [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../../docs/architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) §Revocation (lines 350–359); compare [`GROUP_CHAT_ARCHITECTURE.md`](../../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md) §Device revocation.
Problem: For human devices, the path "AccountControlState revokes → groups learn → MLS Remove" is defined, and AccountControlHead gossip carries it. For services, "endpoint compromise: rotate/revoke endpoint in ServiceControlState" stops there. The docs do not say how Room members learn of new ServiceControlState heads (there is no head-gossip equivalent), or which member commits the MLS Remove of a revoked service endpoint, and by when. Until someone does, a compromised endpoint that was explicitly revoked keeps receiving plaintext in every Room it belongs to. ADR-0122 rule 8 accepts bounded staleness, but no bound is defined here.
Proposed fix: Add a section. ServiceControlState heads are carried in Room control state or gossiped like AccountControlHead. Any member holding the relevant capability (with a deterministic tie-break) must commit Remove for endpoints revoked in a validated head within a defined window. Removal is a monotone reduction under ADR-0122 rule 1. Add the case to V-011 and the threat register.
Resolution: accepted — head carriage and gossip, stop-and-remove, membership-basis removal with ADR-0128 rule 1 liveness, monotone (SERVICE_PRINCIPALS §Revocation; ADR-0129 rule 6; R-245, T-073, V-011).

### F-R3-6 — Client and UX docs still say recovery is off by default (contradicts ADR-0111)

Severity: major
Where: [`UI_UX_SYSTEM.md`](../../docs/architecture/ux/UI_UX_SYSTEM.md) §Recovery UX line 250 ("Default is **Not configured** ... losing all authorized devices loses this identity"); [`SECURITY_PRIVACY_PROFILES.md`](../../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md) line 44 (Recommended: "no recovery unless user explicitly enables it"), line 73 (Maximum: same), line 97; [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) §6 line 93.
Problem: Under ADR-0111, every identity is created with a working RecoveryPolicy. Turning it off is an explicit, confirmed choice, and Maximum Privacy uses a recovery key only; it does not disable recovery. These lines give the opposite default. They are the texts a UI engineer and the V-018 comprehension study will build from.
Proposed fix: Rewrite UI_UX §Recovery UX: the default is **On**, with the custodian shown (passkey or recovery key), plus a "Turn off recovery" flow with the ADR-0111 warning. In the profile presets, Recommended and Hardened get "recovery on (passkey custodian)", Maximum gets "recovery on (recovery key only)", and the human-effect example becomes the turn-off warning. DEVICE_VAULT §6 becomes: "If recovery was explicitly turned off, identity loss is a possible outcome."
Resolution: accepted — recovery On by default with custodian named and a Turn-off flow (UI_UX §Recovery UX); presets say recovery on (SECURITY_PRIVACY_PROFILES); DEVICE_VAULT §6 rewritten (ADR-0129 rule 19).

### F-R3-7 — Distribution strategy claims PQ/hybrid E2EE for group messaging (contradicts ADR-0107, R-173)

Severity: major
Where: [`PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md`](../../docs/architecture/client/PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md) §3 lines 40–41.
Problem: The list reads "OM-native 1:1 and group messaging; PQ/hybrid E2EE through OM Core". ADR-0107 says v1 groups and group calls are classical, and "no surface may claim PQ confidentiality for them". The validator's overclaim rule does not catch this phrasing.
Proposed fix: Change to "OM-native 1:1 (hybrid-PQ) and group (classical MLS until ADR-0107 triggers) messaging through OM Core". Consider widening the validator pattern to catch "PQ" near "group" on the same line.
Resolution: accepted — PWA_NATIVE §3 now says 1:1 hybrid-PQ, groups classical MLS until ADR-0107 triggers. Validator widening left to the integrator (apply notes).

### F-R3-8 — Client docs assume a user-owned PWA origin, but T0/T1 users use a shared ClientHost that no doc defines

Severity: major
Where: [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) §3 line 28 ("user owns the origin/account rather than one universal OM origin"); [`PWA_REFERENCE_CLIENT.md`](../../docs/architecture/client/PWA_REFERENCE_CLIENT.md) line 287 ("user-owned origin"); [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md) §1 ("self-owned PWA ClientHost"); compare ADR-0112 rule 3 and [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §4.2.1.
Problem: Under ADR-0102/0112, most early PWA users (T0 sponsored, T1 hosted) will not own an origin. By default they use "a PWA origin that the host does not control". In practice that is a shared public ClientHost, likely a canonical or community one. Whoever operates it can ship code to every user's endpoint. That is the T-042 problem at larger scale, and it is exactly the "reference distribution becomes privileged" case. The client docs' main mitigation (user ownership) does not apply to these users. No doc names this role, who may run it, or its obligations.
Proposed fix: Add a "Shared ClientHost" section to [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md). Name it as a distribution role under ADR-0049 and ADR-0082. Require that several independent operators exist and are selectable, that deployments are served only from transparency-logged releases, and that the R-164 client-side digest check applies. Give it a threat-register entry separate from T-042. Onboarding should state which origin serves the app. Correct the DEVICE_VAULT and PWA_REFERENCE mitigation lines to say "where the user owns the origin".
Resolution: accepted — CLIENTHOST_UPDATE_LIFECYCLE §9 Shared ClientHost (distribution role, plural, transparency-only, named origin); DEVICE_VAULT §3 and PWA_REFERENCE mitigation lines corrected (ADR-0129 rule 13; R-247, T-074).

### F-R3-9 — ClientHost auto-update has no owner-selected release root, delay, or transparency precondition

Severity: major
Where: [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md) §2 line 16, §4 lines 26–35, §8 line 58 ("emergency minimum-version policy"), resolution line 65.
Problem: The reference controller "verifies signed/TUF-style metadata" from "canonical project releases" and then atomically replaces the origin's code. The doc never says the ClientHost owner selects or pins the release root (canonical, a fork, or their own). It does not require that a release appear in independent transparency logs *before* it is deployed; R-164 only detects substitution afterwards. It does not offer owner-approved updates as an alternative to automatic ones. As written, the canonical signing root becomes a push-code channel into every self-hosted endpoint. "Emergency minimum-version policy" is also undefined, and it reads like a remote enforcement lever, which ADR-0104 forbids.
Proposed fix: In §4, add rules. (a) The release root is chosen and pinned by the ClientHost owner at deploy time, and changing it requires the owner. (b) Automatic deploy only of releases observed in at least 2 independent transparency monitors, after an owner-configurable delay; an emergency path shortens the delay but never skips transparency. (c) Owners can choose automatic or approval-required updates. Redefine "emergency minimum-version policy" as ADR-0104 peer-local floors and SecurityAdvisories, with no remote disable.
Resolution: accepted — CLIENTHOST §4 rules: owner-pinned root, ≥2 transparency monitors before deploy, delay with bounded emergency path, automatic or approval mode; §8 emergency floor redefined per ADR-0104 (ADR-0129 rule 12; R-246, T-075).

### F-R3-10 — Push handles link IdentityProfiles; one platform push token or subscription is shared across profiles

Severity: major
Where: [`NOTIFICATIONS_AND_PUSH.md`](../../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md) §3 (lines 28–38) and §Multiple IdentityProfiles (lines 89–91); [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../../docs/architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md) line 23; [`METADATA_THREAT_MODEL.md`](../../docs/threat-model/METADATA_THREAT_MODEL.md) §9.
Problem: The docs rely on "opaque per-profile/per-device wake handles". But an APNs/FCM token is per app install, and a Web Push subscription is per service-worker registration. If profiles A and B use different Homes or PushGateways, each receives the **same** platform token or endpoint. Those parties, and the platform push service (which sees two VAPID keys or senders targeting one endpoint), can link the profiles. That defeats the isolation `MULTI_IDENTITY` requires ("no public stable ClientInstallationID may link profiles").
Proposed fix: State the constraint and choose a design. Options: per-profile service-worker registrations with distinct scopes on web, so each profile gets its own subscription; a single user-chosen gateway that fans out through unlinkable per-profile handles and is declared as a linkage point; or disclosing that "Keep separate" profiles must disable vendor push. Add a MetadataExposureDescriptor line and a V-015 case.
Resolution: accepted — per-profile service-worker registrations on web; native token is a declared linkage point with per-profile opt-out or chosen gateway (NOTIFICATIONS §Multiple IdentityProfiles, MULTI_IDENTITY; ADR-0129 rule 14; R-248, T-076, V-015). Descriptor line in apply notes.

### F-R3-11 — DeviceVault wrapping and the ADR-0111 recovery custodian may share one synced passkey; §5 and the resolution also disagree on passphrases

Severity: major
Where: [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) §5 (lines 65–74), 2026-09-23 resolution (lines 108–110).
Problem: (1) The resolution makes WebAuthn PRF the production vault wrapper. ADR-0111 makes a **synced** passkey's PRF the default recovery custodian. Neither doc requires separate credentials or PRF salts. If one synced passkey both wraps the vault and is the RecoveryRoot, then anyone who compromises the passkey provider account and obtains a copied browser profile can unseal local state, which removes the at-rest boundary the vault exists for. The recovery delay protects authority, not local plaintext. (2) §5 lists a passphrase-derived KEK as wrapping method 3, while the resolution says passwords "are not the identity-key storage primitive". Implementers will diverge.
Proposed fix: In §5, require a device-bound (non-synced) credential for vault wrapping where available, and distinct PRF salts and labels from the RecoveryRoot in every case. Where only a synced passkey exists, state the reduced at-rest property. Reconcile method 3 with the resolution: a passphrase may add a second wrapping factor but is never the sole wrapper of a production DeviceIdentity. Add the case to V-006.
Resolution: accepted — device-bound credential where available, distinct credential/PRF salt from RecoveryRoot always, reduced property stated; passphrase only as an added factor (DEVICE_VAULT §5; ADR-0129 rule 15; R-249, T-077, V-006).

### F-R3-12 — Theme fonts are a text-spoofing and parser-attack channel; fallback is optional and triggered only by legibility

Severity: major
Where: [`THEMES_FONTS_AND_PERSONALIZATION.md`](../../docs/architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md) §Fonts (lines 92–110), §Theme format line 43.
Problem: A theme or imported font can remap glyphs through cmap or GSUB, so rendered text differs from the underlying string. Safety numbers, verification codes, link destinations, identity names in warnings and permission text can all be altered without any "change link destinations" or "spoof prompt" rule being broken at the data level. Line 110 lets security dialogs fall back to known-good fonts only "if a theme font compromises legibility", and glyph substitution is legible. Font files are also a well-known parser attack surface, and import validation checks only hash, MIME, size and dimensions.
Proposed fix: Make it a hard rule. Security-reserved surfaces (the list in §Reserved semantic UI plus safety numbers, codes, URLs, principal and bridge labels, permission and recovery prompts) **always** render with built-in fonts. Imported fonts are treated as hostile content under ADR-0062 and pass a font sanitizer (OTS-class) before use. Add a V-018 or theme-compiler test in which a glyph-remapping font must not affect reserved surfaces.
Resolution: accepted — reserved surfaces always use built-in fonts; imported fonts pass an OTS-class sanitizer; glyph-remap test (THEMES §Fonts; ADR-0129 rule 17; R-250, T-078, V-018).

### F-R3-13 — Reserved semantic UI omits the ecosystem trust indicators

Severity: major
Where: [`THEMES_FONTS_AND_PERSONALIZATION.md`](../../docs/architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md) §Reserved semantic UI (lines 64–77); [`UI_UX_SYSTEM.md`](../../docs/architecture/ux/UI_UX_SYSTEM.md) §Conversation UI line 196 and §Security events (lines 198–208).
Problem: The reserved list covers identity, device and transport warnings. It does not cover these indicators: service/automation authorship ("Assistant", "Alice · via Assistant"), bridge-active and "OM encryption ends at Bridge X", foreign-provenance and `foreign_actor_unverified` labels, full-Room service or plaintext-recipient roster entries, and device-link notices. The ecosystem docs and ENDPOINT §2 rule 5 require all of them to be unhideable. UI_UX's "no routine padlock/encryption badges" rule, with no named exceptions, and its list of security events without these cases, invite themes and designers to drop them.
Proposed fix: Add these indicators to the THEMES reserved list and to the plugin doc's host-owned chrome list, which already names bridge indicators. In UI_UX, add "a service or bridge can now read this conversation" and "a new device was linked" to Security events. Note under Conversation UI that plaintext-recipient and automation labels are an exception to "no badges".
Resolution: accepted — ecosystem indicators added to THEMES reserved list and PLUGIN chrome list; UI_UX badge exceptions and three new security events (ADR-0129 rule 9; R-251).

### F-R3-14 — In the PWA, plugins and mini-apps need site-level isolation that the docs do not require

Severity: major
Where: [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../../docs/architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) §Local client plugin execution format (line 61, "no shared OM process memory"; line 69), §Mini-app / web-app surface (lines 165–183); V-010 in [`machine/architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl).
Problem: In a browser, a Wasm component instantiated in the ClientHost origin, or in a same-origin worker, runs in the same renderer process as OM Core's wasm linear memory, which holds ratchet and MLS secrets. Import restriction does not stop Spectre-class reads, so "no shared OM process memory" cannot hold unless plugins run in a **different site** with process isolation. Mini-app iframes raise the same issue. A mini-app panel can also draw pixels that imitate OM prompts ("re-enter recovery key", passkey dialogs), and the doc bans that only for themes. The postMessage bridge has no origin-binding rule. V-010 does not test any of this.
Proposed fix: In the PWA, plugins must execute in a sandboxed frame or worker on a separate registrable domain, with cross-origin isolation. Mini-apps must be separate-site, framed in host-owned chrome that always shows the origin and cannot be occluded, and bound to an exact origin on every bridge message. Mini-apps may not mimic OM security prompts. Host prompts for mini-app requests render outside the frame. Add these points to V-010's pre-engineering criteria.
Resolution: accepted — PWA plugins and mini-apps on a separate site with cross-origin isolation and exact-origin binding; mini-app chrome and no prompt mimicry (PLUGIN §execution format, §Mini-app; ADR-0129 rule 8; R-252, T-079, V-010).

### F-R3-15 — The no-silent-link rule does not cover plaintext-egress grants, RepresentationGrants or personal automations

Severity: major
Where: [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../../docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §2 (lines 19–31).
Problem: The proximate-adversary rule makes device linking loud because a silent link gives persistent reading access. A personal service installation or plugin with plaintext read plus egress, a RepresentationGrant, or a personal AutomationRule that forwards messages does the same job, and each can be set up in seconds on an unlocked phone. The paragraph at line 31 covers only full-Room services and bridges. Whether personal installations sync to other devices, or are visible from them, is unspecified.
Proposed fix: Extend rule 1. Creating or expanding any personal grant that gives plaintext read with egress, representation or automation produces the same authenticated, non-suppressible notice on the account's other devices, and appears in a "Connected services" list next to Devices, with one-tap revoke. Add a failing-trace case to V-021 ("egress grant with no notice").
Resolution: accepted — ENDPOINT §2 rules 6–7: notice for personal egress/representation/automation grants, synced, Connected services list; V-021 failing trace (ADR-0129 rule 7; R-253).

### F-R3-16 — ExpressiveAssetRef cannot be decrypted or verified and contradicts the CDDL `attachment-ref` shape

Severity: major
Where: [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../../docs/architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md) §ExpressiveAssetRef (lines 106–120), lines 126, 136–138; [`spec/cddl/events.cddl`](../../spec/cddl/events.cddl) lines 42, 49 and 54–65.
Problem: `ExpressiveAssetRef` carries `blob_id`, the size and dimensions only. It has no encryption descriptor (profile and key) and no `ciphertext_digest`. The frozen `attachment-ref` needs both to decrypt and verify an encrypted blob. A custom reaction (`ReactionSet` key 3, reserved for W1) or an inline `custom_emoji` (reserved inline type 4) built to this doc cannot be rendered. The doc also says "`blob_id` is the rendering authority" where CDDL makes `ciphertext_digest` the integrity binding. It is further silent on whether each send re-encrypts the asset (fresh blob) or reuses the pack's blob. Reuse gives one BlobID across senders and conversations, a cross-conversation correlation handle for BlobStores, and conflicts with the no-content-dedup rule in `MULTI_IDENTITY`.
Proposed fix: Define `ExpressiveAssetRef = attachment-ref + {kind, media_profile_id, alt_text, fallback_text?, pack_ref?}` for the W1 reserved slots. Require fresh per-send encryption by default, or a declared shared-blob mode with its metadata-exposure line in V-015. Replace "blob_id is the rendering authority" with "the attachment-ref's digest-bound blob is the rendering authority".
Resolution: accepted — ExpressiveAssetRef = attachment-ref + expressive fields; fresh per-send encryption by default, declared shared-blob mode (EXPRESSIVE §ExpressiveAssetRef; ADR-0129 rule 10; R-254). Schema change in apply notes.

### F-R3-17 — Bridge doc and threat model keep the superseded bridge-scoped fingerprint rule

Severity: minor
Where: [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../../docs/architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) §13 lines 256 and 259; [`METADATA_THREAT_MODEL.md`](../../docs/threat-model/METADATA_THREAT_MODEL.md) §13 line 263.
Problem: Line 259 ("mapping/bridge-domain scoped by default") and line 256 (dedup "for the same mapping/direction") contradict ADR-0124 rule 3: the fingerprint is Room-scoped, and dedup works across bridges within a Room. Only the 08:21 paragraph was struck through.
Proposed fix: Change both lines in §13, and the threat-model line, to "keyed with a Room-scoped secret over (origin network, foreign origin id) (ADR-0124); dedup is per Room across bridges".
Resolution: accepted — BRIDGES §13 both lines now Room-scoped per ADR-0124. The METADATA_THREAT_MODEL §13 line is outside this scope; exact edit in apply notes.

### F-R3-18 — CLIENT_AND_EXTENSION still allows skins and renderer extensions with plaintext access

Severity: minor
Where: [`CLIENT_AND_EXTENSION_ARCHITECTURE.md`](../../docs/architecture/client/CLIENT_AND_EXTENSION_ARCHITECTURE.md) §Safe presentation customization (lines 102–113).
Problem: The text lists "skins" and "renderer extensions for known safe content types", which "should not receive plaintext from unrelated conversations unless required". That implies themes and renderers may receive plaintext. This contradicts ADR-0054 and `THEMES_…` (themes never access messages) and ADR-0061 (no renderer extension gets DOM or script).
Proposed fix: Replace the list with "declarative themes (ADR-0054), density and accessibility presets. Rendering of new content types goes through protocol extensions plus host components or sandboxed plugins (`PLUGIN_…`)". Delete the "unless required" sentence.
Resolution: accepted — CLIENT_AND_EXTENSION §Safe presentation rewritten; "unless required" removed (ADR-0129 rule 19).

### F-R3-19 — PlatformAdapter contract omits the leakage controls ENDPOINT §4 requires

Severity: minor
Where: [`UI_CORE_PLATFORM_CONTRACT.md`](../../docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) §PlatformAdapter (lines 70–95).
Problem: ENDPOINT §4 says "the platform adapter must expose each control's availability". The adapter families have no secure-surface/screenshot, backup-exclusion, input-privacy (IME/spellcheck), notification-removal or sensitive-clipboard capability. Shells will add them ad hoc.
Proposed fix: Add `SecureSurface`, `BackupExclusion`, `InputPrivacy` and `NotificationRemoval` families, extend `Clipboard` with a sensitive-write flag, and have each report `available | partial | unavailable` as the matrix does.
Resolution: accepted — SecureSurface, BackupExclusion, InputPrivacy, NotificationRemoval families and sensitive Clipboard flag, each `available | partial | unavailable` (UI_CORE §PlatformAdapter; ADR-0129 rule 18; R-256).

### F-R3-20 — DeviceVault KDF labels are not registered and break the frozen label pattern

Severity: minor
Where: [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) §4 (lines 52–61); [`spec/registries/om-domain-separation.json`](../../spec/registries/om-domain-separation.json) rules.
Problem: `om:v1:local-db` and the other labels are given as the derivation scheme. They are not in the frozen registry and do not follow `OM-<NAME>-<USE>-v<N>`. Either they are normative and missing, or they are implementation-private and should say so.
Proposed fix: Mark them explicitly as implementation-private, non-interoperable local labels ("e.g., not registered"), or register `OM-VAULT-<PURPOSE>-KDF-v1` entries in the next registry version.
Resolution: accepted — labels marked implementation-private, not registered, not interoperable (DEVICE_VAULT §4; ADR-0129 rule 16); registry unchanged.

### F-R3-21 — Export and rehydration say nothing on disappearing and ViewOnce content

Severity: minor
Where: [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) §8–§10 (lines 117–168).
Problem: The doc says erased content is not resurrected. It does not say whether *not yet expired* disappearing or ViewOnce items are exported, or whether importers must enforce the original expiry. The `portable/` JSON projection has no expiry field. One conforming client could rehydrate disappearing messages with no timer.
Proposed fix: ViewOnce is never exported. Disappearing items are exported only in secure export, carrying their canonical expiry. Importers apply expiry on import and drop items already expired. Human-readable export excludes disappearing items by default, with a warning.
Resolution: accepted — ViewOnce never exported; unexpired disappearing items only in secure export with `expiry`; importers enforce; readable export excludes by default (IMPORT_EXPORT §9; ADR-0129 rule 11; R-255).

### F-R3-22 — Minor wording and staleness

Severity: minor
Where: [`UI_UX_SYSTEM.md`](../../docs/architecture/ux/UI_UX_SYSTEM.md) line 155; [`ACCESSIBILITY_AND_INPUT.md`](../../docs/architecture/ux/ACCESSIBILITY_AND_INPUT.md) line 23 and §Calls line 75.
Problem: (a) New Chat lists "paste a personal `workers.dev` contact address". That names a vendor in core UX, which is stale after ADR-0102/0112 (T0/T1, second non-cloud Home). (b) Remote captions or transcription would send other participants' speech to a remote service, but the text says only "make remote processing explicit" and does not route it through the ServicePrincipal disclosure model.
Proposed fix: (a) Change it to "paste an OM contact address/link". (b) Say that remote transcription is a ServicePrincipal invocation visible to all call participants.
Resolution: accepted — (a) New Chat says "OM contact address/link"; (b) remote transcription is a visible service participant (UI_UX, ACCESSIBILITY §Calls; ADR-0129 rule 19).

## Checked and sound

Typed `PrincipalRef` (account/service/bridge) matches the frozen `principal-kind 1..3` and `author_principal`/`author_endpoint_id` in [`events.cddl`](../../spec/cddl/events.cddl), and W0's rejection of the reserved `representation`/`foreign_provenance` keys matches the W4 placement of those features. Also sound:

- selected-context invocation as the default, and the statement that revocation cannot claw data back;
- no-self-bootstrap, consistent with ADR-0122 rules 5–6;
- permissionless ServiceGenesis kept separate from admission and reach;
- bridge authorship, the E2EE-termination language and "no single lock" (§8, §15);
- archives never restoring authority or private keys, consistent with ADR-0097, ADR-0125 and `om-secret-object-types`;
- content packs as inert data, with rendering independent of catalogs and entitlements, and no payment rails;
- themes as data only, with a CSP and Trusted Types direction;
- the typed-intent UI↔Core contract;
- accessibility as a shared release gate;
- honest statements of PWA origin-compromise limits and push wake-timing leakage;
- the no-deniability stance in ENDPOINT §3.2.

I found no provider-name-as-wire-semantics or AccountID-as-routing violation in scope.
