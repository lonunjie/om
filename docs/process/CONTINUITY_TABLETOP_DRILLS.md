# Continuity Tabletop Drills (V-017)

- **Status:** Written drills, pre-engineering (V-017 criterion 3). Live drills are pre-release criteria.
- **Author:** Claude agent, 2026-09-24. Independent review is still required (ADR-0109; under ADR-0117 a non-Anthropic agent or a human).
- **Tools:** [`tools/continuity_bundle.py`](../../tools/continuity_bundle.py) (build/verify bundles, registry immutability,
  manifest drift check), [`tools/om_drop.py`](../../tools/om_drop.py) export.
- **Normative-doc manifest:** [`spec/continuity/om-crypto-v1-bundle-manifest.json`](../../spec/continuity/om-crypto-v1-bundle-manifest.json)
  lists every `docs/architecture/`/`docs/constitution/` doc a bundle carries, plus the checked `exceptions` for
  normative-looking references that are deliberately left out. `continuity_bundle.py build`/`verify` enforce it;
  `continuity_bundle.py check-manifest` recomputes the doc set from the repo and fails if the manifest drifted
  (F-8, 2026-09-26 independent review — the prior basename allowlist silently dropped real semantic docs).

Each drill asks four questions. What breaks? Who can act, with which mechanism? What must already exist *today* for
the answer to hold? Which gaps did the drill expose? The gaps are listed at the end with their dispositions.

## D1. Founder, forge and domain loss

**Scenario.** The founder is permanently unavailable. The `lonunjie` GitHub org is suspended, and `lonunjie.org`
lapses or is seized.

| Asset | What happens | Continuation path |
|---|---|---|
| Protocol meaning | unaffected | Every published generation has a Protocol Continuity Bundle. `continuity_bundle.py verify` checks it offline, and it carries specs, vectors, registries, ledgers and licenses. |
| Source | forge gone | Any clone plus the bundle's `reference_source_revision`; licenses (MPL/AGPL/Apache/CC-BY-SA) permit continuation. |
| Registries | steward gone | Assigned IDs keep their meaning (§4). A successor publishes a new registry *lineage*; `check-registries` blocks reuse. |
| Release trust | root holder gone | Existing installs keep working (no kill switch). New releases need a new root. Users must re-trust explicitly, and must never be silently migrated. |
| Users' accounts | unaffected | AccountID and control live on users' devices and Homes, not on project infrastructure. |
| Discovery/Homes | unaffected unless run by SeedLife | T0/T1/T2/T3 Homes are independent. A SeedLife-run T1 Home (if any) must give ≥30 days of overlap routes (R-187). |

**Must exist today:** bundles published to ≥2 independent mirrors per release (PR gate); a documented successor
procedure in [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) §8. **Gap G1:** no bundle mirror list exists yet. Disposition:
add a mirror list at first public release (pre-release criterion of V-017).

## D2. A cryptographic primitive breaks

**Scenario.** A practical break of X25519, ML-KEM-768 or Ed25519 is announced.

- 1:1 sessions are hybrid (PQXDH-class). A break of one component alone does not expose them, while a break of both
  would. Groups are classical in v1 (ADR-0107), so an X25519 break exposes group confidentiality; the docs already
  refuse PQ claims for groups.
