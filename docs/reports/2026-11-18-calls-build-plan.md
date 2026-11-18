# Calls build plan (scope `calls`, authorized by ADR-0217)

Date: 2026-11-18. Status: slices 0 to 5 merged to main (2026-11-19, `65048c2`); slices 6 to 8 not started. Slice 7 e2e note: the calls spec is Chromium-only in the hermetic run (fake media flags, loopback ICE candidate); a Firefox/WebKit run needs manual devices. Earlier text: plan, no product code written. Authorization: [`ADR-0217`](../../decisions/ADR-0217-authorize-calls-engineering.md) (explicit operator instruction; V-007 pre-engineering waived for engineering, kept as a release obligation).

## Design sources read

Read in full: [`CALLS_AND_REALTIME_MEDIA.md`](../architecture/calls/CALLS_AND_REALTIME_MEDIA.md), [`LIVE_CALLS_UX.md`](../ux-design/specs/LIVE_CALLS_UX.md), ADR-0013, ADR-0132 (the call and A-035 rules), [`call-session.logical.schema.json`](../../spec/protocol/call-session.logical.schema.json), the V-007 gate and the `calls` entry of the scope contracts, ADR-0185 (decision and authority sections), ADR-0210 (decision and metadata tables).

Read in part (the passages that name calls): ADR-0128 rules 14 and 15, ADR-0156 F-06, [`NOTIFICATIONS_AND_PUSH.md`](../architecture/client/NOTIFICATIONS_AND_PUSH.md) section 7, [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md), [`PLATFORM_CAPABILITY_MATRIX.md`](../architecture/client/PLATFORM_CAPABILITY_MATRIX.md), [`IMPLEMENTATION_INTERFACES.md`](../architecture/providers/IMPLEMENTATION_INTERFACES.md) (`MediaProvider`, `RoutePolicy`), [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) (call rows and the AEAD lines), the V-007 section of the feasibility note, the V1 scope map, the component catalog and screen map call entries, [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md) (Realtime row).

**Not read, and every slice must read before it builds:** [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) and [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md) in full, [`HOME_WAKE_CHANNEL.md`](../architecture/providers/HOME_WAKE_CHANNEL.md), [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md) and the observer matrix, [`HAPTIC_AND_SOUND_SEMANTICS.md`](../ux-design/specs/HAPTIC_AND_SOUND_SEMANTICS.md) in full, `ACCESSIBILITY_AND_INPUT.md`, the discovery documents (the `allow_calls` flag in [`discovery.cddl`](../../spec/cddl/discovery.cddl)), [`home-api.md`](../../spec/protocol/home-api.md). Each slice below lists its mandatory `python3 tools/design_gate.py needs <paths>` run; read what it prints.

## 1. Where the design stands, in one page

- **Layering (fixed):** OM relationship or Room authentication, then encrypted call signalling, then call key agreement, then SFrame (RFC 9605) media encryption, then WebRTC transport over LAN, direct, TURN or SFU. The route never changes confidentiality; the UI must not show a route as a trust rating (`LIVE_CALLS_UX` section 4).
- **Signalling:** eight ephemeral kinds (`call.offer`, `answer`, `reject`, `end`, `ice_update`, `media_update`, `participant_update`, `key_epoch`), carried as encrypted control traffic, not durable chat events. No call-identity service.
- **Who may ring:** an offer from a principal without an accepted relationship is dropped (at most a message-request-class missed-call notice: no ringing, ICE or TURN). A Room ring needs `room.start_call`; joining needs membership only.
- **Keys:** one-to-one derives from the existing secure session with CallID and participant-bound labels. Group uses per-sender SFrame keys sent only to joined participant DeviceIDs over authenticated pairwise channels, rekeyed on every join, leave or revocation. The Room MLS epoch is never the media-key source.
- **Not frozen anywhere:** the wire. There is no [`call.cddl`](../../spec/cddl/call.cddl); the schema is logical only (ADR-0139). This is the first thing to build (slice 0).

