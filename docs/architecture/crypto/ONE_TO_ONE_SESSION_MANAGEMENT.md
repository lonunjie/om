# One-to-One Multi-Device Session Management

- **Status:** Canonical architecture direction; exact state machine is a blocking pre-engineering item
- **Checkpoint:** 2026-09-23 05:45 PHT

## 1. Problem

An OM account is not one endpoint. Every authorized DeviceIdentity is an independent cryptographic endpoint. Therefore a human-to-human conversation may require multiple device-to-device sessions plus synchronized knowledge about which devices are currently eligible.

This layer is distinct from the underlying asynchronous handshake/ratchet and distinct from account-private history/preferences sync.

## 2. Session manager responsibilities

The 1:1 session manager owns:

- discovery of currently authorized recipient devices;
- acquisition/validation of asynchronous handshake material;
- creation and persistence of one ratchet/session per required device pair;
- fanout of a logical message to eligible recipient devices;
- delivery to the sender's other eligible devices when the product semantics require self-sync;
- simultaneous initiation/crossed-session handling;
- stale session detection;
- session reset/recovery without identity confusion;
- reaction to device quarantine/revocation;
- newly added recipient devices;
- duplicate/replayed envelope handling;
- safe interaction between control-state changes and sends already in flight.

## 3. Authority source

The session manager does not trust a relay/directory list as account authority.

```text
AccountControlState
  -> authorized DeviceIdentity set
  -> device credentials / CryptoProfile eligibility
  -> session eligibility
  -> per-device encrypted delivery
```

Provider data may help locate material, but authorization is cryptographically checked.

**Inbound initiator binding (K12, ADR-0136 rule 4).** The chain above runs in both directions. An initiator binds
the recipient through ACS → DeviceCredential → dual-signed AsyncKeyBundle (ADR-0135 §2). A responder MUST bind the
initiator before it accepts a new inbound session. The initiation's X25519 identity key `IK_A` must be the
`key_agreement_public_key` of **exactly one** DeviceCredential that is committed (by digest) by the trusted ACS head
the session manager holds for the relationship whose route delivered the envelope (K6/K7), and that device must be
active and not quarantined at that head. The check runs before any secret-key operation, one-time-key consumption,
session creation or plaintext release. On no match, several matches or an inactive device, the initiation is
rejected and nothing changes. When the held head may be stale (for example, a peer's newly linked device), the
session manager refreshes and validates the peer's head (never accepting an older one, K7) and only then processes the
initiation; until then the envelope is held unprocessed. 1:1 device sessions exist only inside an accepted
relationship: first contact uses the HPKE `MessageRequest` (ADR-0126), never an unbound PQXDH initiation, so a
responder always has a relationship head to bind against. No wire field is added: the binding uses the frozen
DeviceCredential and AccountControlState objects.

## 4. Logical-message identity versus envelopes

A single user-visible message has one canonical logical event/message identity even if encrypted into several independent device envelopes.

Per-device transport/session envelope IDs are not canonical message IDs. Duplicate successful deliveries to two recipient devices MUST NOT create duplicate conversation events after synchronization.

## 5. Device changes

### New recipient device

Once the recipient's AccountControlState authorizes the device, senders may establish a new device session according to freshness/policy rules. Historical access is separate and is never implied merely by receiving future traffic.

### Quarantine/revocation

A sender that learns a sufficiently authoritative quarantine/revocation stops creating new secrets/envelopes for the affected device according to [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../identity/ACCOUNT_CONTINUITY_AND_CONTROL.md).

Already transmitted ciphertext and plaintext already possessed by the endpoint cannot be revoked retroactively.

### Stale sender view

If a send races a device-list change, the exact security/liveness behavior must be defined. The architecture MUST prefer temporary delivery failure/reconciliation over silently authorizing a device not supported by valid current/acceptable control state.

## 6. Concurrency

The state machine must define crossed initiations, multiple sender devices, multiple recipient devices, session replacement, and delayed messages. It must not rely on a single online homeserver to serialize all state.

The design may borrow reviewed concepts from multi-device session-management protocols such as Sesame, but OM's authority source remains its own AccountControlState rather than a centralized server's device record.

## 7. Persistence and backup

Ratchet/session persistence belongs to the DeviceVault/local security boundary. Backup/restore cannot roll a session backward in a way that causes nonce/key reuse or resurrects authorization for revoked devices.

Where secure session state cannot be safely restored, the protocol must specify authenticated re-establishment rather than unsafe rollback.

## 8. Freeze gate

Before 1:1 wire/security freeze:

- write the explicit state machine;
- prove logical-message deduplication/fanout semantics;
- define crossed-initiation convergence;
- define device-add/revoke races;
- define session-reset UX/security;
- test offline multi-device combinations;
- integrate with asynchronous key-material depletion and account-control freshness.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0120](../../../decisions/ADR-0120-one-to-one-session-manager-rules.md): rules K1–K11 are MUST rules for the session manager: persist before send (K1); dedupe durable before the relay ack (K2); re-send on an authenticated reset (K4); re-check eligibility at every encryption (K5); recipients only from validated AccountControlState (K6); keep peers' control heads across crash and restore (K7); never restore ratchet state (K8); reject traffic from revoked devices (K9); prekey header until confirmed (K10); no prekey secrets in backups, and fresh prekeys after a restore (K11). [ADR-0136](../../../decisions/ADR-0136-core-kernel-review-remediation.md) adds K12: bind the inbound initiator's identity key to exactly one active committed DeviceCredential before accepting a session (§3). Each rule's V-003 vector is its conformance test.
- [ADR-0120](../../../decisions/ADR-0120-one-to-one-session-manager-rules.md): lazy repair is accepted. A device that lost sessions does not broadcast; an undecryptable message triggers an authenticated reset and a re-send.

## Last-resort handshake replay ledger

No-one-time-key PQXDH acceptance is idempotent by an exact transcript `handshake_id` persisted before plaintext release
or acknowledgement. The replay set is scoped to DeviceID + signed-prekey epoch, has no LRU/probabilistic eviction and
is retained through that prekey epoch's acceptance overlap. At 4096 accepted last-resort ids the endpoint rejects
more last-resort initiations until fresh key material is published. Backup restore creates a fresh DeviceID/prekey
state rather than rolling this ledger backward.
