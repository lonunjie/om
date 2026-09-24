# Architecture review R1: identity, crypto, discovery, delivery, storage

- **Reviewer:** Claude Opus (claude-opus-5-5), independent reviewer under ADR-0117
- **Date:** 2026-09-24
- **Base commit:** b33c518
- **Scope:** `docs/architecture/identity/`, `docs/architecture/crypto/`, `docs/architecture/discovery/`,
  `docs/architecture/delivery/`, `docs/architecture/storage/` (ChatGPT-authored text). Checked against ADRs,
  `machine/` ledgers, `spec/cddl`, `spec/registries` (frozen, ADR-0119), `spec/protocol`, `spec/metadata` and
  [`verification/tla/AccountControl.tla`](../tla/AccountControl.tla).
- **Brief:** reviewer R1. Priority: security flaws, then contradictions with frozen spec/ADRs/ledgers, then
  underspecification that would make honest implementations diverge, then constitutional violations, then minor
  issues. Claude-authored text (ADR-0110+, sections headed "2026-09-24 ...", and the ADR-0111 amendments in
  [`RECOVERY_AND_BACKUP_POLICY.md`](../../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) §1/§10 and [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §11) is context only.

**Verdict:** The core ideas are sound: random AccountID, capability-addressed relays, disjoint one-time pools,
parent-state authorization, and "backups restore history, never authority". But the older ChatGPT text was not
reconciled with ADR-0111 (recovery on by default, one vote, delay and veto). As a result several passages still let a
recovery secret, a guardian quorum or a single device exercise authority with no delay and no veto. First-contact
cryptography, the per-transition authorization table and continuity proofs are unspecified, although all three sit
inside W0 `core_kernel`. W0 engineering on identity and IntroGate should wait for the three blockers.

## Findings

### F-R1-1 — RecoveryPolicy is still treated as standalone authority, bypassing the ADR-0111 delay, veto and one-vote rule

Severity: blocker
Where: [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §3 lines 66-68, §4 Stage B lines 90-93, §5 line 127, §8 line 167;
[`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../../docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) §2 rule 1 (line 32); [`RECOVERY_AND_BACKUP_POLICY.md`](../../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) §7 lines 116-127;
[`IDENTITY_PROTOCOL.md`](../../docs/architecture/identity/IDENTITY_PROTOCOL.md) line 209; [`machine/requirements.jsonl`](../../machine/requirements.jsonl) R-171.
Problem: ADR-0111 rule 4 says the identity key confers authority "only through rule 2" (delay plus veto, or co-sign by
a current controller), and rule 3 makes it exactly one vote. The TLA model agrees: `Retire` requires `Authorized`,
which needs a non-empty controller set for a recovery vote, and `ResolveFreeze` needs a device-only majority. The
older prose says otherwise:
- permanent revocation can be done by "a valid configured RecoveryPolicy authorization" (§4);
- a safety freeze and a fork are resolved by "constitutional quorum **or** RecoveryPolicy" (§5, §8);
- recovery authority is "logically above ordinary control-device disputes" (§3);
- AccountRetirement is authorized by "a valid RecoveryPolicy" with no delay (Lifecycle §2; R-171);
- activation defenses are optional ("may include"), and Maximum Privacy "may choose none of these" (Recovery §7).

With the default custodian (a synced passkey, which is phishable through the platform account, §10.7), an
implementer following these passages could let the custodian alone permanently revoke every controller or
irreversibly retire the account, with no veto window. That is exactly the silent takeover ADR-0111 forbids.
Proposed fix: In each cited passage, replace "or a valid RecoveryPolicy" with "or a RecoveryPolicy vote under
[`RECOVERY_AND_BACKUP_POLICY.md`](../../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) §10.3-10.4: immediate only with ≥1 current non-quarantined controller co-signing,
otherwise only after the activation delay with no RecoveryVeto". State explicitly that AccountRetirement with
`authorization_kind = recovery_policy` is subject to the same delay and veto. State that freeze and fork resolution
need a device-only majority, matching the TLA `ResolveFreeze`. Rewrite §3 line 68 as "counts as one vote". Rewrite
Recovery §7 so that delay (≥24 h) and live-controller veto are mandatory whenever recovery is on. Amend R-171 to match.
Resolution: accepted — ADR-0127 rule 1; recovery is one vote with delay and veto in ACCOUNT_CONTINUITY_AND_CONTROL §3/§4/§5/§8, ACCOUNT_LIFECYCLE §2 rule 1, RECOVERY_AND_BACKUP_POLICY §7 (delay and veto mandatory); R-171 amended, R-200.

### F-R1-2 — ContinuityProof has no authorization rule, and guardian or single-device continuity bypasses constitutional control

Severity: blocker
Where: [`IDENTITY_CONTINUITY.md`](../../docs/architecture/identity/IDENTITY_CONTINUITY.md) Level 1 lines 88-98, Level 2 lines 100-115, UX lines 188-193; [`IDENTITY_PROTOCOL.md`](../../docs/architecture/identity/IDENTITY_PROTOCOL.md)
ContinuityProof lines 152-161; [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../../docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) §5 line 67.
Problem: A ContinuityProof moves contacts from Account A to Account B automatically ("Alice moved to a new OM account.
Your conversation and contact stay the same"). But it can be produced by "an existing trusted device" (Level 1), and
"any 3 authorized guardians" can bind a **new** Account B as successor (Level 2). Neither path requires A's
constitutional majority, the ADR-0111 delay, or a live-controller veto. Neither retires A, so A and B coexist.

A stolen messaging-only device, or colluding guardians, can therefore redirect every relationship to an
attacker-controlled account and get the strongest continuity UX while the real owner still holds working
controllers. Level 2 also contradicts the current recovery design, where guardians hold Shamir shares of a second
RecoveryRoot that recovers the **same** AccountID through §10.3.

ContinuityProof has no schema in `spec/`, no requirement, and no V-002 coverage. It also overlaps with
`AccountRetirement(moved, successor)`, which is constitutionally authorized.
Proposed fix: Make `AccountRetirement(reason_class = moved, successor_account_id)`, authorized under F-R1-1's rules,
the only cryptographic continuity proof that clients render as "moved". Delete the "existing trusted device" and
"guardians co-sign a successor" paths from Level 1 and Level 2, or require them to produce that same retirement
object. Guardian recovery keeps the same AccountID and goes through §10.3. Mark anything else as Level 3 (human
evidence). Add a requirement and a V-002 case for "successor pointer without constitutional authority is rejected".
Resolution: accepted — ADR-0127 rule 3; only AccountRetirement(moved) renders as moved: IDENTITY_CONTINUITY Levels 1-2, IDENTITY_PROTOCOL ContinuityProof, ACCOUNT_LIFECYCLE §5; R-201; V-002 remaining item added.

### F-R1-3 — First-contact (MessageRequest) cryptography is unspecified inside W0

Severity: blocker
Where: [`INVITES_INTROGATE_AND_SPAM.md`](../../docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) §4-§5 (lines 55-88); [`DISCOVERY_FABRIC.md`](../../docs/architecture/discovery/DISCOVERY_FABRIC.md) §7 lines 146-162;
[`IDENTITY_PROTOCOL.md`](../../docs/architecture/identity/IDENTITY_PROTOCOL.md) lines 122-133 and 304; [`CONTACT_SHARING.md`](../../docs/architecture/discovery/CONTACT_SHARING.md) line 37; [`spec/cddl/discovery.cddl`](../../spec/cddl/discovery.cddl) (no
MessageRequest object).
Problem: IntroGate and first contact are in W0 `core_kernel` ([`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../../docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md)), yet
no in-scope document or schema says:
1. how the MessageRequest payload is encrypted and to which key (the alias-specific intro identity, or recipient
   device prekeys from a KeyMaterialProvider);
2. whether the sender's AccountID or device is hidden from the IntroGate (the W0 metadata descriptor claims
   `stable_account_id: no_by_design` for `introgate`);
