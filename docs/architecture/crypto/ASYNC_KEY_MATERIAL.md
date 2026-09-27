# Asynchronous Key Material Publication

- **Status:** Canonical architecture; W0 key-material wire semantics re-frozen by ADR-0140
- **Checkpoint:** 2026-09-27 AsyncKeyBundle v1 remediation

## 1. Why this is first-class

Asynchronous encrypted session establishment requires recipient-device handshake material to be obtainable while the recipient is offline. OM therefore models key-material publication as an explicit replaceable provider capability rather than hiding it inside a crypto library or generic directory.

The service distributes authenticated public/one-time material. It is never account authority and never receives private identity/session keys.

## 2. Logical interface

```text
KeyMaterialProvider
  publishHandshakeMaterial(device, generation, bundle_set)
  fetchHandshakeMaterial(device, requested_profile)
  publishMlsKeyPackages(device, package_set)
  fetchMlsKeyPackage(device, group_profile)
  inventory(device)
  revokeGeneration(device, generation)
```

Providers return provider-signed/transport-authenticated receipts where useful, but recipient authenticity is proven by OM account/device authorization and cryptographic signatures/bindings, not provider identity. `revokeGeneration()` is an operational deletion hint; recipient-side freshness state is the rollback-security boundary.

### 2.1 Fetch authorization (ADR-0126, ADR-0140, R-209)

`fetchHandshakeMaterial` and `inventory` are not open to anyone who knows a device id:

- **Relationship fetch.** Fetching a device's bundle, and consuming one-time prekeys, needs a fetch capability the device owner issued: per relationship (exchanged inside the accepted relationship) or per IntroductionTicket (after acceptance of a MessageRequest). Capabilities are distinct per provider, like relay capabilities.
- **Anonymous fetch** returns only request-key material ([`INVITES_INTROGATE_AND_SPAM.md`](../discovery/INVITES_INTROGATE_AND_SPAM.md) §5.1), never one-time prekeys or a device list.
- **Drain limits.** Providers rate-limit one-time consumption per capability and per device, so one capability cannot exhaust a pool; a rate-limited fetch gets only the signed EC and PQ prekeys (PQXDH without a one-time key), never a weaker handshake.
- **Visible last resort.** When a session is established with the last-resort key because one-time keys ran out, the owner's devices see it (as inventory state and in diagnostics), so a drain attack is noticed.
- `inventory(device)` is callable only by the owner's devices.

**W0 privacy boundary.** AsyncKeyBundle v1 does **not** carry `acs_generation` or `acs_state_hash`. A KeyMaterialProvider therefore receives no protocol-mandated stable account-wide ACS correlator from the bundle. It still sees `subject_device_id`, `provider_pool_id`, timing and inventory shape, so stable per-device correlation remains honestly disclosed. Account authorization happens only at the relationship-pinned fetcher (§3).

## 3. AsyncKeyBundle v1 authorization and wire semantics

For OM-Crypto-v1 the canonical W0 body is defined by [`keys.cddl`](../../../spec/cddl/keys.cddl) and ADR-0140:

```text
AsyncKeyBundle v1
  subject_device_id
  crypto_profile
  generation                 # signed-prekey generation
  signed_prekey              # EC
  pq_signed_prekey           # mandatory
  optional one-time/fallback material
  provider_pool_id
  expires_at
```

Numeric keys 3 and 4 are permanently reserved after retirement of the pre-deployment `acs_generation` and `acs_state_hash` fields; they MUST be absent and are never reused.

The fetcher verifies in this order before PQXDH secret-key work or one-time-key consumption:

```text
RelationshipContext(expected peer AccountID, trusted peer ACS head)
  -> DeviceCredential digest committed by that ACS
  -> credential AccountID == expected peer AccountID
  -> DeviceID is active and eligible at that head
  -> AsyncKeyBundle composite device signature
  -> bundle generation anti-rollback/equivocation checks
  -> one-time-prekey context signature
  -> PQXDH
```

The provider is not asked to prove account membership. By the time a normal stable-account bundle is fetched, first contact has already passed `MessageRequest` acceptance and reciprocal `IntroIdentityBinding`, so the relationship has the authenticated ACS context required for this check.

## 4. Signed-prekey generation and anti-rollback

`generation` is global for one `(DeviceID, CryptoProfile)` across all KeyMaterialProviders. It is not a provider-local publication sequence.

A device MUST strictly increase generation whenever either signed-prekey secret changes, and a rotation creates fresh EC and PQ signed-prekey secrets plus fresh key IDs. Replenishing one-time keys, changing providers or changing `provider_pool_id` does not reset the generation namespace.

Each recipient persists a monotonic freshness floor:

```text
BundleFreshnessFloor(peer AccountID, peer DeviceID, CryptoProfile)
  highest_generation
  generation_core_hash
```

where:

```text
generation_core_hash =
  DS-HASH("OM-PREKEY-GENERATION-CORE-v1",
          OM-CBOR [DeviceID,
                   CryptoProfile,
                   generation,
                   signed_prekey,
                   pq_signed_prekey,
                   pq_last_resort_prekey-or-null])
```

Acceptance is fail-closed:

- generation below `highest_generation` -> reject `prekey_generation_rollback`;
- equal generation with a different core hash -> reject `prekey_generation_equivocation`;
- greater generation -> after signature/credential/expiry validation, atomically raise the floor before successful establishment is committed.

