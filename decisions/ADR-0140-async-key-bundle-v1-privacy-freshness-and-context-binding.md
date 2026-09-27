# ADR-0140 — AsyncKeyBundle v1 privacy, freshness and one-time-prekey context binding

- **Status:** Accepted architecture remediation; reopens and re-freezes the affected W0 key-material design only
- **Date:** 2026-09-27
- **Author:** GPT-5.6 Sol architecture-remediation pass
- **Amends:** ADR-0119, ADR-0126, ADR-0131, ADR-0135, ADR-0138, ADR-0139
- **Supersedes:** ADR-0138 §2's decision to defer account-unlinkable AsyncKeyBundle semantics to a later object version
- **Scope:** architecture/specification and bounded verification artifacts only; no product engineering authorization is implied

## Context

A post-closure review found three coupled defects in the frozen W0 asynchronous-key design:

1. `AsyncKeyBundle` exposed `acs_generation` and `acs_state_hash` to every KeyMaterialProvider even though an accepted relationship already carries the authenticated peer AccountControlState needed to authorize the device;
2. the signed-prekey `generation` had no normative recipient-side high-water mark, so a provider could replay an older, still-valid signed bundle after a recipient had already accepted a newer generation; and
3. an individually signed one-time `PrekeyItem` authenticated only `(key_id, kind, public_key)`, not the device/profile/generation/provider-pool context in which the item was issued.

These are pre-deployment wire/security defects. There are no deployed OM v1 clients whose compatibility outweighs fixing them. The affected W0 bytes are therefore deliberately amended once now rather than carrying known metadata and rollback debt into the first implementation.

## Decision

### 1. The privacy design formerly called `AsyncKeyBundle v2` is OM v1

`AsyncKeyBundle` remains object type 9, object version 1, but its W0 body is re-frozen without provider-visible AccountControlState fields:

```text
0   object_type = 9
1   object_version = 1
2   subject_device_id
3   RESERVED — retired acs_generation; MUST be absent
4   RESERVED — retired acs_state_hash; MUST be absent
5   crypto_profile
6   generation                 # signed-prekey generation
7   signed_prekey              # EC signed prekey
8   pq_signed_prekey           # mandatory for OM-Crypto-v1
9   optional one_time_prekeys
10  optional pq_one_time_prekeys
11  optional pq_last_resort_prekey
12  provider_pool_id
13  expires_at
23  optional extensions
```

Registry assignments 3 and 4 are never reused. Their old meanings remain historical registry facts, but a current v1 decoder rejects their presence as reserved/unknown fields.

The bundle remains signed by the subject device under `OM-PREKEY-BUNDLE-SIG-v1`. The fetcher validates the signing DeviceCredential against the **relationship-pinned trusted peer AccountControlState** before accepting any key material. A KeyMaterialProvider never decides device authorization and no stable account-wide pseudonym replaces the removed ACS fields.

The authorization sequence is therefore:

```text
accepted RelationshipContext
  -> trusted peer AccountControlState head
  -> committed active/non-quarantined DeviceCredential
  -> composite device signature on AsyncKeyBundle
  -> generation freshness checks
  -> context-bound one-time-prekey checks
  -> PQXDH/session establishment
```

First contact remains `MessageRequest` followed by reciprocal `IntroIdentityBinding`; normal stable-account bundle fetching begins only after the relationship is ACTIVE and has an authenticated peer control head.

### 2. Signed-prekey generation is global per DeviceID + CryptoProfile

`AsyncKeyBundle.generation` is the signed-prekey generation, not a provider-local publication counter.

For one DeviceID and CryptoProfile:

- generation starts at a positive implementation-chosen value and **strictly increases** whenever the EC signed-prekey or PQ signed/last-resort prekey pair rotates;
- a rotation generates fresh signed-prekey secrets and fresh key IDs; re-labeling the same signed-prekey secrets as a higher generation is non-conformant;
- replenishing one-time keys or changing provider pools does **not** reset or decrease the generation;
- every provider pool serving the same device/profile observes the same generation namespace;
- `revokeGeneration()` is an operational deletion hint only. It is not the freshness security boundary.

