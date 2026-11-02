# OM UI/UX brief for agents

**Read this before you touch any screen, flow or animation.** It is a short digest of decisions that are already made;
it does not replace them. When this brief and a source below disagree, the source wins and this brief is wrong: fix it.

Precedence (from [`README.md`](README.md)): constitution / accepted ADRs / machine truth, then `docs/architecture/`
(UX: [`UI_UX_SYSTEM.md`](../architecture/ux/UI_UX_SYSTEM.md), [`RECONCILED_INTERACTION_MODEL.md`](../architecture/ux/RECONCILED_INTERACTION_MODEL.md),
[`MOTION_AND_PERFORMANCE.md`](../architecture/ux/MOTION_AND_PERFORMANCE.md)), then `docs/ux-design/`, then prototypes.
Journeys: [`SCREEN_MAP_AND_JOURNEYS.md`](SCREEN_MAP_AND_JOURNEYS.md). First run: [`specs/DISCOVERY_AND_ONBOARDING.md`](specs/DISCOVERY_AND_ONBOARDING.md)
and [`specs/INVITE_AND_EMPTY_STATE.md`](specs/INVITE_AND_EMPTY_STATE.md). Words: [`specs/CONTENT_AND_MICROCOPY_VOICE.md`](specs/CONTENT_AND_MICROCOPY_VOICE.md).

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
- **First-run page** (`OnboardingFrame`): brand + "Step n of m" on top, one card below. On a phone the card is plain and top-aligned;
  from 40 rem wide it is a raised, centred card. Safe-area insets respected. Nothing scrolls sideways, down to 320 px.
- **Empty Chats is the invite screen** ("Invite someone to chat": share link, show QR, scan QR, paste a link). It recedes once conversations exist. No tutorial carousel, ever.

## 4. First run: the budget

Landing "Get started" → **Protect my chats** (one tap; the device's own prompt follows) → **Invite someone to chat** (ready; skip allowed).
Three taps, no account form, no safety-code step (ADR-0111: recovery is on by default; the safety code is offered later from the shell).
Power options (own password) sit behind "More options". Rules:

- Say what the device is about to do *before* its prompt appears, and never leave a dead button: the waiting state names the
  prompt, and after 6 s offers "Skip the prompt" (same call falls back to the key stored in this browser; the weaker protection is shown afterwards, never hidden).
- Every ceremony starts from a click (user gesture); nothing retries by itself.
- A step moves focus to its heading, announces busy states in a live region, and never loses typed input on error.
- Weaker-than-expected protection (synced passkey, device key) is always visible as a note; the long explanation is a disclosure.
- Home facts (who can see what) are disclosed at the decision point, in plain words, behind one disclosure; no provider names (a copy test enforces it).

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

## 7. Words

Plain, warm, short; "protect my chats", "safety code", "password", "Home" glossed once. No crypto or infrastructure vocabulary for normal users, no vendor names.
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
`apps/web/src/core/{RealApp,Shell}.tsx` (composition), `apps/web/src/landing`, `apps/web/src/copy/en.ts`;
`packages/om-web-platform/src/vault` (passkey and device key), `packages/om-design` (tokens, themes).
