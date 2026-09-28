# Notifications and Push

## 1. Principle

Push is a **wakeup optimization**, never identity authority, message storage, or protocol correctness.

OM message ciphertext remains at the user's relay(s). Vendor push should carry only opaque/generic information sufficient to wake the client.

## 2. Standard flow

```text
sender -> recipient relay stores sealed envelope
                     |
                     +-> PushGateway -> APNs/FCM: opaque wake token
                                          |
                                          v
                                    recipient app wakes
                                          |
                                    fetches ciphertext
                                          |
                                    decrypts locally
                                          |
                                    OS local notification
```

The push provider should not need sender name, conversation name, message plaintext, group ID, attachment title, or stable OM AccountID.

## 3. PushGateway separation

`PushGateway` is replaceable and narrowly scoped:

- maps an opaque OM wake handle to a platform push token;
- should not receive conversation plaintext;
- should minimize durable correlation between relay mailbox identity and platform token;
- can be self-hosted/community-operated where platform credentials allow;
- cannot authorize account/device/control changes.

Per-device wake handles should rotate when practical.

## 4. Notification privacy

The OM app decrypts locally, then chooses local display according to NotificationPolicy:

- sender + message preview;
- sender only;
- generic "New OM message";
- no lock-screen content.

This is independent from E2EE.

## 5. Android

Normal installations may use FCM for reliability/efficiency, but FCM is **not** a V1 protocol dependency. The Android `PlatformAdapter` supports a replaceable wake capability:

```text
fcm | unifiedpush | persistent | none
```

A V1 reference Android distribution MAY support one or more and SHOULD provide at least one practical non-FCM path when implementation quality permits. A UnifiedPush-compatible distributor/gateway is a replaceable `PushProvider` boundary; a persistent path is an authenticated encrypted relay/Home wake/fetch connection. Neither receives or creates account authority, and changing adapters never changes AccountID, DeviceID, RelationshipBinding, EventID or canonical message state.

Persistent connections cost battery/network and OS background allowance; UnifiedPush availability depends on the selected distributor. These differences must be measured and disclosed rather than advertised as equivalent by assumption. `none` is valid and means delayed wake, never message loss.

## 6. iOS

APNs is the practical mechanism for reliable background wake/alerts under iOS lifecycle rules.

Apple documents background/silent notifications as low-priority and not guaranteed; excessive silent background notifications may be throttled. Therefore OM MUST NOT promise that disabling APNs preserves instant closed-app delivery on iOS.

Maximum Privacy may offer:

> **Disable Apple push** — Apple no longer receives OM wake timing through APNs, but messages may not appear until iOS grants background time or the app is opened.

Users choose the tradeoff.

## 7. Calls

Incoming-call UX may require platform-specific push/call mechanisms. The same rule holds: push carries only the minimum opaque call wake/rendezvous material; actual call authentication and media keys are established through OM's encrypted/authenticated call protocol.

## 8. Failure behavior

Push failure must never lose a message. Opening/resuming OM fetches pending relay envelopes. Push token rotation, adapter switching or provider migration must not change DeviceIdentity or AccountID. Old registrations are revoked/retired where the platform/provider permits. If retirement cannot be confirmed, the obsolete registration is residual metadata/wake exposure only: a late wake can at most trigger the ordinary local fetch path and is never message/account/fetch authority. OM claims a finite retention bound only when the provider/adapter contract declares an enforceable finite lease; otherwise duration is provider/platform-defined and disclosed honestly (ADR-0153).

## 2026-09-22 operatorless push refinement

The reference PWA should prefer per-user Web Push ownership:

```text
sender -> recipient PersonalRelay/Worker -> browser Web Push endpoint -> recipient PWA
```

The OM Project does not need a universal push gateway.

For the V1 Android/iOS/Desktop reference distributions, APNs/FCM/UnifiedPush-compatible gateways or other capability-declared wake adapters may be operated by that distribution or another configured PushProvider. Their operational/legal boundary belongs to the distribution/provider and never becomes canonical protocol infrastructure.

## Multiple IdentityProfiles

A V1 client installation may host several IdentityProfiles and the reference client exposes switching when two or more profiles are configured. Push routing uses opaque per-profile/per-device handles that the recipient client maps locally. Push services must not receive a stable cross-profile person identifier. The notification UI may indicate the destination profile only according to that profile's privacy policy.

**The platform push token is itself a linkage point (ADR-0129).** An APNs/FCM token is per app install, and a Web Push subscription is per service-worker registration. If profiles A and B hand the same token or endpoint to different Homes or PushGateways, those parties, and the platform push service that sees two senders targeting one endpoint, can link the profiles. Opaque wake handles above the token do not prevent this. Therefore:

1. **Web:** each IdentityProfile registers its own service worker with a distinct scope and obtains its own Web Push subscription (distinct endpoint and keys). No subscription is shared between profiles.
2. **Native (one token per app):** the token is a **declared linkage point**. The push provider's MetadataExposureDescriptor states that one token is shared by every profile on the install that uses vendor push, so any party receiving it for two profiles can link them.
3. **"Keep separate" profiles** ([`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](MULTI_IDENTITY_AND_PROFILE_ISOLATION.md)) must not silently share the token. They offer, per profile: disable vendor push for this profile (it then wakes on app open or background fetch, with the §6 delay warning), or route every profile's wakes through one user-chosen gateway that is declared as the linkage point and fans out through unlinkable per-profile handles. The UI states the tradeoff in one sentence.
4. V-015 tests that two profiles on one install never present the same subscription or token to two different parties unless the descriptor declares it.
