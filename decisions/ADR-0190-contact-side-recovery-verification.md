# ADR-0190 — How a contact verifies a recovery-only successor: evidence travels, time never does

- **Status:** Accepted
- **Date:** 2026-11-14
- **Decided by:** operator delegation 2026-11-14 to fill silence ("make the best decisions ... easy as possible with different security profiles; users can tighten security later", the wording of the ADR-0187 confirmation, same day). This ADR fills silence only and amends no audited text (AGENTS "Design gate" rule 6). The few places that depend on a reading of audited text are listed under "Not decided here" so the operator can overrule them.
- **Answers:** the open question in [`2026-11-13-recovery-flows-plan.md`](../docs/reports/2026-11-13-recovery-flows-plan.md) ("how does a contact obtain the RecoveryPolicy body and the RecoveryRequest") and the gap noted in the header of `crates/om-core/src/flows/recovery_watch.rs`.
- **Amends:** nothing. **Does not amend:** [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) (section 10.3 and "Withholding-resistant activation"), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md), [`IDENTITY_CONTINUITY.md`](../docs/architecture/identity/IDENTITY_CONTINUITY.md) (ID-03), ADR-0111, ADR-0127, ADR-0131 section 6, ADR-0144, ADR-0147 section 5, ADR-0154, [`SEALED_SENDER_DELIVERY_PROFILE.md`](../docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md), any CryptoProfile, the frozen W0 CDDL (additions only).

## Design sources read, and not read

Read in full: AGENTS "Design gate"; RECOVERY_AND_BACKUP_POLICY; ACCOUNT_CONTINUITY_AND_CONTROL; IDENTITY_CONTINUITY; ADR-0131; ADR-0147; ADR-0189 (format and the carriage pattern it set). Read in the relevant parts only: ADR-0154 (sections 1 and 2, the veto-sweep rule), [`2026-11-13-recovery-flows-plan.md`](../docs/reports/2026-11-13-recovery-flows-plan.md) (the findings and the slice-4 decisions, not the slice build logs), [`CONTENT_AND_MICROCOPY_VOICE.md`](../docs/ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md) (rules and patterns), the passages of SEALED_SENDER_DELIVERY_PROFILE that fix what the sealed inner holds and how replay state works, [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) (sections 1 and 2). Code read: `recovery_watch.rs` (to line 420), `control.rs` (`verify_successor`, `verify_recovery_successor`, `RecoveryLinkEvidence`, `verify_chain_with`), `flows.rs` `first_contact_head` and `on_session_envelope` (to the delivery-key lookup), `flows/multi.rs` (`peer_refresh`, `on_acs_update`, held wrappers), the recovery state types in `state.rs`, the recovery views in `api.rs`, `recovery.rs` (header, constants, `RecoveryPolicy`), the recovery objects in [`account.cddl`](../spec/cddl/account.cddl).

**Not read in full:** [`AGENT_ARCHITECTURE_RULES.md`](../docs/process/AGENT_ARCHITECTURE_RULES.md) (the Design gate and the "Where did authority go?" rule were applied from AGENTS.md only), [`IDENTITY_PROTOCOL.md`](../docs/architecture/identity/IDENTITY_PROTOCOL.md), [`IDENTITY_DISCOVERY_VERIFICATION.md`](../docs/architecture/identity/IDENTITY_DISCOVERY_VERIFICATION.md), [`IDENTIFIER_SPOOFING_AND_DISPLAY.md`](../docs/architecture/identity/IDENTIFIER_SPOOFING_AND_DISPLAY.md), [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md); the crypto_1to1, discovery_first_contact, metadata_privacy, relay_reachability and time_ordering_events documents that `python3 tools/design_gate.py needs` prints for the two code paths; ADR-0127, ADR-0141, ADR-0168 and the other recovery ADRs (only cited through the documents above); the Home endpoint code. Consequence: nothing below is placed in `spec/` or `crates/`. The wire object is specified in the appendix, and the implementing session moves it to `spec/cddl/` after reading those domains (the way ADR-0189 did), and re-checks every "builder confirms" item.

## Context