A recipient persists a `BundleFreshnessFloor` keyed by:

```text
(peer AccountID, peer DeviceID, CryptoProfile)
    highest_generation
    generation_core_hash
```

`generation_core_hash` is `DS-HASH("OM-PREKEY-GENERATION-CORE-v1", OM-CBOR [DeviceID, CryptoProfile, generation, signed_prekey, pq_signed_prekey, pq_last_resort_prekey-or-null])`. It deliberately excludes `provider_pool_id`, one-time items and expiry so the same signed-prekey generation may be published through several disjoint provider pools.

Acceptance rules, before consuming a one-time item or doing PQXDH secret-key work:

1. `generation < highest_generation` -> reject as `prekey_generation_rollback`.
2. `generation == highest_generation` -> `generation_core_hash` MUST equal the stored hash; otherwise reject as `prekey_generation_equivocation`.
3. `generation > highest_generation` -> after all signature/credential/expiry checks pass, atomically raise the floor and store the new core hash before session establishment is considered successful.

The floor is relationship security state. It is persisted in DeviceVault and synchronized account-privately between the user's devices with a monotonic merge: larger generation wins; equal generation with different core hash is a security conflict, never last-writer-wins.

Provider-pool observations may additionally be cached as diagnostics, but a new/migrated `provider_pool_id` never creates a fresh rollback namespace.

### 3. Equal generation across providers is valid only for the same signed-prekey core

Multiple providers may legitimately publish different `provider_pool_id`, expiry values and disjoint one-time pools for one generation. They must nevertheless carry the same signed EC/PQ prekey core for that device/profile/generation.

If independently fetched valid bundles expose different signed-prekey cores at the same generation, clients treat it as device equivocation or corruption and fail closed for new establishment to that device until a later valid generation or authenticated device/control-state change resolves it.

Where more than one configured provider is reachable, a client should fetch available candidates and consider the maximum valid generation before establishment. This reduces single-provider rollback exposure but does not create a provider quorum authority.

### 4. One-time-prekey signatures bind their complete security context

The `PrekeyItem` wire shape stays compact:

```text
{ 0: key_id, 1: kind, 2: public_key, 3: signature }
```

For kinds 3 and 4 (one-time EC/PQ prekeys), key 3 is a composite device signature under `OM-PREKEY-SIG-v1` over this canonical context body:

```text
OM-CBOR [
  1,                  # prekey-context version
  subject_device_id,
  crypto_profile,
  bundle_generation,
  provider_pool_id,
  kind,
  key_id,
  public_key
]
```

The outer DS-SIG tuple also names `crypto_profile`, role `device`, and signer `subject_device_id`, so the device and profile are intentionally bound twice: once by the generic signature framework and once inside the object-specific context whose semantics are independently testable.

A fetched one-time item is accepted only together with a verified `AsyncKeyBundle` whose DeviceID, profile, generation and provider-pool ID exactly match the signed context. Cross-device, cross-profile, cross-generation and cross-pool transplantation therefore fails signature verification.

Signed EC/PQ prekeys in keys 7/8 are already covered by the composite signature over the entire AsyncKeyBundle and do not require a second item signature. If key 11 is used as separate profile-defined reusable fallback material, its semantics remain profile-defined and the bundle signature binds it to the same generation/pool.

### 5. Compromise and rotation semantics

If a signed-prekey private secret for generation `g` is suspected or known compromised:

1. the device creates fresh EC and PQ signed-prekey secrets at generation `g+1` (or greater), with fresh key IDs;
2. it publishes that generation to every configured provider pool and asks providers to revoke/delete `g`;
3. local devices synchronize the new publication/freshness state;
4. any peer that has accepted `g+1` permanently rejects a replay of `g` for that DeviceID/profile, even if the old bundle remains correctly signed and unexpired;
5. one-time prekeys signed for `g` remain bound to `g` and cannot be transplanted into `g+1`; after the device retires the old signed-prekey generation, unused private one-time material associated with `g` is deleted as well.