3. what object performs the "E2EE AccountID proof / binding" after acceptance, which ties the intro identity to the
   recipient's AccountControlState.

Without item 3, a directory or IntroGate that substitutes the intro identity is undetectable even by a sender who
already knows Alice's AccountID. Two honest implementations will choose different constructions and will not
interoperate.
Proposed fix: Add a design-queue item and an ADR that define:
- `MessageRequest`: a sealed payload to a recipient-published request key bound to the intro identity, a
  sender-identity-hiding rule, and the size bound taken from the IntroductionTicket;
- `IntroIdentityBinding`: a signature under a controller or device credential of the recipient ACS over the intro
  public key, disclosed only inside the accepted relationship;
- the transition from an accepted request to PQXDH session establishment.

Add both objects to the W0 CDDL through an ADR-0119 addendum, and add V-015/V-016 vectors.
Resolution: accepted — ADR-0126: RequestKey, MessageRequest (HPKE base mode), MessageRequestPayload, IntroIdentityBinding in discovery.cddl and registries; INVITES_INTROGATE_AND_SPAM §5.1, DISCOVERY_FABRIC §7; R-202, R-203; V-015/V-016 remaining items.

### F-R1-4 — There is no per-transition authorization table, and IDENTITY_PROTOCOL's default authority policy contradicts the control plane

