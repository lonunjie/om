# OM UI/UX brief for agents

**Read this before you touch any screen, flow or animation.** It is a short digest of decisions that are already made;
it does not replace them. When this brief and a source below disagree, the source wins and this brief is wrong: fix it.

Precedence (from [`README.md`](README.md)): constitution / accepted ADRs / machine truth, then `docs/architecture/`
(UX: [`UI_UX_SYSTEM.md`](../architecture/ux/UI_UX_SYSTEM.md), [`RECONCILED_INTERACTION_MODEL.md`](../architecture/ux/RECONCILED_INTERACTION_MODEL.md),
[`MOTION_AND_PERFORMANCE.md`](../architecture/ux/MOTION_AND_PERFORMANCE.md)), then `docs/ux-design/`, then prototypes.
Journeys: [`SCREEN_MAP_AND_JOURNEYS.md`](SCREEN_MAP_AND_JOURNEYS.md). First run: [`specs/DISCOVERY_AND_ONBOARDING.md`](specs/DISCOVERY_AND_ONBOARDING.md)
and [`specs/INVITE_AND_EMPTY_STATE.md`](specs/INVITE_AND_EMPTY_STATE.md). Words: [`specs/CONTENT_AND_MICROCOPY_VOICE.md`](specs/CONTENT_AND_MICROCOPY_VOICE.md).

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
Page one (`RelaySetup` in `packages/om-ui/src/onboarding`, copy `onboardingCopy.relay`): short sentences on why OM is different ("You own your data, not us."), "Pick how you would like to set up your first delivery service. Come back here once this is set up.", then **Set up my own, free**,
which says it uses Cloudflare and that the one-click setup also needs a GitHub or GitLab account (Cloudflare clones its template into one). The other ways (container, install script, tunnel, open ports) are plain text under "Coming later", never controls. Choosing the first shows a **setup value** (the public hash of an admission secret made on this device, `@om/provider-http` `pendingAdmissionHash`;
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

## 5. Adding a device (multi-device) — design, not yet buildable

Sources: [`MULTI_DEVICE_SYNC.md`](../architecture/crypto/MULTI_DEVICE_SYNC.md), [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md),
[`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md). **There is no master phone**: every device is a first-class endpoint.

- **New device, first screen:** two equal choices, "Start new" and "I already use OM" (the second is shown only when Core can do it; never show a flow that ends in `Rejected(unsupported)`).
- **New device:** shows a QR and a link plus a short verification code (`LinkThisDevice`). It never approves itself.
- **Existing device:** scan or paste, compare the code, then an explicit **Approve / Reject** (`LinkDeviceFlow`, `LinkReview`). Linking is never silent: every other device gets a non-suppressible notice, and Settings > Devices lists each device with enrolment time.
- Adding a messaging device is light; promoting a control device is constitutional and needs its own friction.
- Suspend, pending, contested/Safety Freeze and permanent removal are four different words and four different screens. Never promise to erase data a removed device already holds.
- Account-private sync (drafts, mute, archive, theme) is a convenience: say plainly when another device cannot share it.
- Status: the UI pieces exist in `packages/om-ui/src/devices`; **Core does not implement the ceremony yet** (see [`docs/reports/2026-11-02-multi-device-plan.md`](../reports/2026-11-02-multi-device-plan.md)).

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
- **Unbuilt flows are not offered:** Settings > Devices shows a plain "not ready yet" note instead of "Link a new device" until Core can link (`DevicesScreen linking="later"`; flip it in `Shell.tsx` when phase 4 of the multi-device plan lands).
- **Round everything (2026-11-03):** buttons and the search field have round ends (`--om-radius-pill`), like the OM mark. A press grows a soft circle from where the finger landed (`pressOrigin`, `.om-button::after`; centre for a key press; reduced motion collapses it). Hover and press highlight in circles, never boxes.
- **Wayfinding:** a page opened from a menu has a round arrow, no words, on the heading's line (`SubPage` in `Shell.tsx`, like X's settings; phones only). Navigation buttons look like buttons (the Chats rail button is an outlined pill with an icon, filled when current). **Chats is a place, not "back"**: it goes to `/` and never walks history through the Settings pages that were open. An empty pane is a quiet picture (cog, chat bubble), never a sentence ("Choose something to change" was filler and is gone).
- **Who you are, not "Settings":** the profile control shows the person's initial and name; Settings opens from it. A red dot on it, and a card at the top of Settings, say the safety code is not saved yet (it no longer sits in the chat list).
- **Settings search:** type anywhere on the Settings list to search (a letter focuses the field), Enter opens the first match; rows carry hidden keywords (`settingsKeywords`). Rows lead with an icon in a circle; the current row's bar is on the right, the side the page opens from.
- **Settings > Lock** (`/settings/lock`, `LockScreen`, services in `lockServices.ts`): says how this device opens OM, and changes it. A key kept in this browser is "Not locked on this device" in red with "Anyone who can use this browser can read your chats." Actions: add / change / remove a password (`DeviceVault.changePassphrase`: same profile key, same device key or passkey, only the wrapper is rebuilt; a wrong current password changes nothing), use the device's fingerprint/face/code (`rotateWrapping` "auto"; a browser with no passkeys says nothing changed), and **Stop locking** (red panel: says anyone who can use or copy this browser can read the chats, asks for the password if one is set, button "Stop locking" / "Keep locking"). "Lock when I am away" (only when OM closes, 1, 5, 15 minutes, 1 hour) is a per-device preference in this browser (`lockPrefs.ts`, `useIdleLock`: compares clock times, so a sleeping tab still locks on return); "Lock now" closes the open identities that have something to unlock with (a key kept in this browser has nothing to ask for, so it is never locked: it would only flicker). Without a lock the time choices are replaced by one sentence saying to add a lock first.
- **Unlock = "Welcome back!"**: a password vault shows its field focused at once; a passkey vault shows one button ("Open my chats", a passkey prompt needs a click); a device-key vault opens by itself.
- **Adding an identity** opens the first-run page with a round Back arrow (returns to Settings) and no second password choice: how the device locks is one choice for the whole device, in Settings > Lock.
- **First-run card position:** from 40 rem wide the card sits about 38% down the page (golden section, eye level), not at the geometric middle; a phone keeps it near the top, clear of the keyboard.
- **Confirmations are pictures:** "Link copied" is a green tick in a circle that fades after ~2.6 s (the words stay for screen readers); "Sharing was cancelled" is not said at all (cancelling is not an error). Icons only where they help scanning: share, link, QR, scan, settings rows, safety code.
- **Button width:** the one dominant action of a first-run card is full width (a thumb target, nothing competes); everywhere else buttons are as wide as their label.
- Phone: the pane that opens slides in from its side (`om-slide-in`), so Back reads as going back. No dead controls: an unbuilt control (the Omnibar) is not shown.
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
