# OM LAN Rendezvous and Offline Local Mode

- **Status:** Canonical direction; platform capability verification remains
- **Checkpoint:** 2026-09-21 10:08 PHT

## 1. Goal

Existing contacts—and peers who just exchanged a QR/invite—should be able to discover each other on the same local network without broadcasting stable AccountIDs, usernames, device names, or globally linkable service identifiers.

Longer-term goal: when WAN access is unavailable but the LAN remains functional, OM should still support local messaging, file transfer, and calls where cryptographic state permits.

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

Canonical OM events remain the same. When WAN service returns, normal sync/deduplication reconciles the local exchange.

## 7. Security boundaries

- LAN presence does not imply trust.
- Discovery only identifies a candidate endpoint; OM cryptographic relationship verification remains mandatory.
- No silent fallback from encrypted OM semantics to plaintext LAN protocols.
- Public Wi-Fi adversaries must not learn stable identity from advertisements.
- Clients must defend against token flooding/spoofing and resource exhaustion.
## 2026-09-22 hardening — platform capability boundary

The LAN protocol remains client-agnostic, but discovery substrates are capabilities, not universal guarantees. Native clients may use OS mDNS/DNS-SD/local-network APIs where permitted. PWA clients may require explicit local-network permission and may need QR/manual/known-endpoint/WebRTC-style rendezvous when browser APIs do not expose native-equivalent discovery.

OM promises interoperable local-mode semantics where a route can be established; it does not promise identical automatic discovery on every platform.

## 2026-09-23 LAN resolution

Relationship-derived, short-lived opaque rendezvous tags replace stable identifier broadcast. Native mDNS/DNS-SD is optional capability; QR/manual/known-endpoint is universal fallback. Browsers/PWAs are never required to expose native-equivalent automatic LAN discovery. LAN continues to carry ordinary OM E2EE ciphertext.