Severity: major
Where: [`IDENTITY_PROTOCOL.md`](../../docs/architecture/identity/IDENTITY_PROTOCOL.md) "Default authority policy" lines 204-215 and DeviceIdentity `revoked_at?` line 78;
[`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §4 lines 72-109, §6 lines 133-141, §10 line 191; [`spec/cddl/account.cddl`](../../spec/cddl/account.cddl)
(`quarantined_control_devices` only).
Problem: IDENTITY_PROTOCOL says:
- "revoke device: authorized device", but permanent revocation is constitutional (§4 Stage B);
- "add device: one trusted authorized device", but §6 says only a *control* device may link;
- "recovery-policy replacement: explicit confirmation under current account authority", but it is constitutional
  (§1, plane 3).

ACCOUNT_CONTINUITY never states who may issue a quarantine (any controller? any device?). It never states what
authorizes removing a *messaging* device, and whether messaging devices can be quarantined at all (the ACS has no
field for it). It never defines "disable a configured safety threshold" (§4 rule 4). Because the ACS body does not
record the transition type, a verifier must diff parent and child to decide which rule applies. That rule set is the
"formal authorization-state transition table" that §10 lists as required and that no document provides. `revoked_at`
on DeviceIdentity implies time-based revocation state outside the ACS.
Proposed fix: Add to [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) a normative table with one row per ACS delta. Each row gives
the required signer roles and count, measured against the parent state, and whether the delta may be single-signer:
- link messaging device;
- promote;
- quarantine control device;
- quarantine or remove messaging device;
- permanent revoke;
- recovery-policy change;
- floor change;
- retirement.

Replace IDENTITY_PROTOCOL lines 204-215 with a pointer to that table. Drop `revoked_at` from DeviceIdentity. Mirror
the table in V-002.
Resolution: accepted — ADR-0127 rule 2; normative table ACCOUNT_CONTINUITY_AND_CONTROL §4.1; IDENTITY_PROTOCOL points to it and drops `revoked_at`; R-204; V-002 remaining item.

### F-R1-5 — Fork resolution contradicts the "descend from the latest trusted head" rule and cannot be encoded

Severity: major
Where: [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §2 lines 49-54 versus §8 lines 163-167 and the 2026-09-23 resolution
(line 201); [`spec/cddl/account.cddl`](../../spec/cddl/account.cddl) field 4 (single `previous_state_hash`).
Problem: §2 says a client MUST NOT accept a state that "does not descend from the latest trusted head it knows". §8
resolves a fork with a bridge state "authorized under the last common trusted state" that "explicitly acknowledges
branches". A client that had already trusted branch X's head must, by §2, reject a bridge that descends from the
common ancestor or from branch Y. The frozen ACS has one `previous_state_hash`, so a bridge cannot reference both
branch heads. Implementations will differ on whether and how a freeze ends.
Proposed fix: Define the bridge state explicitly: its parent is the common ancestor, and a critical extension (key 23,
e.g. `om.acs.fork_ack`) carries the digests of every observed branch head. Authorization is a device-only majority
of the common ancestor's controllers (per the TLA model). Amend §2 to read "descends from the latest trusted head, or
is a valid bridge state covering that head". Register the extension through an ADR-0119 addendum.
Resolution: accepted — ADR-0126 `om.acs.fork_ack` (account.cddl, numeric-fields `extensions`), ADR-0127 rule 4; ACCOUNT_CONTINUITY §2 and §8 rewritten; R-205.

### F-R1-6 — The downgrade floor has no defined order and has an undefined exception path

Severity: major
Where: [`CRYPTOGRAPHY_AND_AGILITY.md`](../../docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) CryptoProfile lines 50-61 and "Downgrade floor" lines 103-116;
[`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) line 53; [`spec/registries/om-crypto-profiles.json`](../../spec/registries/om-crypto-profiles.json) (no generation or rank field);
[`spec/protocol/crypto-profile.logical.schema.json`](../../spec/protocol/crypto-profile.logical.schema.json) (has `generation`).
Problem: The floor is "generation G", but the doc's CryptoProfile struct and the frozen registry carry only an opaque
`profile_id`, while ADR-0120 needs "the maximum of the restored record and the ACS minimum profile". Profile ids are
allocation order, not strength. ADR-0107 also makes profiles incomparable by conversation type (1:1 hybrid versus
classical MLS). Separately, both documents allow lowering the floor under "an explicit authenticated migration
exception" or "an explicitly permitted emergency migration procedure", but neither is defined anywhere in the
repository. An undefined escape hatch from the downgrade floor is exactly what an implementer, or an attacker
arguing compatibility, will fill in ad hoc (absolute rules 4 and 6).
Proposed fix: Add a registry field (filling a reserved construction or through an ADR-0119 addendum) giving each
profile a `security_generation` per conversation type, and define "floor" as a comparison on that field. Either
delete the exception clauses or define the single allowed case: a SecurityAdvisory marks a profile `forbidden`, and
the floor moves to a non-forbidden profile of equal or higher generation, never lower. Record this in ADR-0104's
vocabulary and add a negative vector.
Resolution: accepted — ADR-0126 `security_generation` in om-crypto-profiles; exception clauses deleted in CRYPTOGRAPHY_AND_AGILITY and ACCOUNT_CONTINUITY §2; only the advisory-forbidden case remains; R-206; negative vector as V-003 remaining item.