After a lost-all-devices recovery the account's control chain gains a link signed by the recovery method alone. Each verifier must wait its own full delay from its own first valid observation of the request and see no veto (ADR-0131 section 6, ADR-0147 section 5, ADR-0154, ID-03). The audited text says every verifier does this, "including every contact's devices", but names no way for a contact to **see** the request. A request is fanned out to the account's Home routes and live controllers (RECOVERY 10.3), and in a lost-all case there is no live controller. So today a contact keeps its pinned head forever, `control::verify_chain` rejects the recovery link (it needs verifier-local `RecoveryLinkEvidence` that only the observing device has), `first_contact_head` calls `verify_chain` and so refuses the recovered account, and "contacts see a continuity notice" (RECOVERY 1) cannot happen. Relaying another verifier's elapsed time is not an option: the delay is verifier-local so that a RecoveryRoot holder cannot hide a request and claim the window has passed.

Three facts make a small answer possible. The request is **self-authenticating**: it is signed by the recovery key in the policy, and the policy is committed by hash in the ACS the contact already pins. The policy and request carry **no time**. The contact **already has** the account's relationship routes, and the recovering person already restores a RelationshipSnapshot with the contacts' delivery capabilities (RECOVERY 10.1, `recovers = relationships`).

## Decision

### 1. One principle

Every contact is a verifier with its own durable record. **Evidence** (policy body, request, bridge states, successor, vetoes) may be carried by anyone over any path, because each item is checked against the contact's own pinned state. **Time** is never carried: the contact's interval starts at its own first valid observation, runs on its own monotonic clock, and is never read from a message, a Home, a custodian or a peer. No object defined here has a time field, and a parser rejects one.

### 2. What the contact needs, and the one object that carries it

A new unsigned, self-authenticating object **`RecoveryNotice`** (object type 34, appendix). It carries: the `RecoveryPolicy` body the request's parent state commits to; the `signed-RecoveryRequest`; up to 8 **bridge states** (ordinary signed ACS states from just after the contact's pinned head up to and including the request's `parent_state_hash`) with the credentials their signers need; optionally the recovery-signed **successor** (`child`, `next_policy`, the new device's `credential`, exactly the existing `SuccessorBundle`); and optionally vetoes already seen. Nothing in it is secret, and nothing in it is trusted before it verifies.

A contact opens it in this order, cheapest first, and creates **no state** until step 5 passes. All limits are checked before any signature:

1. the relationship is `Active`, not blocked, and has a pinned head of the account;
2. total size is at most 131,068 bytes, bridge at most 8 states, credentials at most 16, policy at most `MAX_POLICY_BYTES`, request at most `MAX_REQUEST_BYTES`;
3. bridge states are of the pinned account and chain from the pinned head's hash; each is verified with `verify_successor` and adopted by the existing `peer_refresh` rule (strict descendant only). A recovery link inside the bridge is not followed (it would need its own evidence); the bridge then stops before it;
4. the policy body hashes to the `recovery_policy_commitment` of the bridged `parent_state_hash` state, is `on`, and has a usable recovery key;
5. the request opens under the policy's recovery key (one composite signature check), names this account, the policy's epoch and that `parent_state_hash`, is not stale (`is_stale`) and its target device is fresh.

Then, and only then, the contact creates its record (section 4). A spoofed notice cannot reach step 5 without a genuine policy for the account's genuine head, and cannot make a record start earlier than the contact first saw valid evidence.

### 3. How a contact obtains it: three existing paths, no new authority

**3.1 Push by the recovering device, over the existing sealed 1:1 delivery.** The recovering device holds the RecoveryRoot, restores the RecoveryRecord and its RelationshipSnapshot, and so knows each contact's relationship route and the contact's delivery key record (RECOVERY 10.1, 10.2.1). For each contact it seals a `RecoveryNotice` in an ordinary `SealedDeliveryEnvelope` (type 23, unchanged buckets, padding and replay rules) to that contact's relationship route. The inner plaintext is the notice instead of an ordinary session envelope; it is told apart by its first map entry `0: 34` (builder confirms no collision with the ordinary envelope's first entry). The contact's mailbox and delivery key are per relationship, so the contact knows which relationship it arrived on. The outer HPKE open authenticates nothing (ADR-0142 section 7); section 2 is the authentication. This is **not** sending as the identity: the notice names no device credential as sender, carries no message and needs no session. The restricted state of RECOVERY 10.3 ("cannot send as the identity") is unchanged. The sender sends at request creation, again at one third and two thirds of the delay and when the successor is ready, and a fresh `delivery_id` each time, at most once per contact per 24 hours otherwise. A lost or expired contact record is the normal queue-and-refresh path (ADR-0142 section 9); it never falls back to clear delivery.

