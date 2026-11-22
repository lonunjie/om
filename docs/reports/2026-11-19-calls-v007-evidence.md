# One-to-one calls and V-007 evidence

## Design sources read

The calls build plan and ADR-0217 through ADR-0220, the calls architecture and UX specification, the Home wake and push
decisions (ADR-0185 and ADR-0210), the optional provider and release decisions (ADR-0043, ADR-0165 and ADR-0183),
the route and accessibility rules, the Home API, and the V-007 criteria and metadata trace plan. The broader mapped design
set and non-call gates were consulted selectively. This record is confined to one-to-one calls.

## V-007 criteria

| Phase | Criterion | State | Evidence and remaining work |
|---|---|---|---|
| Pre-engineering, waived for engineering by ADR-0217 | SFrame key schedule, counters and replay across two browser engines and one native runtime through an SFU, with reconnect and membership rekey | Partly met | SFrame implementation tests cover the RFC 9605 vector, replay, counters and local overlap. The Chromium two-party test exercises the WebRTC pipeline and ICE restart with one implementation. Rerun it after these changes and obtain the second engine, native runtime, SFU, and real membership-rekey evidence. |
| Pre-engineering | R-234 no ringing, ICE or TURN allocation for callers without an accepted relationship, and relay-only mesh toward non-contacts | Partly met | Core checks outgoing admission of an unknown principal, and inbound admission for a blocked relationship, late delivery and the local call switch. The new disallowed-contact browser trace awaits a run and measures peers, capture and lease requests. The actual non-contact receive path end to end and the non-contact mesh trace remain open. The Maximum Privacy coturn test measures local relay-only candidates in one-to-one calls. |
| Pre-engineering | R-283 silent Room/SFU listener cannot open media using only the Room epoch | Open | The SFrame test forwards ciphertext to a keyless receiver and one provisioned with an unrelated fixture secret. It proves the framing fails closed for those inputs. It has no Room, MLS key distribution, independent receiver or actual SFU. |
| Pre-engineering | ADR-0156 wire freeze and specialist-reviewed joined-device key delivery (R-343) | Partly met | [ADR-0218](../../decisions/ADR-0218-calls-wire-freeze-and-delegated-constructions.md) records the two-codec one-to-one wire evidence. Group call bodies, key delivery to joined devices without a 1:1 relationship, and its specialist review are open. |
| Pre-release | Full claimed matrix, including simulcast/SVC where claimed | Open | Chromium, Firefox, WebKit, native and manual-device runs and the applicable independent implementation are outstanding. Simulcast and SVC have no claim here. |

Independent review and an independent implementation remain required. V-007 stays open and calling remains an engineering opt-in.

## Trace entry points

[The calls specification](../../apps/web/e2e/calls.spec.ts) checks protected audio, camera permission timing, keyboard operation, reconnect and track release.
It also checks an existing device's closed-app drain and late delivery, the visible wake socket, and a simulated opaque wake for a hidden tab.
The direct call trace records operation names, presence of clear SDP and whether URLs contain a query or fragment. It attaches no request body or bearer value.
The no-ring trace records only whether a peer, track or media-lease request was created.
[Push tests](../../apps/web/src/sw/push.test.ts) cover the empty push and generic notification click separately; actual vendor push delivery is a device check.
[SFrame tests](../../packages/om-web-platform/src/calls/sframe.test.ts) include the opaque-forwarding fixture.

To run the relay-only candidate test, use an isolated local coturn server on loopback. These two fixture accounts exist only for the local test.

```sh
turnserver -n --listening-ip=127.0.0.1 --relay-ip=127.0.0.1 \
  --listening-port=34790 --realm=om.test --allow-loopback-peers \
  --lt-cred-mech --user=test-alice:om-e2e-only --user=test-bob:om-e2e-only \
  --no-cli --no-tls --no-dtls
```

Run `OM_E2E_LOCAL_TURN=1 pnpm --dir apps/web e2e calls.spec.ts --project=chromium` from the repository root.
The browser test supplies the test leases to the client and uses the real local TURN data path. Provider parity tests exercise
the Home lease endpoint against a bounded fixture backend. Neither exercise establishes production data-plane byte limits.
The ordinary Chromium fixtures require `--allow-loopback-in-peer-connection` because same-host LAN candidate pairs can fail even on a working network.

## Manual device matrix — 2026-11-19

| Runtime | Checks | Result |
|---|---|---|
| iOS Home Screen app | Generic push while closed, open inside ring window, screen lock and background audio/video, Bluetooth output swap, Wi-Fi to cellular and return | Pending |
| Android Chrome PWA | Same scenarios, notification click and app resume, change audio output device, microphone removal and replacement | Pending |
| Desktop Firefox and WebKit | Run two devices or processes with actual audio/video devices. Start and answer, mute, route change, frame continuity and keyboard/live announcements | Pending |
| SFU and native runtime | Independent sender/receiver pair, reconnect, joined-device rekey, leave/rejoin, revocation and silent-listener trace | Pending |

Record engine, OS, device, direction, route change, elapsed recovery time, observed media state, audio-device behavior and whether the call remained the same.
Use fixture accounts and avoid retaining packet contents, ICE server credentials, addresses, session IDs or identity IDs in shared traces.

## Cross-engine runs and robustness regressions, 2026-11-22

Design sources read: the calls architecture, ADR-0217 through ADR-0221 and ADR-0228. The metadata observer matrix and the
Home wake and push documents were consulted by search only.

| Engine (Playwright 1.63.0) | What runs | What it cannot do headlessly |
|---|---|---|
| Chromium | Every case in the calls specification, with fake devices and loopback ICE from launch flags | Nothing found; the local TURN case needs the loopback coturn fixture above |
| Firefox | The same cases, with fake devices, no permission prompt and loopback ICE from preferences set in `playwright.config.ts`. Device permissions cannot be granted through the automation protocol, so the specification does not request them there. The ICE restart after the emulated offline blip occasionally does not finish inside the 20 second limit (about one run in eight), so Firefox has one retry; a retried pass is reported as flaky | The local TURN case needs the coturn fixture |
| WebKit (the Playwright build) | Only the refusal: the build has mock capture devices but no `RTCRtpScriptTransform`, so the client refuses a protected call before capture, ICE or any signal. The test asserts exactly that | Placing a call. Branded Safari has the transform; that and real devices stay in the manual matrix below |

Regressions added in the same file, each run on Chromium and Firefox:

- turning "Allow calls" off while a call rings clears the ring with no capture, peer or lease; a call while it is off does not ring; turning it on again lets the next call ring;
- a brief outage on the answering device keeps the same call, the same peer and the mute state, and media resumes;
- a whole call, an ICE restart and the hang-up present the Home only operations, header names and URL shapes that an ordinary chat exchange already presents. No query or fragment, no media request when the Home advertises none, and no clear description. Deposit sizes are recorded as an attachment and not compared, because the padding bucket is the existing, documented signal;
- the ICE restart on the original caller (already present) now also runs under the trace above.

Host fix found by the Firefox run: a pending device list could hold the call preparation open, so every later call from the same page did nothing. The device list is now fetched without holding the preparation.

The TURN adapter contract and its parity suite for both Homes are in [`MEDIA_TURN_ADAPTER_CONTRACT.md`](../../infra/MEDIA_TURN_ADAPTER_CONTRACT.md).