### F-R1-7 — LegacyPolicy is not committed anywhere verifiable, and its contest window is unbounded below

Severity: major
Where: [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../../docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) §2 rule 2 (line 33), §4 lines 44-63; [`spec/cddl/account.cddl`](../../spec/cddl/account.cddl)
AccountRetirement key 9; [`spec/registries/om-numeric-fields.json`](../../spec/registries/om-numeric-fields.json) `recovery_method_kind`; TLA `ActivateLegacy`.
Problem: §4 says LegacyPolicy is "committed the same way" as RecoveryPolicy, but no frozen W0 object can hold it:
`recovery_method_kind` has no legacy kind, and the ACS has only `recovery_policy_commitment`. A verifier therefore
cannot check `legacy`-role signatures against anything the user committed. The contest window is carried in the
retirement object that the legacy guardians sign, and CDDL allows 0, so the guardians choose their own window.
The TLA model also requires owner inactivity (`idle >= LegacyWait`) before activation, which the prose omits.
Finally, rule 2 says retirement is terminal, while §4 says it is cancellable during the window. No pending-state
semantics tell a contact what to render, or whether to stop sending, during the window.
Proposed fix:
- Define where the LegacyPolicy commitment lives (a RecoveryPolicy method kind or an ACS extension), including the
  guardian keys, the threshold and a policy-fixed minimum contest window.
- Require verifiers to reject a legacy retirement whose window is below the committed minimum.
- Add the inactivity precondition.
- Define a `LegacyRetirementPending` state that is non-terminal and cancellable by any live controller signature. It
  becomes terminal only at window end.

Otherwise, defer legacy authorization out of W0 by ADR.
Resolution: deferred — A-030: legacy activation is outside the W0 scope list, so ADR-0127 rule 5 defers LegacyPolicy authorization to W1 (W0 rejects legacy_policy); inactivity precondition and non-terminal LegacyRetirementPending added to ACCOUNT_LIFECYCLE §4 now; R-207.

### F-R1-8 — DirectoryEntry publishes AccountID and state hash, contradicting AccountID-hiding discovery, and key transparency has no defined subject

