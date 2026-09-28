# ADR-0148 — Promote low-risk product foundations into OM V1 without expanding kernel authority

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** Run 6A V1 product-base promotion; normative design only, with executable/conformance evidence deferred to Run 6B
- **Amends:** ADR-0036 software-update trust, ADR-0060 IdentityProfile UI sequencing, ADR-0088 messenger baseline, ADR-0139 multi-device implementation boundary
- **Does not amend:** OM-Crypto-v1, AccountControlState authority, the frozen W0 ConversationEvent body grammar, MLS/group security, IntroGate, relay authority, or provider trust semantics
- **Amended by:** ADR-0149 adversarial hardening of extension registration/fallback equivalence, private-sync causality/restore, profile execution context and release-signature authority binding

## Context

The Run-5B package was architecture-closed enough for external review, but a V1/V2 boundary review found several features that had been deferred mainly for sequencing rather than because they required new authority or difficult protocol machinery. Leaving all of them for later would make the first engineering base unnecessarily spartan and would force product work to rediscover boundaries that are already designed.

The useful promotion test is strict: move a feature into V1 only when it can reuse an existing frozen trust boundary, has a safe old-client fallback, does not require a new cryptographic primitive or mandatory project-operated service, and can be verified without reopening the W0 kernel wire objects.

Five items satisfy that test:

1. visible multi-IdentityProfile switching in the reference V1 client;
2. bounded account-private encrypted synchronization for drafts and selected non-authoritative preferences;
3. complete voice-note presentation using an optional event extension beside an ordinary audio attachment;
4. one-shot static location cards using an optional event extension beside a W0-valid textual fallback; and
5. algorithm-agile, post-quantum-ready software-release signatures from the first public-release format.

Live location, SPQR/Triple Ratchet, PQ MLS, cryptographic group deniability, stories/status, guaranteed server-side scheduled sending, payments, mini-apps, massive private broadcast crypto and erasure-coded StoragePools remain later work.

## Decisions

### 1. Visible multi-profile switching is a V1 reference-client capability

OM Core and storage remain profile-namespaced exactly as ADR-0060 already requires. The sequencing change is that the reference V1 client no longer hides that capability indefinitely: when two or more IdentityProfiles are configured on one installation, the reference client exposes an explicit profile switcher.

Switching profiles is presentation/navigation state only. It MUST NOT:

- log out, re-enroll or rekey an account merely to switch views;
- copy credentials, sessions, drafts, contacts, search state, provider handles or decrypted caches between profiles;
- create a public stable installation/person identifier linking profiles;
- infer the target profile for an unknown deep link from hidden cross-profile heuristics; or
- weaken the existing per-profile push-linkage disclosure and Keep-separate behavior.

The active profile is local installation state and is not itself synchronized as an account preference. Each profile preserves its own compose/navigation state. A per-profile theme remains optional local/account-private presentation data, never public identity state. ADR-0149 further requires every profile-sensitive asynchronous operation to capture an immutable `ProfileExecutionContext`; switching the visible profile can never retarget an already-dispatched operation.

Third-party conforming clients may intentionally expose only one profile at a time, but they must preserve the same storage/key namespace isolation. This ADR makes multi-profile switching a **reference V1 product baseline**, not a new peer interoperability requirement.

### 2. V1 account-private sync includes drafts and an explicit preference allowlist

The already-settled authenticated E2EE account-private sync channel is used in V1 for a bounded set of non-authoritative records between currently eligible devices of the same IdentityProfile.

The V1 minimum allowlist is:

- unsent message drafts, including text/caption plus reply/thread/topic compose context;
- mute state;
- personal archive/folder/label organization;
- personal bookmarks/pins that are not shared Room pins;
- topic/thread follow state; and
- the profile's ordinary appearance/theme selection when the selected theme exists on the receiving device.

Implementations MAY add other clearly non-authoritative account-private preferences only when they fit the same boundary.

The sync channel MUST NOT be used as an alternate authority path for:

