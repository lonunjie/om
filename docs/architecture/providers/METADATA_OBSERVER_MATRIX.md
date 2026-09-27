# W0 Metadata Observer Matrix

- **Status:** Canonical W0 metadata-observer model
- **Checkpoint:** 2026-09-27 Run 3
- **Decision:** ADR-0137, amended by ADR-0138 and ADR-0142
- **Machine companion:** [`spec/metadata/w0-metadata-exposure-descriptors.json`](../../../spec/metadata/w0-metadata-exposure-descriptors.json)

## 1. Purpose

Privacy claims must describe what an observer can actually see, including request authentication and colocated roles. Looking only at encrypted request bodies is insufficient. This matrix separates four layers:

1. **protocol-object observation** — clear fields in envelopes, descriptors, capabilities and tokens;
2. **authentication observation** — signer/device material required to authenticate a provider operation;
3. **transport/operational observation** — source address, timing, size, retry and quota behavior; and
4. **coalition observation** — what one operator can correlate when it runs several provider roles.

`always`, `sometimes` and `no_by_design` in the machine descriptor mean protocol-level W0 exposure for the named observer. They are not retention promises. Operator logs can increase practical linkage; a provider must not advertise a stronger privacy property merely because its policy says it will discard data later.

## 2. Provider-authentication rule

ADR-0138 removes `device_auth` from routine provider administration. Home/provider roles receive independent
resource-scoped opaque capabilities rather than a DeviceCredential + ACS head merely to administer provider-local
state. Reuse of a capability may correlate operations on the **same provider resource**, which is inherent to that
resource; the capability is not a stable OM device/account credential and is different across unrelated providers
and role resources.

Canonical signed objects remain end-to-end verifiable. A provider's acceptance of a write never makes an otherwise
invalid/stale descriptor, bundle or account object valid to clients.

## 3. Core W0 observer matrix

| Observer | Stable device correlation | Other important observations | Must not infer from the claim |
|---|---|---|---|
| mailbox-class Home / relay | **no stable sender-device correlator for established 1:1 payloads by design** (ADR-0142) | deposit/drain timing, sealed ciphertext padding bucket, opaque delivery-key/delivery ids, hosted contact edges; **on RecoveryRequest fan-out:** AccountID + recovery-attempt fact; source network address operationally. First contact remains the separate MessageRequest/IntroGate surface. | sealed delivery is not traffic anonymity; timing/IP/volume/route correlation remains possible; not message plaintext or universal AccountID routing authority |
| KeyMaterialProvider | **always** — `subject_device_id` in AsyncKeyBundle and authenticated publication | bundle fetch/publish timing, device/bundle availability, `provider_pool_id`; account/contact linkage only when deployment/traffic supplies it, not from ACS fields (ADR-0140) | not conversation plaintext, account-wide ACS identity, or authority to add devices |
| push provider/gateway | **always** for the registered push token/device handle | wake timing; cross-profile linkage where platform tokens are shared | not message plaintext or canonical conversation identity from the opaque wake protocol |
| discovery publisher | no stable device correlator from provider auth by design | published descriptor and provider-local descriptor resource handle; query/publication timing; descriptor-to-mailbox linkage when colocated | alias/descriptor is not AccountID/controller authority |
| RecoveryRecord store | no stable device correlator from provider auth by design | opaque record locator/capability use, ciphertext size, access/setup/recovery timing | cannot derive AccountID from the record capability or independently authorize recovery/read plaintext; the encrypted snapshot may be stale and provider generation checks are not account authority |
| BlobStore / StoragePool | no stable device correlator from provider auth by design | blob size, access timing, provider-local repair capability; shared-blob linkage when explicitly enabled | blob locator is not AccountID/ConversationID; store has no content key by protocol design |
| IntroGate | no stable device correlator from the W0 referral token protocol by design | network address, timing, ticket/admission class, spent-token state; contact edge may arise from explicit referral policy | token blindness is not a traffic-analysis guarantee; gate cannot decrypt MessageRequest plaintext |
| platform credential manager | provider/platform-dependent | OM RP ID, random RecoveryHandle (`user.id`), human recovery locator (`user.name`), recovery/passkey use timing and platform telemetry | random `user.id` avoids directly encoding AccountID but is not an unlinkability guarantee; OM makes no claim that the platform cannot correlate these values/telemetry |
| ClientHost origin | no OM stable account/device field by design | client fetch timing/IP and release selection | serving client bytes gives no AccountControlState authority |
| LAN observer | no stable OM identity by design | fixed-cardinality rotating rendezvous advertisements plus link-layer metadata | link-layer address reuse may still correlate epochs if the platform cannot randomize it |

The machine descriptor is authoritative for the complete field-by-role list. This table explains the security meaning and the auth-layer additions most likely to be missed during implementation.

## 4. Colocation and collusion

When one operator runs several roles, OM treats them as one observer. For example a hosted Home may run the mailbox, discovery publisher and IntroGate. It can join those logs regardless of internal microservice boundaries. Internal credential/storage separation remains worthwhile defense in depth, but it is not a privacy non-collusion assumption.

The following must therefore be stated separately in privacy analysis:

- what a role learns alone;
- what the same operator learns by combining its roles;
- what two independent operators could learn if they collude; and
- what still cannot be derived because E2EE keys or protocol authority are absent.

## 5. Privacy-property vocabulary

OM uses these terms narrowly:

- **cryptographic unlinkability:** the cryptographic transcript does not provide the linking relation;
- **protocol-identifier unlinkability:** the wire objects do not carry a stable identifier that provides the relation;
- **transport unlinkability:** the network path does not provide the relation;
- **operational unlinkability:** timing, size, account, logging and deployment data do not provide the relation.

A claim about one layer is never silently promoted to another. In particular, W0 Privacy Pass referral tokens target cryptographic/protocol unlinkability of issuance-to-redemption; they do not erase IP/timing/cardinality observations.

## 6. Design consequence

Any new W0/W1 operation must be reviewed by asking, in order:

1. what clear object fields does each role see?
2. what authentication material must it see?
3. what stable correlators result from those two layers?
4. what timing/size/network observations are unavoidable?
5. which roles are likely to be colocated?
6. what authority, if any, did the new material grant?

The descriptor and this matrix must be updated together before a privacy claim is considered complete.