**3.2 Carried with the chain, so a stale or late contact is never stuck.** Wherever a device already sends its account chain (the `KIND_ACS_UPDATE` payload, the first-contact binding chain, the device-link transfer), it adds an optional **dossier** for each recovery link in that chain: for each such link `{policy, next_policy, request}` (a `RecoveryDossierEntry`, appendix). It never adds an elapsed time. A receiver that already tracks the request uses it; one that does not treats it as a notice without bridge and successor (section 2 steps 4 and 5, with the chain's own states as the bridge).

**3.3 Live controllers, in-session.** A live controller that vetoes pushes the `signed-RecoveryVeto` to every active contact over the ordinary session (new in-session kind `RECOVERY_VETO`). A contact may ask the same devices over the session (`RECOVERY_SWEEP`, section 7). A controller that co-signs a successor ("That was me") sends the co-signed successor the same way; this is the immediate path and needs no interval, but still needs the policy and request from 3.1 or 3.2.

**3.4 If no path reaches a contact.** The snapshot is missing (`recovers` lacks relationships), or the route has expired. The contact keeps its pinned head (fail closed). The recovering person sees "Couldn't reach 3 people yet", and reconnects by the ordinary invite/re-introduction path (RECOVERY 10.7). A re-introduction arrives as a first contact from a known account: `first_contact_head` treats a chain with a recovery link as a late joiner (section 8). No directory, no Home read of contacts, no new metadata: a Home sees one more padded sealed deposit on a route it already serves.

Group-only members (no 1:1 relationship) are reached by the same mechanism through the person's snapshot routes where one exists; otherwise they treat the new device exactly as an unknown device (ADR-0189 enrollment notice list) until a notice reaches them. Groups get no separate recovery protocol here.

### 4. Contact-local states

Per relationship, at most two tracked recoveries (digest-keyed). The record reuses `RecoveryObsRec` (digest, raw request, `first_seen_ms`, boot session, last monotonic reading, `credited_ms`, vetoes) plus the policy and next-policy bodies, the held successor, and a local `decision`. States:

| State | Meaning | Contact sees |
|---|---|---|
| `waiting` | valid evidence seen; own interval running, no veto | one calm banner: "<Person> is recovering their account on a new device. About N days to go." |
| `ready` | interval ran, no veto, but a step is missing: the successor, the ADR-0154 sweep, or (Hardened/Maximum) the contact's own confirmation | banner with the one action that is missing |
| `accepted` | verified like a completion with this verifier's own interval: pinned head becomes the successor | a notice, then ordinary chat |
| `vetoed` | a valid veto by a live controller was observed | "their own device stopped this" |
| `declined` | the contact chose "This isn't them" | local only |
| `stale` | the account moved on (the pinned head changed, the target was enrolled or revoked) | dropped, no message |

Transitions are exactly those of `recovery_watch.rs` for a controller (`Waiting`, `Vetoed`, `Stale`, `Completed`), plus `ready` and `declined`. `accepted` runs `verify_recovery_successor(parent = pinned head, policy, next_policy, request, observation = {this verifier's credited seconds, vetoes it holds})`, then replaces the pinned head, credentials and relays like `peer_refresh`, stores a `RecoveryLinkRec` for the relationship (section 8), and runs `retry_held`. A veto, a stale head or a failed verification can only keep or end `waiting`; nothing ever shortens it.

**Profiles.** The delay is the policy's own (Recommended 72 h, Hardened and Maximum 7 d); a profile never lowers it (RECOVERY 10.6, ADR-0037). Recommended: the contact accepts when `ready` conditions hold, without asking, and then shows the `accepted` notice. Hardened and Maximum Privacy: the contact must also tap "Yes, it's them" after the wait, and the banner shows "Check with <Person> another way" from the first day; neither uses the ADR-0154 gap credit (already so). The contact-local decline exists on every profile and is never needed in the common case.

### 5. What a contact does with messages from the new device while a recovery is pending

**Hold, hide, ask nothing of the sender, trust nothing.** The new device's DeviceID, named in the request, is not in the pinned head, so its wrappers are indeterminate (ADR-0144, K12): they go to the existing bounded held list (8 wrappers, dropped when their own expiry or the 7-day hold limit passes), unprocessed, with no plaintext, no receipt, no typing signal, no notification beyond the banner, and no counting toward integrity failures. While a recovery is `waiting`, `ready`, the new device also gets no key-material fetch authority, no group Add or Admit authority, and no merge into the person's record. Held wrappers are processed by the existing `retry_held` after `accepted`, and dropped on `vetoed`, `declined` and `stale`. A wrapper that expired during a long wait is simply gone: the sender's ordinary retry seals a fresh one (this ADR does not lengthen any wrapper lifetime). Messages the contact writes meanwhile are queued locally ("will be delivered when <Person>'s new device is confirmed"); they are never encrypted to a device the pinned head does not list (K5). The contact is never shown content from an unverified device, and never silently switched to the new device.

### 6. The timer: durable, verifier-local, no clock origin to forge

- **Start.** The record is created once, at the contact's first valid observation (section 2 step 5), with `first_seen_ms` from its own clock for display only. A later copy of the same digest never resets it; a different digest is a different request with its own interval.
- **Credit.** `credit()` of `recovery_watch.rs` unchanged: monotonic readings within one boot session, nothing for downtime between sessions, a backward reading credits nothing. The ADR-0154 guarded wall-assisted gap credit for `process_only` Recommended web verifiers, and the final sweep it requires, apply unchanged when the platform adapter provides them; nothing new is invented here.
- **Durability.** The record is written in the same staged transaction as the observation, and re-checkpointed at every tick (existing `recovery_tick` pattern). A crash can only lose uncheckpointed credit (later, never earlier). A lost, corrupt or rolled-back record restarts the interval at the next valid observation, which the sender's resends (3.1) and the dossier (3.2) supply.
- **Restore.** `peer_recovery` records are **inert history after a restore from backup**, like bootstrap state (ADR-0147 section 4): never merged into the live path, never credited. A restored device re-observes and restarts.
- **No forgery.** The notice has no time field and unknown fields are rejected. The request has none either (only a nonce). Home receipt times, the sender's clock, the custodian, another verifier's elapsed time and the contact's own wall clock (outside ADR-0154's guarded rule) never credit anything. A sender that delays a notice only delays the contact. A sender that replays an old request after the account moved on gets `stale`. The sender cannot choose the start: only the contact's first valid observation does, and an earlier one is impossible because the evidence includes the request digest the veto-holders can name.

