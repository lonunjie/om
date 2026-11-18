# ADR-0217 — Authorize engineering scope `calls` (W2), by explicit operator instruction

- **Status:** **Accepted** (operator, 2026-11-18, explicit instruction: "yes, authorize calls"). This is a decision of the operator, not a review result and not a delegation.
- **Date:** 2026-11-18
- **Authorizes:** engineering scope `calls` only. No other scope changes state.
- **Depends on:** `core_kernel` (ADR-0169) and `groups` (ADR-0177), both already authorized.
- **Waives:** the **pre-engineering** phase of gate V-007 (`sframe_interop`) for engineering purposes only. Its pre-release phase is untouched and stays open.
- **Plan:** [`2026-11-18-calls-build-plan.md`](../docs/reports/2026-11-18-calls-build-plan.md)

## Context

`calls` (W2) is the one locked scope that bears on daily use: the V1 scope map lists one-to-one audio and video calling and live-call state language, and the group-call UI where Core exposes group calling. The design is complete and audited: [`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md), ADR-0013 (SFrame, adaptive routes), ADR-0128 rules 14 and 15 (who may ring, participant-bound keys), ADR-0132 (A-035 resolved: per-sender SFrame keys to joined devices only), and the logical schema [`call-session.logical.schema.json`](../spec/protocol/call-session.logical.schema.json).

The unlock contract ([`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md)) otherwise requires every gate that lists the scope to have `pre_engineering_status` passed or waived by an ADR. V-007 lists `calls`; its four pre-engineering criteria (a two-browser plus native SFrame interoperability prototype through an SFU; the no-ring and relay-only admission traces; the silent-listener trace; and the ADR-0156 wire freeze with a specialist-reviewed key-delivery channel) are not met. The operator has chosen to authorize engineering now and to keep that evidence as a release obligation. This is the same shape as ADR-0170 (V-018) and ADR-0183.

## Decision

1. **`calls` is authorized**, by explicit operator instruction. Engineers follow [`ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md), the `calls` entry of [`engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json) and the design gate in [`AGENTS.md`](../AGENTS.md).
2. **What the scope covers.**
   - One-to-one calls first (voice, then video). Group calls follow in a later wave of slices, after one-to-one is built and its evidence is in.
   - Media end-to-end encryption with SFrame (RFC 9605), applied in the client; the transport (direct, relay, SFU) never holds a media key.
   - Call signalling (`call.offer` to `call.key_epoch`) carried as ephemeral encrypted control traffic over the existing authenticated sessions and Home queues. No new global call identity or call-signalling service.
   - Nothing visible to a relay beyond what the design already allows (an opaque deposit; for a wake, the empty push of ADR-0210). A call adds no new relay-visible signal.
   - The call UI states in the shared `om-ui` (ringing, in call, ended, missed) and the platform adapter for microphone, camera and WebRTC.
3. **Gate V-007 pre-engineering is waived by this ADR, for engineering only.**
   - *Risk accepted:* code for call signalling and key handling can be written before the SFrame interoperability evidence and the specialist cryptographic review exist. A construction chosen during engineering could be found wrong by that review.
   - *Compensating controls:* (a) the first build slice freezes the call wire (CDDL, registered numeric fields and domain-separation labels, positive and negative vectors, two independent codecs agreeing) before any other call code is written, by the V-005 method, additively and under ADR-0165; (b) every construction choice the design leaves open is written down in an ADR by delegation that does not amend audited text, and is listed in the plan; (c) one-to-one first means no group key-delivery channel is built until the specialist review of it (R-343) exists; (d) the feature is built behind a capability flag that is off in every release until the release evidence below is in.
4. **Still required BEFORE RELEASE, not before engineering.** Any release that offers calls, any release declaring `calls` in `public_releases`, and any claim about call security needs V-007 pre-engineering and pre-release evidence in full: SFrame key schedule and counter and replay rules interoperating across at least two browser engines and one native runtime through an SFU with reconnect and membership rekey; no ringing, ICE gathering or TURN allocation for an offer from a principal without an accepted relationship, and relay-only candidates toward non-contacts (R-234); the silent-listener trace (R-283); the specialist-reviewed key-delivery channel for joined participants without a 1:1 relationship (R-343), required before group calls ship; and the full claimed matrix. `om-open-beta-1` does not list `calls` and stays as it is.
5. **Constraints.** No new cryptographic primitive: only the suites already in the crypto profile (HKDF-SHA-256, the AEAD already used, HPKE already in Core). **Floors are unchanged:** no call path may use a weaker profile than the account's floor, no setting disables media encryption to regain reachability, and a fallback never lowers privacy ([`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md) "Mid-call migration"; [`engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json) forbidden shortcuts). No post-quantum confidentiality claim for group calls (R-173). Providers (STUN, TURN, SFU) never become call membership or key authority. No design change: where the design is silent the engineer writes the question in the plan's list and stops, or an ADR by delegation records a construction that does not amend audited text.
6. **Where did authority go?** Nowhere new. Call authority is the existing relationship (one-to-one) or Room membership plus `room.start_call` (group). The Home and any media provider carry opaque bytes and allocate resources; they cannot start, join, key or end a call.

## Applied on acceptance

- [`machine/repository_contract.json`](../machine/repository_contract.json): `engineering_scopes.calls.authorized = true`, `authorizing_adr = "ADR-0217"`.
- [`machine/architecture_verification_gates.jsonl`](../machine/architecture_verification_gates.jsonl): V-007 `pre_engineering_status` and `pre_engineering_independence_status` become `waived_by_adr` with a waiver sentence naming this ADR. Its overall `status` stays `open`.
- [`machine/decisions.jsonl`](../machine/decisions.jsonl) D-227; [`INDEX.md`](INDEX.md); [`AGENTS.md`](../AGENTS.md), [`SNAPSHOT.md`](../SNAPSHOT.md), [`ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md) and the unlock table name the fifth authorized scope (5 of 12; 7 gates still have pre-engineering open).

## Consequences

- The review basis is the operator's decision, not independent review. The independent-reviewer criterion of V-007 moves to the release obligations in point 4.
- Slice 0 of the plan is design-bound specification work (wire freeze) and is the only thing that may be built first.