## 2. Architecture of the build

| Layer | Where | Responsibility |
|---|---|---|
| Call state machine, signalling intents, key schedule | `crates/om-core` (new `calls` module) | the only authority: who may ring, which call exists, which keys are valid. Emits host effects; never touches WebRTC. |
| Wasm and TS binding | `om-wasm`, `packages/om-core-wasm`, `packages/om-platform-api` | typed intents (`StartCall`, `AcceptCall`, `DeclineCall`, `EndCall`, `SetCallMedia`) and a `CallView` in the snapshot |
| Media driver | `packages/om-web-platform` (new `CallTransport` family capability) | `RTCPeerConnection`, devices, `RTCRtpScriptTransform` worker, SFrame (WebCrypto), reports facts back to Core as host events |
| UI | `packages/om-ui` | `CallView`, `CallControlBar`, `CallStatus`, incoming surface, `CallTimelineNotice` (names from the component catalog) |
| Signalling carriage | existing Home queues, deposit, wake channel (ADR-0185), push (ADR-0210) | unchanged; calls add no endpoint |
| ICE servers | Home optional feature (slice 7) | STUN and TURN credentials, only if the person's Home advertises them |

The Core call machine is the same pattern as groups and direct transfer: transport-neutral, tested against a fake host first, driver second, UI third.

## 3. Open design questions and the answers this plan takes

"Delegation" means an ADR by delegation that records a construction the design leaves silent and does not amend audited text. **Operator yes needed** marks an item where the plan picks a default that is a product choice; the slice that depends on it does not start without the yes.

