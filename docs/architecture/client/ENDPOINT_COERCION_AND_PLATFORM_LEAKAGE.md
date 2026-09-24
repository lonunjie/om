# Endpoint Coercion, Proximate Adversaries and Platform Leakage

- **Status:** Canonical architecture
- **Checkpoint:** 2026-09-24 02:29 PHT
- **Decision:** ADR-0103
- **Gate:** V-021
- **Threats:** T-022..T-028 in [`machine/threat_register.jsonl`](../../../machine/threat_register.jsonl)

## 1. Scope

E2EE protects messages in transit and at providers. Most real-world messenger compromises happen at the endpoint, through someone who can touch the device or through the platform around the app. The 09:20 threat model covered remote compromise of an endpoint. It did not cover:

- **coercive adversaries**: device seizure, compelled unlock, border inspection, detention;
- **proximate adversaries**: intimate partners, family members, employers, and anyone with brief physical access or knowledge of a passcode;
- **platform leakage**: OS and browser features that copy plaintext outside OM's storage classes.

OM cannot defeat an adversary holding an unlocked device. It can remove silent persistence, shrink what an unlock exposes, and state the limits honestly.

## 2. Proximate adversary: no silent device linking (R-166)

The classic messenger stalking attack is to link an attacker's device (a web/desktop session) while the victim's phone is briefly unlocked, then read everything indefinitely.

Rules:

1. Every new messaging or control device enrollment produces an **authenticated, non-suppressible notice** on every other active device of the account. The notice comes from AccountControlState, not from provider UI.
2. **Devices** (Settings) lists every messaging/control device with enrollment time and last-seen class. Suspend (quarantine) is one tap.
3. Linking requires an unlock of the enrolling device's DeviceVault and a short-lived ceremony (already required). Hardened adds a second confirmation on another existing device when one exists.
4. The client periodically reminds the user of long-idle linked devices ("You have a device linked 3 months ago that hasn't been used"). This follows NotificationPolicy and never blocks messaging.
5. Themes, plugins, bots and bridges cannot hide or restyle these notices (existing R-057).
6. **Personal grants are as loud as device links (ADR-0129).** A personal grant that gives persistent reading does the same job as a silent link, and each can be set up in seconds on an unlocked phone. Creating or expanding any of these produces the same authenticated, non-suppressible notice on every other active device of the account:
   - a personal service installation or plugin that holds plaintext read together with any egress (including a `plaintext_egress` policy change);
   - a `RepresentationGrant`;
   - a personal `AutomationRule` that reads or forwards messages;
   - a `local_component` ServicePrincipal.

   These grants are account state: they sync to the account's other devices, never live only on the device that created them.
7. **Connected services** (Settings, next to Devices) lists every such grant, plus full-Room services and bridges in the user's Rooms, with who created it, when, from which device, and what it can read and send. Revoke is one tap from any device.

V-021 includes a failing trace for this rule: an egress grant created on one device with no notice on the others must be rejected by the model.

## 3. Coercive adversary

### 3.1 What OM provides

| Control | Behavior |
|---|---|
| App lock | Separate from device unlock; biometric or PIN; configurable timeout. Recommended: off by default, offered after onboarding. Hardened: on. |
| Per-profile lock | Each IdentityProfile can have its own lock (profile isolation already required by R-064). |
| Quick lock | One gesture/menu action locks every profile immediately and purges decrypted caches (DerivedCache and temporary files). |
| Disappearing defaults | Per-conversation and account-wide default timers (existing semantics). |
| Minimal lock-screen content | NotificationPolicy; Hardened default is generic. |
| Remote suspend | From any other device or recovery path: quarantine the seized device, which stops future secrets (existing two-stage revocation). |
| Travel posture (optional) | Temporarily unlink a profile from this device: local data is wiped and the profile is re-linked later from another device or recovery. This is honest data removal, not concealment. |

### 3.2 What OM does not promise (R-167)

- **No plausible-deniability hidden profiles or duress PINs that pretend to be empty.** Forensic tools and coercers can often detect hidden-volume schemes. A false promise endangers users more than an honest absence does. OM may revisit this only through a research ADR with external review.
- No protection of plaintext already displayed, screenshotted or exported.
- Remote wipe of a device controlled by an adversary is best-effort. It only takes effect if the device comes online with OM still intact.

## 4. Platform leakage controls (R-168)

Each `LocalDataClass` ([`LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md`](../storage/LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md)) maps to platform controls. The platform adapter must expose each control's availability, and V-021 publishes the matrix: [`PLATFORM_CAPABILITY_MATRIX.md`](PLATFORM_CAPABILITY_MATRIX.md) (with the deletion-closure checklist).

| Channel | Risk | Required behavior |
|---|---|---|
| OS/cloud device backup (iOS iCloud/device backup, Android Auto Backup/device transfer, desktop backup tools) | Copies keys and history outside OM's recovery design | Exclude `Secret`, `Persistent`, `ViewOnce`, `DerivedCache` from OS backup where the platform allows; use OS file-protection classes that keep data sealed while the device is locked, where available. OM backup is only the explicit encrypted RecoveryPolicy/BackupStore path (R-041). |
| App-switcher snapshots / screenshots / screen recording | Plaintext frames saved by OS | Secure-surface flag for ViewOnce always, and for all conversations under Hardened. Recommended: blank app-switcher snapshot, screenshots allowed. Platform-dependent; PWA cannot guarantee it and says so. |
| Keyboard/IME learning and cloud keyboards | Typed text retained or uploaded by third-party keyboards | Request no-personalized-learning/incognito input for composer fields where the platform supports it (Hardened default; Recommended offers it). Warn once when a known cloud keyboard is active, where detectable. |
| Browser/OS cloud spellcheck and "enhanced" writing tools | Composer text sent to a vendor service | PWA composer sets `spellcheck`/writing-suggestion attributes per NotificationPolicy-like ComposerPolicy. Hardened disables them. Document that OS-level writing tools may override page hints. |
| Browser extensions with site access | Extensions can read the decrypted PWA DOM | PWA shows a one-time, plain warning that browser extensions with access to all sites can read OM messages. High-assurance guidance points to a dedicated browser profile or a native client. |
| OS notification history / wearables / car displays | Previews persist outside OM | NotificationPolicy governs preview content. Deletion closure clears OM-posted notifications where the platform API allows. |
| Clipboard | Sensitive copies persist | Existing rule: no automatic sensitive clipboard writes. Mark OM clipboard writes sensitive where the platform supports it. |
| Share sheets / "open in" | Plaintext handed to other apps | Explicit user action only; ViewOnce cannot be shared. |
| Accessibility services | Can read screen content | Never blocked (accessibility is release-blocking), but threat model states it; Android's accessibility abuse by malware is covered by the compromised-endpoint class. |

## 5. Shared devices

Family tablets and shared computers are common. OM assumes one OS user equals one trust domain. On a shared device:

- use an app lock and per-profile lock;
- the PWA should run in a separate browser profile per person;
- the product says plainly that a shared OS account is not a private place.

## 6. Hosting-account compromise (PWA)

See [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §5 (R-164). For PWA users, the hosting account that deploys the ClientHost is part of the endpoint.

## 7. Verification (V-021)

- **Pre-engineering:** a per-platform capability matrix for every control in §4, built from platform documentation and non-production probes. Tests for the device-link notice in the AccountControlState model (a link without a notice on other active devices is a failing trace). A deletion-closure checklist covering OS notification and snapshot surfaces.
- **Pre-release:** a forensic-style audit on each shipped platform: backup exclusion, snapshot suppression, and plaintext residue after quick lock and after disappearing expiry.
