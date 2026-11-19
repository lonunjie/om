# ADR-0233 — Device-separated call media keys; group calls remain unauthorized

- **Status:** Accepted by delegation (2026-11-19) for one-to-one calls: it restates ADR-0228 section 1 and changes no wire byte. **Group calls are not authorized**; they need their own specialist-approved design.
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** No wire, CDDL, registry or label change; client and Core behavior already required by ADR-0228 section 1.
- **Answers:** the cryptographic ruling on who holds a media secret in a one-to-one call with several callee devices, and the statement that group calls stay gated.
- **Amends:** nothing. **Does not amend:** ADR-0218 (frozen wire, labels, suite), ADR-0219, ADR-0220, ADR-0221, ADR-0228, [`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md).
- **Scope:** `calls` (ADR-0217).
- **Controlling sources:** ADR-0218 sections 3 to 5 and 9, ADR-0219 to ADR-0221, ADR-0228 section 1, [`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md), [`call.cddl`](../spec/cddl/call.cddl); gate V-007 (real-device media).

## Design sources read, and not read

Read in full for this import: the six proposals; ADR-0204, ADR-0225, ADR-0227, ADR-0228, ADR-0229 and the passages of ADR-0199, ADR-0203 and ADR-0226 cited here; [`call.cddl`](../spec/cddl/call.cddl), [`controller-change.cddl`](../spec/cddl/controller-change.cddl), [`lan.cddl`](../spec/cddl/lan.cddl), the recovery-record CDDL, [`recovery-sheet.md`](../spec/protocol/recovery-sheet.md), the object-type, domain-separation and numeric-field registries and Home API v1 section 8.2. Not read in full: ADR-0216, the constitution documents, and the domain documents the design gate lists for these paths. The proposals were checked against the sources above only; if an unread text fixes a point differently, that text wins and this ADR is corrected by a follow-up.

## Decision: one-to-one calls

1. A caller assigns one fresh 128-bit `CallSessionID` and generates a **separate independent 32-byte CSPRNG media secret for each receiving DeviceID**, not one per person/call. The existing `CallSignal` type 44 offer and `CallKeyPackage` `{0: call_secret(32),1: suite(1)}` are **unchanged**; only each recipient's already authenticated, separately ratcheted one-to-one session carries that recipient's secret. Do not wrap the same secret separately; wrapping does not prevent a losing callee device from deriving the winning pair's keys.
2. The caller's transient state is `call_id -> {target_DeviceID -> Zeroizing<secret32>, deposit/seq state}`. The first authenticated, eligible and timely answer selects its sender DeviceID. The caller derives send/receive SFrame keys from **that selected leg's secret only**, discards and zeroizes secrets for all other recipients, and sends the existing `answered_elsewhere` end to losers. An answer from a DeviceID not offered that exact call, an ineligible device, a duplicate answer, a stale call or a misbound call id is refused. A late loser cannot change the selected secret. A callee never learns another leg's secret from signal replay.
3. Keep the already registered directional derivation **exactly** as ADR-0218: `HKDF-SHA256(salt=CallSessionID, IKM=per-device secret32, info=canonical CBOR ["OM-CALL-MEDIA-BASE-KDF-v1", sender_DeviceID, recipient_DeviceID, key_generation=0], L=32)`, then the frozen SFrame suite AES-128-GCM + SHA-256 (RFC 9605). Do not change key-generation format, label or key count. The session/recipient identity is authenticated by the enclosing one-to-one encryption context. No stream encryption until the accepted-answer authentication and host installation succeed; the strict relay-only profile refuses media capture/ICE when no valid owner-controlled TURN lease exists. Exact retry and watchdog from ADR-0228 §§2–4 remain separate requirements.
4. **Existing wire is sufficient for 1:1.** There is **no new call CDDL, object type, label or negotiation bit**. For clarity, the unchanged schema is:

```cddl
CallSignal = {
  0: 44, 1: 1, 2: 1..8, 3: bstr .size 16, 4: 0..4294967295,
  ? 5: 1..2, ? 6: bstr .size (1..16384), ? 7: call-key-package,
  ? 8: 1..7, ? 9: 0..3, ? 23: extensions,
}
call-key-package = {0: bstr .size 32, 1: 1}
```

5. **Group calls NOT authorized.** `CallSignal` kinds 7/8 and numeric keys 10–15 are reserved but intentionally **unfrozen** for group calls; do not invent them in the 1:1 codec. A design in which all account/Room members, a Home, or a nonparticipant gets a reusable group SFrame secret violates the audited participant-only confidentiality rule. Group calls require a **separate protocol-level, independent specialist approval** of authenticated participant enrollment, pairwise transport to **each currently joined DeviceID**, sender-key ratchet/rekey on membership leave/rejoin/device loss, consistent membership/epoch snapshots, downgrade/partition behaviour, replay/fork limits and metadata. Meeting only the 1:1 negative-knowledge vectors is insufficient. V-007 real-device evidence remains required before a release claim for calls.