| # | Question | Answer taken (constitution-based) | Status |
|---|---|---|---|
| Q1 | The design says one-to-one media secrets derive from a "session exporter". The ratchet has no exporter. | The caller draws a fresh 256-bit call secret and sends it inside `call.offer`, which travels in the already authenticated ratchet session of each (caller device, callee device) pair. SFrame base keys are HKDF-SHA-256 of that secret with labels binding CallSessionID, both DeviceIDs and the direction. Forward secrecy and authentication are those of the session; no new primitive. | Delegation in slice 0; specialist review before release |
| Q2 | Which SFrame cipher suite? | One v1 suite, the RFC 9605 mandatory-to-implement AES-128-GCM with SHA-256 key derivation, run through WebCrypto in the browser. RFC 9605 names no other suite that is practical in browsers; the design names RFC 9605 as the framing, so this is its suite, not an OM invention. Registered as the `media_keying` profile entry, additively (ADR-0165). | Delegation in slice 0; review before release |
| Q3 | How does a group sender key reach a joined co-member with no 1:1 relationship (R-343)? | **Not decided here.** Candidate for the specialist review: per-recipient HPKE to each joined device's credentialed key from the Room's device directory, signed by the sender DeviceID, bound to CallID, ConversationID, epoch, generation and the sorted device set. Group calls do not start until that review exists. | Blocks group calls only |
| Q4 | Ring window and signalling lifetime. | 45 seconds. An offer carries `expires_at` and is deposited with that lifetime; an offer found after it is shown as a missed call, never rung. | **Ruled 2026-11-18: 45 s confirmed** (ADR-0218 rule 6) |
| Q5 | Several devices of the callee. | The offer goes to every callee device (as messages do). The first Answer wins; the caller then sends `call.end` with reason `answered_elsewhere` to the others, so they stop ringing. Reasons are part of the slice 0 registry. | Delegation |
| Q6 | A second call while one is active. | One call per device in v1. A new offer gets `call.reject` with reason `busy`; the person sees a missed-call line afterwards. | Delegation |
| Q7 | Signalling volume against the person's own Home quota. | Gather ICE candidates fully (up to 3 seconds) before sending the offer and the answer, no trickling; `call.ice_update` is used only for ICE restart. A normal call is three deposits (offer, answer, end). Calls are metered by the existing limits profile, and a refused deposit becomes "This call couldn't start", never a silent retry loop. | Delegation |
| Q8 | STUN and TURN: who supplies them without an operator? | No server address is compiled into the client (no default service; absolute rule 5). A Home advertises `MediaCapabilities` (STUN list, optional TURN credential endpoint) like any optional feature; the reference Home may point at Realtime inside **the person's own** Cloudflare account. A Home that advertises nothing gives host and peer-reflexive candidates only; a call that cannot connect says so plainly and offers Try again. TURN is slice 7 and needs a new optional Home API feature (`om.media.turn.v1`) and its ADR. Zero-cost policy: TURN only when advertised and within the Home's usage state. | **Ruled 2026-11-18:** only through the person's own Home as an optional feature, off unless advertised; no third-party STUN by default; without it calls work only where direct candidates connect and the UI says so plainly (ADR-0218 rule 7) |
| Q9 | Incoming call while the app is closed (PWA). | No ringing is possible: ADR-0210 fixes the push as empty and the worker as unable to open the vault or fetch, so the person sees the generic "New message" notification, opens the app, and the app drains and shows the incoming screen if the offer is still inside its window, otherwise a missed call. No call-specific push type is added (it would be a new relay-visible signal). `LIVE_CALLS_UX` section 9 permits this honest statement. Native shells (CallKit, Telecom) are out of this plan: ADR-0178 makes them thin hosts later. | Delegation |
| Q10 | What if the browser lacks encoded transforms (`RTCRtpScriptTransform`)? | Calls are unavailable and the UI says "This browser can't make protected calls". Never fall back to transport encryption alone (floors unchanged). | Delegation |
| Q11 | Incoming video: camera. | Per `LIVE_CALLS_UX` section 5: Answer video and Answer audio are separate actions, and the camera is never switched on by arrival. | In the audited text |
| Q12 | Call history. | A local `CallTimelineNotice` only (component catalog: view item, not a durable signalling event). Multi-device history sync is not built; if wanted it is a later ADR on the account-private sync rules. | Delegation |
| Q13 | Offer spam from a contact. | After three unanswered offers from one contact in ten minutes, further offers fold into one missed-call line with no ring. Blocked contacts never ring (existing block rule). | Delegation |
| Q14 | Per-contact "allow calls" setting. | Not designed for contacts (the `allow_calls` flag exists only in introductions). | **Ruled 2026-11-18:** yes; default allowed for established contacts, never for message requests or unknown accounts; a simple switch in contact details (ADR-0218 rule 8) |
| Q15 | Release gating of the feature. | A build flag and a Core capability, off in every release until the V-007 release evidence is recorded and `calls` is declared in a `public_releases` entry. `om-open-beta-1` stays as is. | Delegation |
| Q16 | Screen sharing, captions, call handoff between devices, recording. | Out of v1 (scope map; `LIVE_CALLS_UX` section 12; no server recording). The captions hook is an empty slot only. | In the audited text |

## 4. Platform limits (PWA first, ADR-0178)

- **Closed app:** no incoming-call ringing on iOS or Android PWAs (Q9). Where the person turns Alerts on (ADR-0210) they get a generic notification; the app copy says opening the app is what picks the call up (`LIVE_CALLS_UX` section 9 wording rule: a capability explanation, not a growth prompt).
- **iOS:** Web Push needs the installed Home Screen app. Backgrounding the app may suspend capture and the audio track; slice 8 measures it on a real device and the copy states what was measured, nothing more. No CallKit in the PWA.
- **Android Chrome:** WebRTC, encoded transforms and wake lock are available in current Chrome; background audio behaviour is measured in slice 8, not assumed. No Core-Telecom in the PWA.
- **Encoded transforms:** the feasibility note records `RTCRtpScriptTransform` as available in current browsers; interoperability is not proven by availability (V-007).
- **Permissions:** `Permissions-Policy` already allows `microphone=(self), camera=(self)` (`apps/web/csp.ts`). Permission requests only on the action that needs them (`LIVE_CALLS_UX` section 8). `connect-src` already allows `wss:`; WebRTC needs no CSP change.
- **Screen on:** the Screen Wake Lock API during an active video call where available, released on end.