### 7. Veto, and the authenticated sweep

A valid `RecoveryVeto` (signed by a live controller of the contact's pinned head, under the credential that head commits to, naming this digest and epoch) ends the request for this contact (`vetoed`). Vetoes arrive by 3.3, inside a notice, or in a sweep reply.

The **sweep** is the concrete form of ADR-0154's "authenticated refresh of the recovering account's control state from at least one of its known routes", and a best-effort check for everyone else. The contact sends `RECOVERY_SWEEP {request_digest, held_head_hash}` over the ordinary session to each control device its pinned head lists as live. A live device answers over the same session with its current chain (`KIND_ACS_UPDATE` shape) and any veto it holds for the digest. The sweep is **complete** when at least one authenticated answer arrived after this contact's interval ran out. A superseding head or a veto in the answer ends `waiting`. For a verifier that used gap credit, an incomplete sweep keeps the request `ready`, never accepted (ADR-0154). For every other verifier an incomplete sweep blocks nothing: in a lost-all case there is nobody to answer, and the interval alone decides, as ADR-0147 requires. See "Not decided here" 2.

The contact-local **decline** is the contact's own veto, with a deliberately narrow effect: it ends that request for this contact only, keeps the old pinned head, and is not a message, a vote or a statement to anyone else. It adds no authority over the account.

### 8. Late-joining verifiers and chain history

A verifier that first meets the account after the recovery (a new contact, a re-introduction, a restored profile) has no earlier head to protect, but the audited text says a verifier that has not itself observed a recovery-alone request waits its own interval (ID-03). This ADR applies that text as written: `first_contact_head` and `peer_refresh` stop at the last state before an unevidenced recovery link (`verify_chain_partial`, section "Core work"), the dossier of 3.2 supplies the policy and request, and the verifier starts its own interval at the first sight of that evidence. Until it passes, the relationship is `AcceptedUnbound` (ADR-0131 section 8): the intro profile may be shown, no stable-account history is attributed, no key-material fetch is granted. A co-signed link needs no interval. A chain with more than 4 recovery links, or a dossier that does not match its links, is refused. Each accepted link leaves a per-relationship `RecoveryLinkRec` (policy, next policy, request, **this verifier's** credited seconds: local, never sent), so `verify_chain_with` can replay the peer's chain from stored evidence; the dossier a verifier forwards later never contains seconds.

### 9. Where the default stays easy

Most people with another device never reach any of this (a device-link ceremony or an immediate co-signed recovery needs no interval). In the lost-all case the owner starts one recovery; a contact sees one banner with one action ("Check with <Person>"; "This isn't them" behind a menu on Recommended, visible on Hardened and Maximum), then a one-line notice when it ends. No contact is asked a question on Recommended. The contact never sees policy bodies, digests, epochs or intervals, except behind Details.

### 10. Metadata

A contact learns, only while a recovery is pending or after it completes, the account's recovery policy body (method kinds, the per-epoch public recovery key, delay, flags) and that a recovery was requested. It already knows the account, its heads and the relationship route. The policy is sent only by the account's own recovery device or live controller, only on that relationship's route, and only after a request exists. The Home still sees only a sealed deposit. The metadata descriptor for recovery fan-out (RECOVERY 10.3) gains "to each reached contact, operation-scoped, only during a recovery" (builder adds it).

## UI copy keys (voice rules of [`CONTENT_AND_MICROCOPY_VOICE.md`](../docs/ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md): object or state, what it means, safe next action; "safety code" and "delivery service", no protocol words)

New group `recoveryCopy.contact` in `apps/web/src/copy/en.ts`, type in `@om/ui` `RecoveryCopy`:

| Key | Text (`{name}`, `{days}`) |
|---|---|
| `waiting.title` | "{name} is getting their account back on a new device" |
| `waiting.body` | "OM waits about {days} so no one else can take it over. Until then, anything from their new device stays on hold. Messages you write will be delivered once the new device is confirmed." |
| `waiting.check` | "Check with {name}" (opens: "Call them or meet them to be sure it's them. Their other devices can stop this if it isn't them.") |
| `waiting.decline` | "This isn't them" |
| `ready.title` | "The wait is over. Is this really {name}?" (Hardened and Maximum only) |
| `ready.confirm` / `ready.notSure` | "Yes, it's them" / "Not sure yet" |
| `ready.checking` | "One more check is needed before this can finish." (incomplete sweep, gap credit only) |
| `accepted` | "{name} is back on a new device. Your chat and earlier messages are unchanged. Their security code changed." |
| `accepted.verifyAgain` | "Verify {name} again" (only when previously verified) |
| `vetoed` | "{name}'s own device stopped this. Nothing changed in your chat." |
| `declined` | "You turned this down. Your chat with {name} is unchanged. They can ask again." |
| `provisional.title` | "{name}'s account was recovered recently" |
| `provisional.body` | "OM is double-checking for {days} before you can chat. Nothing is lost." (late joiner) |
| `sender.unreached` | "Couldn't reach {count} people yet. OM keeps trying." (recovering device) |
| `sender.reconnect` | "Reconnect with a QR code" |

Plural, accessible names and the banner placement follow the existing recovery screen and the conversation header; no new component.

## Not decided here (the operator's call)

1. **Late joiners wait.** Section 8 applies ID-03 literally. If the operator reads ID-03 as concerning only verifiers that already pin an earlier head, a first contact with a recovered account could instead accept an unevidenced recovery link by first-contact trust, with a "recovered" detail. That shortcut changes audited timing text, so it is not taken here.
2. **No readable surface for the sweep when nobody is alive.** ADR-0154's final sweep needs an authenticated answer from "a known route". No design text says what answers when the account has no live device (the lost-all case), and Homes are not a time or veto authority (ADR-0131 section 6). A web contact that used gap credit therefore cannot complete its sweep in that case and falls back to the strict interval. A readable veto-or-head record on the Home would fix it but is new Home surface and is not invented here.
3. **Reading of "cannot send as the identity".** Section 3.1 reads it as "cannot send messages or act as a device of the account", so a request-bound, RecoveryRoot-authenticated notice is allowed. If that is read more strictly, only 3.2 and 3.4 remain, and contacts of a lost-all account hear only after re-introduction.
4. **`notify_contacts_on_completion`.** The code default is `false` while RECOVERY 1 says contacts see a continuity notice. This ADR makes authentication independent of the flag (a contact cannot verify without the notice) and lets the flag govern only whether the completion notice text is shown. The default value is for the operator.
5. **Change class (ADR-0165).** The notice and the dossier field are new objects and a new optional field, with safe old-client behavior (unknown inner is a counted failure, an unknown field is ignored, the contact keeps its pinned head). They are classed `additive_noncritical_extension`. A reviewer who classes the new sealed inner as changing the sealed-delivery inner contract must escalate it.

## Consequences

Contacts can accept a recovery-alone successor after their own interval, with no trusted clock, no new service, no directory and no change to authority; an attacker holding only the recovery key gains no speed. The cost is availability: a contact that is offline, behind, unreachable or on a strict profile waits longer, and a contact who is never reached keeps the old pinned head. The recovering person has to keep OM open enough to resend (or re-introduce).

## Core work (list for the builder; no code in this ADR)

**Types** (`state.rs`, `api.rs`): `PeerRecoveryRec { obs: RecoveryObsRec, policy, next_policy, successor: Option<RecoverySuccessorRec>, decision: Option<Declined|Confirmed>, sweep_asked_ms, sweep_answered, gap: Option<GapCredit> }` and `PeerRecoveryState { Waiting, Ready, Accepted, Vetoed, Declined, Stale }`; `Relationship.peer_recovery: Vec<PeerRecoveryRec>` (max 2) and `Relationship.peer_links: Vec<RecoveryLinkRec>`, both `#[serde(default)]`; a `peerRecovery` field on the relationship/conversation view (`state`, `activatesAt`, `daysLeft`, `canDecline`, `needsConfirm`); intents `DeclinePeerRecovery { conversation_id, request_id }` and `ConfirmPeerRecovery { conversation_id, request_id }`, both added to `DESTRUCTIVE_KINDS` (they end or accept an authority change).

**Wire and events:** `RecoveryNotice` (appendix); optional `recovery_dossier` key on the `KIND_ACS_UPDATE` payload and on the first-contact binding chain and the device-link transfer; two in-session kinds `RECOVERY_VETO` and `RECOVERY_SWEEP` (first free numbers above 11); a `PeerRecoveryChanged { conversation_id, state }` event beside the existing recovery events, so the UI and notifications can follow. Sender side in `recovery_watch.rs`: `send_recovery_notices` (3.1) with the resend schedule and a `Purpose` for it; it reads the restored snapshot, never a Home list of contacts.

**Core changes:**
- `control.rs`: `verify_chain_partial(chain, credentials, links) -> (Acs, Option<PendingLink>)` returning the last head before the first recovery link that has no evidence or is not yet complete, with what is missing; `verify_chain` and `verify_chain_with` keep their signatures and behavior. `RecoveryLinkEvidence` stays verifier-local; add `dossier_of(chain, links) -> Vec<RecoveryDossierEntry>` that never includes seconds.
- `flows.rs`: `first_contact_head` calls `verify_chain_partial`; a pending link makes the relationship `AcceptedUnbound` with a `waiting` record (section 8). `on_session_envelope` hands a `0: 34` inner to `observe_recovery_notice` before the session path.
- `flows/multi.rs`: `peer_refresh` uses `verify_chain_partial`, adopts the prefix, never fails the refresh because of a recovery link, and routes the dossier to `observe_recovery_notice`; hold rule of section 5 in the held-wrapper decision.
- `flows/recovery_watch.rs`: `observe_recovery_notice(conv, bytes, mono)` (section 2 order), `observe_peer_veto`, `peer_recovery_tick` (credit, stale check, accept), `peer_recovery_sweep` (send and collect), `decline_peer_recovery`, `confirm_peer_recovery`; reuse `credit`, `is_stale`, `verify_held`'s checks; `finish_peer_recovery` replaces the pinned head like `peer_refresh`, stores the `RecoveryLinkRec`, runs `retry_held`, and emits the event. `broadcast_account_update` and the multi-device update add the dossier from `recovery_links`.
- Backup/restore: `peer_recovery` and the web gap state are excluded from restored live state (ADR-0147 section 4).

**Tests (Rust, `crates/om-core/tests/recovery_peer.rs`):** a contact with a pinned head and a notice sealed to its relationship route reaches `waiting` and holds the new device's wrapper unseen; the interval runs on monotonic credit only and a wall-clock jump, a backward reading, a Home timestamp and a second delivery of the same digest do not move it; accept only after the full delay plus no veto, then `retry_held` shows the held message; a veto (pushed, inside a notice, in a sweep reply) ends it; a veto by a non-controller, a quarantined device or for another digest does not; crash and restart mid-wait resumes from the checkpoint, a dropped record restarts the interval; a restored backup carrying a `waiting` record does not credit it; a stale notice (head moved on, target enrolled) is `stale`; a policy that does not hash to the committed one, a request signed by another key, a request for another epoch or parent, an oversize or time-carrying notice, and a bridge with a recovery link are refused with no state change; a contact behind by two ordinary states is bridged; a late joiner with a dossier waits its own interval and stays `AcceptedUnbound`; a co-signed link needs none; a chain of 5 recovery links is refused; `verify_chain_with` replays a peer chain from stored evidence and a dossier never leaks seconds; Hardened requires confirm, Recommended does not; decline keeps the old head; sweep: incomplete blocks only a gap-credit verifier; no sender notice goes out without a snapshot route and none falls back to clear delivery; `peer_refresh` still rejects a fork and an older head. A mutation test per rule of section 6 (reset on redelivery, credit from a field, accept at delay minus one tick).

**Spec and vectors (ADR-0165, builder, after reading the unread domains):** move the appendix CDDL to `spec/cddl/` (new `recovery-notice.cddl` or [`account.cddl`](../spec/cddl/account.cddl) additive), add the `RecoveryNotice` row to [`om-object-types.json`](../spec/registries/om-object-types.json) (type 34, `max_bytes` 131068, unsigned, source this ADR) and a logical schema, the in-session kinds where they are registered, a two-codec round trip, positive and negative vectors (every refusal above, and "unknown field refused"), the metadata-descriptor line of section 10, and a threat-model line next to T-041 for a late-delivered notice. No new domain-separation label is needed: signatures and hashes are the existing request, veto and policy ones.

**UI (`reference_client_ui`):** the `recoveryCopy.contact` keys above; one banner component in the conversation header driven by `peerRecovery`; actions per profile of section 4; tests that no policy, digest or epoch text appears outside Details and that no content of a held message is rendered.

## Where did authority go?

Nowhere new. Every accepted fact is verified against the contact's own pinned ACS, the policy it commits to and the request's own signature. The notice is not a vote, a time source or a directory: it carries evidence. The contact's decline is local. A Home, a relay, a custodian, a peer and the recovering device itself still cannot start, shorten or complete a contact's interval.

## Appendix: CDDL (to move to `spec/cddl/` by the builder)

```cddl
; type 34, unsigned, self-authenticating (ADR-0190 section 2). Carried as the inner plaintext of a
; SealedDeliveryEnvelope (type 23). No time field exists; unknown keys other than 23 are refused.
RecoveryNotice = {
  0: 34, 1: 1,
  2: bstr .cbor RecoveryPolicy,             ; the body the request's parent state commits to
  3: signed-RecoveryRequest,
  ? 4: [0*8 signed-AccountControlState],    ; bridge: pinned head (exclusive) .. parent_state (inclusive)
  ? 5: [0*16 bstr .cbor DeviceCredential],  ; credentials the bridge signers need
  ? 6: RecoverySuccessorBundle,             ; the recovery-signed child, not yet final anywhere
  ? 7: [0*16 signed-RecoveryVeto],
  ? 23: extensions,
}
RecoverySuccessorBundle = {
  0: signed-AccountControlState,            ; child
  1: bstr .cbor RecoveryPolicy,             ; next_policy
  2: bstr .cbor DeviceCredential,           ; the new device's credential
}
; Optional key 5 of the KIND_ACS_UPDATE payload and of the first-contact binding chain: one entry per
; recovery link in the chain that follows. Never carries seconds, never carries a time.
RecoveryDossierEntry = {
  0: digest,                                ; child state hash of the link
  1: bstr .cbor RecoveryPolicy,
  2: bstr .cbor RecoveryPolicy,             ; next_policy
  3: signed-RecoveryRequest,
}
```
