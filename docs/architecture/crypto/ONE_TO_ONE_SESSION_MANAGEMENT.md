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
- safe interaction between control-state changes and sends already in flight;
- sealed-delivery key selection, expiry/revocation, durable outer replay state and no-downgrade failure handling.

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

**AsyncKeyBundle freshness (ADR-0140).** For each `(peer AccountID, peer DeviceID, CryptoProfile)`, the session manager persists the highest accepted signed-prekey generation plus `OM-PREKEY-GENERATION-CORE-v1`. A fetched bundle below that floor is rejected before one-time-key consumption or PQXDH secret work. Equal generation with a different signed-prekey core is an equivocation/security conflict. Provider migration or a new `provider_pool_id` never resets the floor. Account-private multi-device sync merges the floor monotonically (higher generation wins; equal conflicting core fails closed).

**Inbound initiator binding (K12, ADR-0136 rule 4).** The chain above runs in both directions. An initiator binds
the recipient through ACS → DeviceCredential → dual-signed AsyncKeyBundle (ADR-0135 §2). A responder MUST bind the
initiator before it accepts a new inbound session. The initiation's X25519 identity key `IK_A` must be the
`key_agreement_public_key` of **exactly one** DeviceCredential that is committed (by digest) by the trusted ACS head
the session manager holds for the relationship whose route delivered the envelope (K6/K7), and that device must be
active and not quarantined at that head. The check runs before any secret-key operation, one-time-key consumption,
session creation or plaintext release. On no match, several matches or an inactive device, the initiation is
rejected and nothing changes. When the held head may be stale (for example, a peer's newly linked device), the
session manager refreshes and validates the peer's head (never accepting an older one, K7) and only then processes the
initiation; until then the envelope is held unprocessed.

**ADR-0137 API rule:** the expected peer AccountID/trusted-head tuple is a required `RelationshipContext` input to
every inbound 1:1 receive/decrypt path. It is not an optional argument, mode bit or default. Existing sessions are
checked against the same relationship route as new initiations, so a valid established session cannot be replayed as
authenticated traffic on a different accepted relationship. An implementation may have lower-level private helpers,
but no public/session-manager acceptance path may omit this context.

1:1 device sessions exist only inside an accepted relationship: first contact uses the HPKE `MessageRequest`
(ADR-0126), never an unbound PQXDH initiation, so a responder always has a relationship head to bind against. No wire
field is added: the binding uses the frozen DeviceCredential and AccountControlState objects plus trusted local
relationship state.

## 3.1 Sealed delivery is part of accepted-relationship session state

For OM-Crypto-v1, every normal mailbox/relay-bound 1:1 send in an `ACTIVE` relationship selects a current
`SealedDeliveryKeyRecord` for the exact recipient DeviceID and exact RelayEntry. The route binding, recipient-device
signer eligibility and key expiry are checked before sealing. Missing or stale key material triggers authenticated
state refresh/queueing or an explicit failure; it never causes a clear-sender fallback.

On receive, the endpoint validates the bounded outer `SealedDeliveryEnvelope`, enforces expiry and durably creates
an ADR-0143 `RESERVED` replay record for `(delivery_key_id, delivery_id)` plus the exact-wrapper fingerprint before
releasing inner plaintext/event data or acknowledging the delivery. A crash while `RESERVED` does not turn the exact
same-wrapper retry into permanent loss: that retry may resume. A different wrapper under the same key/id is rejected.
HPKE open only recovers the ordinary inner envelope. The existing mandatory `RelationshipContext`,
ACS/DeviceCredential checks, PQXDH/ratchet authentication and replay rules then run unchanged.

ADR-0144 composes this replay state with K7/K12. If inner authentication is temporarily **indeterminate** only because
the held trusted peer ACS head may be stale, or its authenticated monotonic refresh is unavailable, the wrapper stays
`RESERVED`; no plaintext/event is released and no successful delivery acknowledgement is emitted. After refresh, a
valid initiator proceeds normally; a definitive invalid/ineligible result may transition the exact wrapper to
`REJECTED`. A need for stale-head refresh is never itself a terminal authentication failure. The inner K2
session/dedupe state and replay transition to `COMPLETE` become durable in one crash-consistent commit before
application release/ack; a `COMPLETE` retry is idempotent and never repeats inner effects. Successful outer open alone
therefore cannot create or authenticate a session.

First contact remains `MessageRequest`. Its signed payload carries a fresh request-scoped `reply_delivery_key` bound to the
request-scoped `reply_route`; that key can seal exactly one logical post-acceptance bootstrap/control response for the request while ADR-0146 permits bounded physical transport retries/candidates.
Pending-request state binds it to the request's ticket id, request-key digest and CryptoProfile; its expiry cannot exceed
the reply route; key/id reuse across requests is forbidden; and bootstrap material is never promoted into stable relationship delivery state. The first valid response whose authenticated bootstrap/import state commits atomically wins the request; a second
different wrapper cannot become another bootstrap even if another recipient device raced acceptance. The winning wrapper's
`COMPLETE` replay record remains a replay-only tombstone through the bounded acceptance window, so an exact lost-ack retry
can be acknowledged before delivery-key lookup without retaining the completed bootstrap private key. Any transition out of request `OPEN` atomically terminalizes non-winning `RESERVED` siblings before bootstrap-key destruction, so no resumable record can outlive required key material. Established per-device/per-route delivery records carry the reciprocal response; relationship `ACTIVE` still waits for both stable identity bindings (ADR-0144/0145/0146).

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

If a send races a device-list change, ADR-0120 K5/K6 is decisive: eligibility is re-checked against the current validated AccountControlState at every encryption/fan-out/retry. A device no longer supported by that state receives no new envelope. If the view is stale/indeterminate, delivery waits for authenticated reconciliation rather than silently authorizing the device; an authenticated reset may trigger bounded K4 re-encryption on a fresh valid session.

## 6. Concurrency

V-003 and ADR-0120 define the per-device session manager for crossed initiation, multiple sender/recipient devices, replacement/reset, delayed messages, dedupe and persistence. No online Home serializes this state. Crossed/duplicate work converges through stable logical-message identity plus durable session/dedupe state; recipient eligibility is always derived from validated AccountControlState.

The implementation may borrow reviewed concepts from multi-device session-management protocols such as Sesame, but OM's authority source remains its own AccountControlState rather than a centralized server's device record.

## 7. Persistence and backup

Ratchet/session persistence belongs to the DeviceVault/local security boundary. Backup/restore cannot roll a session backward in a way that causes nonce/key reuse or resurrects authorization for revoked devices. ADR-0120 K8/K11/P1 therefore never restores ratchet/prekey state: a restored installation publishes fresh prekeys and repairs lazily through authenticated reset/re-establishment when a peer next communicates. Peer control heads and security floors survive with the monotonic rules already specified.

## 8. Freeze/evidence gate

The design rules for the 1:1 session manager are selected by ADR-0120 and subsequent ADR-0140..0147/0158 amendments. V-003 remains the evidence gate: its executable model/vectors cover crossed initiation, fanout/dedupe, add/revoke races, authenticated reset/re-send, restore/redelivery, async-key freshness, sealed delivery and account-private sync. The remaining open criterion is independent model review; implementation must consume the frozen rules rather than reinterpret this older checklist as open design.

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