- AccountControlState, RecoveryPolicy, controller votes or recovery activation;
- CryptoProfile/security floors, credential validity, prekey/session/ratchet/MLS authority or replay state except where another canonical protocol explicitly defines monotonic private-state synchronization;
- provider-resource credentials/capabilities, push tokens/endpoints or deployment credentials;
- app-lock/PIN/biometric secrets, DeviceVault wrapping keys or RecoveryRoot material;
- per-device notification-preview/privacy posture, active-profile selection or other settings whose meaning is intentionally device-local; or
- any operation that sends a message, accepts a request, grants authority or changes shared conversation state merely because a preference record merged.

Preference convergence is typed per record; there is no generic provider-owned merge rule. Ordinary scalar preferences may use deterministic last-writer semantics under an account-private logical/HLC ordering, while set-like preferences may use an appropriate deterministic set merge. Providers transport opaque ciphertext and have no merge authority.

Drafts receive a stronger loss-avoidance rule: concurrent unsent draft edits from different eligible devices MUST NOT be silently discarded by a single timestamp winner. An implementation may preserve concurrent branches, perform a deterministic content merge, or surface a conflict for user resolution. It may compact superseded draft branches only after their contents are represented by the surviving resolved draft. Draft synchronization never implies send authorization.

Revoked/ineligible devices receive no new sync records after revocation propagation, and profile A records can never merge into profile B even on the same installation.

ADR-0149 further freezes causal anti-resurrection semantics: authenticated per-origin sequence/causal context, tombstone-wins for concurrent stale state, draft-resolution parent binding, bounded treatment of remote-future HLC, and historical-restore non-republication under a fresh DeviceID.

The exact record transport/codec remains implementation-owned behind the authenticated E2EE sync interface, consistent with ADR-0139. Run 6B supplied first-order convergence/isolation evidence; ADR-0149 deliberately reopens bounded adversarial evidence for Run 7B rather than freezing a second general-purpose CRDT wire protocol.

### 3. Voice-note presentation is V1 through a safe optional extension

A voice note remains an ordinary encrypted audio `AttachmentRef` with `duration_ms`; no new transport or cryptographic message class is created.

V1 standardizes the optional, non-critical event extension **`om.voice-note.v1`**. It is carried in ConversationEvent key 23 beside an otherwise valid W0 `message.create` body. ADR-0149 registers it canonically and hardens the deterministic-CBOR payload to the integer-keyed `VoiceNoteV1Extension` struct:

```text
{
  0: attachment_index uint,          ; 0..31, indexes immutable MessageCreate.attachments
  ? 1: waveform_u8 bstr              ; 16..256 normalized amplitude samples
}
```

Rules:

- the referenced attachment MUST be audio and MUST carry `duration_ms`;
- `waveform_u8`, if present, is an untrusted presentation hint; clients MAY regenerate it locally;
- the extension is non-critical and MUST NOT be listed as a required capability for ordinary playback;
- a receiver that ignores or does not understand the extension still presents a normal playable audio attachment; and
- malformed/out-of-range metadata is ignored/rejected as extension metadata without reinterpreting the attachment's security semantics.

The V1 reference UX includes record/lock/cancel, pause/resume, waveform display when available, scrubbing and playback-speed controls, subject to platform capability/accessibility rules.

### 4. One-shot static location cards are V1; live location is not

V1 standardizes the optional, non-critical event extension **`om.location-card.v1`**. It carries a point the sender explicitly chooses to share once; it is not a tracking session and grants no background-location capability.

ADR-0149 registers the extension canonically and hardens its deterministic-CBOR value to the integer-keyed `LocationCardV1Extension` struct:

```text
{
  0: lat_e7_offset uint,              ; 0..1800000000; signed lat_e7 = value - 900000000
  1: lon_e7_offset uint,              ; 0..3600000000; signed lon_e7 = value - 1800000000
  ? 2: accuracy_m uint,               ; 1..1000000
  ? 3: label tstr                     ; 1..256 UTF-8 bytes
}
```