Provider deletion reduces exposure/availability of stale material but cannot make `g` invalid by itself; the recipient-side floor is what prevents rollback after a newer observation.

A peer that has **never** observed a generation newer than `g` cannot infer from a malicious provider alone that `g` has been superseded. The remaining first-observation exposure is bounded by bundle expiry, authenticated device revocation/quarantine, and independent-provider comparison where available. OM does not claim a stronger freshness oracle that it does not possess.

### 6. Restore and crash behavior

The publishing device's generation counter and signed-prekey secrets are durable DeviceVault state. Existing K11 remains stronger: restoring an old device backup does not resurrect an old DeviceID/prekey state; recovery creates a fresh DeviceID/material, so a publisher never rolls its signed-prekey generation backward under the same DeviceID.

For the fetcher, `BundleFreshnessFloor` is durable relationship security state and must be included in account-private state synchronization/backup with monotonic max merge. Restoring a snapshot must never overwrite a higher floor learned from surviving local devices or newer authenticated sync state.

If recovery has only an old snapshot and no surviving copy of a newer floor, the client cannot cryptographically reconstruct an observation it no longer possesses. It marks freshness as restored/stale, compares every reachable configured provider and accepts only the highest valid generation it can authenticate; expiry and current peer ACS/device eligibility still apply. This residual is documented rather than silently treating a provider deletion API as an anti-rollback oracle.

### 7. Interaction with AccountControlState changes

Removing `acs_generation`/`acs_state_hash` from the bundle does not weaken authorization:

- `RelationshipContext` supplies the expected peer AccountID and trusted peer head;
- the DeviceCredential digest must be committed by that head;
- the credential AccountID must match the peer AccountID;
- the DeviceID must be active and not revoked/quarantined where the profile forbids use;
- the bundle signature must verify under that exact credential.

If the relationship head advances while the DeviceCredential remains valid, the same unexpired bundle may remain usable subject to the generation floor. If the device is revoked/quarantined or its credential is replaced, the old bundle is rejected regardless of its generation.

### 8. Freeze/compatibility ruling

ADR-0119's immutability rule protects assigned numeric meanings; it does not require OM to ship a known-bad pre-deployment field set. This ADR makes a one-time pre-engineering correction:

- object type 9 and object version 1 remain unchanged;
- numeric keys 3 and 4 are retired/reserved and never reused;
- all other assigned keys retain their meanings;
- `OM-PREKEY-SIG-v1` is frozen with the context body in §4;
- `OM-PREKEY-GENERATION-CORE-v1` is added as a frozen W0 hash label;
- frozen vectors/validators are regenerated against these semantics.

Because no compatible OM v1 release exists, there is no legacy decoder mode and no dual-format negotiation. Accepting keys 3/4 “for compatibility” would recreate the privacy leak and is forbidden.

## Consequences

- The previously planned AsyncKeyBundle-v2 privacy improvement ships in v1.
- KeyMaterialProviders still see stable `subject_device_id`, pool identifiers, timing and inventory shape, but no longer receive a protocol-mandated stable account-wide ACS generation/hash correlator.
- A provider that replays an older bundle after the peer has seen a newer generation cannot steer that peer back to compromised signed-prekey material.
- One-time prekeys cannot be transplanted across device/profile/generation/provider-pool contexts.
- First-observation rollback cannot be eliminated without adding a separate freshness authority/transparency mechanism; OM explicitly bounds and documents that residual instead of inventing one.
- SPQR/Triple Ratchet, sealed delivery and unrelated later-wave features remain unchanged by this ADR.
- Architecture returns to frozen status for this slice only after CDDL/registries/prose/vectors/static checks agree with this ADR.