- Response path: a signed SecurityAdvisory raises the peer-local CryptoProfile floor
  ([`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](../architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md)). Clients refuse sessions below the floor, re-key 1:1 sessions
  and ReInit groups to a new suite (V-019 path). No remote disable is involved.
- AccountIDs are random and algorithm-independent, so identity survives. AccountControlState rotates controller
  keys under the current majority.

**Must exist today:** CryptoProfile registry IDs are immutable, and ReInit is part of the v1 wire. **Gap G2:** how a
client acts on *conflicting* advisories (one signer raises a floor, another says it is not needed) is only prose.
Disposition: covered by the V-016 advisory model; `check-registries` guards the profile IDs.

## D3. Release-root loss or compromise

**Scenario.** The single S0 release-root key is lost, or the laptop holding it is stolen.

- Lost: no new signed releases. Existing clients keep working. A new root is published through the transparency log
  with a signed statement from the *old* root if it still exists; otherwise users must opt in manually. This is
  deliberately inconvenient, because a silent root swap would be the attack.
- Compromised: an attacker can sign a malicious release. Mitigations: release transparency (≥2 independent observers,
  R-164 §5.3), reproducible builds, and client pinning that refuses a release absent from transparency.

**Gap G3:** S0 is still 1-of-1 (R-083, ADR-0114). Disposition: a multi-party release root is an S2 requirement;
until then, public wording must say so.

## D4. A dominant extension or client captures the ecosystem

**Scenario.** One client distribution gains 90% share and ships a private extension that everyone depends on.

- Private extensions cannot redefine baseline IDs. The baseline interoperability profile admits only public,
  independently implementable semantics (§4).
- Critical-extension rules ([`PROTOCOL_EXTENSIBILITY.md`](../constitution/PROTOCOL_EXTENSIBILITY.md)) make a peer that does not understand a critical extension
  fail visibly instead of silently misinterpreting it.
- Users can export with om-archive-v1 (V-014) and move to another client without changing AccountID.

**Gap G4:** there is no measurement of "dependence on non-baseline extensions". Disposition: add an interop-profile
conformance report per distribution to PR gates (proposed; needs an ADR).

## D5. Vendor-wide provider block

**Scenario.** Cloudflare disables every OM Worker at once. This matters more now, because the first T1 reference is on
Cloudflare (ADR-0112).

- Affected: T2 own-cloud Homes, Cloudflare-hosted T1 Homes and their members, and T0 mailboxes sponsored from those
  Homes.
- Survivors: relationships whose RelaySet has a fallback on another provider family (Recommended profile prefers one),
  T3/static-binary Homes, and LAN/T4.
- Recovery: re-home via surviving routes, or re-introduce through contacts, keeping AccountID and history. Identity
  recovery (ADR-0111) does not depend on the Home.

**Gap G5:** with Cloudflare-first T1, the fraction of relationships that have a non-Cloudflare fallback on day one
may be near zero. Disposition: the V-020 vendor-block model must compute that fraction for realistic invite graphs,
and the T1 host console should offer a one-click fallback route on a second provider family. This is added to V-020
pre-engineering scope.

## D6. Coordinated vulnerability response

**Scenario.** A remotely triggerable parser bug in the reference client's image decoder.

- Report to [`SECURITY.md`](../../SECURITY.md) → fix → signed SecurityAdvisory with affected versions → distributions ship updates.
  Peers may apply a peer-local floor (refuse to send risky media types to clients below version X), with no remote
  disable.
- Homes can drop a known-malicious payload shape on ingress only when it is identifiable without decrypting (for
  example size or type metadata in opaque envelopes, which is usually not possible). Content filtering stays on endpoints.

**Gap G6:** the design has no advisory "embargo window" guidance for multi-distribution coordination. Disposition:
record it as a pre-release criterion of PR-level vulnerability response (proposed).

## D7. Offline reconstruction from the bundle alone (F-8)

**Scenario.** An independent implementer has only a Protocol Continuity Bundle produced by
[`continuity_bundle.py build`](../../tools/continuity_bundle.py) — no live OM registry, no forge, no site — and must locate the
rules for every frozen W0 object and every provider-observation interaction using only the bundled bytes.

This drill is mechanical, not narrative: `verify` already proves the reviewer can trust what is enumerated below,
because it (a) requires every doc listed in
[`spec/continuity/om-crypto-v1-bundle-manifest.json`](../../spec/continuity/om-crypto-v1-bundle-manifest.json) to be present
and hash-correct, and (b) rejects any relative markdown/CDDL/JSON reference from inside a bundled normative doc that
does not resolve inside the bundle or the manifest's `exceptions`. What was previously untested is whether the
manifest's member list actually *covers* the semantics needed to read the frozen bytes. The two tables below are
that coverage check, worked by hand once; `continuity_bundle.py check-manifest` keeps the member list itself from
drifting afterward, but it cannot know whether a table row is *correct* — that judgment stays a human/independent-review
task, to be redone whenever a row's CDDL type or provider role changes shape.

### Frozen W0 objects (`spec/cddl/*.cddl` top-level types) → defining bundled doc

| CDDL type(s) | Defined in (`spec/cddl/`) | Rules live in (bundled doc) |
|---|---|---|
| `AccountControlState`, `AccountControlHead`, `constitutional-policy`, `control-key-epoch`, `device-credential-ref`, `fork-ack` | [`account.cddl`](../../spec/cddl/account.cddl) | [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) |
| `RecoveryPolicy`, `RecoveryRequest`, `RecoveryVeto`, `recovery-method`, `recovery-activation` | [`account.cddl`](../../spec/cddl/account.cddl) | [`RECOVERY_AND_BACKUP_POLICY.md`](../architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) |
| `AccountRetirement` | [`account.cddl`](../../spec/cddl/account.cddl) | [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) |
| `DeviceCredential`, `DeviceEnrollmentNotice` | [`account.cddl`](../../spec/cddl/account.cddl) | [`IDENTITY_PROTOCOL.md`](../architecture/identity/IDENTITY_PROTOCOL.md), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) (device authority) |
| `AsyncKeyBundle`, `prekey-item` | [`keys.cddl`](../../spec/cddl/keys.cddl) | [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md) (named in the CDDL header comment) |
| `ContactDescriptor`, `ContactCapsule`, `request-key` | [`discovery.cddl`](../../spec/cddl/discovery.cddl) | [`CONTACT_SHARING.md`](../architecture/discovery/CONTACT_SHARING.md) |
| `ReferralAttestation`, `Invite`, `IntroductionTicket`, `MessageRequest`, `MessageRequestPayload`, `intro-service-ref` | [`discovery.cddl`](../../spec/cddl/discovery.cddl) | [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) |
| `IntroIdentityBinding` | [`discovery.cddl`](../../spec/cddl/discovery.cddl) | [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) (session acceptance), [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) (introduction flow) |
| `SponsoredMailboxGrant` | [`delivery.cddl`](../../spec/cddl/delivery.cddl) | [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) |
| `RelaySet`, `relay-entry` | [`delivery.cddl`](../../spec/cddl/delivery.cddl) | [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md) |
| `BlobLocationRecord` | [`delivery.cddl`](../../spec/cddl/delivery.cddl) | [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) |
| `ConversationEvent` and variants (`event-create/-edit/-delete/-reaction/-receipt/-other`), `MessageCreate`, `MessageEdit`, `MessageDelete`, `ReactionSet`, `ReceiptUpdate`, `hlc` | [`events.cddl`](../../spec/cddl/events.cddl) | [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md) (named in the CDDL header comment), [`TIME_AND_EXPIRY_SEMANTICS.md`](../architecture/events/TIME_AND_EXPIRY_SEMANTICS.md) (`hlc`) |
| `RichText`, `block*`, `inline*`, `attachment-ref` | [`events.cddl`](../../spec/cddl/events.cddl) | [`RICH_TEXT_AND_CONTENT_SECURITY.md`](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md) |
| `HomeError`, `Relay*Request/Response`, `BlobPut*`, `BlobRepairRequest`, `*CapabilityRequest/Record`, `IntroTicketRequest`, `KeyBundleFetch*`, `RecoveryRequestFanout`, `SponsorshipRedeemRequest` | [`home-api.cddl`](../../spec/cddl/home-api.cddl) (ADR-0133) | [`IMPLEMENTATION_INTERFACES.md`](../architecture/providers/IMPLEMENTATION_INTERFACES.md) |
| `account-id`, `device-id`, `event-id`, `conversation-id`, `blob-id`, `digest`, `hash-alg`, `signature`, `sig-role`, `principal-ref`, `principal-kind`, `extensions` | [`common.cddl`](../../spec/cddl/common.cddl) | [`CANONICAL_ENCODING.md`](../architecture/crypto/CANONICAL_ENCODING.md), [`IDENTITY_PROTOCOL.md`](../architecture/identity/IDENTITY_PROTOCOL.md) (`account-id`/`device-id` scope), [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) (`signature`/`sig-role`) |

### W0 provider roles (`spec/metadata/w0-metadata-exposure-descriptors.json`) → defining bundled doc

| Provider role | Defining/observation rules in (bundled doc) |
|---|---|
| `home_relay_mailbox`, `t0_sponsored_mailbox`, `t1_hosted_home` | [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) |
| `key_material_provider` | [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md) |
| `blob_store` | [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) |
| `push_provider` | [`NOTIFICATIONS_AND_PUSH.md`](../architecture/client/NOTIFICATIONS_AND_PUSH.md) |
| `discovery_publisher` | [`DISCOVERY_FABRIC.md`](../architecture/discovery/DISCOVERY_FABRIC.md), [`KEY_TRANSPARENCY_AND_DIRECTORY.md`](../architecture/discovery/KEY_TRANSPARENCY_AND_DIRECTORY.md) |
| `introgate` | [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) |
| `recovery_record_store` | [`RECOVERY_AND_BACKUP_POLICY.md`](../architecture/identity/RECOVERY_AND_BACKUP_POLICY.md), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) |
| `platform_credential_manager` | [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) |
| `clienthost_origin` | [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md), [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md) |
| `lan_observer` | [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md) |

**Gap G7:** the two tables above are maintained by hand; a CDDL type renamed/added or a provider role added to the
metadata descriptors will not automatically grow a new row. Disposition: `check-manifest` guards the *doc set*, not
this doc-to-object mapping; re-walk these tables whenever `spec/cddl/*.cddl` or
[`w0-metadata-exposure-descriptors.json`](../../spec/metadata/w0-metadata-exposure-descriptors.json) changes shape (added as
a pre-release criterion of V-017, alongside G1).

## Gaps found

| Gap | Severity | Disposition |
|---|---|---|
| G1 bundle mirror list | medium | V-017 pre-release: ≥2 independent mirrors per release |
| G2 conflicting advisories | medium | V-016 advisory model |
| G3 1-of-1 release root | high (known) | S2 requirement; honest wording until then (R-083) |
| G4 extension dependence measurement | low | proposed PR gate |
| G5 Cloudflare-first T1 lowers day-one diversity | **high** | added to V-020 scope: fallback-route fraction model plus host-console second-family fallback |
| G6 embargo guidance | low | proposed vulnerability-response criterion |
| G7 hand-maintained object/role → doc tables (D7) | low | re-walk tables when CDDL types or W0 provider roles change; V-017 pre-release |
