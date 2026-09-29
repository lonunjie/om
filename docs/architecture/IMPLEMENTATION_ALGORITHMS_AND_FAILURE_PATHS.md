# Dangerous Algorithms and Failure Paths

- **Status:** Canonical normative implementation-order supplement
- **Checkpoint:** 2026-09-29 Run 6 / ADR-0164
- **Engineering:** not authorized

This document specifies *ordering and state-transition semantics*, not cryptographic primitives or implementation technology. Names in pseudocode refer to existing OM objects and state defined by the linked normative sources.

## 1. General rule: validate before effect

For any security-sensitive operation:

```text
load authenticated parent/context
→ validate structure and version
→ validate authority/eligibility against the authenticated parent
→ validate monotonicity/lineage
→ persist durable intent/replay context if the operation can cross an uncertainty boundary
→ perform the external/cryptographic effect
→ persist canonical completion (or durable recovery state)
→ release application-visible effect / acknowledgement
```

A failure before canonical completion is not success. A timeout after an external effect may be **unknown**, not rejected.

Never reorder a later check ahead of an earlier one merely for performance if the reorder can consume a one-time secret, create authority, release plaintext, or create durable application state.

---

## 2. AccountControlState child validation

**Sources:** [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §§2–8; [`account-control-transition-policy.json`](../../spec/protocol/account-control-transition-policy.json); ADR-0137/0161.

### Algorithm

```text
validate_acs_child(parent, child, signer_set, recovery_observation):
  1. reject if parent is terminal/retired
  2. require child.previous_state_hash == canonical_hash(parent)
     OR enter the separately defined fork-bridge path
  3. require child.generation == parent.generation + 1
  4. parse and validate the child object strictly
  5. derive ALL deltas by diffing child against parent
  6. reject if any delta is unlisted
  7. reject if any permanently_revoked DeviceID appears in messaging/control eligibility
  8. validate profile-floor transition on security_generation.one_to_one only
  9. determine the authorization requirement for EVERY delta
     - a combined child must satisfy all structural constraints
     - its required signer threshold is the strongest applicable row
     - a signer that is forbidden for one delta cannot be counted merely because another delta permits it
 10. count ordinary signers only from parent active AND non-quarantined controllers
 11. apply recovery only where that delta row permits it; recovery never substitutes for a freeze/fork restriction
 12. apply safety-freeze rules if the parent is quarantine-frozen
 13. require all transition-specific postconditions (revocation tombstones, nonzero voting controllers, etc.)
 14. accept only after the child hash/signatures and all derived invariants validate
```

### Critical edge cases

- A revoked DeviceID is terminal even if the physical device is reinstalled. Re-enrollment uses a fresh DeviceID.
- `minimum_crypto_profile` is ordered only by `security_generation.one_to_one`; group generation is ignored.
- A child combining deltas is not authorized by whichever single delta happens to have been checked first.
- Safety-freeze resolution is a distinct transition class; recovery cannot silently become a freeze vote.
- A larger `generation` without the authenticated parent/authorization chain is not authority.

### Forbidden shortcuts

- last-write-wins for constitutional state;
- counting quarantined controllers as ordinary voters;
- reviving a revoked DeviceID;
- ordering profiles by numeric profile ID or group generation;
- accepting a child because it has “more signatures” without applying the parent transition policy.

---

## 3. One-to-one inbound + sealed-delivery acceptance

**Sources:** [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md); [`SEALED_SENDER_DELIVERY_PROFILE.md`](delivery/SEALED_SENDER_DELIVERY_PROFILE.md); ADR-0142–0147.

### Algorithm

```text
receive_outer(envelope, RelationshipContext):
  1. parse bounded outer envelope; reject malformed/expired data
  2. validate delivery-key / route binding
  3. durably create-or-resume RESERVED(replay_key, exact_wrapper_fingerprint)
     before plaintext release or successful acknowledgement
  4. open the outer HPKE layer
  5. if authenticated peer ACS head may be stale:
       keep RESERVED; refresh/reconcile; do not release plaintext
  6. bind inbound IK_A to exactly one active, non-quarantined DeviceCredential
     under the authenticated peer AccountControlState
  7. validate ProfileExecutionContext / CryptoProfile eligibility
  8. consume/use session material only after the preceding identity/authority checks
  9. apply session dedupe using the canonical logical EventID
 10. atomically/equivalently commit session state + dedupe + replay COMPLETE
 11. only after that commit, materialize/release the application event and acknowledge
```

### Replay states

```text
ABSENT → RESERVED → COMPLETE
              ↘ REJECTED
```

A crash while `RESERVED` permits the **same exact wrapper** to resume. A different wrapper under the same replay identity is rejected. `COMPLETE` is idempotent and never repeats application effects.

For first-contact MessageRequest bootstrap, the request-scoped candidate/response state follows the same rule: exact candidates are durable, bounded, and terminalized atomically when the request leaves `OPEN`; bootstrap keys are never promoted into stable relationship delivery state.

### Forbidden shortcuts

- authenticating a sender from outer sealed-delivery opening alone;
- consuming a one-time key before identity/authority validation;
- treating stale ACS as definitive rejection;
- acknowledging before durable replay/session completion;
- creating a new logical message because a transport acknowledgement was lost;
- restoring ratchet/prekey state from backup.

---

## 4. Group Commit preparation/publication/recovery

**Sources:** [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md); [`group-control-transition.logical.schema.json`](../../spec/protocol/group-control-transition.logical.schema.json); ADR-0163; V-004.

### Journal states

```text
INTENT_DURABLE
      ↓
CANDIDATE_PREPARED
      ↓
PUBLISHED_UNCONFIRMED
      ↓
ACCEPTED
```

Terminal alternatives from an unaccepted candidate are `SUPERSEDED` and `ABANDONED`.

### Algorithm

```text
prepare_group_transition(intent, canonical_parent):
  1. authenticate/authorize intent against canonical parent GroupControlState
  2. persist INTENT_DURABLE
  3. create the exact MLS Commit/Binding for that parent
  4. persist exact Commit bytes/hash + tentative resulting state as CANDIDATE_PREPARED
  5. publish the exact candidate
  6. persist PUBLISHED_UNCONFIRMED
  7. reconcile the candidate against the currently canonical group chain
  8. if accepted:
       atomically/equivalently promote MLS state + GroupControlState + canonical head + dedupe marker
       mark ACCEPTED
  9. if superseded/stale/invalid:
       mark SUPERSEDED or ABANDONED; candidate becomes inert
 10. only ACCEPTED state may produce application/group-control effects
```

### Restart rules

- If the exact prepared Commit exists and its parent is still canonical, retry the **same bytes**.
- If its parent is no longer canonical, do not manufacture an “equivalent” Commit; reconcile and rebase/re-prepare as required by the intent class.
- If prepared state is missing, fail closed and require authenticated reconciliation.
- A relay ACK is transport evidence only, never canonical acceptance.
- No message may be encrypted/sent against a tentative branch.
- Reducing operations rebase only after re-validating authorization at the new parent; non-reducing operations are dropped if their authorization no longer holds.

---

## 5. Mutable RelaySet / BlobLocationRecord branch selection

**Sources:** [`REACHABILITY_AND_OBJECT_REPAIR.md`](delivery/REACHABILITY_AND_OBJECT_REPAIR.md); [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md); ADR-0161.

### Algorithm

```text
accept_mutable_record(candidate, retained_lineage):
  1. authenticate signer against the newest validated ACS available
  2. validate genesis/successor rules and canonical parent hash
  3. if exact parent is missing: retain as PENDING, not permanently invalid
  4. reject version jumps, cross-lineage parents, malformed genesis and invalid hashes
  5. retain structurally valid branches without letting arrival order select one
  6. among eligible candidates, select:
       highest version
       then lexicographically smallest canonical record hash
  7. if signer later becomes ineligible, remove that candidate from effective selection
     but retain sufficient lineage metadata to validate descendants
```

The selected record is availability/location state only. It never becomes identity, relationship or message authority.

---

## 6. HistoryBundle import isolation

**Sources:** [`GROUP_CHAT_ARCHITECTURE.md`](groups/GROUP_CHAT_ARCHITECTURE.md) history rules; [`history-bundle.logical.schema.json`](../../spec/protocol/history-bundle.logical.schema.json); ADR-0161.

### Algorithm

```text
import_history_bundle(bundle, authenticated_recipient_context):
  1. validate bundle authentication/encryption envelope
  2. require exact ConversationID match
  3. require target AccountID and immutable membership incarnation match
  4. require author eligibility and bound GroupControlState/MLS epoch context
  5. validate item/resource bounds
  6. assign bundle-local history_item_id namespace
  7. store as inert archive/display material
  8. never insert claimed original EventIDs into live dedupe
  9. never feed bundle contents to GroupControlState, MLS, roles/grants,
     reactions/edits/deletes, polls/votes, read state or scheduled-send reducers
```

Claimed original author/time/EventID are provenance labels only. Rendering must not imply authenticated original authorship that the importing device cannot verify.

---

## 7. Extension dispatch

**Sources:** [`MESSENGER_BASELINE_V1.md`](events/MESSENGER_BASELINE_V1.md); ADR-0148/0149/0155/0157.

```text
dispatch_extension(ext, containing_object):
  1. parse extension envelope under strict canonical encoding
  2. determine wire-visible criticality class
  3. if the extension's wire criticality conflicts with its registered class:
       reject containing object
  4. if extension is unknown:
       critical → reject containing object
       non-critical → ignore extension; preserve base semantics
  5. if known and non-critical:
       schema/object/reference/semantic failure → ignore whole extension
       and preserve the authenticated base object
  6. if known and critical:
       any required schema/semantic failure → reject containing object
  7. apply only semantics registered for that extension/object/version
```

A logical JSON schema with permissive `additionalProperties` does not expand wire semantics; canonical CDDL, registries and domain-separation rules remain authoritative.

The future-critical `!` namespace rule prevents a sender from changing validity by merely clearing the critical bit on an extension unknown to an older registry.

---

## 8. LAN frame/channel admission

**Sources:** [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md); ADR-0157.

```text
accept_lan_connection(channel):
  1. establish bounded TLS 1.3 transport
  2. derive exporter bytes from the exact TLS channel
  3. verify both role-separated HMAC channel proofs against the relationship/invite secret
  4. reject any identity-bearing OM object before channel proof succeeds
  5. after proof, accept only the closed LanDeliveryProfile-v1 frame set
  6. validate frame length before decoding/allocating payload storage
  7. route offer must be ordinary RelayEntry + signed route-bound SealedDeliveryKeyRecord
  8. message delivery must use canonical SealedDeliveryEnvelope / MessageRequest rules
  9. blob requests are receiver-pull, 1..65536 bytes, max four outstanding per blob/channel
 10. reject unsolicited/out-of-bounds blob data and validate ciphertext_digest before plaintext release
 11. LAN reachability never grants relationship, device, Room membership or authority
```

A PWA that cannot expose equivalent channel binding reports `local_delivery=unavailable`; it does not use a weaker browser-only mode.

---

## 9. Unknown provider/network outcome

**Sources:** ADR-0163; Home API; implementation-readiness transaction contract.

```text
send_mutation(op):
  1. persist stable request/logical id and security parent/context
  2. issue exact operation
  3. if definitive authenticated success → COMMITTED
  4. if definitive authenticated rejection → REJECTED(reason)
  5. if transport failed before any possible effect → PENDING_NETWORK
  6. if effect may have happened but outcome is unknown → PENDING_RECONCILIATION
  7. reconcile using the same idempotency identity
  8. never originate a second semantic operation merely because the first response was lost
```

Provider restart does not reset the idempotency horizon. Same ID + same authenticated operation resumes the same semantic operation; same ID + different operation is rejected.

---

## 10. Cross-cutting adversarial examples

An implementation is non-conforming if it does any of the following:

| Shortcut | Required behavior |
|---|---|
| “Higher ACS generation wins.” | Validate parent hash/fork path and authorization first. |
| “Revoked phone came back, so reuse its DeviceID.” | Fresh DeviceID/DeviceCredential. |
| “Group Commit ACK means accepted.” | Persist as unconfirmed; reconcile canonical chain. |
| “Equivalent new Commit is fine after restart.” | Retry exact prepared bytes if parent remains canonical; otherwise rebase/re-prepare. |
| “History EventID looks valid, so dedupe it against live events.” | Keep bundle-local history IDs; claimed IDs are inert provenance. |
| “Unknown extension can be rendered as plain text.” | Apply criticality rules; unknown critical rejects, unknown non-critical is ignored. |
| “LAN peer is nearby, so it is trusted.” | Prove relationship/invite-secret possession over the bound channel. |
| “HTTP timeout means failure.” | Distinguish no-effect failure from unknown outcome and reconcile exact operation. |
| “A stale ACS means reject immediately.” | If staleness is the only uncertainty, hold/reconcile; definitive invalidity rejects. |
| “Schema accepts extra JSON fields, therefore wire supports them.” | Wire CDDL/registries remain authoritative. |

## 11. Verification boundary

This document is a design/implementation contract, not a substitute for evidence. Existing obligations remain:

- **V-002:** AccountControlState authorization/fork/recovery formal validation;
- **V-003:** 1:1/session/sealed-delivery model and independent review;
- **V-004:** group wire/two-codec/formal/runtime evidence;
- **V-005:** extension/encoding evidence, including the Rust-dependent criterion already open;
- **V-024:** LAN CDDL, two-codec, feasibility and real-LAN interoperability evidence.

No algorithm in this document authorizes engineering or marks a gate passed.