Severity: major
Where: [`IDENTITY_PROTOCOL.md`](../../docs/architecture/identity/IDENTITY_PROTOCOL.md) DirectoryEntry lines 105-120; [`KEY_TRANSPARENCY_AND_DIRECTORY.md`](../../docs/architecture/discovery/KEY_TRANSPARENCY_AND_DIRECTORY.md) line 12 and "Open
question" line 39; [`DISCOVERY_FABRIC.md`](../../docs/architecture/discovery/DISCOVERY_FABRIC.md) §7 lines 146-164; [`spec/cddl/discovery.cddl`](../../spec/cddl/discovery.cddl) ContactDescriptor (no
AccountID field); [`METADATA_THREAT_MODEL.md`](../../docs/threat-model/METADATA_THREAT_MODEL.md) line 102.
Problem: DirectoryEntry puts `account_id` and `account_state_hash` in a public directory record. That creates a
globally enumerable alias-to-AccountID map, and a linkable state hash, which contradicts DISCOVERY_FABRIC §7,
IDENTITY_PROTOCOL line 304 and the frozen ContactDescriptor. KEY_TRANSPARENCY logs "alias -> signed Account/Discovery
record", but with alias-specific intro identities it is undefined what is logged: the intro key, the descriptor or
the ACS head. So split-view detection (DISCOVERY §11) has no concrete subject. The workers.dev personal publisher has
no transparency at all, and the residual trust-on-first-use risk is not stated.
Proposed fix: Replace DirectoryEntry with a pointer to the signed ContactDescriptor, with no AccountID and no state
hash. State in KEY_TRANSPARENCY that the logged binding is `alias -> intro identity public key (+ descriptor
digest)`. State that relationship-time protection comes from IntroIdentityBinding (F-R1-3) plus ACS head gossip, and
that first contact without key transparency is trust-on-first-use until safety-number verification.
Resolution: accepted — IDENTITY_PROTOCOL DirectoryEntry has no AccountID/state hash; KEY_TRANSPARENCY logs alias -> intro key (+ descriptor digest), three protection layers and TOFU stated; R-208.

### F-R1-9 — KeyMaterialProvider fetch is unauthorized and links an account's devices

Severity: major
Where: [`ASYNC_KEY_MATERIAL.md`](../../docs/architecture/crypto/ASYNC_KEY_MATERIAL.md) §2 lines 14-22, §3 line 36, §6 lines 60-75; [`spec/cddl/keys.cddl`](../../spec/cddl/keys.cddl) AsyncKeyBundle keys
2-4; [`spec/metadata/w0-metadata-exposure-descriptors.json`](../../spec/metadata/w0-metadata-exposure-descriptors.json) role `key_material_provider`.
Problem:
- `fetchHandshakeMaterial(device, ...)` and `inventory(device)` have no access control, so anyone who learns a
  device id can drain one-time pools. That forces last-resort keys and loses one-time forward secrecy, and it
  probes device existence.
- Every bundle carries `acs_generation` and `acs_state_hash`, which are identical across all of an account's
  devices and stable across providers. A provider, or anyone who fetches, can group devices per account and track
  the account across providers. The metadata descriptor claims `device_count: no_by_design` and
  `stable_account_id: no_by_design`.
- §6 lists these issues as "policy must define", but the W0 schema is already frozen with the linking fields in
  clear.

Proposed fix: Specify fetch authorization: a per-relationship or per-ticket fetch capability, with anonymous fetch
allowed only for the pre-acceptance request key (F-R1-3). Add per-capability drain rate limits, and make last-resort
use visible to the owner. Either move the ACS binding inside a value the fetcher verifies but the provider cannot
correlate, or correct the descriptor to `device_count: sometimes`. Record the choice in V-015.
Resolution: accepted — ASYNC_KEY_MATERIAL §2.1 (per-relationship/per-ticket fetch capability, anonymous request-key only, drain limits, visible last resort); W0 descriptor corrected to `sometimes` (ADR-0126); unlinkable binding deferred to A-031; R-209.

### F-R1-10 — ReferralAttestation cannot be verified by the parties that must act on it

