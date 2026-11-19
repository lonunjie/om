# ADR-0235 — Independent metadata ruling for direct-only typing indications

- **Status:** Proposed / blocked. **Not accepted.** Implementation is blocked by the separate direct-transport authorization (`local_delivery` is not authorized for product code) and by the need for an independent metadata review, accepted by the design authority, with executable packet and no-relay evidence.
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** Would be `additive_noncritical_extension` (ADR-0165): one frame kind on an existing direct channel.
- **Answers:** the metadata review that ADR-0204 requires before a typing frame may be built (Q-CHAT-5 follow-up).
- **Amends:** nothing. **Does not amend:** ADR-0204 (its accepted-with-gates status stands), ADR-0158, ADR-0194.
- **Scope:** the already authorized direct channel and the opt-in client setting only; no Home, provider or Room feature.
- **Controlling sources:** ADR-0204, ADR-0158 and ADR-0194, [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md), [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md), [`SECURITY_PRIVACY_PROFILES.md`](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md), [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md), [`lan.cddl`](../spec/cddl/lan.cddl); the [`AGENTS.md`](../AGENTS.md) rule that `local_delivery` stays unauthorized.

## Design sources read, and not read

Read in full for this import: the six proposals; ADR-0204, ADR-0225, ADR-0227, ADR-0228, ADR-0229 and the passages of ADR-0199, ADR-0203 and ADR-0226 cited here; [`call.cddl`](../spec/cddl/call.cddl), [`controller-change.cddl`](../spec/cddl/controller-change.cddl), [`lan.cddl`](../spec/cddl/lan.cddl), the recovery-record CDDL, [`recovery-sheet.md`](../spec/protocol/recovery-sheet.md), the object-type, domain-separation and numeric-field registries and Home API v1 section 8.2. Not read in full: ADR-0216, the constitution documents, and the domain documents the design gate lists for these paths. The proposals were checked against the sources above only; if an unread text fixes a point differently, that text wins and this ADR is corrected by a follow-up.

## Independent metadata conclusion and decision

1. **Conditional approval for direct-only mechanism, not for release.** A typing packet is distinct behavioural metadata: an observer learns when a person is preparing text, and the peer learns active/idle periods even when no message is ever sent. On an already open mutually authenticated direct channel this adds **no new Home deposit or relay-visible object** and does not establish a new network connection; it does add direct-channel packet timing/size to the peer and on-path observers. Thus the ADR-0204 direct-only form is consistent with its stated *relay-visible* floor, but **not** a promise of typing anonymity or zero traffic metadata. An on-path observer may infer typing from packet bursts unless indistinguishable from comparable transfer traffic, and the peer sees state intentionally.
2. **Off in both directions until explicit consent.** Each identity profile has independent `send_typing` and `show_typing` preferences initialized false, and neither profile changes them implicitly. Recommended may opt into either; Hardened may show on explicit choice but still cannot send until separately opted in; Maximum Privacy requires a separate explicit user action to opt into either direction and must warn that activity timings are shared. A receiver must not display a hint until show is enabled; a sender must not transmit until send is enabled **and** the recipient is currently an authenticated direct peer, not a Room member fanout.
3. **Already-open, bound direct channel only.** Do not run rendezvous, ICE, TURN, a LAN probe, a fresh direct connection, wake, push, deposit or Home request *because* a user typed. The transport must already be open for a non-typing reason and cryptographically bound to the contact's verified session; the receiver checks which contact the channel belongs to before displaying anything. If direct transport is not authorized in product code, the typing feature remains unavailable rather than silently falling back to relay, MLS Room traffic or sealed ephemeral one-to-one deposits. If the current LAN DataChannel frames lack end-to-end authentication of the session peer or leak content on a shared origin beyond the verified contact, that transport must be reviewed and repaired before enabling typing. The WebRTC data plane's DTLS alone is not a substitute for verified peer-to-AccountID binding.
4. **Rate and bounded life.** A single `start` is an ephemeral hint of maximum **8 seconds**; sender refresh no more often than every **4 seconds**, optional `stop` on send/clear/blur. Sender keeps no queued hint for disconnection/replay; receiver derives expiry from its monotonic local arrival clock (no peer timestamp), collapses duplicated starts and expires on lost stop/connection loss. No persisted history, ratchet app event, notification, cross-device sync, analytics field or debug logging. A parse failure, old/unknown kind, unauthenticated peer, absent consent or channel closure is silent/drop.
5. **No Rooms and no new Home-visible timing.** Rooms lack a pre-existing individually authenticated direct channel to every intended recipient; multicast typing over MLS would create extra relay deposits and depart from ADR-0204. The product may say “typing hints work only when a direct connection is already active”, never “typing always works.” A request to use relay-based typing **must change the audited privacy design** with an explicit operator decision and fresh threat model.

