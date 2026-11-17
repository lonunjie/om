# ADR-0199 — RecoveryRecord carriage: the signed head travels in the record, one wire form, and who writes it when

- **Status:** Accepted
- **Date:** 2026-11-18
- **Decided by:** operator delegation 2026-11-18 for the owner-side recovery questions Q-REC-1, Q-REC-2 and Q-REC-5 ("normal users never handle keys or type URLs; easy by default; fail closed; additive only, no amendment of audited text"). This ADR fills silence only and amends no audited text (AGENTS "Design gate" rule 6).
- **Class:** `additive_noncritical_extension` ([`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) section 2, ADR-0165). Four new object types (35 to 38) and four new labels; no frozen W0 object, label, key or registry value changes meaning. No RecoveryStore, writer or reader exists yet, so no deployed behavior changes.
- **Answers:** Q-REC-1 (how a fresh device obtains the signed parent AccountControlState), Q-REC-2 (the wire encoding of the RecoveryRecord and its AEAD) and Q-REC-5 (who writes the record, and when). Companions: ADR-0200 (what the key sheet carries), ADR-0201 (the Home side).
- **Amends:** nothing. **Does not amend:** [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) (10.1, 10.2.1 and the ADR-0141 freshness section), ADR-0138 section 6, ADR-0141 section 3 and 4, [`recovery-record.logical.schema.json`](../spec/protocol/recovery-record.logical.schema.json) (extended with optional properties only), ADR-0190 (the notice, its bridge of at most 8 states and its section 3.1 sender), ADR-0195 (its bounds are reused, not changed).

## Design sources read, and not read

Read in full: AGENTS "Design gate"; [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md); ADR-0190 (with its amendment and delegated rulings); ADR-0195; ADR-0165; ADR-0141; ADR-0138; ADR-0129; [`home-api.md`](../spec/protocol/home-api.md) and [`home-api.cddl`](../spec/cddl/home-api.cddl); the RecoveryRecord logical schema; [`recovery-notice.cddl`](../spec/cddl/recovery-notice.cddl); [`delivery.cddl`](../spec/cddl/delivery.cddl); the registry entries of [`om-domain-separation.json`](../spec/registries/om-domain-separation.json), [`om-object-types.json`](../spec/registries/om-object-types.json) and the structure of [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json); `infra/cloudflare-home/src/limits.ts` and `capabilities.ts`; [`ACCESSIBILITY_AND_INPUT.md`](../docs/architecture/ux/ACCESSIBILITY_AND_INPUT.md) and [`INVITE_QR_MEDIA.md`](../docs/ux-design/specs/INVITE_QR_MEDIA.md).

Read in part: ADR-0181 (all sections, long lines truncated at 700 characters); ADR-0186 (decision and consequences); [`2026-11-13-recovery-flows-plan.md`](../docs/reports/2026-11-13-recovery-flows-plan.md) (headings, the design-requirements and gap sections and the first 150 lines, not the slice build logs); the recovery key display and `BeginRecovery` code in `crates/om-core` (`flows.rs`, `recovery.rs`, `flows/recovery_watch.rs`, `api.rs`, read where they touch the key text, the request and the veto); [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) section 3.4 only.

**Not read:** ADR-0120, ADR-0131, ADR-0147, ADR-0154, ADR-0163 (the crash and retry transaction contract), ADR-0168; the other documents of the eight domains `python3 tools/design_gate.py needs spec/cddl/home-api.cddl spec/registries/om-domain-separation.json crates/om-core/src/flows/recovery_watch.rs infra/cloudflare-home/src` prints (`ACCOUNT_CONTINUITY_AND_CONTROL`, `IDENTITY_*`, `CRYPTOGRAPHY_AND_AGILITY`, `PRODUCTION_CRYPTO_INTEGRATION_GATE`, `MULTI_DEVICE_SYNC`, `CANONICAL_ENCODING`, `METADATA_THREAT_MODEL`, `TIME_AND_EXPIRY_SEMANTICS`, `MULTI_RELAY_AND_DELIVERY_RESILIENCE`, `MESSENGER_BASELINE_V1` and the rest of the 12 ADR lists). Consequences: the Core transaction wording of section 6 is stated in terms of the outbox pattern already in `om-core` and is **not** checked against ADR-0163; the builder confirms it. Account-chain limits (65 states, 320 credentials) are taken from ADR-0195 and [`account.cddl`](../spec/cddl/account.cddl), not re-derived. If any unread text fixes a construction this ADR fixes differently, that text wins and this ADR is corrected by a follow-up.

## Context

After a lost-all-devices event the recovering device holds only the RecoveryRoot. It can fetch an encrypted RecoveryRecord from the Home (ADR-0138 section 6), but the record as modeled carries only `last_known_acs_head {generation, state_hash}`, the recovery epoch and snapshots, and "cannot authorize" a head. A successor `AccountControlState` needs the parent state, its credentials and the policy body, and Home API v1 has no way to read account state. No wire form, nonce rule or KDF layout exists for the record, and no device writes one.

## Decision

### 1. The record carries the signed head and what a successor needs (Q-REC-1)

The RecoveryRecord v1 plaintext gains a required **`acs_evidence`** (key 5 of type 36, [`recovery-record.cddl`](../spec/cddl/recovery-record.cddl)):

- `head`: the signed `AccountControlState` that `last_known_acs_head` names;
- `tail`: up to **8** signed states before the head, oldest first (the same bound as the ADR-0190 bridge);
- `credentials`: the `DeviceCredential` bodies that the head and the tail reference, no duplicates, none unreferenced (at most 320 bodies, at most 700,000 bytes in total);
- `recovery_policy`: the `RecoveryPolicy` body the head's `recovery_policy_commitment` names.

Authentication is the record's AEAD (section 3), under a key only a RecoveryRoot or RecoveryKit holder can derive. Each carried state is still a signed `AccountControlState` and still verifies against the chain rules; the head is advisory exactly as ADR-0141 says. There is **no new Home read of account state**: the Home stores and returns ciphertext and learns nothing more.

What the carried evidence is and is not. It lets a fresh device build a `RecoveryRequest` (`parent_state_hash` is the head's hash), name the credential set, and build the successor and the next policy. It is not an anchor of account authority: the oldest carried state is trusted only as far as the AEAD authenticates it, nothing derived from it is "pinned" by anyone else, and every verifier (a live controller, a contact) checks the request and successor against **its own** pinned head, policy commitment and interval (ADR-0190). A record whose head is older than the account's real head makes the request stale (ADR-0190 `is_stale`) and the successor is refused; that is the fail-closed outcome.

### 2. Generation: one writer per slot (Q-REC-5, part)

`record_generation` is the unsigned integer **`(acs_generation << 20) | refresh`**, with `refresh` below 2^20 and `acs_generation` the generation of the head the record carries. It is strictly increasing for one RecoveryStore lineage, so the ADR-0141 rules (a lower value than the durable high-water mark is stale, the same value with different authenticated plaintext is fail-closed equivocation) apply unchanged. Two writers never share a slot:

- **Author slot, `refresh = 0`:** written only by the device that authored the ACS transition that produced the head.
- **Steward slots, `refresh >= 1`:** written only by the head's **steward**, the live (active, not quarantined, not suspended) controller device with the lowest `DeviceID` in that head. A steward counts its own refreshes durably per head and never reuses a value. When the head changes the steward may change; slots of the new head are a new range.

### 3. Wire form, keys and nonce (Q-REC-2)

**Envelope** (type 35, unsigned): `{0: 35, 1: 1, 2: crypto_profile, 3: nonce (12 bytes), 4: ciphertext}`, at most 1,048,576 bytes. A `RecoveryRecordPutRequest` carries it as its bytes (ADR-0201).

**Keys.** All keys and capabilities are derived the same way: `HKDF-SHA-256(salt = none, IKM = RecoveryRoot (32 bytes), info = ASCII(label) || recovery_scope_id (32 bytes), L = 32)`. This fixes the layout the registry left open ("`label || recovery_scope_id`", no salt, as `OM-RECOVERY-AUTH-KEY-SEED-v1`). It applies to `OM-RECOVERY-RECORD-READ-CAP-v1`, `OM-RECOVERY-RECORD-WRITE-CAP-v1`, `OM-RECOVERY-RECORD-KEY-v1` and, in the normal profile, `OM-RECOVERY-RELATIONSHIP-SNAPSHOT-KEY-v1`. `OM-RECOVERY-PROVIDER-METADATA-KEY-v1` is **not used by the v1 encoding** (provider metadata is inside the outer AEAD) and stays reserved for a later record version. `opaque_locator` is the 32-byte digest value of `DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)`.

**AEAD.** The crypto profile's AEAD (OM-Crypto-v1: AES-256-GCM-SIV, RFC 8452) under `OM-RECOVERY-RECORD-KEY-v1`, with the frozen associated data `OM-CBOR [OM-RECOVERY-RECORD-AEAD-AD-v1, recovery_scope_id, opaque_locator, 1]` (scope and locator as 32-byte byte strings). The associated data does not name the generation or any part; the generation is inside the plaintext, and nothing is split into parts, so nothing can be recombined across generations.

**Nonce.** 12 bytes from the CSPRNG, fresh for every sealing, carried in the envelope. It is never derived and never reused on purpose. A writer seals **once** per staged write and stores the sealed bytes; every retry sends those exact bytes (section 6). A nonce repeat under the nonce-misuse-resistant AEAD would reveal only equality of two plaintexts.

**Padding.** The plaintext is padded inside the AEAD (key 22) so the encoded plaintext is exactly one of **16,384, 65,536, 262,144 or 1,048,000** bytes; the ciphertext is that plus the 16-byte tag. A plaintext within 3 bytes of a bucket uses the next one. The Home learns one of four sizes, never the number of contacts.

**Versioning.** Envelope `object_version` 1, plaintext `object_version` 1 and associated-data `record_version` 1 move together. A reader of version 1 rejects any other value, an unknown top-level key (other than a noncritical key 23 it preserves and never reads), a missing required key and a malformed bound, and tells the person to update the app, never guessing. A future incompatible form is a new version under a new associated-data version, never an edit of v1 (ADR-0165).

**Second layer (Maximum Privacy).** Key 8 holds `nonce (12) || AEAD ciphertext` of a `relationship-snapshot` under an independent `RelationshipRecoveryKey` (ADR-0141 section 4), associated data `OM-CBOR [OM-RECOVERY-RELATIONSHIP-SNAPSHOT-AEAD-AD-v1, recovery_scope_id, record_generation, 1]`. Exactly one of keys 7 and 8 is present. The normal profile writes key 7 and does not use the snapshot key; the label stays derivable for a later use.

**Size and bounds.** At most 9 states, 320 credentials (700,000 bytes), 4 provider homes, 256 relationship entries, 1,048,000 bytes of plaintext. These reuse the ADR-0195 bounds (65 states, 320 credentials, 65,536 bytes per item): the **chunking** of ADR-0195 is not needed because a Home takes one body of up to 1 MiB, not a 100,000-byte message. A writer that cannot fit the record drops relationship entries (least recently active first, setting `truncated`), then stops with a visible "recovery information is too large" state; it never drops evidence.

### 4. What the reader checks

A device with the RecoveryRoot (or a kit) fetches the envelope and: (1) rejects it over 1,048,576 bytes, of another version, or with an unknown profile; (2) derives the key and opens the AEAD, any failure being "can't read it", never a partial read; (3) decodes the plaintext and requires first entry `0: 36` (or `0: 37`, ADR-0200); (4) checks, in order: the head's `OM-ACS-HASH-v1` equals key 4's `state_hash` and its generation equals key 4's; the tail links by `previous_state_hash` and the last tail state is the head's predecessor; every state is for one account; the credential set is exactly the one the states reference, each body hashing to its reference; the policy body hashes (`OM-RECOVERY-POLICY-HASH-v1`) to the head's `recovery_policy_commitment`; `record_generation >> 20` equals the head's generation; exactly one of keys 7 and 8; (5) for a device that holds a durable high-water mark, the ADR-0141 stale and equivocation rules. A failed check ends the read with nothing used. The result stays advisory: routes and capabilities are revalidated before use, and the request it enables is judged by others.

### 5. The RecoveryKit: who can write

A writer needs the write capability, the locator, the record key and the scope, which are derived from the RecoveryRoot, and the RecoveryRoot is shown once and not kept. So the account's controller devices keep a **RecoveryKit** (type 38): `{scope, host, port, read_cap, write_cap, record_key, inbox_cap, optional snapshot key, epoch}`. The kit holds **no** recovery signing key and cannot start a recovery; it lets a device write and read the record, which that device could already learn from the account's own data (the chain, the contacts) except the inbox capability. The kit is class `om.recovery_secret`: it lives in DeviceVault, is never put in a backup (a restored device has no kit and must receive one), never sent to a Home, and never logged.

- **Made** on the device that generates the RecoveryRoot, at the moment the key is displayed (or the passkey custodian is created), while the root is in memory. The `inbox_cap` is fresh random.
- **Distributed** to every other controller device over the authenticated in-session channel by a new in-session kind `RECOVERY_KIT` (the builder takes the next free kind number where in-session kinds are registered): after a device link completes (from the approving device), and on any later kit change (custody rotation, a Home move). A device without a kit never writes and never errs: it simply is not a writer yet; the author slot of an ACS change it authors without a kit is left to the steward's catch-up (section 6).
- **Rotated** when custody rotates (ADR-0190's vetoed-request guidance, a new RecoveryRoot): new scope-bound keys, a new locator, new kit, and a `DELETE` of the old record (ADR-0201).
- An account created before this ADR whose root was never kept has no kit and cannot write. It becomes recoverable through the existing `recovery_policy` change (parent majority) that commits a new RecoveryRoot's policy and issues a new sheet; no new authority is introduced.

### 6. The writer schedule (Q-REC-5)

1. **First write: when the custodian is confirmed.** For the recovery key, at `ConfirmRecoveryKeyStored`; for a synced passkey, when its creation is verified. The first write is the author slot of the then-current head, `refresh = 0` (or the next free slot if the head already has one). Until the first put is acknowledged by the Home the recovery status says recovery is not yet ready ("finishing setup") instead of "on" and the checks of ADR-0111 10.5 apply as for a pending custodian; the UI owes the person no action.
2. **On each ACS change, atomically with the change's transaction.** The device that authors a transition builds the new record (head, tail, credentials, policy, providers, relationships), seals it **once** with a fresh nonce and stages the sealed bytes with a fixed 128-bit `request_id` in the **same** durable transaction that stores the new head. Either both are committed or neither. After commit the outbox delivers it; a record staged but not yet acknowledged is replaced by any later staged record (older bytes are dropped, never resent after a newer slot was staged on the same device).
3. **Steward refresh and catch-up.** The steward writes slot `refresh >= 1` of the head: (a) when the relationship snapshot or provider snapshot changed (compared by a content hash), at most once per 24 hours; (b) at least every 90 days, unchanged, so that no record falls under the Home's 400-day retention; (c) as **catch-up** when it adopts a new head and finds no record acknowledged for it after 1 hour (the author lost the device or never had a kit).
4. **Concurrency: compare and swap.** Every replace sends the stored envelope's `ETag` as `If-Match`; a `stale_generation` answer makes the writer fetch the record (it holds the read capability in its kit), decrypt it and compare generations: a remote slot at or above its own means its write is superseded and dropped; a remote slot below its own is replaced with `If-Match` of the remote value. Nothing a device does can lower a surviving high-water mark (ADR-0141).
5. **Crash safety and idempotency.** The staged bytes, the `request_id` and the target `ETag` are durable before the first attempt. A timeout is an unknown outcome and is retried with the same `request_id` and the same bytes (Home API v1 section 5), never re-sealed. A crash between attempts resumes from the stored bytes. A lost outbox entry for an acknowledged-unknown write is covered by the steward catch-up of rule 3(c). Sending the same bytes twice has one outcome.
6. **Failure is visible and calm.** A write that cannot be acknowledged for 24 hours turns the recovery status to "needs attention" with a retry; it never blocks messaging and never reports "on" while the newest head has no acknowledged record.

### 7. The RelationshipSnapshot (Q-REC-5)

`relationship-snapshot` holds up to 256 entries, one per active 1:1 relationship, each with: `conversation_id`; the contact's `account_id`; an optional unverified display label; **1 to 4 delivery routes** (the contact's `relay-entry` with its deposit capability); **1 to 4 delivery targets** (the contact device's `delivery-key-material` and `DeviceID`, one per route, matched by the route binding of ADR-0142); an optional relationship capability for fetching the contact's key bundles (to establish fresh sessions after recovery); and optionally the contact's head this device had pinned. That is what ADR-0190 section 3.1 needs to seal a `RecoveryNotice` to each contact and what a recovered device needs to reach people again; sessions, ratchet state and message history are never in it (backups never restore authority, ADR-0120 K11, RECOVERY 8). Entries are authenticated only by the AEAD and are revalidated before use; a contact that cannot be opened is the ADR-0190 "couldn't reach" path, never a clear delivery. Group-only members are not listed. In the Maximum Privacy profile the whole snapshot moves to key 8 under the independent key.

### 8. Test-vector plan (vectors by the builder)

A Python generator and the Rust and TypeScript codecs, cross-checked as for ADR-0190 and ADR-0195, producing `verification/vectors/recovery-record-vectors.json.gz`: (a) key derivation (read cap, write cap, record key, locator, snapshot key) from fixed roots and scope ids, including `info` layout and a wrong-scope negative; (b) envelope seal and open with fixed nonces, the associated-data bytes, and negatives for a flipped bit in ciphertext, nonce, scope and locator; (c) each padding bucket and the "within 3 bytes" rule; (d) plaintext positives (minimal; maximal at the bounds; a Maximum Privacy second layer) and one negative per rule of section 4 (head and key 4 disagree, broken tail link, a credential missing, extra or mismatched, policy hash mismatch, generation shift mismatch, both or neither of keys 7 and 8, unknown key, version 2, oversize); (e) the generation arithmetic, including two writers of one head never sharing a slot; (f) the writer rules as state-machine tests (first write, author write atomic with a transition, steward catch-up, `stale_generation` reconciliation, crash mid-retry resending identical bytes); (g) `RecoveryKit` round trip and the rule that it is never in a backup (the backup writer rejects `om.recovery_secret` content).

## Where did authority go?

Nowhere new. The record authorizes nothing: the head it carries is a remembered, authenticated observation that every verifier checks against its own pinned state. A kit holder gains no vote, no recovery signature and no Home authority beyond ciphertext read and write for one record (ADR-0138 section 6). The Home learns nothing beyond an ordinary mailbox plus the four-valued size bucket and the times of writes (ADR-0201 section 5).

## Consequences

A fresh device can build a request and a successor from the record alone; Home API v1 needs no read of account state. The cost is size (up to about 1 MiB per account) and a secret on every controller device (the kit), which compromise of a controller already exceeds in power. A record can lag the real head (a writer lost before its write): the request is then stale and refused, and the steward catch-up bounds the lag to an hour of reachability. Residual: the AEAD construction is OM's own composition over standard primitives and needs the independent review [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md) section 2 asks for before any production claim; this ADR does not claim it. The unread ADR-0163 may constrain the transaction wording of section 6.

## Work list for a builder

1. **Builder 1 (Core).** `recovery.rs`: `RecoveryKit` (derive, persist in DeviceVault, exclude from backup), key derivation per section 3, `seal_record`/`open_record`, padding, the section 4 checks. `state.rs`: kit, per-head steward counter, the staged-write outbox item. `flows`: first write on `ConfirmRecoveryKeyStored`, author write inside every ACS-change transaction, steward refresh and catch-up, `RECOVERY_KIT` in-session kind after link completion and on change, `stale_generation` reconciliation. The RelationshipSnapshot builder. Move the codec work to `crates/om-codec` and the registry load. Status copy "finishing setup" and "needs attention".
2. **Registry and spec** are done by this ADR's spec commit; the builder adds the vectors of section 8 and the codec tests.
3. Departures the builder must record, not decide: anything that ships differently from this ADR.

## UX line

Recovery just works: the person saves their key once, and OM keeps the rest up to date by itself.

## Traces

Requirement R-438, threat T-182, decision D-216. Related: ADR-0138, ADR-0141, ADR-0190, ADR-0195, ADR-0200, ADR-0201.
