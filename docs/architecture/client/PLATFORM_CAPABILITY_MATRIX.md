# Platform Capability Matrix and Deletion Closure (V-021)

- **Status:** V-021 pre-engineering deliverable. It covers the matrix for every control in
  [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §4 and the deletion-closure
  checklist. The device-link notice invariant is `NoSilentLink` in [`verification/tla/AccountControl.tla`](../../../verification/tla/AccountControl.tla).
- **Evidence class:** *documented, not measured.* Entries come from platform documentation known as of mid-2026.
  Each one is re-verified by the V-021 pre-release forensic audit on real devices, and any entry marked `verify` is
  uncertain now.
- **Rule for the platform adapter:** every control reports `available | partial | unavailable` at runtime. The UI
  words its claims from that report, never from this table.

## 1. Controls per platform

Legend: ✅ available · ◐ partial or version-dependent · ✗ not available.

| Control (§4) | iOS/iPadOS native | Android native | macOS | Windows | Linux | PWA (Chromium) | PWA (Safari) | PWA (Firefox) |
|---|---|---|---|---|---|---|---|---|
| Exclude data from OS/cloud backup | ✅ `isExcludedFromBackup` + Keychain `…ThisDeviceOnly` + `NSFileProtectionComplete` | ✅ `dataExtractionRules` (cloud + device-transfer) / `allowBackup=false`; Keystore keys non-exportable | ✅ backup exclusion attribute; Keychain this-device-only | ◐ store under `%LOCALAPPDATA%` (not roamed); no app-level File History opt-out | ◐ XDG data dir; no standard backup opt-out | ✗ browser profile may be backed up by the OS | ✗ | ✗ |
| Keys sealed while locked | ✅ Data Protection classes, Secure Enclave keys | ✅ Keystore/StrongBox, `setUnlockedDeviceRequired` | ◐ Keychain; FileVault is disk-level | ◐ DPAPI/TPM-backed keys | ◐ keyring varies | ◐ non-extractable WebCrypto is an API property, **not** at-rest sealing; DeviceVault wrapping with PRF or a passphrase is the real protection | ◐ same | ◐ same |
| Block screenshots / screen recording | ◐ no official block; detect via `UIScreen.isCaptured` / capture notifications; secure-field layer technique is unofficial (`verify`) | ✅ `FLAG_SECURE` (screenshots, recording, recents) | ✅ `NSWindow.sharingType = .none` | ✅ `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` | ✗ | ✗ | ✗ | ✗ |
| Blank app-switcher snapshot | ✅ overlay on resign-active | ✅ `FLAG_SECURE` or `setRecentsScreenshotEnabled(false)` | n/a | n/a | n/a | ✗ OS takes it | ✗ | ✗ |
| Keyboard / IME learning off | ◐ refuse third-party keyboards per app; autocorrection off | ✅ `IME_FLAG_NO_PERSONALIZED_LEARNING` | ◐ autocorrect off per field | ◐ | ◐ | ◐ `autocomplete`/`autocorrect` hints only | ◐ | ◐ |
| Cloud spellcheck / writing tools off | ✅ `writingToolsBehavior = .none` (iOS 18+) | ◐ keyboard-vendor dependent | ✅ same as iOS on macOS 15+ | ◐ | n/a | ✅ `spellcheck="false"` (disables Enhanced Spellcheck for the field) | ✅ `spellcheck="false"`, `writingsuggestions="false"` | ✅ `spellcheck="false"` |
| Browser extensions reading the DOM | n/a | n/a | n/a | n/a | n/a | ✗ cannot prevent; warn once | ✗ | ✗ |
| Clear posted notifications | ✅ `removeDeliveredNotifications` (also clears mirrored watch copies) | ◐ `cancel()` clears the shade, but the user-visible **notification history (Android 11+) cannot be cleared by the app** | ✅ | ✅ | ◐ | ◐ `getNotifications()` + `close()` | ◐ `verify` | ◐ |
| Mark clipboard data sensitive | ✅ `UIPasteboard` `localOnly` + expiration | ✅ `ClipDescription.EXTRA_IS_SENSITIVE` (13+) | ◐ `org.nspasteboard.ConcealedType` convention | ✅ `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory=0` | ✗ | ✗ | ✗ | ✗ |
| Share sheets only on explicit action | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Limit non-assistive accessibility readers | ✗ | ◐ `setAccessibilityDataSensitive` (14+) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Keep message content out of OS search / suggestions | ✅ do not index; delete donated `INInteraction`s | ✅ do not publish; remove dynamic/sharing shortcuts | ✅ do not index | ✅ | ✅ | ✅ nothing is indexed by default | ✅ | ✅ |

### PlatformAdapter family mapping (V-021)

The interface names used by [`UI_CORE_PLATFORM_CONTRACT.md`](../ux/UI_CORE_PLATFORM_CONTRACT.md) map to the per-platform rows above as follows; each family reports `available | partial | unavailable` at runtime rather than assuming the documentation row is always true:

| PlatformAdapter family | Capability rows in this matrix |
|---|---|
| `SecureSurface` | Block screenshots / screen recording; blank app-switcher snapshot |
| `BackupExclusion` | Exclude data from OS/cloud backup; keys sealed while locked where the adapter exposes a file/key protection class |
| `InputPrivacy` | Keyboard / IME learning off; cloud spellcheck / writing tools off |
| `NotificationRemoval` | Clear posted notifications |
| sensitive `Clipboard` write | Mark clipboard data sensitive |

This mapping records availability for every shipped platform column in the table; runtime adapters still probe/report the actual capability, and pre-release forensic testing verifies the claims on supported OS/browser versions.

### Consequences for product claims

1. The **PWA cannot** exclude OS backups, block screenshots, blank the app switcher or stop extensions. Hardened and
   Maximum profiles recommend a native client, and the PWA says so instead of implying parity (R-167).
2. **Android notification history** keeps notification text that OM cannot delete. So Hardened defaults to
   content-free notifications on Android, and ViewOnce and disappearing messages never put text in notifications on
   any platform.
3. **Non-extractable WebCrypto keys are not an at-rest control.** Security-critical key material in the PWA must be
   wrapped by the DeviceVault hierarchy (V-006), so a copied browser profile is sealed.
4. **Linux** has the fewest controls. The Linux client states that screenshots, clipboard history and backups are
   outside OM's control.

## V1 reference-family capability expectations (ADR-0150)

This table is a **contract expectation, not a frozen API-availability claim**. Runtime adapters probe/report actual support and the UI states limits. It exists to prevent the PWA-first vertical slice from hard-coding web assumptions into Core or, conversely, native shells from inventing protocol semantics.

| Capability family | PWA V1 reference | Android V1 reference | iOS V1 reference | Desktop V1 reference | Semantic rule |
|---|---|---|---|---|---|
| `LocalDiscovery` | manual/QR/known-endpoint and browser-permitted local/WebRTC paths; automatic native-equivalent discovery is not assumed | permission-gated native local-network discovery where available, else manual/QR/known endpoint | permission-gated native local-network discovery where available, else manual/QR/known endpoint | native local discovery where OS support/permission exists, else manual/QR/known endpoint | every path yields only an ephemeral transport candidate; no platform path grants identity/relationship authority |
| `LocalDelivery` | conditional: ordered/reliable host-local WebRTC DataChannel with manual/QR `LanSignalEnvelopeV1`; report unavailable if browser permission/policy cannot establish it | ADR-0158 WebRTC DataChannel profile, native discovery optional | ADR-0158 WebRTC DataChannel profile, native discovery optional | ADR-0158 WebRTC DataChannel profile, native discovery optional | all platforms carry the same LanFrameV1/SealedDeliveryEnvelope/MLS bytes; V-024 must demonstrate browser feasibility before claiming PWA support |
| Background wake | Web Push/browser lifecycle | `fcm | unifiedpush | persistent | none`, capability-reported | APNs/background lifecycle or `none` with disclosed delay | OS notification/background/persistent mechanisms as available, or `none` | wake transport stays opaque; no platform gets plaintext or protocol authority |
| Secure key/storage surfaces | DeviceVault + browser capabilities with stated limits | Keystore/StrongBox where available | Keychain/Secure Enclave where compatible | Keychain/DPAPI-TPM/keyring or equivalent, capability-reported | handles/availability differ; canonical protocol output and account authority do not |
| `DurableElapsedClock` (ADR-0154) | `process_only`: per-session monotonic plus guarded wall-assisted gap credit under Recommended only | `boot_session` where a sleep-inclusive elapsed clock plus boot identifier is confirmed, else `process_only` | `boot_session` where a continuous monotonic clock plus boot-session identifier is confirmed, else `process_only` | `boot_session` per OS where confirmed, else `process_only` | security-critical verifier delays never credit reboot time on `boot_session`, and never credit wall time without the high-water and final veto sweep |
| Share/media/system calling | browser-capability limited | native adapters | native adapters | native filesystem/share/window/notification adapters | PlatformAdapter capability only; shared `om-ui`/Core semantics remain canonical |

V1 means all four are explicit reference-family targets, not that every row is feature-identical or that all binaries/desktop OS variants ship on the same day.

## 2. Deletion-closure checklist

When a message is deleted, expires, or is ViewOnce-consumed, **every** surface below is closed or disclosed. The
checklist is a W0 conformance test (`conformance:deletion-closure`), and the V-021 pre-release forensic audit
verifies it per platform.

| # | Surface | Closure action | Platform limits |
|---|---|---|---|
| 1 | Message store rows | delete; for SQLite, `secure_delete=ON` and periodic `VACUUM`; purge WAL on checkpoint | IndexedDB compaction timing is browser-controlled (disclose) |
| 2 | Full-text search index | delete postings with the row, in one transaction | none |
| 3 | Thumbnails, decoded media, previews and link-preview cache | delete by content key; derived caches share the message's lifecycle class | none |
| 4 | Blob cache and downloaded attachments | delete local copies; ask the BlobStore to delete when the sender owns the blob | remote deletion is best-effort (STORAGE doc) |
| 5 | Posted OS notifications | remove by id | Android notification history ✗ (disclose; §1.2) |
| 6 | Wearables / car displays | covered by #5 where the OS mirrors removals | third-party mirroring apps ✗ |
| 7 | OS search, Siri/Assistant suggestions, share shortcuts | never index content; delete donations and shortcuts that reference the conversation | none |
| 8 | App-switcher snapshot | refresh or blank when the app next backgrounds | PWA ✗ |
| 9 | Clipboard | if the last OM write is still on the clipboard and came from this message, clear it | cannot clear after other apps copied it |
| 10 | Linked devices | propagate the delete/expiry event; each device runs this checklist | an offline device closes on reconnect |
| 11 | OS/cloud backups | nothing to do if excluded (§1) | PWA ✗ (sealed by DeviceVault instead) |
| 12 | OM encrypted backups (BackupStore) | expiry metadata travels with backups; restore drops expired items | a backup made before deletion still holds ciphertext until it rotates (disclose) |
| 13 | Diagnostics and logs | never contain plaintext, so there is nothing to do | none |
| 14 | Push payloads | opaque by design, so there is nothing to do | none |
| 15 | Exported archives (om-archive-v1) | outside OM's control once exported; export UI says so | inherent |
| 16 | Recipient screenshots and cameras | cannot be closed; ViewOnce UI says so | inherent |
