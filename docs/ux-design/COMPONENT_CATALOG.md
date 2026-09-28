# OM Shared Component Catalog — seed

These are shared `om-ui` primitives/components, not native-platform duplicates.

## Foundations

- AppShell
- IdentityProfileSwitcher
- ResponsivePane / SplitView
- NavigationBar / NavigationRail
- Button / IconButton
- TextField / SearchField
- List / ListItem
- Avatar
- Badge (actionable/status only)
- Menu
- Dialog / Sheet / Popover
- Toast / UndoToast
- InlineNotice / SecurityNotice
- Progress / Skeleton
- EmptyState
- Tooltip
- Divider
- Tabs where truly appropriate

## Messaging

- ConversationList
- ConversationListItem
- ConversationView
- MessageGroup
- MessageBubble
- MessageMeta
- ReplyPreview
- ReactionBar
- AttachmentTile
- MediaViewer
- VoiceMessage (record/lock/cancel, waveform, scrub, speed, pause/resume)
- LocationCard
- Composer
- DeliveryState
- Typing/Presence presentation (policy-controlled)
- DisappearingTimer presentation

## People/discovery

- PersonCard
- ContactCapsuleCard
- MessageRequestCard
- ShareMyOMCard
- QRScannerSurface
- InviteLinkCard
- IdentityChangeNotice

## Calls

- CallView
- ParticipantTile
- CallControlBar
- CallStatus
- CaptionPanel hook
- NetworkQualityIndicator (only when useful)
- NativeCallHandoff bridge surface

## Settings/security

- SettingsList
- SettingsRow
- ProtectionPresetPicker
- DeviceCard
- RecoveryMethodCard
- OMHomeHealthCard
- ProviderAdvancedPanel
- ThemePreview
- ThemeImportResult

## Component rules

1. Semantic HTML/native semantics first; ARIA only where needed.
2. Keyboard/focus behavior ships with component.
3. Motion is selected by semantic token, not component-local arbitrary timing.
4. Theme styles semantic tokens only; no arbitrary selectors/code.
5. Core-domain identifiers do not become DOM IDs or telemetry labels unnecessarily.
6. Components do not directly perform provider/crypto/account-control operations.
7. Every component has light/dark/high-contrast and reduced-motion test states where relevant.