## Additive local-frame CDDL (conditional on transport authorization)

```cddl
; Add one alternative to existing lan-frame-v1, preserving kinds 1..8:
lan-frame-v1 = lan-object-begin / lan-object-chunk / lan-object-end /
               lan-ack / lan-fetch-async-key / lan-fetch-blob / lan-error /
               lan-object-receipt / lan-typing
lan-typing = {
  0: 1,                      ; existing LAN frame profile version
  1: 9,                      ; additive direct-only kind; register before use
  16: 1 / 2,                ; start=1, stop=2; no text, target, timestamp or account id
}
```

`lan-typing` is a direct DataChannel application frame; **the channel must be authenticated and bound** as above. It does not require a distinct `DS-HASH`, KDF or AEAD label because the packet is authenticated by the **existing session context** and conveys no crypto authority; inventing an independent key solely for typing would create new protocol complexity. If the transport implementation cannot prove record-level or channel-level authenticated integrity and identity binding without a fresh connection, block implementation and submit a versioned, reviewed authenticated transport format with its own registered label. The assigned `kind=9` must be checked against the local-delivery registry by the owner of that gated area before merge (kinds 1..8 verified at this snapshot).

## Metadata disclosure accounting

| Party | Can observe from direct-only typing | Mitigation/explicit limit |
|---|---|---|
| Person's own Home, contact's Home | No additional HTTP/deposit/wake merely from typing | Prove with zero-Home-traffic tests; existing unrelated traffic still observed |
| Contact on already authenticated channel | Start/stop and 8-second refreshed activity | Explicit send opt-in; independent receive control; never history |
| LAN/ISP/TURN/on-path observer of direct channel | Packet timing, frequency, sizes, peer IP/contact topology already exposed by pre-existing direct session | No new ICE probe; constant tiny frame shape; rate limit; cannot claim concealment of writing behaviour from traffic analysis |
| Other Room members, unjoined devices, cross-account Home operator | No typing frame | No fanout, no relay fallback or MLS Room object |
| Malicious contact | Can send forged starts **on its authenticated channel** and spam indications | Per-connection 4-second rate acceptance, 8-second expiration, foreground-only presentation, user blocking/consent off |

## Golden vectors and gate tests

- `TYPE-01`: canonical CBOR map `{0:1,1:9,16:1}` (start) and `{0:1,1:9,16:2}` (stop), expected exact canonical CBOR hex **`a3000101091001`** / **`a3000101091002`**, independently confirmed by Rust/TS codec; duplicate field, state 0/3, unknown additional key or profile version fail closed. Register kind 9 only after independent conflict check.
- `TYPE-02`: 0→4→8-second starts keep one indicator alive until last accepted start+8s; a resend at 3 seconds violates sender rate; packet loss/reorder, stop before start, disconnected channel expire without persistence.
- `TYPE-03`: all three profiles, send/show opt-in matrix, off by default after restart, different identity profiles and blocked contacts; **no Home/network relay calls or deposits** from any typing event; no new ICE connection from composer input.
- `TYPE-04`: inspect authenticated direct-session endpoint/peer binding, DTLS/ratchet properties, and traffic packet captures with/without typing on a pre-existing connection; record timing leakage and ensure no cross-identity forwarding. A mock DataChannel alone does not discharge the transport gate.

## Conditions precedent

ADR-0204's two original gates remain independent: (a) product-code direct transport authorization by the controlling documents, and (b) this metadata ruling reviewed **by someone independent of the author and accepted by design authority**, with executable packet/no-relay evidence. Do not claim passing a paper review alone enables code, or use a newly opened transport path under `local_delivery` without that area's design gate.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision, nothing reserved.** In [`lan.cddl`](../spec/cddl/lan.cddl), `lan-frame-v1` kinds 1 to 8 are allocated and kind 9 is free; field key 16 is free (the frames use keys 0 to 15); `object_kind 1..8` is a different field. Canonical CBOR `{0:1, 1:9, 16:1}` is `a3000101091001` and `{0:1, 1:9, 16:2}` is `a3000101091002` (hand-checked). No label is needed. Because this ADR is blocked, kind 9 is **not** reserved: the owner of the direct-transport area confirms the number when that gate opens.
- **Status unchanged on purpose.** The reviewer's status (proposed, implementation blocked) is kept exactly. Acceptance needs both the authorization of a direct transport for product code and an independent metadata review accepted by the design authority, as ADR-0204 already requires; neither exists.
- **Relation to ADR-0204.** This is the independent metadata review ADR-0204 asks for, in proposal form. It does not change ADR-0204's accepted-with-gates status.
- **Edit.** Provisional-number sentence removed.
