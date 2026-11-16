# ADR-0192 — Device-link ceremony: key schedule, transcript MAC, comparison code, replay and expiry context, and the notice signer role

- **Status:** Accepted
- **Date:** 2026-11-17
- **Decided by:** operator delegation 2026-11-16, to fill silence and fail closed. This ADR fills silence only and amends no audited text.
- **Class:** `additive_noncritical_extension` ([`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) section 2, strongest applicable class; ADR-0165). Nothing published changes meaning: the label `OM-DEVICE-LINK-CEREMONY-MAC-v1` is `reserved` in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) ("a reserved construction may be filled once"), the provisional derivation in `flows/ceremony.rs` was never specified or released as normative (deviation D24, gap 1 in [`2026-10-28-relay-status.md`](../docs/reports/2026-10-28-relay-status.md)), and `DeviceEnrollmentNotice` (W0, frozen) is not touched. Three new labels are added. The link payload version byte moves from 1 to 2; a version 1 link is refused with a plain message (section 6). Frozen W0 corpus unchanged.
- **Amends:** nothing. **Does not amend:** [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) (4.1, 12), [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md), [`account.cddl`](../spec/cddl/account.cddl), ADR-0119, ADR-0127, ADR-0181, D24 (its transport decision of 2026-11-04 stands).

## Design sources read

Read in full: [`AGENTS.md`](../AGENTS.md) "Design gate"; [`AGENT_ARCHITECTURE_RULES.md`](../docs/process/AGENT_ARCHITECTURE_RULES.md); [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md); [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md); ADR-0191 (as the house format); the D24 and D27 rows of the relay status report; [`2026-11-04-multi-device-core-status.md`](../docs/reports/2026-11-04-multi-device-core-status.md); the omitted-gap answers in [`2026-11-13-multi-device-phase5-status.md`](../docs/reports/2026-11-13-multi-device-phase5-status.md); the ceremony code of `crates/om-core/src/flows/ceremony.rs` (key schedule, envelope wrapping, link encode/decode, offer, request, inbound, approval, notice signing and staging, sibling `on_enrolled`; the unit tests were not read).

Read in part: [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) (the 4.1 authorization row, the ceremony paragraph at line 209, sections 12, 13 and the continuity table; sections 1 to 3 and 5 to 11 were only searched); [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) section 2; [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) (rules, constructions and the label entries involved); [`om-object-types.json`](../spec/registries/om-object-types.json) type 8; [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json) `enrollment_ceremony`; [`account.cddl`](../spec/cddl/account.cddl) (`DeviceEnrollmentNotice`); [`CONTENT_AND_MICROCOPY_VOICE.md`](../docs/ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md) (rules 1 to 8).

Not read (stated, not claimed): the other documents and ADRs that `python3 tools/design_gate.py needs crates/om-core/src/flows/ceremony.rs` prints for `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events` (about 30 documents and 49 ADRs), in particular `ONE_TO_ONE_SESSION_MANAGEMENT`, `ASYNC_KEY_MATERIAL`, `CRYPTOGRAPHY_AND_AGILITY`, `PRODUCTION_CRYPTO_INTEGRATION_GATE`, `TIME_AND_EXPIRY_SEMANTICS`, ADR-0143, ADR-0144, ADR-0154, ADR-0181 in full, ADR-0126, ADR-0129; the V-021 model and the TLA+ models. If any of those fixes a construction for this ceremony, that text wins and this ADR must be corrected by a follow-up, not shipped around.

## Context

The ceremony of D24 has two open items. (1) The registry reserves `OM-DEVICE-LINK-CEREMONY-MAC-v1` for a "short-lived linking-ceremony transcript MAC" with no construction, and the provisional code uses that one label for four different jobs (HKDF salt and info, AEAD associated data, the mailbox id hash, the code MAC), which the registry rule "a label names exactly one use" forbids; it also leaves the Home URI, the link expiry and the version out of the transcript, truncates the code with a 32-bit modulo, and never binds the approval to the request it answers beyond echoing a nonce. (2) The notice signer role was closed on 2026-11-13 by reading the registries; that reading is recorded here so it has a decision number and a test.

## Decision

### 1. Notation and labels

OM-CBOR is the canonical deterministic CBOR of [`CANONICAL_ENCODING.md`](../docs/architecture/crypto/CANONICAL_ENCODING.md). `DS-HASH(label, body)` is the registry construction, taken as the raw 32-byte SHA-256 (as `ds_hash` does). HKDF is HKDF-SHA-256 (RFC 5869); HMAC is HMAC-SHA-256.

| Label | Use | Status after this ADR |
|---|---|---|
| `OM-DEVICE-LINK-CEREMONY-MAC-v1` | `mac`: the two ceremony MACs of sections 4 and 5, as `MACIN` below | reserved, now filled |
| `OM-DEVICE-LINK-CEREMONY-KDF-v1` | `kdf`: context hash, salt and expand info of section 3 | new |
| `OM-DEVICE-LINK-CEREMONY-AEAD-AD-v1` | `aead_ad`: associated data of every sealed ceremony message | new |
| `OM-DEVICE-LINK-CEREMONY-MAILBOX-HASH-v1` | `hash`: the local polling id of the response queue | new |

`MACIN(role, body) = OM-CBOR [ "OM-DEVICE-LINK-CEREMONY-MAC-v1", role: uint, body: bstr ]`. Roles: 1 = comparison code, 2 = approval binding. Role is inside the MAC input, so one label serves one construction with two named roles and a code can never be replayed as a binding.

### 2. Inputs

All inputs are in the link both devices hold, and nowhere else: `version` = 2, `expiry_s` (POSIX seconds, uint), `home_uri` (the exact UTF-8 bytes of the link), `A_cap` (deposit capability of the request queue), `B_recv` (receive capability of the response queue), and `S`, the one-time secret: at least 16 random bytes from the platform CSPRNG, never reused and never derived. `S` appears in the link and nowhere on the wire.

### 3. Key schedule

```text
CTX         = OM-CBOR [ 2, expiry_s, home_uri, A_cap, B_recv ]
ceremony_id = DS-HASH("OM-DEVICE-LINK-CEREMONY-KDF-v1", CTX)           ; 32 bytes, also the transcript hash
PRK         = HKDF-Extract(salt = ceremony_id, IKM = S)
K_x         = HKDF-Expand(PRK, info = OM-CBOR [ "OM-DEVICE-LINK-CEREMONY-KDF-v1", purpose ], 32)
              purpose: "req" (new to existing), "resp" (existing to new), "code", "bind"
```

The salt is the public context hash, so a change to the Home, the expiry, the version or either capability changes every key. No key is used for two purposes.

### 4. Sealed messages

Every ceremony message is sealed with XChaCha20-Poly1305: key `K_req` for new-to-existing, `K_resp` for existing-to-new; a fresh random 24-byte nonce per message; associated data `AD = OM-CBOR [ "OM-DEVICE-LINK-CEREMONY-AEAD-AD-v1", ceremony_id, role ]` with role 1 for the request direction and 2 for the response direction. The padding, bucket and type-23 envelope wrapping of D24 note 9 are unchanged. A message opened under the wrong direction key or AD is dropped without effect.

### 5. Comparison code and approval binding

`request_bytes` is the exact plaintext of the request message as the new device sent it; both devices MACs those bytes and never re-encode them.

```text
m    = HMAC(K_code, MACIN(1, request_bytes))
code = ( big-endian u64 of m[0..8] ) mod 1_000_000, written as six digits with leading zeros,
       shown as two groups of three ("482 913")
```

The two devices show the code; a person compares them by eye. The code is never typed, so there is no online guessing surface and no retry counter. The code informs the person and does not gate approval by itself. A mismatch means the request the existing device holds is not the one the person's new device sent, so the only correct action is Reject. Reject, expiry or any failure ends the ceremony; the link is then dead (section 7).

The approval message gains key 11, `binding`:

```text
binding = HMAC(K_bind, MACIN(2, OM-CBOR [ request_digest, approver_device_id, new_state_hash,
                                          notice_digest, roster_count, history_count ]))
request_digest = SHA-256(request_bytes); new_state_hash = the 32-byte hash of the successor
AccountControlState; notice_digest = SHA-256(signed DeviceEnrollmentNotice bytes);
the counts are the values of approval keys 7 and 9.
```

The new device recomputes `binding` from what it received and compares in constant time before it adopts anything. A missing or wrong binding is a refused approval (nothing adopted, nothing published). The AEAD already authenticates each message; the binding states, once and testably, which request, successor and notice the approval is for.

`recv_mailbox = DS-HASH("OM-DEVICE-LINK-CEREMONY-MAILBOX-HASH-v1", B_recv)[0..16]` (a local polling id only).

### 6. Replay and expiry context

1. **Lifetime.** At most 300 s from the offer (`CEREMONY_TTL_MS`). The new device refuses a link whose `expiry_s` is at or before its wall-clock now, or more than 360 s after it (60 s tolerance for a device with an empty profile and only a wall clock). The existing device uses its own recorded expiry. Neither side accepts any ceremony message at or after expiry; every envelope's `expires_at` is at most `expiry_s`.
2. **Freshness.** A new offer replaces any unapproved earlier one (which becomes Expired) and draws a new `S` and new capabilities. A secret or capability is never reused across offers. A message from another ceremony fails the AEAD open.
3. **Once.** The first well-formed request is accepted once and later ones are ignored (one offer, one device; D24 note 7). Approve and Reject are terminal. Each later message type is idempotent: an introduction once per conversation, a roster entry and a history chunk once per index. A replayed or duplicated message is a no-op and never advances state. Approval also needs the ADR-0181 single-use confirmation nonce.
4. **Cleanup.** At Linked, Rejected, Expired or any failure the device deletes `S`, the four keys and the capabilities and keeps only a secret-free summary.
5. **Versions.** A version 1 link is refused ("This link came from an older version of OM. Make a new one on your other device."). Nothing interoperates with the provisional derivation.

### 7. Notice signer role (closes D24 gap 2 as a decision)

A `DeviceEnrollmentNotice` for `ceremony = 1 (link_ceremony)` is signed by the approving device as role **`controller`** (the DS-SIG role, label `OM-DEVICE-ENROLLMENT-NOTICE-SIG-v1`), with `device_class` 1 (messaging), `enrolled_by_device_id` equal to the signer, `acs_generation` and `acs_state_hash` those of the successor that adds the device, and `enrolled_at` the approver's POSIX seconds (display only, never authority). A receiver accepts it only if the signer is in the `active_control_devices` of the state the notice names, is not quarantined there, has a credential that state lists, and the notice's own device is the one that state newly lists. The `recovery` role stays reserved for `ceremony = 2 (recovery_completion)` and `genesis` (3); a notice for ceremony 1 signed as `recovery` is rejected. A messaging device never signs a ceremony notice.

## Where did authority go?

Nowhere new. Authority to add the device is still exactly one `link_messaging` successor authorized by the parent state (ACCOUNT_CONTINUITY 4.1). The MACs and the code authenticate the ceremony's conversation, never the account: the new device still adopts nothing unless the chain lists its own rebuilt credential and a verified notice accompanies it. Whoever holds the link can race a request; the person's comparison of the code (and the Reject button) is the control, and the one-offer-one-device rule bounds the race to one attempt of probability about 10^-6 per link for a code match by luck.

## Consequences

Provisional and final builds do not interoperate (links are five minutes old at most, so the cost is one retried link). The registry gets four lines changed or added in the same change as the vectors. Residual: a six-digit code is a human check, not a cryptographic proof, and a person who approves despite a mismatch links the attacker's device; the notice then appears on every other device (ADR-0193 covers the offline ones), where Devices lists it and removal is one action. A longer code for a Hardened profile would change only the modulus and needs a link-format flag; it is not decided here.

## Work list for a builder

1. **Code** (`flows/ceremony.rs`): replace `LBL_CEREMONY` uses by the four labels; implement `ceremony_keys` per section 3 (including `expiry_s`, `home_uri` and version in `CTX`); `aad` per section 4; `verification_code` per section 5 (u64 modulo, grouped display string; keep the plain six-digit field in the views); the `binding` field (approval key 11) built in `approve_inner`, verified in `on_approval` before any adoption; `recv_mailbox` per section 5; link version 2 in `encode_link`/`decode_link` with the refusal copy for version 1; the expiry window checks of section 6.1; zeroization of section 6.4.
2. **Types and events:** `RejectReason::Expired` for a version 1 link or a stale expiry (no new variant needed); `LinkRequestView.code` stays a string; add `code_display` (grouped) only if the UI wants it; no new intent or query.
3. **Registry** ([`spec/registries/om-domain-separation.json`](../spec/registries/om-domain-separation.json)): set the MAC label to `frozen` with the construction text of section 1 and 5; add the KDF, AEAD-AD and MAILBOX-HASH labels as `frozen` only after vectors pass. This is in design-bound `spec/`: read the six printed domains first and carry the trailers.
4. **Vectors** (`verification/vectors/device-link-ceremony-vectors.json.gz`, generated by an independent implementation in `verification/prototypes/device-link-ceremony/`, plus the Rust loader): fixed `S`, `A_cap`, `B_recv`, `home_uri`, `expiry_s`, a fixed request and approval; expected `CTX`, `ceremony_id`, `PRK`, all four keys, `AD` for both roles, the code and its digit-string for several `m` including values near the modulus and leading zeros, the binding tag, one sealed message with a fixed nonce, the mailbox id. Negatives: changed `home_uri`, `expiry_s`, version, either capability (all keys change); request re-encoded (code differs); code MAC replayed as binding (role differs); wrong direction key or AD; binding over a different request, state hash, notice or count; a `recovery`-role notice for ceremony 1; a notice signed by a messaging or quarantined device or one not listed in the named state.
5. **Tests:** two Cores link (existing `device_link.rs` stays green); a second request while one is pending is ignored; a wrong-code Reject ends the ceremony and a later request on the same link is dropped; expiry at both ends (offer, request, approve, and a response arriving after expiry); a v1 link is refused; a replayed request, approval, roster entry and history chunk are no-ops; an approval with a bad binding adopts nothing; secrets are absent from persisted state after each terminal state.
6. **Docs:** update D24 gap 1 in the relay status report to point here; add the `ceremony` and `signer` rules to the Core status report; keep the vectors' README stating which codec produced them.
7. **UI copy keys** (`apps/web/src/copy/en.ts`; no jargon): `joinCopy.codeCompare` ("Check that both devices show this code: {code}"), `joinCopy.codeMismatch` ("The codes are different. Don't approve. Start over on both devices."), `joinCopy.oldLink` (the version 1 message above), `devicesCopy.linkRequestRow` unchanged.

## UX line

Both devices show one short code; if the two codes match you approve, and if they differ you say no and start over.

## Traces

Registry: [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) (after the builder's change); D24 in [`2026-10-28-relay-status.md`](../docs/reports/2026-10-28-relay-status.md). Requirement R-431, threat T-175, decision D-208. Companion ADRs: ADR-0193 (enrollment liveness), ADR-0194 (direct transfer receipt).