Severity: major
Where: [`CONTACT_SHARING.md`](../../docs/architecture/discovery/CONTACT_SHARING.md) §3 lines 45-53; [`spec/cddl/discovery.cddl`](../../spec/cddl/discovery.cddl) ReferralAttestation (signer role `device`,
`relationship_context`); R-195 (reserved capacity for referral tickets).
Problem: The attestation is "signed inside Carol/Bob's or Carol/Alice's established relationship context". Alice can
verify it only if she knows Carol's device key; she cannot verify a Carol/Bob-context signature. The IntroGate must
verify it to admit a request into R-195's reserved capacity, but it has no knowledge of Alice's contacts unless Alice
uploads her contact keys to the IntroGate, which is a social-graph leak. Otherwise anyone can claim a referral and
consume the reserved queue, which defeats R-195.
Proposed fix: Define referral admission as capacity Alice issues: Alice pre-issues referral tokens to her contacts
(for example blinded or Privacy Pass-style tokens bound to the IntroGate). Carol attaches one; the IntroGate verifies
only the token, never Carol's identity. The "Introduced by Carol" label is verified client-side by Alice only when
Carol is her contact. Update CONTACT_SHARING §3 and the V-016 model.
Resolution: accepted — referral admission is Alice-issued Privacy Pass-style tokens; INVITES_INTROGATE_AND_SPAM §6, CONTACT_SHARING §3; label verified client-side; token profile is A-032; R-210.

### F-R1-11 — RelaySet authority, signer and scope are undefined, so a quarantined device can redirect or blackhole delivery

Severity: major
Where: [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../../docs/architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) §2 lines 23-42, §6 lines 85-89; [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../../docs/architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md)
lines 11-25 and 128; [`REACHABILITY_AND_OBJECT_REPAIR.md`](../../docs/architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md) §2 lines 20-37; [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §4 line
82; [`spec/registries/om-object-types.json`](../../spec/registries/om-object-types.json) RelaySet (role `device`); [`spec/cddl/delivery.cddl`](../../spec/cddl/delivery.cddl).
Problem: No document says which device may sign a RelaySet, or that peers MUST validate the signer against the
recipient's current ACS (active, non-quarantined, non-revoked). Quarantine (§4) only stops a device *receiving* new
RelaySets; it does not stop it *issuing* one. A quarantined or stolen device can publish a newer-version RelaySet
pointing at attacker queues, and contacts' messages then go to the attacker. They stay E2EE but are silently dropped,
and delivery metadata goes to the attacker.

Scope is also ambiguous. RELAY says capabilities are per relationship; MULTI_RELAY says "recipient ... RelaySet" per
account or device. That choice changes the blast radius of a leaked capability. REACHABILITY §2 calls `valid_from`,
`overlap_until`, `emergency_revocation` and `optional_recovery_rendezvous` mandatory semantics, but the frozen RelaySet
has none of them. The same signer gap applies to BlobLocationRecord (`authorization` is "appropriate to the object").
Proposed fix: State that a RelaySet is per relationship and signed by a device that the recipient's current ACS lists
as active and not quarantined. Peers reject other signers, and on a version conflict they prefer the version whose
signer is still valid. Map REACHABILITY §2's semantics onto the frozen fields (per-entry `expires_at` as overlap,
recovery rendezvous in ReachabilityRecoveryDescriptor), or register extensions. Give BlobLocationRecord the same
signer rule: a device of the original sender's account or of a conversation member.
Resolution: accepted — MULTI_RELAY_AND_DELIVERY_RESILIENCE §2.1 (per relationship, active non-quarantined signer, conflict rule), REACHABILITY §2 mapping onto frozen fields (ADR-0126, no extension needed), BlobLocationRecord signer rule; R-211.

### F-R1-12 — Disappearing-message semantics contradict the frozen time registry

Severity: major
Where: [`DISAPPEARING_MESSAGES_AND_RETENTION.md`](../../docs/architecture/storage/DISAPPEARING_MESSAGES_AND_RETENTION.md) "2026-09-23 expiry resolution" line 47 and §2 lines 9-20;
[`spec/registries/om-time-fields.json`](../../spec/registries/om-time-fields.json) rows `MessageCreate.expires_after` and `ConversationEvent.expiry`.
Problem: The doc says v1 interoperable disappearing messages use **absolute after-send** expiry, and that
receipt/read-based timers stay local. The frozen registry defines `MessageCreate.expires_after` as a monotonic
duration "from local first display/receipt". That is after-receipt semantics, carried on the wire. `event.expiry` is
a separate absolute field. The doc does not say which field carries the v1 policy, so two implementations will delete
at different times.
Proposed fix: State in the doc that `ConversationEvent.expiry` (absolute, wall clock) is the v1 interoperable timer,
and that `expires_after` is an optional local hint which receivers MUST NOT use to extend retention beyond `expiry`.
Or, if the registry is right, amend the doc and the 2026-09-23 resolution through an ADR.
Resolution: accepted — option 1: DISAPPEARING_MESSAGES_AND_RETENTION names `ConversationEvent.expiry` as the v1 timer; `expires_after` is a local hint (time registry note, ADR-0126); R-212.

