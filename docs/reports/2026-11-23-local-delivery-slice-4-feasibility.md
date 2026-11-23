# local_delivery slice 4: DataChannel feasibility spike (first result)

Date: 2026-11-23. Design sources read: ADR-0158 (in full), the local_delivery build plan. Not read in full: the UX brief and the transport adapter documents (this spike writes no product UI).

**Question (V-024 open criterion, plan Q1):** can each engine establish the ordered, reliable `om-lan-v1` DataChannel with host candidates only and no project-run STUN, TURN or signalling, and stream `LanFrameV1`-sized frames with the profile's backpressure marks?

**Method.** [`feasibility.mjs`](../../verification/prototypes/lan-datachannel/feasibility.mjs): two pages in one browser context, `iceServers: []`, offer and answer passed through the script (the copy/paste or QR path), 256 frames of 16 KiB, sender pauses above 262,144 buffered bytes and resumes on `bufferedamountlow` at 65,536. Raw result: [`lan-datachannel-feasibility-2026-11-23.json`](../../verification/results/lan-datachannel-feasibility-2026-11-23.json).

| Engine | Established | Frames in order | Candidates | Result |
|---|---|---|---|---|
| Chromium | yes | 256 of 256 | host, mDNS-named; none srflx or relay | passes |
| Firefox | yes | 256 of 256 | host, mDNS-named; none srflx or relay | passes |
| WebKit (Playwright, Linux) | no | n/a | only `.local` host names | not established here |

**What this does and does not show.** It shows two engines can run the profile's channel and framing on one machine with no infrastructure, and that backpressure events fire. It does not show anything about iOS Safari, real phones, a real Wi-Fi network, client isolation, or the iOS local-network permission. Browsers name host candidates with mDNS (plan Q1); on one machine that resolved for two engines and not for WebKit, so a real network is the test that matters. A runtime that cannot establish the channel reports `local_delivery = unavailable` and offers QR/manual mode only where the channel can still be built (ADR-0158 section 4); no weaker fallback exists.

**Left (operator or real-device):** real iPhone and Android runs, two phones on one Wi-Fi, a network with client isolation, the permission prompt. Slice 4's product driver (`om-web-platform`, capability row, manual and QR signalling) is not built yet.
