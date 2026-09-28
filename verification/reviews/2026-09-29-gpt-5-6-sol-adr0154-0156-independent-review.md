# Independent review — ADR-0154 through ADR-0156 and current V1 closure

- **Date:** 2026-09-29
- **Reviewer:** OpenAI GPT-5.6 Sol
- **Independence class:** different vendor from the Claude Opus 5.5 author of ADR-0154–0156; satisfies ADR-0117 only for those reviewed Anthropic-authored artifacts
- **Scope reviewed:** ADR-0154, ADR-0155, ADR-0156; canonical extension rules; multi-device/group-history boundary; scheduled send; LAN architecture and A-055; machine gates/registries
- **Important:** the reviewer authored ADR-0157/0158 remediation in this same pass. Those new changes **do not** self-satisfy `independent_reviewer` and still require a different-vendor review before affected engineering authorization.

## Result

ADR-0154 is accepted as written with its documented T-132 residual. ADR-0155 and ADR-0156 are accepted only with the amendments below. No W0 wire object is broken or renumbered. Engineering remains unauthorized.

## Findings and resolutions

| ID | Severity | Finding | Resolution |
|---|---|---|---|
| OAI-01 | **Major** | T-133 treated a future critical registration sent as `critical=false` as an inherent residual. It is not inherent before V1 release: a baseline client can reserve a self-identifying critical namespace so old/new registry versions converge. | ADR-0157 reserves `om.critical.*`; unknown IDs under it reject regardless of the sender bit. Existing frozen `om.acs.*` critical IDs are grandfathered. R-346/T-136; V-005 gains convergence vectors. |
| OAI-02 | **Major** | ADR-0156 puts every `HistoryBundle` behind an implementation-private codec namespace, but the group architecture uses history sharing to newcomers as a cross-implementation group feature with security-visible bundle-author provenance. | ADR-0157 splits implementation-private same-owner `DeviceHistorySnapshot` from interoperable `GroupHistoryBundle`; V-004 must freeze/test the latter. R-347/T-137. |
| OAI-03 | **Minor** | Scheduled-send rules did not explicitly forbid precomputing ciphertext/fanout at compose time, leaving stale eligibility/epoch behavior possible. | ADR-0157 makes the queue compose-intent only; current eligibility/routes/epoch and encryption are resolved at actual send. R-344/R-348/T-138. |
| OAI-04 | **Major, pre-existing A-055** | LAN delivery was intentionally unbound: no canonical signalling/framing, route descriptor, K/epoch, browser path, async-key fetch rule or backpressure. | ADR-0158 resolves the design: 900 s epoch, K=8, registered derivations, seed-encrypted signalling that hides SDP/ICE credentials, ordered/reliable WebRTC DataChannel, frozen `LanFrameV1`, ephemeral `om+lan://` RelayEntry reusing the existing sealed-delivery route binding, ordinary IntroGate first contact, signed async-key fetch only after relationship activation. A-055 becomes resolved; V-024 remains open for evidence/review. |
| OAI-05 | **Minor** | ADR-0156's “implementation/codec identifier” had no collision-resistant concrete form. | ADR-0157 fixes account-private codec identity as random 32-byte `codec_id` plus `codec_major`, authenticated in the sync frame and never reassigned. |

## LAN design evidence note

WebRTC is used as the interoperable browser-capable byte channel because current browser standards expose ordered/reliable `RTCDataChannel`, DTLS protection, application-defined subprotocols, message-size negotiation/limits, `bufferedAmount`, and `bufferedAmountLowThreshold`. The profile deliberately caps OM chunks at 16 KiB—well below the commonly documented 64 KiB default when no `max-message-size` SDP attribute is present—and uses explicit high/low-water backpressure. This is architectural feasibility evidence only, **not** the real-browser demonstration required by V-024.

References consulted during this review:

- W3C WebRTC Recommendation (2025): https://www.w3.org/TR/webrtc/
- MDN, Using WebRTC data channels: https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Using_data_channels
- MDN, RTCDataChannel `bufferedAmountLowThreshold`: https://developer.mozilla.org/en-US/docs/Web/API/RTCDataChannel/bufferedAmountLowThreshold
- MDN, RTCSctpTransport `maxMessageSize`: https://developer.mozilla.org/en-US/docs/Web/API/RTCSctpTransport/maxMessageSize

## What this review does not close

- The consolidated `core_kernel` independent review still needs a **non-OpenAI** reviewer where the reviewed artifacts are OpenAI-authored.
- ADR-0157/0158 and their new registry/CDDL consequences need a different-vendor independent reviewer; this OpenAI review cannot approve its own changes.
- V-024 still needs two-codec vectors, observer and first-contact mutation models, real-browser feasibility evidence (or an honest unavailable capability result), and later two-implementation LAN interoperability.
- Existing tool/runtime gates (Rust/TLC/browser/platform/human/external crypto review as applicable) remain unchanged.
