# OM Shared Component Catalog — Reconciled R15

These are shared semantic `om-ui` components/primitives. Native/web implementations may render differently while preserving state/action/accessibility contracts.

## Foundations

- AppShell
- IdentityProfileSwitcher / AccountControl
- ResponsivePane / SplitView / MultiPane
- DestinationTabs / NavigationRail (conditional/user-composable)
- Omnibar / OmnibarResult / CommandPaletteAdapter
- Button / IconButton
- SemanticIcon (reviewed built-in registry; ADR-0162)
- TextField / RichTextField / SearchField
- List / ListItem / VirtualizedList
- Avatar / FallbackAvatar
- Badge (status/actionable only)
- Menu / ContextMenu
- Dialog / Sheet / Popover
- Toast / UndoToast
- InlineNotice / SecurityNotice
- Progress / Skeleton
- EmptyState
- Tooltip
- Divider
- FocusIndicator / SelectionIndicator

## Messaging

- ConversationList
- ConversationListItem
- ConversationView
- MessageGroup
- MessageBubble
- MessageMeta
- ReplyPreview
- ThreadPreview / ThreadSurface
- TopicChip / TopicNavigator
- ReactionBar
- PollCard / PollComposer / PollResults
- AttachmentTile / TransferState
- MediaViewer
- Sticker / ExpressiveMedia (static/reduced-motion fallback required)
- VoiceMessageRecorder / VoiceMessagePlayer
- LocationCard
- LinkPreview
- Composer
- RichTextFormattingBar / sanitized-paste notice
- ScheduledMessageIndicator / ScheduledQueueSurface
- DeliveryState
- Typing/Presence presentation (policy-controlled)
- ReadReceipt presentation (policy-controlled)
- DisappearingTimer / ViewOnceState
- CallTimelineNotice (view/history item; not durable call signaling event)

## People/discovery/onboarding

- PersonCard
- ContactCapsuleCard
- MessageRequestCard
- ShareMyOMCard
- QRScannerSurface
- InviteLinkCard
- InviteScopeCard
- IdentityChangeNotice
- HomeTierChoice
- SponsorDisclosure
- ClientOriginChoice

## Spaces/Rooms/Publication

- SpaceNavigator
- SpaceTreeItem (cycle-safe)
- RoomCard / RoomNavigator
- TopicNavigator
- PublicationFeed
- PublicationPost
- PublicationProvenance
- FollowPublicationAction
- DiscussionRoomLink

## Calls

- CallView
- ParticipantTile
- CallControlBar
- CallStatus
- CaptionPanel hook
- NetworkQualityIndicator (only when useful)
- NativeCallHandoff bridge surface
- IncomingCallAdapter surface

## Settings/security

- SettingsList / SettingsRow / SettingsSearchResult
- ProtectionPresetPicker
- ExperienceProfilePicker
- DeviceCard
- DeviceSuspendState
- AccountSafetyFreezeNotice
- RecoveryCustodianCard
- RecoveryOperationState
- OMHomeHealthCard
- ProviderAdvancedPanel
- ConnectedServiceCard / BridgeBoundaryNotice
- ThemePreview
- ThemeImportResult
- EvidenceSelection / ReportDestination

## Component rules

1. Semantic HTML/native semantics first; ARIA only where needed.
2. Keyboard/focus behavior ships with the component.
3. Every gesture has a non-gesture equivalent for essential actions.
4. Motion is selected by semantic token, not component-local arbitrary timing.
5. Color/motion/sound/haptic never carry essential meaning alone.
6. Theme styles semantic tokens only; no arbitrary selectors/code/network fetching.
7. Protected security surfaces are theme-proof where architecture requires it.
8. Core-domain identifiers do not become DOM IDs or telemetry labels unnecessarily.
9. Components do not directly perform provider/crypto/account-control operations.
10. Actionable components bind through semantic Action Registry entries and then typed Core intents or PlatformAdapter capabilities.
11. Every component has light/dark/high-contrast and Reduced Motion states where relevant.
12. Compact targetability follows OM release guidance; visual glyph size and hit target are separate.
13. User content/names support bidi isolation and large-text/reflow behavior.
14. Failure belongs to the affected object whenever possible; generic error dialogs are the exception.
15. Semantic icons come from the reviewed built-in icon registry; untrusted themes cannot replace action/security glyphs (ADR-0162).
16. Omnibar learned state is IdentityProfile-local; asynchronous result arrival cannot retarget the currently armed keyboard selection (ADR-0162).