## Group-only *review sketch*, NOT allocated or valid wire

The following shape expresses the **minimum** a future review must define; numeric positions are placeholders for an **independent group-call CDDL revision**, not current decodable fields:

```cddl
; Unallocated; illustrative, NOT valid CallSignal v1 body.
proposed-group-key-delivery = {
  0: uint,                 ; future new type/kind to allocate
  1: 1,                    ; version
  2: bstr .size 16,       ; group call id
  3: bstr .size 32,       ; canonical participant-roster commitment
  4: uint,                ; membership epoch, strictly increasing
  5: bstr .size 16,       ; originating sender DeviceID
  6: bstr .size 16,       ; intended joined receiver DeviceID
  7: bstr .size 32,       ; independent sender media secret for this epoch
}
```

The actual future design must bind the payload **inside an authenticated pairwise joined-participant session**, specify roster signing/evidence and no same-secret broadcast to nonmembers. It must register a separate group key context label e.g. `OM-GROUP-CALL-PARTICIPANT-KEY-v1` *only after* approving its byte-level derivation. No current system may claim this label as frozen or emit the sketch.

## Metadata analysis

The callee's Home already sees each individual encrypted call-offer deposit, fan-out target, timing/size and each later answer/ring cancellation. Per-device secret separation adds **no new Home-visible field, number of deposits or message kind**. An endpoint holding a losing offer knows the call was offered and can correlate its own UI, but cannot decrypt another device's chosen media. A malicious Home can delay, drop, duplicate or correlate delivery, cannot derive SFrame key from this signal alone, and need not be trusted for selecting winner. The selected WebRTC peer(s), TURN operator and network observer still learn ICE/network/packet timing and approximate call volume; strict relay-only substitutes the person's configured relay, not metadata invisibility. Group calls would expose participant topology and join/leave timing even if media encrypted; this is part of the blocked separate review. No code should claim calls immune to media implementation/endpoint compromise without V-007.

## Known-answer and negative-knowledge vectors

- `CALL-DEV-01`: fixed `call_id = 10..1f`, caller `20..2f`, recipient A `30..3f`, recipient B `40..4f`, recipient C `50..5f`; independently fixed 32-byte offer keys A=`00..1f`, B=`20..3f`, C=`40..5f` **for the test only**. Two independent encoders produce canonical, encrypted-session-plaintext offer bytes with identical call_id but three distinct key-package contents. A production RNG test requires no repeats across 1000 multi-device offers.
- `CALL-DEV-02`: B answers first. Derive base keys using B's secret and exact frozen info encoding; compare full 32-byte known answers against independent Rust/Python derivations and RFC 9605 sender/receiver stream encrypt/decrypt goldens. A's and C's secrets, even with B's known DeviceID, must fail to reproduce either B directional key or decrypt B's authenticated SFrame.
- `CALL-DEV-03`: unknown device D answers, A replies late, B answer duplicate, callID replay, key-generation replay, target membership revoked, 2-leg loser first retransmission: all fail closed/no winner change. Zeroized loser secrets are not in persisted state or crash reports.
- `CALL-DEV-04`: preserve exact ordered `seq` per leg, uncertain answer retry at 2/4/8 seconds with same bytes and operation id, 30-second connecting watchdog, ICE restart 30-second watchdog with max two further attempts, relay-only without lease fails before media capture. Both Homes' backend lease behavior, not just API mocks, needs verification; V-007 remains open.

## Conformance

ADR-0228 section 1 already requires one fresh secret per callee device; this ADR restates it as the cryptographic ruling and adds the negative-knowledge vectors above. An implementation that sends one secret to several devices does not conform and must be brought to the rule before any call release claim. Only the existing 1:1 format is eligible for implementation.

**Gate:** specialist review of the actual implementation and of the negative SFrame fixtures. No release or security approval is implied; V-007 stays open.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision, no new value.** This ADR allocates no number or label. The unchanged `CallSignal` schema above matches [`call.cddl`](../spec/cddl/call.cddl); kinds 7 and 8 and keys 10 to 15 stay reserved for group calls. The label `OM-GROUP-CALL-PARTICIPANT-KEY-v1` named in the review sketch is absent from [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) and is **not** reserved: it is an example for a future review.
- **Relation to ADR-0228.** Section 1 of ADR-0228 is the controlling text; this ADR changes no wire byte.
- **Edits.** The reviewer's section describing a specific unfixed source location was removed in favour of the "Conformance" paragraph; provisional-number sentence removed; the "Gate" paragraph rewritten.
