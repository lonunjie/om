# OM UI/UX brief for agents

**Read this before you touch any screen, flow or animation.** It is a short digest of decisions that are already made;
it does not replace them. When this brief and a source below disagree, the source wins and this brief is wrong: fix it.

Precedence (from [`README.md`](README.md)): constitution / accepted ADRs / machine truth, then `docs/architecture/`
(UX: [`UI_UX_SYSTEM.md`](../architecture/ux/UI_UX_SYSTEM.md), [`RECONCILED_INTERACTION_MODEL.md`](../architecture/ux/RECONCILED_INTERACTION_MODEL.md),
[`MOTION_AND_PERFORMANCE.md`](../architecture/ux/MOTION_AND_PERFORMANCE.md)), then `docs/ux-design/`, then prototypes.
Journeys: [`SCREEN_MAP_AND_JOURNEYS.md`](SCREEN_MAP_AND_JOURNEYS.md). First run: [`specs/DISCOVERY_AND_ONBOARDING.md`](specs/DISCOVERY_AND_ONBOARDING.md)
and [`specs/INVITE_AND_EMPTY_STATE.md`](specs/INVITE_AND_EMPTY_STATE.md). Words: [`specs/CONTENT_AND_MICROCOPY_VOICE.md`](specs/CONTENT_AND_MICROCOPY_VOICE.md).

> **Operator rulings (2026-11-04) settle the open questions of that review** (section 6 of [`docs/reports/2026-11-03-ui-conformance-review.md`](../reports/2026-11-03-ui-conformance-review.md)): the audited design wins. The Omnibar and new-conversation action exist; the audited OM Calm palette (violet focus) is canonical; the outgoing bubble is a tint; red means failure or destructive, green success or verified; chat rows carry identity, preview, time, unread and draft; no one-item rail; invite choices are Share invite link, Scan QR code, Show my QR; the delivery service page states its facts and keeps the raw address behind a disclosure. The golden-section card, brand header, red-dot and card styling, slide-in panes and "round everything" are removed. Kept as recorded deviations (functional or security value): name at setup, web-link invites, Settings search, Lock, Backup, Help, per-device idle lock. Where the paragraphs below still describe the removed look, this note wins.
>
> **Provenance warning (2026-11-03).** Parts of this brief, notably 4a and 6a (web 0.0.4 to 0.0.7, 2026-11-02 and 2026-11-03: web-link invites,
> "round everything", golden-section card, back arrow on the heading line, settings search, slide-in panes), were added by agents while building and are
> **not backed by an audited UX document unless a source is cited**. Some record the operator's in-session direction; the brief does not say which. They are working
> notes, not design. Where they disagree with `docs/architecture/ux/` or `docs/ux-design/specs/`, the audited document wins and the code should follow it. A
> conformance review of the shipped UI against those documents is open (see [`docs/reports/2026-11-03-return-to-the-audited-design.md`](../reports/2026-11-03-return-to-the-audited-design.md), section 5). Do not add a
> new decision here without a source: an audited document section, or the operator's words with the date.

## 1. The test every screen must pass

> Bulletproof underneath; boringly easy on top. Could a person who cannot configure Wi-Fi use this correctly and securely?

If not, the complexity belongs in automatic policy or behind an "advanced" disclosure. Protect people, not metrics: no growth
pressure, no nagging for contacts, no address-book upload, an empty network is not an error.

## 2. One language, every device

The web app (PWA), iOS, Android and desktop share one `om-ui`. Same objects, same words, same trust meaning, same motion roles;
only OS-owned surfaces (permission prompts, biometric sheets, share sheet, camera) differ. A phone browser and a desktop
window show the **same single column / same information hierarchy**: more room reveals more context, never a different layout
logic. Product screens live in `packages/om-ui`; the web app (`apps/web`) only composes them and supplies copy and platform
adapters. Never fork a flow per platform.

Layout states are Compact, Split, Expanded, Multi-pane, chosen by window size, not by device name.

## 3. Pages and shell

- **Chats is the only root.** No one-item tab bar. Other destinations (Calls, People, Spaces, Files) are opt-in presentation
  shortcuts; the Omnibar, deep links and the Action Registry reach everything.