The containing `message.create` MUST also contain the exact canonical `geo:<lat-fixed-7>,<lon-fixed-7>` token derived from those coordinates in its W0-valid RichText fallback. A receiver that finds a nonempty but mismatched fallback ignores the non-critical location extension and renders the base W0 message, so old and new clients never authenticate two different points from one event.

Additional rules:

- sharing requires an explicit user action; clients do not silently convert photo EXIF/current device location into a card;
- location data receives ordinary E2EE message protection and ordinary forwarding/copying caveats;
- a map renderer is presentation only and must not make a third-party map request unless the user's configured map/privacy policy permits it;
- event expiry, if present, is the ordinary message expiry policy; it does not create a live-location session; and
- continuous/live location remains deferred pending a separate background-access, expiry and coercion threat model.

No W0 `MessageCreate` or `AttachmentRef` field is added.

### 5. Software release metadata is algorithm-agile and PQ-ready from V1

Distribution trust remains separate from OM protocol identity. This ADR strengthens the **format and verifier boundary**, not the protocol's peer authentication rules.

From the first public-release format:

- root/targets/release verification MUST support algorithm-tagged multiple signatures rather than hard-code one signature algorithm;
- the owner-pinned root policy, not an application default or remote server, decides which authorized **role + key ID + algorithm + threshold** tuples are required for that distribution; ADR-0149 forbids satisfying policy by algorithm tags alone;
- an unknown signature may be ignored only when it is optional under the already-trusted root policy; an unknown **required** algorithm fails closed;
- adding a second algorithm can begin as an optional co-signature and later become mandatory only through an authenticated root-policy transition;
- once a root policy requires an additional algorithm, stripping that signature or presenting older metadata that omits the requirement is a downgrade and MUST fail under the existing rollback/freeze rules; and
- root rotation can replace/deprecate algorithms without changing AccountID, OM protocol generation or another distribution's trust root.

The reference distribution SHOULD emit ML-DSA-65 co-signatures for long-lived root and release/targets metadata once a maintained, reviewed implementation is suitable for the release toolchain. Until the pinned root policy actually requires that algorithm, the co-signature is defense-in-depth evidence rather than a new universal acceptance condition. Online freshness roles may remain classical initially when that materially simplifies safe operations.

This makes V1 migration-ready without forcing immature PQ release tooling onto every third-party distributor.

## Run 6B verification obligations

Run 6A changes normative scope but intentionally does not claim the new evidence is already complete. Run 6B must close these local obligations before the consolidated external-review handoff is reactivated:

1. **V-021 / profile switching:** prove profile switch/deep-link/cache/draft/notification paths cannot return profile-A state in profile B and that switching does not rekey/logout or silently link profiles.
2. **V-003 / account-private sync:** exercise same-profile eligible-device synchronization, concurrent draft conflict preservation, deterministic typed preference convergence, revoked-device exclusion, forbidden authority-record injection and cross-profile merge rejection.
3. **V-005 / extensions:** add positive/negative deterministic-CBOR vectors for `om.voice-note.v1` and `om.location-card.v1`, including safe W0 fallback, bounds/malformed cases and unknown/non-critical-extension behavior.
4. **V-023 / update signatures:** exercise multi-algorithm metadata, optional co-signature handling, root-policy activation and fail-closed stripping/downgrade after an algorithm becomes required.
5. Run the ordinary architecture/docs/continuity/integrity checks and search for stale statements that still describe these promoted items as later/V2.

Until those obligations are recorded, the different-vendor review brief is paused and engineering remains unauthorized.

## Consequences

This promotion deliberately improves the starting product without broadening OM's core authority model:

- multi-profile switching exposes isolation that already exists;
- draft/preference sync uses the already-designed account-private E2EE boundary;
- voice notes and static location use optional extensions with W0-valid fallbacks;
- software-signature agility belongs to distribution trust and does not become a universal OM root; and
- the expensive/security-sensitive post-V1 items remain deferred.
