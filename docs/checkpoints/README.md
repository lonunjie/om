# Design checkpoints

OM's design evolved through dated checkpoints. Until 2026-09-24 each checkpoint was delivered as a zip
(`om-master-planning-design-YYYYMMDD-HHMM-PHT.zip`) that carried every earlier snapshot under `history/`.
Those snapshots are now ordinary git commits, each tagged `checkpoint/YYYYMMDD-HHMM`:

```sh
git tag -l 'checkpoint/*'                    # every checkpoint
git diff checkpoint/20260923-0920 checkpoint/20260924-0229 --stat
git show checkpoint/20260921-0801:docs/overview/BLUEPRINT.md
```

Older documents still mention paths such as `history/20260921-0801/`; read those as the tag
`checkpoint/20260921-0801`. The `CHECKPOINT_*.md` files in this folder are the checkpoint records
written at the time. They are historical, except the newest, which describes the current state together with
[`SNAPSHOT.md`](../../SNAPSHOT.md).

Checkpoints now arrive either as commits made directly in this repository or as design drops imported with
`tools/om_drop.py import` (see [`AGENTS.md`](../../AGENTS.md)).

## Checkpoint notes carried from the package README

These notes accumulated in the package README, one section per checkpoint, until 2026-09-24. They are kept
here verbatim so the README can stay a reading guide.

## 2026-09-21 10:08 discovery/resilience checkpoint

New canonical material:

- [`DISCOVERY_FABRIC.md`](../architecture/discovery/DISCOVERY_FABRIC.md)
- [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md)
- [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md)
- [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md)
- [`CENSORSHIP_AND_NETWORK_RESILIENCE.md`](../architecture/network/CENSORSHIP_AND_NETWORK_RESILIENCE.md)
- `CHECKPOINT_20260921_1008.md`
- ADR-0019 through ADR-0025
- ContactDescriptor / Invite / IntroductionTicket / RelaySet / discovery-provider logical schemas

The preceding 2026-09-21 08:01 checkpoint is preserved under `history/20260921-0801/`.

## 2026-09-21 21:32 continuity/trust checkpoint

New canonical material:

- [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md)
- [`RECOVERY_AND_BACKUP_POLICY.md`](../architecture/identity/RECOVERY_AND_BACKUP_POLICY.md)
- [`PROVISIONING_AND_BOOTSTRAP.md`](../architecture/providers/PROVISIONING_AND_BOOTSTRAP.md)
- [`CONTACT_SHARING.md`](../architecture/discovery/CONTACT_SHARING.md)
- [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md)
- [`SECURITY_PRIVACY_PROFILES.md`](../architecture/ux/SECURITY_PRIVACY_PROFILES.md)
- [`NOTIFICATIONS_AND_PUSH.md`](../architecture/client/NOTIFICATIONS_AND_PUSH.md)
- [`DISAPPEARING_MESSAGES_AND_RETENTION.md`](../architecture/storage/DISAPPEARING_MESSAGES_AND_RETENTION.md)
- [`PROTOCOL_EXTENSIBILITY.md`](../constitution/PROTOCOL_EXTENSIBILITY.md)
- [`CHECKPOINT_20260921_2132.md`](CHECKPOINT_20260921_2132.md)
- ADR-0026 through ADR-0042
- AccountControlState / AccountControlHead / RecoveryPolicy / ContactCapsule / SecurityProfile / NotificationPolicy / ProtocolExtension logical schemas

The preceding 10:08 discovery/resilience checkpoint is preserved under `history/20260921-1008/`.


## 2026-09-22 07:34 operatorless/ecosystem checkpoint

New canonical material:

