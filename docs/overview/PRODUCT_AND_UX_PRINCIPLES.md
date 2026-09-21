# OM Product and UX Principles

## Prime rule

> **Bulletproof underneath; boringly easy on top.**

Assume a meaningful portion of users do not know what Wi‑Fi, encryption, NAT, a relay, an object store, or a cryptographic key is. That is not a user deficiency; it is a product constraint.

## What ordinary users should never be required to do

- select a cipher or manually approve a crypto migration;
- understand key rotation, MLS epochs, KEMs, signatures, or ratchets;
- choose STUN/TURN/SFU;
- create a Cloudflare/AWS account;
- configure R2 buckets, S3 credentials, WebDAV, or relay URLs;
- understand P2P versus relayed calling;
- manually rebalance infrastructure quotas;
- understand device-key topology to add a phone/tablet/computer;
- know whether their contact changed relay or storage provider.

## Invisible automatic operations

The client should automatically:

1. choose the strongest mutually approved CryptoProfile;
2. rotate/session-rekey in the background;
3. migrate sessions/groups when profiles are superseded;
4. preserve a downgrade floor so old/attacker-injected profiles are rejected;
5. rotate private delivery capabilities;
6. choose relay/storage/media providers according to policy and health;
7. avoid or fail over providers near configured cost/quota boundaries;
8. choose LAN/P2P/TURN/SFU routing automatically;
9. retry/resume uploads, message delivery, and calls after network changes;
10. repair encrypted storage mirrors when possible;
11. present recovery and device-linking in human terms.

## Default UX

The default app should mainly expose:

- people and groups;
- messages/media;
- voice/video call buttons;
- search;
- privacy/safety controls in understandable language;
- device management;
- account recovery.

Infrastructure belongs behind `Settings → Advanced` and should usually be represented as policies:

- **Automatic**
- **Prefer free / no surprise charges**
- **Protect my network address**
- **Use my own providers**

rather than vendor jargon.

## Security state

Security warnings should be exceptional and actionable. The UI should not create “encryption fatigue.” Background upgrades are silent when they preserve or improve guarantees. User attention is reserved for meaningful identity discontinuity, suspicious device changes, an explicit transport downgrade (e.g. future SMS), or an action that exposes new data.

## Product invariants

- A provider migration should not look like creating a new account.
- A crypto migration should not look like creating a new account.
- A relay failure should not destroy conversation history.
- A storage failure should not destroy identity.
- A media provider failure should not change who is in the call.
- A network-path change should not change whether call content is E2EE.
- Advanced users may override defaults, but advanced choices may not weaken other users silently.