## 5. Accessibility and UX contract for the call screens

All of it comes from `LIVE_CALLS_UX` and the shared UI rules; the slice only builds it. Answer and Decline are buttons with keyboard and screen-reader actions (no swipe-only answer); every state change is announced through a polite live region ("Calling Ana", "Connected", "Reconnecting", "Call ended"); mute and camera state use icon plus text, not colour alone; the ringing pulse and any flourish respect Reduced Motion and never obscure caller identity; the route is a diagnostic, not a trust colour; reconnecting keeps the same call object; the incoming surface names the IdentityProfile when several are configured; Answer video and Answer audio are separate. Slice 5 includes jsdom plus axe tests and a keyboard-only Playwright test. Haptic and sound semantics (`call-connected`, `call-ended`) follow [`HAPTIC_AND_SOUND_SEMANTICS.md`](../ux-design/specs/HAPTIC_AND_SOUND_SEMANTICS.md), which the slice must read in full.

## 6. Size budget

- **Groups wasm module** ([`apps/web/build/size-budget.json`](../../apps/web/build/size-budget.json), ADR-0187): delta limit +2,278,023 raw bytes, currently +1,883,997, so **394,026 raw bytes** (about 17 percent of the limit) remain for everything else that lands in that module. One-to-one calls do not touch it: the Core call machine goes in the main module and uses crates already linked (HKDF, HPKE, CBOR). Group-call Core code (slice 11) is the only call code that depends on MLS state; it gets a ceiling of 150,000 raw bytes of that headroom, to be re-measured at each slice, and exceeding it is a stop-and-ask.
- **Main wasm module:** no budget file exists. Slice 1 and slice 2 each record the raw and gzip delta in their report; the expected order is tens of kilobytes (state machine, one codec, key schedule). A measured delta above 150 KB raw is a stop-and-ask.
- **SFrame and WebRTC code** is TypeScript over WebCrypto: zero wasm bytes, no new dependency. It loads as a lazy chunk (dynamic import behind the call button), so a person who never calls downloads none of it. The web JavaScript bundle has no budget file (ADR-0197); each slice reports the main-bundle delta, which must be 0 apart from the lazy-load stub.

## 7. What CI can and cannot prove

| Kind | Proved in CI | Needs a person with devices |
|---|---|---|
| State machine, admission, rekey, expiry, busy, answered-elsewhere, spam fold | Rust scenario tests with two identities and a fake host | no |
| Wire | frozen CDDL, positive and negative vectors, two codecs agreeing | no |
| SFrame | RFC 9605 known-answer vectors, plus counter, replay and rekey tests against our own implementation | interoperation with a second implementation (V-007) |
| Two-party call in a browser | Chromium (and Firefox where the fake-device prefs work) with fake audio and video devices, two pages, host candidates, a real encoded-transform pipeline, decrypt-then-compare of a tone | WebKit and real devices |
| No-ring and relay-only admission | Core tests and a browser trace that no `RTCPeerConnection` and no ICE gathering exists for a dropped offer | none |
| TURN and relay-only path | a local test TURN server run by the test harness (dev-only dependency, chosen in slice 7) | the person's real Home path |
| Echo, jitter, Bluetooth routing, background audio, screen lock, battery | no | yes: slice 8 manual matrix, recorded in a dated report |

## 8. Slices

Each slice is sized for one agent session, ends in a commit, leaves `make check` green, and carries the design-gate trailers. Order matters: slices 0 to 3 can run with no browser; 4 to 6 give a working 1:1 call between two browsers; 7 and 8 harden it; group calls (9 onwards) are outlined, not sized.

