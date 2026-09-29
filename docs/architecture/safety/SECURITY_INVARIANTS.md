# OM Security Invariants

- **Status:** Canonical normative invariant map
- **Checkpoint:** 2026-09-28 Run 5A normative lifecycle/restore closure (ADR-0147)
- **Decision:** ADR-0137, amended by ADR-0138/0142/0143/0144/0145/0146/0147

This file is deliberately compact. It does not replace the architecture documents or ADRs; it names the security properties that implementations and verification artifacts are not allowed to weaken by choosing a convenient API or alternate interpretation.

The invariant identifier is stable. Detailed wording may be clarified only by an ADR that updates every affected normative source and evidence mapping.

## Account authority

| Invariant | Normative property | Primary source | Evidence surface |
|---|---|---|---|
| `A1` | AccountControlState is the sole canonical account-authority state; a provider, timestamp, chat event or recovery service cannot create parallel authority. | [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../identity/ACCOUNT_CONTINUITY_AND_CONTROL.md); ADR-0127 | V-002 |
| `A2` | Signers and thresholds are evaluated against the parent state. Quarantine never shrinks the constitutional denominator. | [`account-control-transition-policy.json`](../../../spec/protocol/account-control-transition-policy.json); ADR-0127 | V-002 |
| `A3` | RecoveryPolicy contributes at most one vote; recovery alone is delayed and can be vetoed by any live eligible controller during the verifier-local observation interval. Across reboot/crash, only durably measured monotonic runtime is credited, where a runtime segment is one OS boot session on a sleep-inclusive monotonic clock; provider/custodian time and restored historical timer state never shorten the interval. The one exception is a web (`process_only`) verifier under the Recommended profile, which may credit wall-clock gaps between app sessions forward of a durable high-water mark and must then pass a final authenticated veto sweep; its clock-advance residual is T-132 (ADR-0154). | ADR-0111/0131/0147/0154; [`RECOVERY_AND_BACKUP_POLICY.md`](../identity/RECOVERY_AND_BACKUP_POLICY.md) | V-002 |
| `A4` | Ordinary quarantine lift uses only active, non-quarantined parent controllers; no hidden “who quarantined whom” state affects validity. | [`account-control-transition-policy.json`](../../../spec/protocol/account-control-transition-policy.json); ADR-0136 | V-002 |
| `A5` | A quarantine-induced safety freeze is resolved only by a parent device majority. Quarantined challenge signatures are valid only for that resolution; recovery contributes zero. | ADR-0137; [`account-control-transition-policy.json`](../../../spec/protocol/account-control-transition-policy.json) | V-002 |
| `A6` | A safety-freeze resolution restores a non-empty subset of quarantined controllers; every restored controller must contribute its own freeze-challenge signature, and the child is valid only if it again has at least the parent majority active and non-quarantined. It need not restore a suspected compromised controller. | ADR-0137 | V-002 |
| `A7` | Fork resolution and quarantine-freeze resolution are distinct transition classes. Neither can be interpreted as the other. | [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §§5,8 | V-002 |
| `A8` | Legacy guardians are a separate terminally narrow authority: a legacy retirement matures only after verifier-local inactivity + contest intervals, provider/guardian time cannot shorten them, eligible controllers may veto, and legacy credentials never become ordinary ACS/recovery authority. | ADR-0138; [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) | V-002 |
| `A9` | `permanently_revoked_devices` is a monotone DeviceID tombstone set. A revoked DeviceID can never re-enter messaging/control eligibility; returning hardware or restore enrolls with a fresh DeviceID/credential. | ADR-0131/0158; [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) | V-002 |

## 1:1 authentication and session establishment

| Invariant | Normative property | Primary source | Evidence surface |
|---|---|---|---|
| `I1` | First contact is HPKE MessageRequest; unbound PQXDH never creates a relationship. | [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md); ADR-0126 | V-003/V-005 |
| `I2` | Every inbound 1:1 decrypt/receive operation has an already-trusted `RelationshipContext` naming the expected peer AccountID. There is no optional/unpinned public acceptance path. | ADR-0137; ADR-0120 K12 | V-001/V-003 |
| `I3` | `IK_A` resolves to exactly one active, non-quarantined DeviceCredential committed by that peer's trusted ACS head before private prekey work, one-time-key consumption, session creation or plaintext release. | ADR-0135 §7; ADR-0136/0137 | V-001/V-003 |
| `I4` | Existing sessions remain relationship-pinned; a valid session key cannot be replayed as authority on another relationship route. | ADR-0137 | V-001/V-003 |
| `I5` | Peer ACS heads never roll backward; a possibly stale head is refreshed before session acceptance. | ADR-0120 K7/K12 | V-003 |
| `I6` | AsyncKeyBundle freshness never rolls backward across providers: a lower signed-prekey generation is rejected; equal generation with a different signed-prekey core is equivocation; one-time prekeys verify only in their signed DeviceID/profile/generation/provider-pool context. | ADR-0140 | V-001/V-003/V-005 |

## Metadata and observer truthfulness

| Invariant | Normative property | Primary source | Evidence surface |
|---|---|---|---|
| `M1` | Metadata claims include protocol bodies, authentication material, transport-visible fields and role colocation. | [`METADATA_OBSERVER_MATRIX.md`](../providers/METADATA_OBSERVER_MATRIX.md); [`w0-metadata-exposure-descriptors.json`](../../../spec/metadata/w0-metadata-exposure-descriptors.json) | V-015 |
| `M2` | Routine provider administration uses provider/resource-scoped opaque capabilities rather than AccountID/DeviceCredential/ACS authentication; a provider-local resource handle is not promoted to protocol identity authority. | ADR-0138; [`home-api.md`](../../../spec/protocol/home-api.md) | V-015/V-020 |
| `M3` | Roles operated by one entity are one observer. Internal service separation is defense in depth, not a non-collusion assumption. | ADR-0130; [`METADATA_OBSERVER_MATRIX.md`](../providers/METADATA_OBSERVER_MATRIX.md) | V-015 |
| `M4` | Established OM-Crypto-v1 1:1 mailbox/relay deposits are mandatory `SealedDeliveryEnvelope`s: the provider-facing outer object carries no sender AccountID/DeviceID/identity key, ConversationID or Double Ratchet header, and there is no silent clear-sender fallback. | ADR-0142; [`SEALED_SENDER_DELIVERY_PROFILE.md`](../delivery/SEALED_SENDER_DELIVERY_PROFILE.md) | V-005/V-015 |
| `M6` | A sealed-delivery HPKE open is **not** sender authentication. Delivery keys are accepted only in their relationship × recipient-device × route context. Replay state binds the exact wrapper and uses crash-safe `RESERVED/COMPLETE/REJECTED`; inner K2/session acceptance and `COMPLETE` commit before release/ack. If inner authentication is temporarily indeterminate only because the trusted peer ACS may be stale, the exact wrapper remains `RESERVED` until authenticated monotonic refresh resolves it; uncertainty is not terminal `REJECTED`. | ADR-0142/0143/0144/0145; [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) | V-001/V-003/V-015 |
| `M7` | A first-contact `reply_delivery_key` is fresh per MessageRequest and bound to the exact pending request, CryptoProfile and `reply_route`; its expiry cannot exceed the route and it is never reused/promoted into relationship authority. Exactly one logical bootstrap response may commit. That atomic commit installs the winning authenticated bootstrap/established-delivery state, closes the request to different wrappers and leaves only replay-only metadata so an exact `COMPLETE` retry can be acknowledged before key lookup after the private bootstrap key is destroyed; `RESERVED` retains the key needed to resume. Historical backup/restore never reactivates that private key or live request/replay state. | ADR-0142/0144/0145/0147; [`INVITES_INTROGATE_AND_SPAM.md`](../discovery/INVITES_INTROGATE_AND_SPAM.md) | V-003/V-005/V-015 |
| `M8` | A MessageRequest reply route authorizes one logical bootstrap response with bounded physical transport use: exact retries do not allocate a new candidate, at most 8 distinct candidate wrappers may ever be admitted for the request, and rejected/GC'd candidates never replenish that lifetime budget while `OPEN`. Every request transition out of `OPEN` atomically terminalizes non-winning candidates before bootstrap-key destruction. The bootstrap acceptance deadline is exactly `min(reply_route.expires_at, reply_delivery_key.expires_at)`; an exact winner retry is bounded by its own wrapper expiry/skew, and `COMPLETE_REPLAY_ONLY` ends after the derived request deadline plus skew. Replay tombstones never extend authority. | ADR-0146/0147; [`SEALED_SENDER_DELIVERY_PROFILE.md`](../delivery/SEALED_SENDER_DELIVERY_PROFILE.md) | V-003/V-015 |
| `M5` | RecoveryRecord storage uses opaque RecoveryRoot-derived read/write capabilities and an opaque locator with no AccountID; RecoveryRequest fan-out is a separate path that explicitly exposes AccountID/recovery-attempt metadata to participating mailbox Homes. | ADR-0138; [`RECOVERY_AND_BACKUP_POLICY.md`](../identity/RECOVERY_AND_BACKUP_POLICY.md) | V-002/V-015/V-020 |

## Group authentication boundary

| Invariant | Normative property | Primary source | Evidence surface |
|---|---|---|---|
| `G1` | OM v1 MLS Rooms are transferably authenticated and make no cryptographic-deniability claim. A future deniable-group mode must be a separate reviewed research profile and cannot weaken ordinary MLS semantics. | ADR-0138; [`GROUP_CHAT_ARCHITECTURE.md`](../groups/GROUP_CHAT_ARCHITECTURE.md) | V-004/V-016 |
| `G2` | HistoryBundle is recipient/Room/membership-bound inert archive material. Claimed historical EventIDs/authors never enter the live event namespace or execute any Room/control reducer. | ADR-0158; [`GROUP_CHAT_ARCHITECTURE.md`](../groups/GROUP_CHAT_ARCHITECTURE.md) | V-004 / conformance:group_history |

## Referral-token privacy

| Invariant | Normative property | Primary source | Evidence surface |
|---|---|---|---|
| `P1` | W0 referral tokens use RFC 9578 type `0x0002` with the exact pinned RFC 9474 Blind-RSA profile. | ADR-0132/0136 | V-016 |
| `P2` | Issuance transcript values alone do not let an issuer, or a verifier colluding with the issuer, map a redemption to the issuance transaction that produced it except with negligible advantage. | ADR-0137; [`INVITES_INTROGATE_AND_SPAM.md`](../discovery/INVITES_INTROGATE_AND_SPAM.md) | V-016 |
| `P3` | Every issuance uses a fresh independent CSPRNG blinding factor; reuse of a nontrivial factor is a protocol failure, not merely a test-quality issue. | ADR-0137 | V-016 |
| `P4` | P2 is cryptographic/protocol unlinkability, not a claim against network/timing/cardinality correlation or post-decryption ReferralAttestation disclosure. | ADR-0137; [`METADATA_THREAT_MODEL.md`](../../threat-model/METADATA_THREAT_MODEL.md) | V-015/V-016 |

## Negative capabilities and anti-capture

| Invariant | Normative property | Primary source | Evidence surface |
|---|---|---|---|
| `C1` | A provider receives only the credentials/keys needed for its role; forbidden powers are absent from its ordinary interface and key material. | [`IMPLEMENTATION_INTERFACES.md`](../providers/IMPLEMENTATION_INTERFACES.md); ADR-0137 | provider conformance |
| `C2` | Combining provider roles gives the operator the union of observations, never extra account/protocol authority. | ADR-0009; ADR-0130; ADR-0137 | V-015/provider conformance |
| `C3` | Relay/Home infrastructure cannot decrypt E2EE content or sign AccountControlState merely because it routes/stores it. | [`IMPLEMENTATION_INTERFACES.md`](../providers/IMPLEMENTATION_INTERFACES.md) | provider conformance |
| `C4` | Directory/alias infrastructure is discovery evidence, never controller or AccountID authority. | [`DISCOVERY_FABRIC.md`](../discovery/DISCOVERY_FABRIC.md); ADR-0009 | discovery conformance |
| `C5` | Recovery infrastructure stores/fans out recovery material but cannot independently become account authority or history plaintext authority; RecoveryRecord is advisory/freshness-checked state, and RecoveryRoot confidentiality is distinct from activation authority. | [`RECOVERY_AND_BACKUP_POLICY.md`](../identity/RECOVERY_AND_BACKUP_POLICY.md); [`IMPLEMENTATION_INTERFACES.md`](../providers/IMPLEMENTATION_INTERFACES.md) | V-002/provider conformance |
| `C6` | IntroGate admission authority ends at first-contact admission; it cannot mint the recipient issuer's private key or obtain sealed MessageRequest plaintext. | [`INVITES_INTROGATE_AND_SPAM.md`](../discovery/INVITES_INTROGATE_AND_SPAM.md) | V-016 |
| `C7` | Provider administration capability never substitutes for end-user account authorization: accepting/storing a signed object does not make it current or valid, a stolen capability is bounded to its named service resource until rotation, and bearer capabilities obey ADR-0141 transport/redaction/no-store hygiene. | ADR-0138; [`IMPLEMENTATION_INTERFACES.md`](../providers/IMPLEMENTATION_INTERFACES.md) | V-015/V-020 |

### Downgrade and wire-authority invariants

| ID | Invariant | Normative source | Verification/evidence owner |
|---|---|---|---|
| `D1` | SecurityAdvisory expiry, supersession or override may remove only an advisory-added floor; it never lowers the relationship's stored security floor or revives a forbidden profile. | ADR-0139; [`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](../providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md) | V-017 / PR-012 / `conformance:security_floor` |
| `D2` | ACS `minimum_crypto_profile` is an account-wide **1:1-only** floor. It must define `security_generation.one_to_one`; ACS ordering ignores `group`, while Room/group floors independently compare `security_generation.group`. | ADR-0126/0158; [`CRYPTOGRAPHY_AND_AGILITY.md`](../crypto/CRYPTOGRAPHY_AND_AGILITY.md) | V-003 / `conformance:security_floor` |
| `E1` | For frozen W0 wire objects, canonical CDDL + frozen registries/domain-separation rules define accepted signed/hashed bytes. Logical schemas cannot add fields or semantics; assigned W0 labels and the registry default 300-second skew are frozen. | ADR-0139; [`CANONICAL_ENCODING.md`](../crypto/CANONICAL_ENCODING.md) | V-005 / validator |
| `E2` | Published protocol/profile/extension meanings are immutable; negotiation never orders security by numeric version/ID, and incompatible semantics require a new immutable lineage/profile/extension rather than rewriting old meaning. | ADR-0162; [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../../process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) | validator / V-017 |

## Interpretation rule

If two APIs or implementations both appear to satisfy prose but one permits a call that omits a security-critical context, grants a provider a credential it does not need, or accepts a larger authority transition set, the narrower design wins unless an ADR explicitly changes the invariant.

The dedicated security-equivalence evidence pass has been executed and recorded in the verification artifacts, including bidirectional bounded accepted-set checks and negative-capability/metadata/issuance checks. That design evidence does **not** self-authorize any gate: toolchain/runtime-dependent execution and any gate-specific independent review remain separate obligations in the verification ledger. Future implementation contradictions require an ADR rather than silently weakening these invariants.