### F-R1-13 — RecoveryPolicy shape and R-029 are stale against ADR-0111 and the frozen CDDL

Severity: minor
Where: [`RECOVERY_AND_BACKUP_POLICY.md`](../../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) §3 lines 42-68; [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../../docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) line 46 (cites
R-029); [`machine/requirements.jsonl`](../../machine/requirements.jsonl) R-029; [`machine/threat_register.jsonl`](../../machine/threat_register.jsonl) T-012.
Problem: §3 lists `None` as a method (it is now `state = off_explicit`), `recovers` without `relationships`, and a
`policy_commitment` field that ADR-0119 ruling 4 removed. R-029 still says "Default RecoveryPolicy is none",
contradicting ADR-0111 and R-183, and T-012 still maps to it.
Proposed fix: Align §3 with [`account.cddl`](../../spec/cddl/account.cddl) RecoveryPolicy (state, methods, the four `recovers` values, no
self-commitment). Rewrite R-029 as "recovery never becomes a provider master key; turning it off requires explicit
user action", or retire it in favour of R-183/R-186.
Resolution: accepted — RECOVERY_AND_BACKUP_POLICY §3 aligned with account.cddl; recovery-policy logical schema drops `policy_commitment`; R-029 rewritten and T-012 remapped in the ledger patch.

### F-R1-14 — Blob migration by re-encryption contradicts the stable BlobID and ciphertext digest

Severity: minor
Where: [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../../docs/architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) "Provider migration" step 3 (line 66) and line 107;
[`REACHABILITY_AND_OBJECT_REPAIR.md`](../../docs/architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md) line 60; [`spec/cddl/events.cddl`](../../spec/cddl/events.cddl) attachment-ref keys 3-4.
Problem: Migration may "upload same ciphertext or re-encrypted ciphertext". Events carry a fixed `ciphertext_digest`
and content key, so re-encrypted ciphertext fails verification, and repairing that would require rewriting canonical
events. The "stable integrity/content identity" wording also conflicts with ADR-0119 ruling 6 (BlobID is random;
integrity comes from the digest).
Proposed fix: Delete "or re-encrypted ciphertext". Re-keying is a new blob referenced by a new event. Reword the
BlobID lines to "random identifier; integrity from `ciphertext_digest`".
Resolution: accepted — STORAGE_AND_BLOB_ARCHITECTURE migration moves the same ciphertext only; BlobID wording is random identifier plus `ciphertext_digest` there and in REACHABILITY §3; R-213.

### F-R1-15 — Remaining references to an OM-operated default directory and verifier

Severity: minor
Where: [`IDENTITY_DISCOVERY_VERIFICATION.md`](../../docs/architecture/identity/IDENTITY_DISCOVERY_VERIFICATION.md) lines 113, 146, 193, 245, 431; [`DISCOVERY_FABRIC.md`](../../docs/architecture/discovery/DISCOVERY_FABRIC.md) lines 185, 191, 296.
Problem: The text uses `alice@directory.om`, "the default OM directory", "official OM directory", "OM Incorporated",
"OM Inc." and "OM-operated verification". ADR-0043 and ADR-0082 (no OM-operated directory, capability does not imply
operation) and the "no global DirectorySet root" resolution make these stale, and they read as a privileged
project-run namespace (constitution P-04).
Proposed fix: Replace these with "a directory the client is configured with (any provider)". Drop the `.om` suffix
example and "OM Inc.", and list "OM-operated verification" only as "a verifier the user or distribution chooses".
Resolution: accepted — directory/verifier references in IDENTITY_DISCOVERY_VERIFICATION and DISCOVERY_FABRIC now say any configured provider; `.om` example, "OM Inc." and OM-operated verification removed.

### F-R1-16 — Stale crypto text: pre-ADR-0118 strategy and a superseded AccountControlState shape

