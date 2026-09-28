# OM LAN Rendezvous and Offline Local Mode

- **Status:** Canonical V1 optional capability; platform capability verification remains
- **Checkpoint:** 2026-09-28 ADR-0150

## 1. Goal

Existing contacts—and peers who just exchanged a QR/invite—should be able to discover each other on the same local network without broadcasting stable AccountIDs, usernames, device names, or globally linkable service identifiers.

V1 optional capability: when WAN access is unavailable but the LAN remains functional, OM supports local messaging and encrypted file transfer for already-related or short-lived QR/invite-linked peers where the platform can establish a route; calls keep their existing LAN/direct RoutePolicy. Automatic discovery is capability-gated and is not promised on every platform (ADR-0150).

## 2. Existing-contact rendezvous

Peers already share relationship secret material. Derive a rotating local rendezvous token using a dedicated KDF domain and short time epoch:

```text
LAN-Rendezvous-Secret
      + epoch
      + local context
          ↓ KDF/HMAC
random-looking token
```

The precise derivation MUST use a purpose-specific exported secret and MUST NOT reuse raw message-ratchet keys.

Properties:

- rotates frequently;
- not reversible to AccountID;
- only known contacts can predict/match it;
- a passive observer sees changing random-looking values;
- replay across epochs is rejected.

### 2.1 Contacts can be trackers (ADR-0130 rule 8)

"Only known contacts can match" is also the risk: any contact, including an estranged partner or a former employer
who is still a contact, can passively detect the user's physical presence on a shared LAN (T-087). And a device that
advertises one tag per contact leaks its contact count, while a tag set that rotates together from one MAC/IP address
is linkable across epochs by a passive Wi-Fi observer (T-088). Rules (R-270, R-271):

1. **Off by default, per contact.** LAN rendezvous runs only for contacts the user marks ("Find each other on the
   same Wi-Fi"). It is off for everyone under Hardened and Maximum Privacy. A QR/invite LAN secret (§3) covers only
   that one pairing.
2. **Retired on block or relationship end.** Blocking or removing a contact, or ending the relationship, immediately
   stops advertising and answering that contact's tags on every device of the account.
3. **Fixed cardinality.** A device advertises exactly K tags per epoch regardless of how many contacts are enabled
   (real tags plus random decoys), so advertisement size carries no information; if more than K contacts are
   enabled, tags rotate through the slots across epochs. A querier-initiated design (the device only answers
   queries) is an acceptable alternative for platforms where it fits. K is a policy constant fixed with V-015, not a
   wire constant.
4. **Rotate with the link layer.** Where the platform exposes it, tag epochs change together with MAC address
   randomization, and a device never answers old-epoch tags from a new address.

V-015 includes a LAN trace criterion for these rules.

## 3. New-contact LAN rendezvous

A QR/invite may carry a temporary LAN rendezvous secret/capability. This lets two nearby devices find each other without Internet service.

Offline invites should be short-lived because immediate remote revocation cannot be enforced while disconnected.

## 4. Service advertisement

OM may use mDNS/DNS-SD or another local discovery substrate, but published fields must be privacy-preserving. Do not advertise a stable `Maj's phone`, username, AccountID, or permanent host identifier.

The privacy requirements in RFC 8882 are useful design guidance: avoid linkable identifiers and unnecessary disclosure of client/server identity or discovery interests.

## 5. Signed local service registration

Where a local registrar architecture is useful, borrow the useful idea from SRP (RFC 9665): registrations are defended by public-key signatures rather than trusting an unauthenticated name announcement.

OM does not need to expose DNS-shaped identities; SRP is an architectural reference for secure registration.

## 6. Offline local behavior

If both peers have enough existing crypto state:

```text
Internet unavailable
      X
local Wi-Fi / LAN alive
      │
Alice OM <------> Bob OM
 messages / files / calls
```

Canonical OM events remain the same. A LAN endpoint/token is an ephemeral transport candidate, never identity or authority. Local delivery uses ordinary OM cryptographic/session authorization and the same logical EventID that a WAN/provider route would carry; route-specific transport wrapping may differ, but route selection cannot change message meaning or security level. When WAN service returns, ordinary sync/deduplication reconciles retries so one logical EventID materializes once.

Encrypted file transfer is likewise transport-neutral: direct LAN transfer carries the same encrypted object named by the canonical BlobID and `ciphertext_digest`. A receiving endpoint authenticates it before plaintext release. Later BlobStore mirroring/backfill relocates the same object without rewriting EventID/BlobID. The client distinguishes local receipt from configured StoragePool durability until provider/storage replication is actually satisfied.

## 7. Security boundaries

- LAN presence does not imply trust.
- Discovery only identifies a candidate endpoint; OM cryptographic relationship verification remains mandatory.
- No silent fallback from encrypted OM semantics to plaintext LAN protocols.
- Public Wi-Fi adversaries must not learn stable identity from advertisements.
- Clients must defend against token flooding/spoofing and resource exhaustion before expensive cryptographic/blob work.
- LAN failure never falls back to plaintext or unauthenticated local messaging; it queues or chooses another allowed encrypted route.
- A transport acknowledgement is not a conversation event, relationship grant, device-eligibility proof or storage-durability proof.
## 2026-09-22 hardening — platform capability boundary

The LAN protocol remains client-agnostic, but discovery substrates are capabilities, not universal guarantees. Native clients may use OS mDNS/DNS-SD/local-network APIs where permitted. PWA clients may require explicit local-network permission and may need QR/manual/known-endpoint/WebRTC-style rendezvous when browser APIs do not expose native-equivalent discovery.

OM promises interoperable local-mode semantics where a route can be established; it does not promise identical automatic discovery on every platform.

## 2026-09-23 LAN resolution

Relationship-derived, short-lived opaque rendezvous tags replace stable identifier broadcast. Native mDNS/DNS-SD is optional capability; QR/manual/known-endpoint is universal fallback. Browsers/PWAs are never required to expose native-equivalent automatic LAN discovery. LAN continues to carry ordinary OM E2EE ciphertext.

## 2026-09-28 wire-binding status (ADR-0156)

The invariants above are frozen, and route-independent EventID dedupe and blob backfill belong to `core_kernel`. The concrete LAN wire binding is **not yet specified**. It is design item A-055 under the W1 scope `local_delivery`, verified by V-024. Until A-055 is resolved, no conforming client may ship LAN message/file delivery. The following rules bind any binding:

1. **No clear sender.** 1:1 messages carry `SealedDeliveryEnvelope` protection or a reviewed equivalent that hides sender identity, device key and Double Ratchet header from LAN observers. Group messages carry MLS `PrivateMessage` bytes.
2. **No stable LAN-visible identifier.** Connection ids, delivery-key ids and route tokens rotate at least per rendezvous epoch and with link-layer address changes.
3. **No LAN-only relationship path.** First contact over LAN uses ordinary MessageRequest/IntroGate semantics; the LAN endpoint is only a reply route.
4. **Registered derivation.** The rendezvous-token KDF, label and epoch are registered in [`om-domain-separation.json`](../../../spec/registries/om-domain-separation.json).