- **Settings** is reached from the profile control, not a tab.
- **Omnibar** is find-or-do. On compact screens it shares the composer's lower zone; the composer and Omnibar never both show a text field.
- **Changing place = navigation; acting within a place = sheet or popover.** Back restores the previous place, focus and draft.
- **Every place has a web address** (`apps/web/src/route/place.ts`, History API; the host and the service worker already answer any path with the shell):
  `/` chats, `/chats#<id>` one chat, `/settings`, `/settings/devices`, `/settings/recovery`, `/settings/delivery`, `/safety-code`. Changing place pushes an entry, so the phone's Back button,
  the browser's Back/Forward and a reload all work; an unknown address becomes `/` (never an error page) and the bar always shows the canonical address. A chat's id is in the fragment, never the path,
  so no host log learns which chat is open. Tab titles name the place, never a chat. A new screen gets a place in `place.ts` and a row in `place.test.ts`; do not keep navigation in component state.
  On a phone a page that replaces the list has a Back control; beside the list (900 px and up) it does not.
- **First-run page** (`OnboardingFrame`): brand + "Step n of m" on top, one card below. On a phone the card is plain and top-aligned;
  from 40 rem wide it is a raised, centred card. Safe-area insets respected. Nothing scrolls sideways, down to 320 px.
- **Empty Chats is the invite screen** ("Invite someone to chat": share link, show QR, scan QR, paste a link). It recedes once conversations exist. No tutorial carousel, ever.

## 4. First run: the budget

Landing "Get started" → **Set up my own delivery service** (only while the install has none, ADR-0186) → **Protect my chats** (one tap; the device's own prompt follows) → **Invite someone to chat** (ready; skip allowed).
Three taps, no account form, no safety-code step (ADR-0111: recovery is on by default; the safety code is offered later from the shell).
Power options (own password) sit behind "More options". The setup page is not counted in "Step n of m" and has no skip: a signed build carries no delivery service, so nobody starts without one of their own (an invited person too).
Page one (`RelaySetup` in `packages/om-ui/src/onboarding`, copy `onboardingCopy.relay`; ADR-0186, operator direction 2026-11-04): heading **"OM is different"**, then two lines: "You own your data, and you own your delivery service." and "So even when we are not around, your messages are yours." Then "Choose how to set up your delivery service. Come back here once it is set up." Two groups:
- **Easy**: the button **Set up my own, free**, which says it uses two free accounts, Cloudflare and GitHub or GitLab (Cloudflare clones its template into one).
- **Advanced**: own server options, coming later, plain text and never controls.
Choosing the Easy button shows a **setup value** (the public hash of an admission secret made on this device, `@om/provider-http` `pendingAdmissionHash`;
shown again unchanged after a reload) with a copy button, one sentence steering the person to a passkey or security key on that account (R-164, [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) 5), and a link that opens the one-click setup in a new tab (`VITE_OM_RELAY_DEPLOY_URL`, https only, set in `apps/web/.env.production` to the `lonunjie/om-relay` template; unset, the page says that way is not ready yet). The person comes back and types the address; it is probed (`/.well-known/om-provider-capabilities`),
and only an address that answers as a delivery service **and advertises `om.admission.v1`** is stored (one that does not was deployed without the setup value and is open to anyone: refused with a plain explanation), together with the matching secret (`adoptAdmission`, bound to that address only), after which first run continues. R2 is never part of this page. Rules:

- Say what the device is about to do *before* its prompt appears, and never leave a dead button: the waiting state names the
  prompt, and after 6 s offers "Skip the prompt" (same call falls back to the key stored in this browser; the weaker protection is shown afterwards, never hidden).
- Every ceremony starts from a click (user gesture); nothing retries by itself.
- A step moves focus to its heading, announces busy states in a live region, and never loses typed input on error.
- Weaker-than-expected protection (synced passkey, device key) is always visible as a note; the long explanation is a disclosure.
- Delivery-service facts (who can see what) are disclosed at the decision point, in plain words, behind one disclosure; no provider names (a copy test enforces it; the one named exception is the setup choice below).
- **Say "delivery service", never "Home" or "relay" to a person** (2026-11-02: "Home" read as a street address). It is the small online service that carries your messages: it holds each one, still locked, until the
  other person's device picks it up; it cannot read it, and chats and keys stay on the devices. Settings calls it "Message delivery". Code, ADRs and docs keep "Home" as the technical name.

## 4a. Invitations are web links (2026-11-02)

What a person sends, shows as a QR code or copies is `https://<this app's own address>/#invite/<token>`, never Core's `om:invite/<token>` (a phone camera
or a chat app cannot open that). The token is in the fragment, so no server sees it. The app reads it once on load, removes it from the address bar and holds it
for that tab only (`apps/web/src/invite/pendingInvite.ts`), then asks **"Accept this invite?"** (`AcceptInvite`): in first-run's last step for a new person, as a card
above the chat list for someone already set up. Nothing is sent before Accept. Pasting or scanning accepts either form, or a whole message around a link
(`parseInviteLink`); Core is always given its own canonical form. After Accept, Core adds the chat at once and the shell opens it: it says it is waiting for the
other person, who sees a "Message request" that says what the name means. Setup asks for **your name** (required, the other person sees it); without it every chat
partner would read "Me". Share = share sheet, else copy; "Copy link" is always there. A slow Home never says "nothing was sent": the card keeps watching the same
operation and finishes by itself.