### Slice 0. Wire freeze and the two delegated constructions (specification, no product code)

- **Reads:** `python3 tools/design_gate.py needs spec/cddl/call.cddl crates/om-core/src/calls` (domains `calls_media`, `crypto_1to1`, `time_ordering_events`, `metadata_privacy`, `relay_reachability`), ADR-0165, ADR-0176 (the freeze method), ADR-0139.
- **Does:** [`spec/cddl/call.cddl`](../../spec/cddl/call.cddl) with the eight signalling kinds, `CallSession` and the one-to-one key package; registered numeric fields, `OM-CALL-*` domain labels, the reason registry (`busy`, `declined`, `answered_elsewhere`, `timeout`, `failed`), the `media_keying` profile entry for Q2; vectors under `verification/vectors/`; ADR by delegation for Q1, Q2, Q4 to Q7, Q13; additive registry changes only. Update the file registry, requirement and trace ledgers.
- **Acceptance:** `python3 tools/validate_architecture.py` and `make check` pass; two independently written codecs (Rust `om-codec` and the second codec used for earlier freezes) decode and re-encode every positive vector identically and reject every negative one (command named in the slice report, same form as the ADR-0176 freeze); a unit test proves no field carries an AccountID or delivery capability (CallSessionID is random and unrelated). Operator yes recorded for Q4 before the number is frozen.

### Slice 1. Core call state machine (one-to-one, no media)

- **Reads:** the six Core domains from `design_gate needs crates/om-core/src/calls/` plus `LIVE_CALLS_UX` sections 2, 3, 5, 6; the existing `direct.rs`, `flows/` and `host.rs` patterns.
- **Does:** `crates/om-core/src/calls/` with states from section 2 of the UX spec (`incoming`, `starting`, `ringing`, `connecting`, `active`, `reconnecting`, `ending`, `ended`, `failed`), intents, host events (`CallMediaState`, `CallRouteFacts`), signalling over the existing session deposit path, expiry from the injected clock, the `CallView`. A fake host drives it. No key material yet (a placeholder key slot).
- **Acceptance (Rust, `cargo test -p om-core --test calls_*`):** accepted contact rings; an offer from a non-contact, a blocked contact or an unbound session is dropped with no host effect (no media start, no ICE request); expired offer becomes a missed call, never rung; busy gives `call.reject busy`; first answer wins and the other device receives `answered_elsewhere`; three unanswered offers in ten minutes fold; restart mid-call never resurrects a call or its keys; an unknown kind is ignored; `ringing` and `active` are never emitted without the matching peer message or host fact; the golden state-snapshot (`w0_conformance`) and backup migration tests still pass (state version bump only if the state shape changes, with a migration test). Report records the wasm delta.

### Slice 2. Call key schedule and SFrame key export (Core)

- **Reads:** slice 0 ADR, [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md), [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md), `crypto/kdf.rs`, `crypto/session.rs`.
- **Does:** the call secret in the offer, per-pair and per-direction SFrame base-key derivation, key ids, the wipe of secrets at end, the `HostCall::InstallMediaKeys` effect that hands only derived SFrame base keys (never ratchet state) to the driver.
- **Acceptance:** golden vectors for the derivation (from slice 0, both codecs); two identities derive identical keys, any change of CallSessionID, DeviceID, direction or role changes the key; a key from call A fails to open call B; secrets are zeroized (the existing `secrets.rs` style test); the call secret never appears in logs, snapshots or backups (a scan test like the existing secrets tests); mutation checks (drop a label, reuse a nonce) each fail a named test.

### Slice 3. SFrame in TypeScript (no WebRTC yet)

