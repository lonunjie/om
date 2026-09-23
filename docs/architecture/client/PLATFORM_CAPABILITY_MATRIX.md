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
