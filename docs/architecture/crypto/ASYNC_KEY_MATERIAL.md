# Asynchronous Key Material Publication

- **Status:** Canonical architecture direction; architecture selected; implementation/formal verification remains pre-engineering gate work
- **Checkpoint:** 2026-09-23 05:45 PHT

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

Providers return provider-signed/transport-authenticated receipts where useful, but recipient authenticity is proven by OM account/device authorization and cryptographic signatures/bindings, not provider identity.

### 2.1 Fetch authorization (ADR-0126, R-209)

`fetchHandshakeMaterial` and `inventory` are not open to anyone who knows a device id:

- **Relationship fetch.** Fetching a device's bundle, and consuming one-time prekeys, needs a fetch capability the
  device owner issued: per relationship (exchanged inside the accepted relationship) or per IntroductionTicket
  (after acceptance of a MessageRequest). Capabilities are distinct per provider, like relay capabilities.
- **Anonymous fetch** returns only request-key material ([`INVITES_INTROGATE_AND_SPAM.md`](../discovery/INVITES_INTROGATE_AND_SPAM.md) §5.1), never one-time
  prekeys or a device list.
- **Drain limits.** Providers rate-limit one-time consumption per capability and per device, so one capability cannot
  exhaust a pool; a rate-limited fetch gets only the signed EC and PQ prekeys (PQXDH without a one-time key), never a weaker handshake.
- **Visible last resort.** When a session is established with the last-resort key because one-time keys ran out, the
  owner's devices see it (as inventory state and in diagnostics), so a drain attack is noticed.
- `inventory(device)` is callable only by the owner's devices.

**Residual linkage.** Every bundle carries `acs_generation` and `acs_state_hash` in the clear (frozen W0 fields).
They are identical for all of an account's devices and across providers, so a provider or fetcher can group devices
per account and follow an account across providers. The W0 metadata descriptor says so (`device_count: sometimes`,
`stable_account_id: sometimes`). An unlinkable binding is design-queue item A-031, recorded under V-015.

## 3. 1:1 handshake material

The exact fields are selected by CryptoProfile. A PQXDH-class profile is expected to need conceptually:

- device identity/authentication key material;
- signed/pre-authorized medium-term prekey material;
- post-quantum signed prekey material, **mandatory** in every bundle (ADR-0118 §6; `pq_signed_prekey`, key 8, ADR-0126);
- one-time prekeys where the selected construction uses them;
- last-resort/fallback material where explicitly specified;
- key IDs/generation/profile/expiry;
- binding to the authorized DeviceIdentity and current control-state lineage.

The protocol schema MUST be CryptoProfile-versioned rather than hard-coded to one library's private serialization.

## 4. Multi-provider one-time semantics

A nominally one-time key MUST NOT be blindly mirrored as the same consumable secret across independent providers. Otherwise two independent fetches can consume the same key without a shared atomic transaction.

OM's default architecture is **disjoint consumable pools per provider/failure domain**:

```text
Provider A -> OTPool A
Provider B -> OTPool B
Provider C -> OTPool C
```

Each published one-time item carries a unique key identifier and provider-pool binding. The device replenishes pools independently.

If a CryptoProfile defines reusable fallback material, that material is explicitly marked as fallback and receives the security semantics of the underlying reviewed protocol; it is never mislabeled one-time.

## 5. MLS KeyPackages

MLS KeyPackages are likewise treated as consumable publication objects according to the selected MLS profile. Independent publication stores SHOULD receive disjoint package sets unless the selected protocol/profile explicitly provides safe shared-consumption semantics.

## 6. Depletion and failure behavior

Clients/providers expose bounded inventory state without turning inventory into identity authority.

Policy must define:

- low-water replenishment;
- offline-device depletion;
- fallback behavior;
- expiry/rotation;
- stale generation rejection;
- replay/duplicate fetch handling;
- provider loss/migration;
- privacy leakage from inventory queries (§2.1: owner-only).

Running out of strong asynchronous material may reduce availability; it MUST NOT silently downgrade to an unauthenticated/weaker handshake outside the allowed CryptoProfile.

## 7. Provider independence

A KeyMaterialProvider can be co-hosted with a DirectoryProvider or RelayProvider for convenience, but those are separate logical authorities/capabilities. Clients may publish to multiple providers.

No provider decides which devices are authorized. Authorization is validated against `AccountControlState`/DeviceIdentity evidence.

## 8. Freeze gate

Before CryptoProfile v1 freezes, OM requires:

- exact logical bundle schema and domain separation;
- selected reviewed handshake construction;
- safe one-time/fallback semantics across multiple providers;
- depletion/replenishment state machine;
- device revocation and generation rollover behavior;
- test vectors for stale/replayed/misbound material;
- privacy/resource-exhaustion limits.