Severity: minor
Where: [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../../docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md) §4 lines 46-58; [`CRYPTOGRAPHY_AND_AGILITY.md`](../../docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) lines
18-29 and 73.
Problem: §4 still presents strategy A versus B, including "prefer wrapping [a PQXDH/Triple-Ratchet library]", as open,
but ADR-0118 chose B with Double Ratchet for v1. CRYPTOGRAPHY lines 18-29 show a competing ACS shape
(`CredentialSet`, "supported CryptoProfiles", "epoch N"), which contradicts IDENTITY_PROTOCOL line 43 ("no separate
account-authority object"). `OM-PQHYBRID-1` does not match the registry name `OM-Crypto-v1`.
Proposed fix: Mark §4 as "resolved by ADR-0118 (strategy B)". Replace lines 18-29 with a pointer to [`account.cddl`](../../spec/cddl/account.cddl),
and rename the example profile.
Resolution: accepted — PRODUCTION_CRYPTO_INTEGRATION_GATE §4 marked resolved by ADR-0118 (strategy B); CRYPTOGRAPHY_AND_AGILITY competing ACS shape replaced by a pointer; example renamed OM-Crypto-v1.

### F-R1-17 — "Expire after one use" is not single-use across multiple IntroGates

Severity: minor
Where: [`INVITES_INTROGATE_AND_SPAM.md`](../../docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) §2 line 29, §3 lines 49-51, 2026-09-23 resolution line 141.
Problem: An invite can list several IntroGates, each with a distinct provider-scoped capability. A `max_uses = 1`
invite can therefore be redeemed once per IntroGate, which is the same non-atomic-consumption problem that
ASYNC_KEY_MATERIAL §4 solves for prekeys.
Proposed fix: State that `max_uses` is per IntroGate. Either split the budget across gates (disjoint), or have the
recipient's client revoke the siblings on the first acceptance. Show the UX as "one use per server" when more than
one gate is listed.
Resolution: accepted — INVITES_INTROGATE_AND_SPAM §3: `max_uses` is per IntroGate; disjoint budget split or sibling revocation on first acceptance; "one use per server" UX; R-214.

### F-R1-18 — Retirement "through head gossip" is not possible with the head object

Severity: minor
Where: [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../../docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) §2 rule 3 (line 34); [`spec/cddl/account.cddl`](../../spec/cddl/account.cddl) AccountControlHead.
Problem: Rule 3 has clients render "Closed" when retirement arrives by head gossip. An AccountControlHead carries only
subject, generation, state hash and epoch. It cannot show that a retirement occurred, and a head relayed by a third
party should not make a contact stop sending.
Proposed fix: Require the signed AccountRetirement object itself. A head that points at an unknown newer state only
triggers a fetch.
Resolution: accepted — ACCOUNT_LIFECYCLE §2 rule 3: only the signed AccountRetirement object renders Closed; a head only triggers a fetch; R-215.

## Checked and sound

- **Identity and control:** a random 256-bit AccountID with no algorithm or provider structure; authorization by the
  parent state rather than by generation or timestamp; majority `floor(N/2)+1` with quarantine not shrinking the
  denominator (consistent with the TLA model); and "backups restore history, never authority", consistent across
  RECOVERY §8, STORAGE, MULTI_DEVICE_SYNC and ADR-0120 K7/K8/K11.
- **Delivery:** relays route on opaque deposit/drain capabilities, never AccountID. Capabilities are distinct per
  relay, group fan-out is re-wrapped per destination, and IntroGate abuse controls are enforced server-side.
- **Crypto:** default disjoint per-provider one-time pools. [`CANONICAL_ENCODING.md`](../../docs/architecture/crypto/CANONICAL_ENCODING.md) matches [`om-cbor-profile.json`](../../spec/registries/om-cbor-profile.json).
  The PQ status table is honest.
- **Storage:** LocalDataClass deletion closure and local-only redacted diagnostics.
- **Discovery:** ContactCapsule forwarding does not expose pairwise capabilities.

Noted outside this review's scope, for the other reviewer: ADR-0118 §6 makes a PQ prekey mandatory in every bundle,
but [`keys.cddl`](../../spec/cddl/keys.cddl) AsyncKeyBundle key 8 (`pq_signed_prekey`) is optional.
Resolution: accepted — key 8 made mandatory in keys.cddl and via OM-Crypto-v1 `required_bundle_fields` (ADR-0126; the registry `req` value is immutable); R-216.
