# ADR-0225 — Owner recovery: verification from the saved checkpoint, lineage-keyed record floors and independent relationship-key custody

- **Status:** Accepted (by operator delegation) Clarified by ADR-0238 (additive note: "complete verified post-checkpoint chain" in section 1 is read as the contiguous verified prefix through the first fully checked non-time owner-recovery link).
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 ("decide design questions from the constitution: fail closed, no operator, user-held keys, recovery optional, simplicity for ordinary users, metadata minimisation; fill silences only, never amend audited text, never weaken a cryptographic or metadata floor"). This ADR fills silence only and amends no audited text (AGENTS "Design gate" rule 6).
- **Class:** `additive_noncritical_extension` ([`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) section 2, ADR-0165). One optional field in an existing plaintext object and a changed local key for a local store. No frozen W0 object, label or registry value changes meaning; a reader that does not know the new field refuses the record and says to update (fail closed).
- **Answers:** Q-REC-16 (evidence when only a checkpoint survives), the Q-REC-17 keying question as it applies to distinct lineages on one Home, and the open Maximum Privacy second-layer custody (D146).
- **Amends:** nothing. **Does not amend:** [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) (10.2, 10.6, 10.7), ADR-0141 sections 3 and 4, ADR-0199 (sections 1 to 7 stand; section 3 "Size and bounds" gains the segment below, a clarification of what ADR-0202 section 1 and ADR-0203 section 1 left to a later specification), ADR-0200, ADR-0202, ADR-0203, ADR-0205.
- **Scope:** `core_kernel` (ADR-0169) and `provider_reference` (ADR-0171) as already authorized for recovery.

## Design sources read, and not read

Read in full: ADR-0199, ADR-0201, ADR-0202, ADR-0203, ADR-0205, ADR-0216, the "Privacy, Governance and Authority" and "Operatorless Core" constitution documents, and the recovery-key, record-floor and relationship-snapshot passages of [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) and ADR-0141. Consulted by search: the other constitution documents, ADR-0195, ADR-0190. Not read in full: the remaining documents of the identity and provider domains the design gate lists. If an unread text fixes one of these points differently, that text wins and this ADR is corrected by a follow-up.

## Decision

### 1. Verification starts at the saved checkpoint, never at genesis

A device that holds a version 2 sheet (ADR-0203 section 1) verifies the evidence **forward from the checkpoint state** named by the sheet: the AccountID, generation and `OM-ACS-HASH-v1` digest. The chain that supplies the evidence must contain a signed state with exactly those three values; every later transition is verified with the ordinary account verifier. Evidence older than the checkpoint is never required, and a genesis-start is neither required nor preferred. A chain that does not contain the checkpoint state, or that contains a different state at the checkpoint generation, grants nothing and leaves the device restricted. Sheets without a checkpoint keep the earlier rule (verification from genesis, otherwise restricted).

A recovery link after the checkpoint is accepted only under the applicable verifier rules of ADR-0202 section 1, which need the recipient's own recovery evidence for that link. A fresh device with only a sheet has none, so a chain that crosses a recovery link after the checkpoint leaves it restricted with the state "this saved sheet is older than a recovery of this account". This is intended: a holder of the recovery key cannot be allowed to present an earlier recovery whose waiting period nobody on this device observed.

### 2. The checkpoint is the anchor, and the record carries the segment that reaches it

1. **Anchor.** The anchor of a recovery key is the head the issuing device verified when it displayed the key: the AccountID, generation and digest it put in the sheet. The `RecoveryKit` (ADR-0199 section 5, device-local storage, not a wire object) gains the optional field `anchor` holding those three values and travels with the kit inside the authenticated session. A kit without an `anchor` is treated as anchored at the account's first state.
2. **Segment.** The RecoveryRecord's `acs_evidence` (type 36, key 5) gains the optional `anchor_segment`: the contiguous signed states from the anchor state, inclusive, up to the state before the oldest state of `tail`, oldest first, with the credential bodies they reference added to `credentials` under the existing rule that the set is exactly what the carried states reference. The head and tail rules of ADR-0199 sections 1 and 4 are unchanged. The segment may hold at most 56 states, so that head, tail and segment together stay within the 65-state chain bound of ADR-0195; the credential and plaintext bounds of ADR-0199 section 3 are unchanged.
3. **Reader.** With the segment present, the reader checks that it starts at a state that equals the pinned checkpoint and that every link is the `previous_state_hash` of the next, reaching the tail; it then proceeds under section 1. A record without a segment, or whose segment does not reach the checkpoint, gives the evidence-needed state; the device prepares and delivers its request as today and stays restricted, and any other revalidated carrier of the same signed states (the approval reply of ADR-0203 section 3, or a contact's head carriage under the existing head-gossip rules) completes it.
4. **Writer.** The writer keeps the anchor state and every state after it for as long as the kit is in force, and writes the segment in every record. If the chain from the anchor to the head cannot fit within the bounds, the writer does not drop evidence (ADR-0199 section 3): it keeps writing the record with the segment omitted, and the recovery status reports **"needs a new safety code"** because the saved sheet can no longer be used without another device. The remedy is the existing replacement (ADR-0205), which moves the anchor to the then-current head.
5. **Completed recovery.** After an owner recovery completes on a device, the recovery status is "needs a new safety code" until a replacement is confirmed, because every earlier sheet crosses the recovery link just adopted (section 1). Nothing else changes about the completed recovery.
6. **Status is three-valued.** "On" requires an acknowledged record for the current head **and** a segment that reaches the anchor (or a chain short enough to need none). A written but unusable record reports "needs a new safety code", never "on".

### 3. Record floors are keyed by storage lineage

The durable record high-water mark of ADR-0141 and ADR-0202 is stored per **scope and record lineage**, where the lineage is the record's `opaque_locator` (it derives from the recovery key, so a new safety code is a new lineage). An entry holds the scope, the locator, the highest generation seen and, once known, the AccountID the lineage is bound to.

1. **Order of checks.** A record establishes or raises a floor only after its AEAD opens, its plaintext checks of ADR-0199 section 4 pass and its account matches the sheet's checkpoint. A record that fails any of them reserves nothing.
2. **Same locator, different account** is equivocation and fails closed. **Different locators** under one scope are independent entries, and none can lower or clear another.
3. **Bound.** At most 16 entries per profile. An entry is dropped only by an explicit "forget this attempt" action of the person, or automatically once its attempt was cancelled and no activity has occurred for 400 days. A full table refuses to add a new lineage and says so plainly; it never evicts a floor in use.
4. **Migration.** An entry from the earlier scope-only table is kept as it is, marked legacy, with its bound account. The first record of an account equal to the legacy account binds the entry to that record's locator and keeps the higher of the two floors. A record of a different account never consults the legacy entry. No floor is lowered or removed by migration.

### 4. Maximum Privacy: an independent relationship key, and what the record holds until it exists

ADR-0199 section 3 key 8 and ADR-0141 section 4 define the second layer. This section specifies its custody and migration.

1. **Key.** The `RelationshipRecoveryKey` is 256 random bits, generated on the device that issues the safety code, never derived from or wrapped by the RecoveryRoot. It is shown once as a **second written code** beside the safety code, with the same display, check and confirm-by-retyping steps and the same text encoding rules as the recovery key (ADR-0200), carrying a distinct kind value so that either code is refused where the other is expected. The person is told plainly that losing it loses only the convenience of reconnecting contacts, never the account.
2. **Who holds it.** Controller devices keep it in the kit's optional snapshot key field so they can write; it is not part of any sheet, backup or record. The Home never sees it.
3. **Switching to Maximum Privacy.** At the moment the profile is chosen, the next record written carries a relationship snapshot with **no entries** in key 7 (the stricter profile discloses nothing of the relationships while the independent key does not yet exist), and the status line says that contact reconnection is limited until a new safety code and relationship code are made. Making them is the existing replacement (ADR-0205): the replacement's record is the first to carry key 8, and it is a new lineage, so an earlier record encrypted under the old root is not extended into the new profile.
4. **Old normal records.** A record written under a root that derived the snapshot key stays readable by that root for its remaining retention, and cannot be made unreadable to a holder of that root; the replacement of section 3 is the means by which the person moves on, and the old record is then deleted at every Home it was written to (the delete of ADR-0201, exact retry). The product does not claim that earlier copies are gone.
5. **Recovery.** With only the safety code, the device recovers identity and control and shows relationships as unavailable; with both codes it restores the snapshot as the normal profile does. Opening key 8 with the safety code alone must fail with "can't read it" and no partial read.
6. **No change to floors.** Profiles change custody and delay, never cryptographic floors (ADR-0037); the AEAD, nonce and padding rules of ADR-0199 section 3 apply unchanged to the second layer.

## Where did authority go?

Nowhere new. The checkpoint anchors a minimum known head held by the owner and grants no vote. The segment is signed evidence that every verifier still checks against the account's own rules; it never lets a recovery link be accepted without its own waiting evidence. The floor is advisory local state that only rises. The relationship key authorizes reading one snapshot and nothing else.

## Evidence required

Core tests: a checkpoint inside the carried window and one far behind it (long chain, segment present and omitted), a chain crossing one earlier recovery link (restricted, status "needs a new safety code"), a replaced key whose anchor moves, two lineages and two accounts on one scope, a record failing the checkpoint check leaving no floor, legacy floor migration, a Maximum Privacy switch (empty snapshot, then key 8 after replacement, safety code alone failing to open key 8). Vectors for the segment field and the second code in both codecs. Home: no change.

## Review requirement

The new second code and the segment field are a custody and a plaintext shape. The independent review that [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md) asks for before any production claim covers them; this ADR does not claim it.

## Traces

Related: ADR-0141, ADR-0195, ADR-0199, ADR-0200, ADR-0201, ADR-0202, ADR-0203, ADR-0205, ADR-0216.