- **Reads:** RFC 9605 (the appendix known-answer vectors), the feasibility note, the CSP and worker rules in `apps/web`, the web guard ([`tools/web_guard.py`](../../tools/web_guard.py)).
- **Does:** an SFrame encrypt and decrypt module in `packages/om-web-platform` using WebCrypto only, key ids, counters, replay window, rekey; the `RTCRtpScriptTransform` worker wrapper as a separate file so the pure module is testable in Node and jsdom.
- **Acceptance (`pnpm --filter ... test`):** RFC 9605 known-answer vectors pass bit for bit; counter reuse and replays are refused; a frame with a wrong key id or a flipped bit fails closed; a rekey keeps the old key for the bounded overlap and then refuses it; the main bundle does not grow (lazy chunk asserted by the build report); `web_guard` and the lint policy pass.

### Slice 4. WebRTC driver and a first working call (no UI chrome)

- **Reads:** [`PLATFORM_CAPABILITY_MATRIX.md`](../architecture/client/PLATFORM_CAPABILITY_MATRIX.md), [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md), `packages/om-web-platform` family patterns, `LIVE_CALLS_UX` sections 8 and 11.
- **Does:** the `CallTransport` family capability: devices, `RTCPeerConnection`, full candidate gathering up to 3 s, the transform worker wired to the slice 2 keys, facts back to Core, ICE restart on a network change keeping the CallSessionID. Calls unavailable (typed refusal) where encoded transforms are missing (Q10). A bare test harness page only.
- **Acceptance (`pnpm e2e` with a new `calls.spec.ts`, Chromium with fake devices):** two pages call each other through the real Core, the offer and answer pass the real relay or the SQLite Home used by the existing e2e, remote audio energy is measured, and the frames on the wire fail to decode without the key (a ciphertext-sniff assertion); a call to a non-contact creates no `RTCPeerConnection` (asserted through a spy); ICE restart after `offline` and `online` keeps the same call; ending releases devices and wipes keys; a browser without the transform API shows the refusal and starts nothing. Firefox run recorded; WebKit recorded as manual until its fake-device support is confirmed.

### Slice 5. Call UI

- **Reads:** [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md), [`LIVE_CALLS_UX.md`](../ux-design/specs/LIVE_CALLS_UX.md) again, [`COMPONENT_BEHAVIOR_AND_STATES.md`](../ux-design/specs/COMPONENT_BEHAVIOR_AND_STATES.md), `ACCESSIBILITY_AND_INPUT.md`, [`HAPTIC_AND_SOUND_SEMANTICS.md`](../ux-design/specs/HAPTIC_AND_SOUND_SEMANTICS.md), [`ACTION_REGISTRY.md`](../ux-design/specs/ACTION_REGISTRY.md), [`CONTENT_AND_MICROCOPY_VOICE.md`](../ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md).
- **Does:** `CallView`, `CallControlBar`, `CallStatus`, incoming surface (Answer audio, Answer video, Decline), header call actions, Omnibar `call NAME` and `video NAME` actions (not bare names), `CallTimelineNotice`, copy for every state and failure in plain words, `Action Registry` entries. Behind the build flag (Q15).
- **Acceptance:** jsdom plus axe for every state; a keyboard-only Playwright test starts, answers, mutes and ends a call; live-region announcements asserted; Reduced Motion removes the pulse; the camera is not requested on an incoming video offer (spy on `getUserMedia`); permission denied shows only fallbacks that exist; a route change produces no warning or colour change; copy contains no key, ICE, TURN or SFrame vocabulary (the existing microcopy lint).

### Slice 6. Closed app, missed calls, and reachability copy

