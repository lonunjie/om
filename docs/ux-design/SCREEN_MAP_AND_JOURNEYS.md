# OM Reference Screen Map and Journeys

## Primary shell

```text
AppShell
├── Chats
│   ├── Conversation list
│   ├── Message requests
│   ├── Conversation
│   └── Conversation details / media
├── Calls
│   ├── Recent/missed calls
│   ├── Start call
│   └── Active call
├── People
│   ├── Requests
│   ├── Contacts
│   ├── Share my OM
│   └── Add/find person
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

## Journey: offline/local

1. WAN drops; global status says **Internet unavailable — local contacts may still work**.
2. Local history remains fully readable/searchable.
3. Existing LAN peers discovered through private rendezvous may show Nearby.
4. Local messaging/files/calls work where available.
5. WAN restoration triggers background reconciliation without replacing the current screen/draft.
