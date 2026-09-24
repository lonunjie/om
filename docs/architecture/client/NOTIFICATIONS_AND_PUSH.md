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

Normal installations may use FCM for reliability/efficiency. A Google-free build/runtime can support a persistent encrypted connection or future alternate push provider such as a UnifiedPush-compatible adapter if implementation quality permits.

Persistent connections cost battery/network and must be tested honestly rather than advertised as equivalent by assumption.

## 6. iOS

APNs is the practical mechanism for reliable background wake/alerts under iOS lifecycle rules.

Apple documents background/silent notifications as low-priority and not guaranteed; excessive silent background notifications may be throttled. Therefore OM MUST NOT promise that disabling APNs preserves instant closed-app delivery on iOS.

Maximum Privacy may offer:

> **Disable Apple push** — Apple no longer receives OM wake timing through APNs, but messages may not appear until iOS grants background time or the app is opened.

Users choose the tradeoff.

## 7. Calls

Incoming-call UX may require platform-specific push/call mechanisms. The same rule holds: push carries only the minimum opaque call wake/rendezvous material; actual call authentication and media keys are established through OM's encrypted/authenticated call protocol.

## 8. Failure behavior

Push failure must never lose a message. Opening/resuming OM fetches pending relay envelopes. Push token rotation or provider migration must not change DeviceIdentity or AccountID.

## 2026-09-22 operatorless push refinement

The reference PWA should prefer per-user Web Push ownership:

```text
sender -> recipient PersonalRelay/Worker -> browser Web Push endpoint -> recipient PWA
```

The OM Project does not need a universal push gateway.

For future native distributions, APNs/FCM gateways may be operated by that distribution or another configured PushProvider. Their operational/legal boundary belongs to the distribution/provider and never becomes canonical protocol infrastructure.

## Multiple IdentityProfiles

A single client installation may later host several profiles. Push routing uses opaque per-profile/per-device handles that the recipient client maps locally. Push services must not receive a stable cross-profile person identifier. The notification UI may indicate the destination profile only according to that profile's privacy policy.

**The platform push token is itself a linkage point (ADR-0129).** An APNs/FCM token is per app install, and a Web Push subscription is per service-worker registration. If profiles A and B hand the same token or endpoint to different Homes or PushGateways, those parties, and the platform push service that sees two senders targeting one endpoint, can link the profiles. Opaque wake handles above the token do not prevent this. Therefore:

1. **Web:** each IdentityProfile registers its own service worker with a distinct scope and obtains its own Web Push subscription (distinct endpoint and keys). No subscription is shared between profiles.
2. **Native (one token per app):** the token is a **declared linkage point**. The push provider's MetadataExposureDescriptor states that one token is shared by every profile on the install that uses vendor push, so any party receiving it for two profiles can link them.
3. **"Keep separate" profiles** ([`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](MULTI_IDENTITY_AND_PROFILE_ISOLATION.md)) must not silently share the token. They offer, per profile: disable vendor push for this profile (it then wakes on app open or background fetch, with the §6 delay warning), or route every profile's wakes through one user-chosen gateway that is declared as the linkage point and fans out through unlinkable per-profile handles. The UI states the tradeoff in one sentence.
4. V-015 tests that two profiles on one install never present the same subscription or token to two different parties unless the descriptor declares it.