The floor is DeviceVault/account-private relationship state. Multi-device sync merges it monotonically: max generation wins; equal generation with conflicting core hash is a security conflict, not last-writer-wins.

A new provider pool never creates a fresh rollback namespace. Multiple providers may carry the same generation with different pool ids, expiry values and disjoint one-time items, but the signed-prekey core must be identical.

Where several configured providers are reachable, the client SHOULD compare valid candidates and consider the highest generation before establishment. This is defense in depth, not a provider quorum or new authority.

### 4.1 First-observation limitation

A peer that has never observed a generation newer than `g` cannot know from a malicious provider alone that `g` was superseded. OM therefore does not claim a nonexistent freshness oracle. The residual is bounded by bundle expiry, authenticated device revocation/quarantine and independent-provider comparison where available. Once a peer has accepted `g+1`, rollback to `g` is permanently rejected for that DeviceID/profile.

## 5. Context-bound one-time prekeys

Independent providers receive disjoint one-time pools. Every selected one-time prekey carries a unique key id and is signed by the device in its exact bundle context.

`PrekeyItem` remains:

```text
{ key_id, kind, public_key, signature }
```

For one-time kinds 3 and 4, the signature under `OM-PREKEY-SIG-v1` covers:

```text
OM-CBOR [
  1,
  subject_device_id,
  crypto_profile,
  bundle_generation,
  provider_pool_id,
  kind,
  key_id,
  public_key
]
```

The generic DS-SIG tuple also names the device signer and profile. A fetcher verifies the item using the DeviceID/profile/generation/pool from the already verified bundle. Cross-device, cross-profile, cross-generation and cross-provider-pool transplantation is therefore rejected.

The EC/PQ signed prekeys in bundle keys 7/8 are already covered by the composite signature over the whole AsyncKeyBundle and do not need a second item signature.

If a CryptoProfile defines reusable fallback material, that material is explicitly marked as fallback and receives the reviewed protocol's semantics; it is never mislabeled one-time.

## 6. Compromise, rotation and provider failure

If signed-prekey private material at generation `g` is suspected compromised:

1. generate fresh EC/PQ signed-prekey secrets and fresh key ids at `g+1` or greater;
2. publish the new generation to every configured provider pool;
3. request provider deletion/revocation of `g` as best-effort hygiene;
4. synchronize the new local publication/freshness state;
5. delete unused private one-time material associated with `g` when that generation is retired.

A peer that has seen the newer generation rejects replay of `g` even when the old bundle is still correctly signed and not yet expired. One-time items from `g` cannot be transplanted into `g+1` because their context signatures bind the generation.

Provider loss/migration does not reset freshness. A migrated pool inherits the same device/profile generation namespace and produces a distinct `provider_pool_id` only for consumable-pool isolation.

## 7. Restore and persistence

The publishing device persists its generation counter and signed-prekey secrets durably. Existing session rule K11 remains stronger: restoring an old device backup does not resurrect an old DeviceID/prekey state; recovery creates a fresh DeviceID/material, preventing local generation rollback under the old DeviceID.

The fetcher's `BundleFreshnessFloor` is durable relationship security state and is synchronized/backed up with monotonic max semantics. An older restored snapshot MUST NOT overwrite a higher floor available from surviving local devices or newer authenticated sync state.

If only an old snapshot survives and no copy of a newer floor exists, the lost observation cannot be reconstructed cryptographically. The restored client marks freshness stale, compares every reachable configured provider, authenticates against the current relationship-pinned ACS, applies expiry, and accepts only the highest valid generation it can establish. Provider `revokeGeneration()` is never promoted into a security oracle to paper over this residual.

## 8. MLS KeyPackages

MLS KeyPackages are likewise treated as consumable publication objects according to the selected MLS profile. Independent publication stores SHOULD receive disjoint package sets unless the selected protocol/profile explicitly provides safe shared-consumption semantics.

## 9. Depletion and failure behavior

Clients/providers expose bounded inventory state without turning inventory into identity authority.

Policy defines:

- low-water replenishment;
- offline-device depletion;
- fallback behavior;
- expiry/rotation;
- stale-generation rejection and equal-generation equivocation;
- replay/duplicate fetch handling;
- provider loss/migration;
- privacy leakage from inventory queries (§2.1: owner-only).

Running out of strong asynchronous material may reduce availability; it MUST NOT silently downgrade to an unauthenticated/weaker handshake outside the allowed CryptoProfile.

## 10. Provider independence

A KeyMaterialProvider can be co-hosted with a DirectoryProvider or RelayProvider for convenience, but those are separate logical authorities/capabilities. Clients may publish to multiple providers.

No provider decides which devices are authorized. Authorization is validated against relationship-pinned `AccountControlState`/DeviceCredential evidence.

## 11. Verification obligations

Before core-kernel implementation authorization, evidence must cover at minimum:

- exact v1 bundle schema and domain separation;
- signed-prekey rollback below a persisted high-water mark;
- equal-generation conflicting-core rejection;
- cross-device/profile/generation/provider-pool one-time-prekey transplantation rejection;
- generation rotation after compromise and stale-provider replay;
- multi-provider disjoint pools with a shared global generation namespace;
- restore behavior that never overwrites a higher known floor;
- depletion/replenishment and last-resort behavior;
- device revocation/quarantine against the relationship-pinned ACS;
- privacy/resource-exhaustion limits.
