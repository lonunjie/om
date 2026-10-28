# V-006 real-device checklist (pre-release)

- **Date:** 2026-10-28. **Gate:** V-006 `web_device_vault_feasibility`, `pre_release_criteria` (ADR-0172, ADR-0180). Pre-engineering already passed on 2026-10-01 from the probe page; this list covers what is still open.
- **Derived from:** the full V-006 criteria text in [`machine/architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl), [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) (sections 4-7 and the 2026-10-01 resolution), [`verification/prototypes/v006-web-vault/README.md`](../../verification/prototypes/v006-web-vault/README.md), the client copy in `apps/web/src/copy/en.ts`.
- **Who:** the operator, on their own devices. About 45 minutes per device. Nothing here needs repo access except Part B, which runs from the Mac.
- **Readiness context:** [`2026-10-28-beta-gate-readiness.md`](2026-10-28-beta-gate-readiness.md).

## 1. What this closes, and what it does not

| V-006 pre-release criterion | Closed by |
|---|---|
| Published support matrix with regression tests on each supported browser | Parts A-C on every run in the matrix (section 2); the agent turns the returned grid into the published matrix and adds per-engine tests |
| Argon2id passphrase layer: unlock time on a low-end Android and an older iPhone, calibrated parameters recorded, nested wrap read by a cross-vendor or human reviewer | Part D for timings; the review is a person, not this list |
| Camera QR scan, PRF passkey path and service worker verified on macOS Chrome/Safari/Firefox, iOS Safari and Android Chrome | Part A, checks C02, C05, C14 |

Also extended from the passed pre-engineering criteria: PRF on synced passkeys and `signalCurrentUserDetails` per browser, device-bound credential availability, new-origin migration, passkey-only recovery being distribution-bound. They were measured on the probe on macOS, iPhone and Android; **Windows Chrome/Edge was never tested**, and no physical security key was tried.

**Not covered by anything here:** independent review of the nested wrap; enrolling a fresh DeviceID at a *native* client from a recovery key (no native client exists); push delivery (PR-006; the Home has no push endpoint, so only capability is recorded).

## 2. The run matrix

Run every row you can. Rows marked REQUIRED are named by the V-006 pre-release criteria; Windows is "if the beta advertises Windows support" (otherwise the beta says Windows is untested).

| Run | Device and browser | Mode | Status |
|---|---|---|---|
| R1 | iPhone, Safari (iOS) | in a browser tab | REQUIRED |
| R2 | iPhone, Safari | installed (Share > Add to Home Screen) | REQUIRED: iOS gives an installed web app storage separate from the tab, and push works only there |
| R3 | Android phone, Chrome | in a browser tab | REQUIRED |
| R4 | Android phone, Chrome | installed (menu > Install app) | recommended |
| R5 | Mac, Chrome | tab (then installed if offered) | REQUIRED |
| R6 | Mac, Safari | tab (then File > Add to Dock) | REQUIRED |
| R7 | Mac, Firefox | tab | REQUIRED |
| R8 | Windows, Chrome | tab, then installed | if Windows is claimed |
| R9 | Windows, Edge | tab, then installed | if Windows is claimed |
| R10 | Oldest iPhone you can borrow | Part D only | REQUIRED for the Argon2id timing |
| R11 | Lowest-end Android you can borrow | Part D only | REQUIRED for the Argon2id timing |

Passkey provider matters as much as the browser: last time Bitwarden on both phones returned no PRF while the platform provider did. For R1-R4 run once with the default provider (iCloud Keychain / Google Password Manager) and, if installed, once with a third-party one. Record which provider the sheet named.

## 3. Before you start

1. **URL under test.** WebAuthn needs HTTPS and ties every passkey to the hostname, so a passkey made on a preview hostname does not exist at the production hostname (this is the "distribution-bound" property, not a bug). Use the preview URL from the cutover plan, `https://<hash>.om-lonunjie.pages.dev` ([`CUTOVER.md`](../../deploy/CUTOVER.md) step 3), for all runs, and repeat R1 and R3 as a short smoke at `https://om.lonunjie.org` once the cutover has happened.
2. **Bundle version.** Open `<URL>/bundle-manifest.json` in the device's browser and note the `version` field. Record it per run.
3. **Versions.** iPhone: Settings > General > About > iOS version; Safari follows the iOS version. Android: Settings > About phone > Android version; Chrome: menu > Settings > About Chrome. Mac/Windows: browser menu > About.
4. **Fresh state for every run.** A browser tab or installed app that already holds OM data shows "This browser already holds OM data" instead of onboarding. Use a private/incognito window for tab runs where the browser allows passkeys there, or clear site data for the hostname first (Safari iOS: Settings > Safari (Settings > Apps > Safari on newer iOS) > Advanced > Website Data; Chrome: site settings > Clear data).
5. **Test hygiene.** Use throwaway identities. Never type a real passphrase. When done, delete the passkeys named "OM" and "OM V-006 probe" from the passkey manager used and clear the site data. Do not paste real recovery keys anywhere; the one shown during onboarding is a test key.
6. **Optional but useful for desktop:** open DevTools before starting (Safari: Settings > Advanced > Show features for web developers).
7. **Phones with a Mac nearby (optional, for console checks).** iPhone: Settings > Safari (Settings > Apps > Safari on newer iOS) > Advanced > Web Inspector on, cable to the Mac, then Safari > Develop > the phone. Android: USB debugging on, Chrome on the Mac at `chrome://inspect`. Without these, take the answers from on-screen text and the probe report.

## 4. Part A: the real client, per run

Open the URL. Do the steps in order. Write each answer into the results grid (section 8). Quote on-screen text exactly.

**A1. Load (C01).** The landing page appears. Note the bundle version (section 3, step 2).

**A2. Service worker (C02, C03).**
- Desktop: DevTools > Application > Service Workers: status "activated and running", scope `/`. Or paste snippet S1 (section 9) in the console and read `serviceWorker`.
- Phone: after the first load, wait 10 seconds, switch to airplane mode, reload. Record whether the landing page loads offline.
- Turn airplane mode off.

**A3. Install (C04).** Use the install action for the run's mode (iPhone: Share > Add to Home Screen; Android/desktop Chromium: browser install prompt or the in-app "Install OM" button; Safari on Mac: File > Add to Dock). Record: was installing offered, what it was called, did the installed app open without browser chrome, did it show the landing or onboarding. For R2 and R4 do A4 onwards inside the installed app, not the tab.

**A4. Lock this device (C05-C08).** Tap Get started, then **Lock this device**. Record:
- every system prompt that appeared (passkey sheet text, which provider it named, fingerprint/face/PIN) and how many;
- any "Press the button again to continue" or "One more confirmation is needed..." screen (Safari can reject a prompt that starts right after another: "The document is not focused");
- the result heading **How your data is locked** and its two lines. Expected texts: "Your data is locked with a passkey from this device or your password manager." (PRF) or "Your data is locked with a key kept in this browser." (browser-held key), with the reason line ("Your passkey works for locking data." / "...does not support locking data, so a browser-held key is used." / "...did not return what is needed to lock data..." / "Creating a passkey did not work..." / "This browser has no passkey support...");
- whether the reduced-protection line appeared ("Your passkey is synced between your devices by its provider, so anyone who controls that account could unlock this data. Protection here is reduced.") or the browser-key limit ("...Anyone who can copy this browser profile can open your local data.");
- the "What this browser cannot do" list, verbatim (this records push, backup, clipboard and local-discovery limits);
- any error text ("This browser cannot protect your data safely enough for OM.", "The passkey prompt was cancelled or did not finish.", and so on) and a screenshot.

**A5. Recovery key and invite (C16 setup).** Continue: write down the test recovery key, type the groups asked for, reach "Add your first contact".

**A6. Non-extractable key (C10).** Desktop only: paste snippet S2 (section 9). Expected when the browser-held key was chosen: one `om-vault-...` database with `hasDeviceKeyRecord: true` and `extractable: false`. Expected when the passkey method was chosen: `hasDeviceKeyRecord: false`. Record the output. On phones this check comes from the probe (Part B, section 2 of the probe) instead.

**A7. Camera QR scan (C14).** Needs two devices. On the other device (call it the inviter) finish onboarding to "Add your first contact" and tap **Show as QR code**. On the device under test tap **Scan a QR code**. Record: the permission prompt text, whether the camera opened, whether the scan was accepted. Then repeat and **deny** the camera: expected "Camera access was not allowed. You can paste a link instead." Then paste the link instead and accept it. For R2/R4 this is inside the installed app.

**A8. Message smoke (C16).** After accepting, send a message from each side through the Home. Record sent, delivered, and any "queued" or "not connected" text. (This also exercises the service worker and camera in the real flow.)

**A9. Unlock after restart (C12).** Close the tab or installed app completely (swipe it away), reopen. Expected: the **Unlock OM** screen; unlock. Record prompts and any error. For one run per platform also restart the device and unlock again.

**A10. Storage (C13).** Desktop: paste S3 (section 9) and record the output; note that OM itself never calls `persist()`, so `persistedBefore: false` is the expected common result. Phone without console: record "not measured". A storage-eviction warning belongs in the notes if the platform shows one (WebKit caps script-written storage for tabs left unused for about seven days and does not apply that cap to installed web apps; record what you see).

**A11. Push capability (C15).** Desktop: snippet S1 gives `push`. Phone: the probe's capability list (Part B) or the "What this browser cannot do" list from A4 ("OM cannot alert you while it is closed." means no push). Record `PushManager`, `Notification.permission`, and for iPhone whether this was the installed app (push exists only there). **Do not enable notifications in the app**: there is no push server yet.

## 5. Part B: the probe, per run (new-origin migration, device-bound credential, Windows)

The probe is the V-006 prototype page. It measures what the client cannot show: whether the credential is synced, a device-bound credential, PRF output, and origin-independent recovery.

**On the Mac (two terminals).**

```sh
cd /Users/maj/dig/lonunjie/om/verification/prototypes/v006-web-vault && PORT=8806 python3 collector.py
```

It prints `token: <TOKEN>` and `open: /?t=<TOKEN>`. In a second terminal, an HTTPS tunnel for the phones and the Windows PC:

```sh
cloudflared tunnel --url http://127.0.0.1:8806
```

It prints `https://<random>.trycloudflare.com`. For sections 5 and 7 (two origins) start a second tunnel in a third terminal with the same command: you get a second hostname that serves the same page. Keep both running until all devices are done, then stop them with Ctrl-C (the tunnels expose the probe page to anyone with the URL; only writes need the token).

**On each device.** Open `https://<random>.trycloudflare.com/?t=<TOKEN>`. Press the buttons in page order and wait for each to finish:

1. Section 1 fills in by itself: note anything marked unsupported.
2. Section 2: **Run WebCrypto test**, reload the page, **Check after reload**.
3. Section 3: **Create vault test credential**, **Unlock vault with PRF**, **Create separate RecoveryRoot test credential**. (Safari: if a prompt is rejected as "not focused", wait two seconds and press again; record it.)
4. Section 4: type any test passphrase, **Layer passphrase over the vault key**.
5. Section 6: press every variant button (Platform, Platform non-discoverable, Cross-platform, the three hints) then **Device-bound credential: PRF at unlock**. Variants that need a security key or a phone time out after 30 seconds: that is a valid result; cancel prompts you do not want to answer. **On Windows and Mac, repeat Cross-platform with a physical FIDO2 security key plugged in if you own one** (never tried so far).
6. Section 7 migration. On origin A (first hostname): **A1. Create state, recovery key and bundle**, **Copy all**, then **A2. Simulate loss of this origin**. Open the second hostname (origin B) with `?t=<TOKEN>`: paste into **Paste all here**, **Split**, **B1. Enroll fresh device from recovery key**, **B2. Reopen new vault**.
7. Section 5 on origin B: paste the credential ID printed on origin A and press **Try credential from another origin**. Expected: it fails (passkey-only recovery is bound to its origin).
8. In the **label** box type the run id and platform, for example `r8-windows-chrome`, then **Send to collector**. The report lands on the Mac as `verification/prototypes/v006-web-vault/results/<label>--<host>.json`. If sending fails, use **Download report** or **Copy report** and send it by any private channel.

The probe page's own text mentions `localhost`; for phones and Windows use the tunnel hostnames instead, because WebAuthn needs HTTPS.

## 6. Part C: Windows specifics (R8, R9)

Do Parts A and B on Chrome and on Edge. Extra records:
- Windows version and build; whether Windows Hello is set up (PIN, face, fingerprint); the passkey sheet text and provider (Windows Hello, a phone via QR/hybrid, a security key).
- Whether PRF came back at create and at get; the probe's synced-or-device-bound flag (BE/BS); whether `signalCurrentUserDetails` exists.
- Whether Edge and Chrome behave the same; whether a Hello passkey survives a sign-out/sign-in.
- Installed-app behaviour (Chrome/Edge install icon in the address bar).

## 7. Part D: Argon2id unlock time (ADR-0180), R10 and R11

Default cost is m = 64 MiB, t = 3, p = 1; floor m = 19 MiB, t = 2. Goal: the numbers needed to decide whether to keep the default.

1. Open the URL on the device. Start a screen recording (so the time can be read from the video).
2. Onboarding: **Lock this device**; on the result screen, switch on **Also protect with a passphrase**, enter a test passphrase of at least 12 characters, confirm, continue. Note the slow-note text shown ("Opening OM will take a few extra seconds, and longer on an older phone...").
3. Close the tab or app completely. Reopen. On **Unlock OM** type the passphrase and tap **Unlock**; stop timing when the conversation list or settings appears. Record whether "Unlocking. This takes a few seconds." showed.
4. Repeat 3 times (fully close between). Record the median in seconds and the worst.
5. Record the device: model, RAM, OS and browser version, battery saver on or off, other apps open.
6. Also type a wrong passphrase once: expected "That passphrase is not right. Try again."

The operator decides the acceptable ceiling (the ADR sets no number); this list only collects the evidence. If the unlock exceeds your ceiling the agent proposes calibrated lower parameters above the stated floor.

## 8. Results grid and how to report back

Copy this table into the reply or into a file. One column per run you did. Use `Y`, `N`, `n/a` (not applicable), `not measured`, or the exact text. Add one line per run with device, OS, browser, mode, provider and bundle version.

| Check | What | R1 iPhone tab | R2 iPhone installed | R3 Android tab | R4 Android installed | R5 Mac Chrome | R6 Mac Safari | R7 Mac Firefox | R8 Win Chrome | R9 Win Edge |
|---|---|---|---|---|---|---|---|---|---|---|
| C01 | Loads over HTTPS; bundle version | | | | | | | | | |
| C02 | Service worker active, scope `/` | | | | | | | | | |
| C03 | Offline reload works | | | | | | | | | |
| C04 | Install offered; opens standalone | | | | | | | | | |
| C05 | Lock result: passkey (PRF) or browser key; reason line | | | | | | | | | |
| C06 | Reduced-protection line shown | | | | | | | | | |
| C07 | Prompts count; any "press again" / "not focused" / error text | | | | | | | | | |
| C08 | Passkey provider named | | | | | | | | | |
| C09 | Credential device-bound? (probe BE flag) | | | | | | | | | |
| C10 | Browser key non-extractable (S2, or probe) | | | | | | | | | |
| C11 | Passphrase layer sets up; unlocks; wrong passphrase message | | | | | | | | | |
| C12 | Unlock after close and reopen (and after restart) | | | | | | | | | |
| C13 | `persisted()` before / after `persist()` | | | | | | | | | |
| C14 | Camera QR: prompt, scan ok, deny path text | | | | | | | | | |
| C15 | Push: `PushManager`, `Notification.permission`, installed-only? | | | | | | | | | |
| C16 | Two-identity message smoke through the Home | | | | | | | | | |
| C17 | Probe report file name | | | | | | | | | |
| C18 | Probe: credential from origin A rejected at origin B | | | | | | | | | |
| C19 | Probe: recovery-key migration to origin B, new vault reopens | | | | | | | | | |
| C20 | Anything odd; screenshots | | | | | | | | | |

Argon2id timings (R10, R11, plus any run where Part D was done):

| Device | OS and browser | RAM | Run 1 s | Run 2 s | Run 3 s | Median s | Slow note shown | Battery saver |
|---|---|---|---|---|---|---|---|---|

**Reporting back.**
1. Fill the grid and the timing table.
2. Probe reports are already on the Mac under `/Users/maj/dig/lonunjie/om/verification/prototypes/v006-web-vault/results/` if you used **Send to collector**; for reports you downloaded, put them in that folder named `<platform>-<browser>.json`.
3. Tell the session: "V-006 results are in" and paste the grid. Screenshots are welcome only if they show no personal data. The agent then writes the support matrix, the ADR-0180 parameter record and the V-006 `progress` update, adds per-engine regression tests, and states any criterion still open.
4. Afterwards delete the test passkeys named "OM" and "OM V-006 probe" on every device and clear the site data.

## 9. Console snippets

Run in the DevTools console of the page under test (desktop browsers, or a phone via the remote inspector). Syntax checked with `node --check`; behaviour not run in a browser.

**S1: environment and capabilities (C02, C04, C13, C15).**

```js
(async () => {
  const pk = window.PublicKeyCredential;
  const reg = await navigator.serviceWorker?.getRegistration();
  const r = {
    ua: navigator.userAgent,
    origin: location.origin,
    secureContext: isSecureContext,
    standalone: matchMedia("(display-mode: standalone)").matches || navigator.standalone === true,
    webauthn: !!pk,
    platformAuthenticator: pk ? await pk.isUserVerifyingPlatformAuthenticatorAvailable() : null,
    clientCapabilities: pk?.getClientCapabilities ? await pk.getClientCapabilities() : "getClientCapabilities unsupported",
    serviceWorker: { supported: "serviceWorker" in navigator, controlled: !!navigator.serviceWorker?.controller, scope: reg?.scope ?? null, state: reg?.active?.state ?? null },
    storage: { persisted: await navigator.storage?.persisted?.(), estimate: await navigator.storage?.estimate?.() },
    push: { PushManager: "PushManager" in window, Notification: "Notification" in window ? Notification.permission : "unsupported" },
    camera: !!navigator.mediaDevices?.getUserMedia,
  };
  console.log(JSON.stringify(r, null, 2));
})();
```

**S2: is the browser-held vault key non-extractable (C10).** Database `om-vault-<profileId>`, store `deviceKey`, key `wrap` (`packages/om-web-platform/src/vault/idb.ts`).

```js
(async () => {
  const open = (name) => new Promise((res, rej) => { const q = indexedDB.open(name); q.onsuccess = () => res(q.result); q.onerror = () => rej(q.error); });
  const get = (db, store, key) => new Promise((res, rej) => { const q = db.transaction([store], "readonly").objectStore(store).get(key); q.onsuccess = () => res(q.result); q.onerror = () => rej(q.error); });
  const out = [];
  for (const { name } of await indexedDB.databases()) {
    if (!name || !name.startsWith("om-vault-")) continue;
    const db = await open(name);
    const rec = await get(db, "deviceKey", "wrap");
    db.close();
    out.push({
      database: name,
      hasDeviceKeyRecord: rec !== undefined,
      key: rec instanceof CryptoKey ? { extractable: rec.extractable, type: rec.type, algorithm: rec.algorithm.name, usages: rec.usages } : null,
    });
  }
  console.log(JSON.stringify(out, null, 2));
})();
```

**S3: storage persistence (C13).** May show a permission prompt (Firefox) or grant silently (Safari installed app).

```js
(async () => {
  const before = await navigator.storage.persisted();
  const granted = await navigator.storage.persist();
  console.log(JSON.stringify({ persistedBefore: before, persistRequestGranted: granted, persistedAfter: await navigator.storage.persisted(), estimate: await navigator.storage.estimate() }));
})();
```

## 10. What each outcome means (so you can judge a result on the spot)

- **Browser-held key chosen** is a valid, designed outcome, not a failure (DEVICE_VAULT 2026-10-01 resolution, point 1): wrapper selection is tested, never assumed, and capability flags are hints only.
- **Reduced-protection line with PRF** is the normal case on every first-release platform tested (point 3): the passkey is synced.
- **"Press the button again" or "not focused"** is the documented chained-prompt behaviour (point 2): record it, it is not a failure unless the flow cannot finish.
- **A credential from origin A working at origin B** would be a real failure (it should be rejected).
- **Migration failing** at origin B is a real failure; record the exact text.
- **No camera, no PRF, or no service worker** in a first-release row blocks that row from the published matrix: record it as unsupported with the on-screen text.