## 5. Adding a device (multi-device)

Sources: [`MULTI_DEVICE_SYNC.md`](../architecture/crypto/MULTI_DEVICE_SYNC.md), [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md),
[`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md). **There is no master phone**: every device is a first-class endpoint.

- **New device, first screen:** two equal choices, "Start new" and "I already use OM" (the second is shown only when this install has a delivery service to carry the ceremony; never show a flow that ends in `Rejected(unsupported)`).
- **Direction (deviation D24, [`relay-status`](../reports/2026-10-28-relay-status.md)):** the docs above assumed the new device shows the code; as built it is the other way round. The **existing** device (Settings > Devices > Add a device, `AddDevice`) shows the QR, the `om:link/...` text and a 5-minute expiry; the **new** device (first run "I already use OM", `JoinWithLink`) scans or types it. Both then show the same six-digit code.
- **Device-link QR is not an invitation QR:** dashed accent frame, a text badge ("Adds a device to your account") and a "not an invitation" warning, so the two are told apart by more than colour. The QR plate itself stays light.
- **New device:** protects itself first (no name asked, no profile created), then the link step, then waits. It never approves itself; only the existing device's decision (reported by Core) moves it on. A refused or expired request offers "Start over" (the host makes a fresh empty profile).
- **Existing device:** compare the code, then an explicit **Approve / Reject** (`LinkReview`, Core confirmation nonce). Linking is never silent: every device gets a non-suppressible notice strip (`DeviceNotices`, no dismiss; Core never retires a notice yet), and Settings > Devices lists each device with enrolment time and revoke.
- Adding a messaging device is light; promoting a control device is constitutional and needs its own friction.
- Suspend, pending, contested/Safety Freeze and permanent removal are four different words and four different screens. Never promise to erase data a removed device already holds.
- Account-private sync (drafts, mute, archive, theme) is a convenience: say plainly when another device cannot share it.
- Status (2026-11-04): wired end to end in the UI and Core; the full ceremony does **not** yet run against the reference Home, which rejects ceremony messages at deposit (see [`docs/reports/2026-11-02-multi-device-plan.md`](../reports/2026-11-02-multi-device-plan.md), phase 6).

## 6. Motion

Movement explains; it never decorates and never delays an action. Use the tokens, never raw milliseconds
(`packages/om-design/src/css/tokens.css`):

| Token | Now | Use |
|---|---:|---|
| `--om-motion-instant` | 80 ms | press, selection |
| `--om-motion-quick` | 150 ms | menu, small sheet |
| `--om-motion-standard` | 220 ms | step / pane / page arrival |
| `--om-motion-slow` | 350 ms | rare large spatial change |

Easing: `--om-ease-enter` for arrivals, `--om-ease-exit` for departures, `--om-ease-move` for moves. Reduced motion collapses the
durations to ~0 and keeps state feedback (focus, status text, progress). Every animation answers "what does this explain?": a new
step rising in = "this is the next one"; a turning ring = "still working". Gestures track the finger and are interruptible.
Expressive delight (OhMessenger) is separate, optional and owned by the recipient. Decoration degrades before typing, focus or action response.

## 6a. Chat and shell look (2026-11-02)

Product screens share the same few rules, all from tokens (`om-ui` conversations.css, shell.css; app layout in `apps/web/src/core/shell.css`; the production bundle loads no `dev.css`, so nothing the app shows may be styled there):
- **Chat:** a sticky header (Back on a phone, initial avatar, name); bubbles with the tail on the last of a run, your messages in the accent colour, theirs raised; the time shows once per run (an unsettled or refused message always says so); a new
  message rises in (`om-rise`), the view follows it if it is yours or you were reading the end; the composer is one pill that grows with the text, Enter sends with a keyboard (a touch screen keeps Enter as a new line), the round send button is an icon with a name.
- **Lists and menus:** edge-to-edge rows with hairlines, a leading bar plus background for the selected row (never colour alone), press feedback (`scale(.97)`, instant) and hover only where hover exists.
- **Cards** (`om-card`) hold one request or reminder each; banners are calm (`om-notice-bar`) and sit in the flow, a test build's banner (`om-dev-banner`) is loud on purpose.
- **Disclosures** (`details/summary`, or a quiet button with `aria-expanded`): accent label on the left, chevron on the right that turns when open, so nothing that opens looks like loose text. A quiet button is accent-coloured, never grey text.
- **Notes about weaker protection** are a calm info panel (`om-onb__notice`), not a heavy outlined box. A result note ("Link copied") sits directly under the buttons that caused it, never below a QR code.
- **Message times:** today shows only the clock ("10:55 PM"); older adds the day, older than this year adds the year (`sentAt` in `copy/en.ts`).
- **Reload / return:** a key kept in this browser opens by itself (`UnlockVault autoUnlock`); the person only sees the branded "Unlock" step when a password or a passkey prompt is needed. The unlock step uses `OnboardingFrame` like every first-run page.
- **Unbuilt flows are not offered:** "I already use OM" appears only on a first run that can reach a delivery service; "Add a device" only on an active control device whose control state is normal (otherwise a plain sentence says which device to use).
- **Radius is a theme value (2026-11-04):** controls take their corners from the radius tokens, not a pill. There is no press circle; press feedback is the instant state change.
- **Wayfinding:** a page opened from a menu has a round arrow, no words, on the heading's line (`SubPage` in `Shell.tsx`, like X's settings; phones only). A wide screen has no rail while Chats is the only destination: the top bar holds the Omnibar trigger and the identity control, and the identity control toggles Settings. **Chats is a place, not "back"**: it goes to `/` and never walks history through the Settings pages that were open. An empty pane is a quiet picture (cog, chat bubble), never a sentence ("Choose something to change" was filler and is gone).
- **Who you are, not "Settings":** the profile control shows the person's initial and name; Settings opens from it. The unsaved safety code is a plain row in Settings, with no dot on the control and no card (the control's accessible name still says it needs attention).
- **Settings search:** type anywhere on the Settings list to search (a letter focuses the field), Enter opens the first match; rows carry hidden keywords (`settingsKeywords`). Rows lead with an icon in a circle; the current row's bar is on the right, the side the page opens from.
- **Settings > Lock** (`/settings/lock`, `LockScreen`, services in `lockServices.ts`): says how this device opens OM, and changes it. A key kept in this browser is "Not locked on this device" in red with "Anyone who can use this browser can read your chats." Actions: add / change / remove a password (`DeviceVault.changePassphrase`: same profile key, same device key or passkey, only the wrapper is rebuilt; a wrong current password changes nothing), use the device's fingerprint/face/code (`rotateWrapping` "auto"; a browser with no passkeys says nothing changed), and **Stop locking** (red panel: says anyone who can use or copy this browser can read the chats, asks for the password if one is set, button "Stop locking" / "Keep locking"). "Lock when I am away" (only when OM closes, 1, 5, 15 minutes, 1 hour) is a per-device preference in this browser (`lockPrefs.ts`, `useIdleLock`: compares clock times, so a sleeping tab still locks on return); "Lock now" closes the open identities that have something to unlock with (a key kept in this browser has nothing to ask for, so it is never locked: it would only flicker). Without a lock the time choices are replaced by one sentence saying to add a lock first.
- **Unlock = "Welcome back!"**: a password vault shows its field focused at once; a passkey vault shows one button ("Open my chats", a passkey prompt needs a click); a device-key vault opens by itself.
- **Adding an identity** opens the first-run page with a round Back arrow (returns to Settings) and no second password choice: how the device locks is one choice for the whole device, in Settings > Lock.
- **First-run card position:** a plain centred card with no brand header or step dots (the golden-section placement was removed 2026-11-04).
- **Confirmations are pictures:** "Link copied" is a green tick in a circle that fades after ~2.6 s (the words stay for screen readers); "Sharing was cancelled" is not said at all (cancelling is not an error). Icons only where they help scanning: share, link, QR, scan, settings rows, safety code.
- **Button width:** the one dominant action of a first-run card is full width (a thumb target, nothing competes); everywhere else buttons are as wide as their label.
- Phone: Back follows the finger (edge swipe, `useEdgeSwipeBack`: commit past 35% or on a flick, instant under Reduced Motion), and a Back control and the browser's Back always do the same. Acting within a place opens a `Sheet`; changing place navigates. The **Omnibar** (tap, or Cmd/Ctrl+K) finds a chat or setting and does an action, locally and deterministically (no message search or learning yet); **New conversation** opens the invite choices. No dead controls.
- **Chat rows (L1):** name and time, then the latest message ("You:" when you wrote it), an unsettled-delivery marker and an unread count; an unsent draft ("Draft:") replaces the preview. Previews come from `GetConversationView` in a TS hook (Core's list has none). Core has no read state, so unread counts messages after a local, encrypted last-seen marker (`ui/seen/`), baselined the first time a chat is seen.
- **Delivery service page:** the facts (service level, what it can see, moving it, where OM runs from) come from `GetHomeSummary` and show on the page; the raw address is behind "Show address".
- **More than one delivery service (2026-11-04, operator ruling; MULTI_RELAY_AND_DELIVERY_RESILIENCE 1, D22):** under **Advanced** on that page (`MoreServices`, copy `homeCopy.advanced`): the person's services with a role (Main, Backup, Moving away) and how each is doing, **Add a backup delivery service** (the first-run setup again: a second setup value, the typed address, the probe; a service that does not ask for the setup value is refused; it takes effect on **Connect now**), and **Move to a different delivery service** (`retire_home` for every unlocked identity, then the service is recorded as leaving so it is collected from but never offered again). Core refuses to move a chat that has no other route; the page says how many chats stay and offers to try again. Leaving the main service says that invitations nobody has accepted yet stop working (they live at the active service). Config: `homes-v1` in the existing IndexedDB config, with the old `provider-url` kept pointing at the main service (`apps/web/src/core/providerConfig.ts`). While more than one service exists the single-service "use a different" and "stop using" controls are hidden. Vendor named only in the reused `relay.cloudflare` group.
- Borrowed from `~/dig/formflow` (its README "Motion" section): movement guides and never decorates, press is instant, hover lifts only on hover devices, "whisper" status tones.

## 7. Words

Plain, warm, short; "protect my chats", "safety code", "password", "delivery service" glossed once. No crypto or infrastructure vocabulary for normal users, no vendor names, **except** the first-run delivery-service setup, which names Cloudflare (ADR-0186 decision 2: "they better know it's Cloudflare"). That exception is the copy group `onboardingCopy.relay.cloudflare` only; `apps/web/src/copy/en.test.ts` allows provider names in that path and nowhere else.
All strings come from copy objects (`apps/web/src/copy/en.ts`, typed by `packages/om-ui/src/*/copy.ts`), none inline in components.
State is never conveyed by colour, motion, sound or haptics alone.

## 8. Accessibility (release-blocking)

44 px touch targets for primary controls; visible focus ring (tokens `--om-color-focus-*`); reflow to 320 px; large text; Reduced Motion;
keyboard path for every gesture; headings are real headings; live regions for async status; axe clean in component tests.

## 9. Definition of done for a UI change

1. Uses `om-ui` primitives and design tokens only (no hard-coded colours, sizes or milliseconds); light and dark both checked.
2. Copy in the copy objects; no jargon; no vendor names.
3. Looked at, not just tested: screenshots at **390×844** and **1280×800** (real Chromium, `playwright@1.63.0` from the repo cache; never install another browser set), plus the failure and slow states.
4. Component test with axe; the flow's unhappy paths (denied permission, slow network, error, retry) have tests.
5. `pnpm typecheck && pnpm lint && pnpm test`, and `make check` for docs. Anything touching the vault or onboarding also runs `make web-e2e`.
6. This brief updated if you made or changed a decision.

## 10. Code map

`packages/om-ui/src/onboarding` (first run), `devices` (link, suspend, remove), `recovery`, `home`, `conversations`, `shell`;
`apps/web/src/core/{RealApp,Shell}.tsx` (composition), `apps/web/src/route` (places and addresses), `apps/web/src/landing`, `apps/web/src/copy/en.ts`;
`packages/om-web-platform/src/vault` (passkey and device key), `packages/om-design` (tokens, themes).