- [`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md)
- [`PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md`](../architecture/client/PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md)
- [`LEGAL_AND_OPERATIONAL_BOUNDARIES.md`](../constitution/LEGAL_AND_OPERATIONAL_BOUNDARIES.md)
- [`EVIDENCE_AND_EXTERNAL_REPORTING.md`](../architecture/safety/EVIDENCE_AND_EXTERNAL_REPORTING.md)
- [`CHECKPOINT_20260922_0734.md`](CHECKPOINT_20260922_0734.md)
- ADR-0043 through ADR-0050
- EvidencePackage / ProvisioningMethod / ClientDistribution logical schemas

Major change: OM is now explicitly modeled as an **operatorless protocol/core plus independent deployments and distributions**. The first reference client is PWA-first for independence, but native shells remain planned for stronger platform integration. Optional future businesses may sell convenience but receive no protocol privilege.

The preceding 2026-09-21 21:32 continuity/trust checkpoint is preserved under `history/20260921-2132/`.


## 2026-09-22 13:42 shared UI/OwnMessenger checkpoint

New canonical material:

- [`UI_UX_SYSTEM.md`](../architecture/ux/UI_UX_SYSTEM.md)
- [`MOTION_AND_PERFORMANCE.md`](../architecture/ux/MOTION_AND_PERFORMANCE.md)
- [`THEMES_FONTS_AND_PERSONALIZATION.md`](../architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md)
- [`ACCESSIBILITY_AND_INPUT.md`](../architecture/ux/ACCESSIBILITY_AND_INPUT.md)
- [`UI_CORE_PLATFORM_CONTRACT.md`](../architecture/ux/UI_CORE_PLATFORM_CONTRACT.md)
- `docs/ux-design/` theme/token artifacts
- [`docs/research/CURRENT_UI_REFERENCES_20260922.md`](../research/CURRENT_UI_REFERENCES_20260922.md)
- [`CHECKPOINT_20260922_1342.md`](CHECKPOINT_20260922_1342.md)
- ADR-0051 through ADR-0058

Major change: the reference product now has one shared `om-ui` for PWA/iOS/Android/Desktop, a semantic MotionSystem, adaptive window layouts, release-gated accessibility and deeply customizable but data-only/sandboxed OwnMessenger themes.

The preceding 2026-09-22 07:34 operatorless/ecosystem checkpoint is preserved under `history/20260922-0734/`.

## 2026-09-22 14:34 PHT — Historical engineering-handoff checkpoint (superseded)

Architecture is now frozen enough to build and attack. New canonical engineering material includes:

- [`ENGINEER_START_HERE.md`](../process/ENGINEER_START_HERE.md) and [`ENGINEERING_HANDOFF.md`](../process/ENGINEERING_HANDOFF.md);
- [`ENGINEERING_REPOSITORY_LAYOUT.md`](../process/ENGINEERING_REPOSITORY_LAYOUT.md);
- [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md);
- [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md);
- [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md);
- [`RICH_TEXT_AND_CONTENT_SECURITY.md`](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md);
- [`LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md`](../architecture/storage/LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md);
- [`FORMAL_VERIFICATION_AND_FUZZING.md`](../process/FORMAL_VERIFICATION_AND_FUZZING.md);
- [`docs/research/SECURITY_CRYPTO_REAUDIT_20260922.md`](../research/SECURITY_CRYPTO_REAUDIT_20260922.md);
- RichText/IdentityProfile/DeviceVault logical schemas;
- ADR-0059 through ADR-0068.

Multiple identities are structurally supported from v1 but the switcher can ship later. Every built-in theme supports light/dark + Auto. Production crypto remains a release gate rather than something coding agents may improvise.

## 2026-09-22 23:02 architecture-hardening checkpoint

The 14:34 engineering transition is superseded. OM remains in architecture hardening until the architecture-freeze contract deliberately authorizes engineering.

New blocking architecture material includes:

- project philosophy/anti-capture constitution;
- layered licensing/contribution policy and SeedLife stewardship/succession rails;
- asynchronous 1:1/MLS key-material publication;
- one-to-one multi-device session management;
- MLS Commit ordering and atomic group-policy/MLS binding;
- catastrophic RelaySet reachability and blob-location repair;
- unified time/expiry semantics;
- self-hosted ClientHost update lifecycle;
- machine-enforced repository contract/file registry/architecture map.

No product engineering phase begins from this checkpoint.
## 2026-09-23 05:45 PHT — architecture-resolution + OM identity checkpoint

The former 60-item open-question ledger and architecture queue A-001..A-015 are resolved into accepted decisions plus typed verification/release/deferred ledgers. [`ARCHITECTURE_RESOLUTIONS_20260923.md`](../reviews/ARCHITECTURE_RESOLUTIONS_20260923.md) is the human-readable closure record. **No design blocker remains open**, but V-001..V-017 still prevent design confidence from being mistaken for implementation proof; engineering remains unauthorized.

Country-specific legal gating was removed from canonical architecture. [`LEGAL_AND_OPERATIONAL_BOUNDARIES.md`](../constitution/LEGAL_AND_OPERATIONAL_BOUNDARIES.md) now uses a role-based, jurisdiction-neutral model and keeps project operation/custody deliberately minimal.