- **Reads:** ADR-0185, ADR-0210, [`HOME_WAKE_CHANNEL.md`](../architecture/providers/HOME_WAKE_CHANNEL.md), [`NOTIFICATIONS_AND_PUSH.md`](../architecture/client/NOTIFICATIONS_AND_PUSH.md), [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md).
- **Does:** with the app open the wake channel rings promptly; with it closed the existing generic notification opens the app, which drains and shows the incoming screen if the offer is still valid or a missed-call line if not; the Alerts page gets one honest sentence on calls. No push or Home change.
- **Acceptance:** a Playwright test with the app in a hidden tab (wake channel path) and a service-worker test of the notification click path against a fake push; a test proves the worker still does not open the vault or fetch (ADR-0210 rule 6); an offer older than the window never rings; a test asserts no new request type is sent to the Home during a call beyond deposits and drains.

### Slice 7. ICE servers: STUN, TURN and the route policy

- **Depends on the operator yes for Q8.** **Reads:** [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md), [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md), ADR-0043, ADR-0183, ADR-0165, [`home-api.md`](../../spec/protocol/home-api.md).
- **Does:** `MediaCapabilities` in `ProviderCapabilities`, the optional feature `om.media.turn.v1` in `infra/cloudflare-home` and the SQLite Home (a Home may decline), short-lived scoped credentials, the client `RoutePolicy`: contacts get LAN, then direct, then TURN; toward a non-contact peer relay-only candidates; strict privacy gives TURN-only. Usage metering and the zero-cost rule. Deploy only on the operator's approval.
- **Acceptance:** conformance tests shared by both Homes; a Home without the feature leaves calls direct-only with the honest failure sentence; a strict-privacy call never gathers host or server-reflexive candidates (candidate-type assertion against a local test TURN); credentials expire and are not reusable across calls; a TURN allocation is never made for a dropped offer; usage state is metered and refuses at the ceiling without spilling over.

### Slice 8. Hardening and the first V-007 evidence (still not a release)

- **Reads:** [`verification/GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md), the V-007 gate, [`METADATA_TRACE_TEST_PLAN.md`](../process/METADATA_TRACE_TEST_PLAN.md).
- **Does:** a cross-engine run (Chromium, Firefox, WebKit) of the slice 4 e2e; a dated manual device report (iOS Home Screen app, Android Chrome: lock screen, background, Bluetooth, route change); a metadata trace that a call adds no relay-visible signal beyond deposits and the empty wake; the no-ring trace (R-234). Records which V-007 criteria these satisfy and which remain.
- **Acceptance:** the report lists each V-007 pre-engineering and pre-release criterion with met, partly met or open and the evidence link; the gate ledger is not flipped (independent review and the second implementation are still required). A bug found here is fixed in its owning slice.

### Slices 9 to 12. Group calls (outline, sized later)

- **9. Key delivery review (blocks the rest):** write the construction chosen for Q3 into a specialist-review package (design, not code), with vectors; no group-call code before the review returns.
- **10. Core group-call state:** joined participant set, generation counter, `room.start_call`, rekey on join, leave and revocation, silent-listener and stale-generation tests. Lands in the groups module under the 150,000-byte ceiling.
- **11. Mesh for very small groups,** relay-only toward non-contacts; **12. SFU path** (Realtime SFU in the person's own account) with the SFU unable to join or hold keys. Both follow `LIVE_CALLS_UX` section 10 (joining is not ringing; disclose which is happening).

## 9. Progress

Slice 0 is done (ADR-0218, [`spec/cddl/call.cddl`](../../spec/cddl/call.cddl), `call-signal-vectors.json.gz`, `verification/prototypes/call-wire`). Slices 1 and 2 follow in that order. Operator rulings of 2026-11-18 on Q4, Q8 and Q14 are recorded in section 3 and in ADR-0218.

## 10. Stop conditions for every slice

Stop and write the question in a report if: the design is silent on something the slice must decide and the question is not already in section 3; a measured size delta passes the ceilings in section 6; a call path would need a media key outside the endpoints or a weaker protection than the account floor; a relay-visible signal beyond the existing deposits and the empty wake would be added; or the work needs a file under `crates/om-core/src`, `apps/web/src` or `packages/om-ui` that another session has uncommitted changes in (check `git status` first).
