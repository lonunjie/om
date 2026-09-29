# OM Reference Screen Map and Journeys

## Primary shell

```text
AppShell
├── IdentityProfile switcher (shown when 2+ profiles are configured)
├── Omnibar (conversations / people / local content / settings / actions)
├── New conversation
├── Conversation workspace
│   ├── Conversation list
│   ├── Message requests
│   ├── Conversation
│   └── Conversation details / media
├── Secondary surfaces (Omnibar/context/optional user pin)
│   ├── Call history / active call
│   ├── Contacts / people / requests
│   └── Files / other local views
└── Profile → Settings
    ├── Profile / identity
    ├── Privacy & protection
    ├── Devices
    ├── Recovery
    ├── Notifications
    ├── Appearance / themes
    ├── OM Home
    └── Advanced / diagnostics
```

The default shell has no required Chats / Calls / People destination tabs. Calls and people remain directly reachable from conversation/person context, the Omnibar, and optional user pins.


## Journey: use the Omnibar

1. User taps the Omnibar or presses `Cmd/Ctrl+K`.
2. Without typing, OM shows a small recognition-oriented set of recent/pinned conversations and common actions.
3. Typing filters local conversations/people/messages/settings first; external discovery is a visibly separate result class.
4. Repeated successful local selections may adjust ordering inside the current IdentityProfile using bounded recency/frequency state.
5. User can remove one learned suggestion, clear/disable learning, or pin an item.
6. Destructive/security actions never auto-run from autocomplete and keep their normal Core authorization/confirmation.

## Journey: first-run ordinary user

1. Welcome → Create my OM.
2. Choose name/avatar.
3. Set up OM Home using provider-native deployment flow.
4. Return to OM; automated health verification.
5. Ready screen with QR/link.
6. Share or scan someone else's OM.
7. Start chatting.

Recovery is not forced into this journey.

## Journey: Carol introduces Alice to Bob

1. Carol opens Alice → Share contact.
2. Carol chooses Bob/conversation/share sheet.
3. Bob opens ContactCapsule card; sees **Shared by Carol**.
4. Bob taps Send message request.
5. Alice receives constrained request.
6. Alice accepts.
7. Alice/Bob establish private relationship capabilities; Carol is no longer involved.

## Journey: unknown sender

1. Incoming request appears under Requests, not normal contact relationship.
2. User previews bounded content.
3. Accept → create relationship and normal capabilities.
4. Block → reject/revoke introduction path locally.

## Journey: stolen device

1. Settings → Devices.
2. Open suspicious device.
3. Tap **Suspend this device**.
4. Core immediately quarantines future secret delivery.
5. UI shows pending/final control-state status if stronger authorization is required for permanent removal.
6. Contested state uses calm Safety Freeze surface rather than LWW.

## Journey: enable recovery

1. Settings → Identity → Recovery.
2. Current state explicitly says Not configured.
3. User chooses Recovery key / Trusted people / Spare device / Advanced future method.
4. UI explains what the method recovers: identity, history, or both as separate choices.
5. User verifies completion; OM never silently claims recovery is configured.

## Journey: customize appearance

1. Settings → Appearance.
2. Choose default theme, import `.omtheme`, font, wallpaper, density and motion intensity.
3. Preview shows chat + settings + warning/destructive state + light/dark/high-contrast.
4. Validator blocks unsafe/invalid bundle before apply.
5. Accessibility overrides remain available above theme preferences.


## Journey: switch IdentityProfile

1. User opens the profile chip/switcher.
2. Client shows only configured local IdentityProfiles, with each profile's own theme/name presentation.
3. Selecting a profile rebinds the shell to that profile's namespaced view/cache/draft state without logout, reload or rekey.
4. Unknown deep links ask which profile should act.
5. Per-profile push/privacy policy remains in force; the switcher itself creates no public cross-profile identifier.

## Journey: share a one-shot location

1. Composer → Attach/share → Location.
2. User explicitly chooses current point or another point; no background tracking begins.
3. Composer shows the location card plus a textual fallback before send.
4. Recipient with `om.location-card.v1` support first verifies that the fallback contains the exact canonical `geo:<lat-fixed-7>,<lon-fixed-7>` token for the extension point, then gets card presentation; on mismatch it ignores the non-critical card extension and renders the same W0 fallback an older client sees (ADR-0149).
5. Opening an external map provider follows the recipient's configured map/privacy policy.

## Journey: offline/local

1. WAN drops; global status says **Internet unavailable — local contacts may still work**.
2. Local history remains fully readable/searchable.
3. Existing LAN peers discovered through private rendezvous may show Nearby.
4. Local messaging/files/calls work where available.
5. WAN restoration triggers background reconciliation without replacing the current screen/draft.
