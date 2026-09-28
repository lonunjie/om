# ADR-0153 — Wake-registration retirement and residual-leakage closure

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** Run 9B clarification discovered while verifying ADR-0152 replaceable Android wake
- **Builds on:** ADR-0129 and ADR-0152
- **Amends:** ADR-0152 only where its prose called an unconfirmed provider-side retirement a generically "bounded" leakage without naming an enforceable bound
- **Does not amend:** OM wire formats, AccountID/DeviceID/RelationshipBinding authority, relay mailbox semantics, platform push-token behavior, or the rule that push is only a wake optimization

## Context

ADR-0152 requires old wake registrations to be retired/revoked where the platform or provider permits. While building the Run 9B V-015 evidence, one overclaim became visible: [`NOTIFICATIONS_AND_PUSH.md`](../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md) described a failed retirement as "bounded metadata/wake leakage", but neither OM nor a conforming client can cryptographically force a third-party platform/provider to delete or expire an obsolete registration on a particular schedule.

The security property OM can enforce is narrower and stronger in the right place: an obsolete wake registration never becomes message storage, fetch authority, account authority, identity continuity or canonical conversation state. Any stale wake can at most cause an endpoint wake/fetch attempt. Duration/retention is provider behavior unless an enforceable lease is actually declared.

## Decision

1. **Retire where possible.** On adapter/provider switch, logout, profile push disablement, token replacement or profile removal, the client/gateway attempts to revoke or retire the obsolete registration using the provider/platform mechanism available.
2. **Generation-current locally.** The client/gateway treats only the current profile-scoped wake-registration generation as active for new wake requests. An obsolete opaque wake handle is never accepted as account/device/relationship/message-fetch authority.
3. **Stale wake is harmless.** If a platform/provider cannot confirm deletion, a late wake from the obsolete registration may cause only the same generic local wake/fetch path that any current wake could cause. It cannot name a message, authorize a fetch, alter AccountID/DeviceID/RelationshipBinding/EventID, or recover plaintext by itself.
4. **No invented time bound.** OM claims a finite retirement/retention bound only when the relevant PushProvider/PlatformAdapter contract and MetadataExposureDescriptor declare an enforceable finite lease/expiry. Otherwise the residual registration's retention duration is provider/platform-defined and must be disclosed honestly.
5. **Metadata accounting remains.** Until confirmed retirement or a declared lease expiry, the obsolete provider may retain token/endpoint/profile-linkage history and may observe late wake attempts. That is residual metadata exposure, not protocol authority.
6. **Keep-separate profiles remain fail-closed.** A native token shared by several profiles may be exposed only through the already-declared single linkage point or other explicitly declared profile-linkage policy. Adapter migration cannot silently fan one token out to unrelated PushProviders.

This decision deliberately avoids introducing a new network object or an arbitrary OM-wide wake-registration TTL. A reference PushGateway may implement finite leases as a useful privacy control, but the architecture must distinguish that provider contract from a cryptographic guarantee.

## Verification consequence

Run 9B V-015 evidence must exercise:

- FCM, UnifiedPush-compatible, persistent and `none` modes over the same canonical relay mailbox;
- provider failure and adapter switching without message loss or authority change;
- successful old-registration retirement where supported;
- unconfirmed retirement as metadata-only/harmless-wake state, without pretending a universal time bound;
- keep-separate cross-profile token/linkage rejection unless the linkage point is explicitly declared;
- descriptor consistency for push timing, stable token/device correlation and cross-profile linkage.

No additional engineering scope is unlocked by this ADR; existing V-015 independence requirements still apply.
