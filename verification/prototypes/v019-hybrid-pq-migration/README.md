# V-019 hybrid-PQ group migration probe

Feasibility prototype for gate V-019 (`pq_group_reinit_migration`). Evidence only: not product code, not for production, test-provider keys only.

It asks whether the v1 group (classical MLS suite 0x0001) can be moved to an experimental hybrid-PQ MLS suite **without changing the ConversationID or the group authority, and without a downgrade path**, with a real MLS library underneath.

## Key finding: this is an OM-level migration, not RFC 9420 ReInit

OpenMLS 0.9.0 does not implement the ReInit proposal. Its own source says so
(`openmls-0.9.0/src/messages/external_proposals.rs:5`):

> `//! `ReInit` is not yet implemented`

Related: `src/group/mls_group/creation.rs` has `// TODO #751: Implement PSK` (no ReInit/resumption PSK in the key schedule) and `src/prelude.rs` keeps the PSK types commented out under the same issue. The library offers `MlsGroup::reboot` (feature `fork-resolution`): a **new** group with the same members and no key-schedule link to the old one. The prototype therefore builds an **OM-level authenticated migration on `reboot`**:

1. Alice (an admin device) signs a diagnostic `Transition` (the stand-in `GroupControlTransition`) with her roster-bound Ed25519 key.
2. Every member runs `verify_transition(transition, roster, floor, authority)`: identity, signature over `OM-V019-DIAG-GroupControlTransition-v0\0` + length-prefixed fields, authority, unchanged GroupControlState hash, profile registry, monotone `security_generation` floor, parent.
3. Alice `reboot`s into suite 0x004F with a new MLS group id, only roster-bound key packages, and an OM binding (ConversationID, state hash, new generation) in a private GroupContext extension (0xFF51).
4. Bob and carol stage the Welcome and check group id, ciphersuite, binding and exact roster match **before** joining.

Hybrid suite used: `MLS_128_MLKEM768X25519_AES128GCM_SHA256_Ed25519` (0x004F, draft-ietf-mls-pq-ciphersuites provisional code point, X-Wing draft-06 KEM = ML-KEM-768 + X25519, `x-wing 0.1.0`, `ml-kem 0.3.2`, `hpke-rs 0.7.0`). Its **signatures and credentials stay classical Ed25519**: the KEM is hybrid, authentication is not post-quantum. The provider also runs ML-DSA suites; the suite matrix below shows all twelve advertised suites migrate and exchange messages.

## Run

```bash
verification/prototypes/v019-hybrid-pq-migration/run.sh
```

Native macOS arm64 plus `wasm32-unknown-unknown` under Node/V8 (wasm-bindgen, same approach as V-004; OpenMLS needs the `js` feature there). Toolchain pinned to rustc 1.98.1, `Cargo.lock` checked in, `--locked`. The script exits non-zero on any failed vector, a build failure, or any native/wasm byte difference, and then leaves the previous results file untouched. Output: [`verification/results/v019-hybrid-pq-migration-latest.json`](../../results/v019-hybrid-pq-migration-latest.json). Whole run is a few seconds once built.

## Results (20 vectors, all pass; native and wasm32 output byte-identical)

| Vector | What is shown |
|---|---|
| `v1_group_three_members_exchange_messages` | 0x0001 group, alice/bob/carol, full-mesh messages |
| `migrate_to_hybrid_all_members_join_exporters_match` | new MLS group id, suite 0x004F, all 3 join, equal exporter secret, mesh messages, exporter differs from old group |
| `conversation_id_and_state_hash_unchanged_across_migration` | ConversationID and state hash identical; binding present in every member's new GroupContext |
| `valid_transition_accepted_and_serialises` | good transition verifies; JSON round-trips; signed bytes start with the domain label |
| `downgrade_hybrid_to_classical_rejected` | `Downgrade` at its true generation; `ProfileGenerationMismatch` if it claims generation 3 |
| `equal_generation_lower_profile_rejected` | 0x004F -> 0x004E (both generation 2): `NotAboveFloor`; unregistered suite: `UnregisteredProfile` |
| `unauthorized_author_rejected` | plain member `Unauthorized`; non-roster signer `UnknownAuthor`; author API refuses and creates no group |
| `tampered_fields_fail_signature` | new group id, suite, generation, state hash, old group id, author swap, other roster key, missing domain label: all `BadSignature` |
| `authorised_author_changing_state_hash_rejected` | valid admin signature, different state hash: `StateHashMismatch` |
| `non_roster_member_cannot_join_new_group` | outsider cannot process the Welcome; author filter refuses non-roster key package; a rogue admin that adds one anyway is caught by both honest joiners (`UnexpectedMember`), nobody joins |
| `joiner_rejects_welcome_not_matching_transition` | group id or suite differs from the signed transition: refused; matching pairing joins |
| `old_group_tail_message_after_{0,1,2}_old_epoch_advances` | tail message from the last pre-migration epoch decrypts for carol and alice (window 2) |
| `old_group_tail_message_after_{3,4}_old_epoch_advances` | same message is **not** decryptable once the window is exceeded |
| `old_group_stays_live_in_mls_after_migration` | informational: old group still sends, receives and commits after migration |
| `replay_of_applied_transition_rejected_idempotent` | `AlreadyApplied` (also for a re-signed copy); applying the floor twice changes nothing |
| `stale_parent_rejected` | valid signature, parent is not the current floor: `StaleParent` |
| `concurrent_competing_migrations_are_order_dependent` | informational: two admins' valid migrations; two verifiers with opposite arrival order end on different groups |

Suite matrix (informational, reboot + messaging, no OM transition): 0x0001, 0x0003, 0x0002, 0x0042, 0x0906, 0x004E, **0x004F**, 0x0050, 0x0052, 0x0051, 0x0907, 0xF042 all migrate and exchange messages on both runtimes. Suites with a non-Ed25519 signature scheme get a different leaf signature key.

## What it proves

- A 3-member OpenMLS group can be moved to a hybrid-PQ suite (X-Wing KEM) via `reboot`, on native and wasm32, with the PQ dependency stack building unchanged for wasm32.
- A signed OM transition can carry the logical migration (same ConversationID and GroupControlState hash, new MLS group id) and a monotone `security_generation` floor rejects downgrade, equal-generation, replayed, stale and unauthorised or tampered transitions.
- Joiners can bind the Welcome to the signed transition and the roster before accepting it.
- The `past_epoch_window` behaves as ADR-0121 rule 4 says for old-group tail messages (decryptable at lag 0..2, not at 3..4).

## What it does not prove

- RFC 9420 ReInit, or any interoperable migration (the pre-release criterion, ADR-0107 triggers). Another MLS implementation would see an unrelated new group.
- Real key management, durable storage, network delivery, KeyPackage publication, crash/restart during migration, or the real GroupControlTransition encoding (this is a diagnostic JSON stand-in, not CDDL/CBOR, not a wire format).
- Post-quantum authentication: transition and leaf signatures are Ed25519.
- Independence: single Claude agent, single vendor (ADR-0167). The same author wrote the checks and the vectors.

## Proposed design updates (G-V19-n, local to this probe, not global IDs)

- **G-V19-1 Past-epoch window never closes after migration.** The old group stops advancing, so an epoch-count window (ADR-0121 rule 4) never expires; every tail vector shows the old epoch unchanged by migration. The vectors only reach "beyond the window" by committing in the old group, which a real migration does not do. Proposal: the applied transition starts a retirement clock (time and/or acknowledgement bound) after which old-group secrets, including the classical ones, are deleted.
- **G-V19-2 `reboot` does not retire the old group.** After migration the old group still sends, receives and commits (informational vector). Proposal: an applied transition marks the old MLS group id SUPERSEDED (same state as ADR-0173 rule 3): no new sends or commits, tail decryption only until retirement.
- **G-V19-3 No cryptographic old-to-new binding.** No ReInit/resumption PSK, so authenticity rests on the OM signature, roster and binding extension alone. Proposal: sign a hash of the new group's initial GroupContext/Commit in the transition (analogous to `mls_commit_hash`); the prototype signs only the group id. Register the binding as a critical GroupContext extension (`om.critical.` prefix, ADR-0157) instead of the private 0xFF51.
- **G-V19-4 Where the group floor lives.** This prototype keeps `security_generation.group` beside the hashed state, which is why the hash is unchanged. If the floor is part of GroupControlState, the hash changes at migration and the property must be restated as "ConversationID, roster and authority unchanged; hash chained by the transition". ADR must choose; recommend a separate authority digest.
- **G-V19-5 Concurrent migrations are order-dependent.** Two authorised devices can each produce a valid transition from one parent; first-seen wins per verifier, so verifiers diverge. Proposal: treat migration as a non-reducing GroupControlTransition under ADR-0173 canonical-successor ordering (tie-break author DeviceID, transition hash), loser SUPERSEDED.
- **G-V19-6 KeyPackage readiness and single-use consumption.** `reboot` needs a fresh new-suite KeyPackage for every roster device up front, and a staged-then-rejected Welcome consumes the joiner's KeyPackage (retry fails with "No matching key package was found in the key store."). Proposal: a migration-readiness check against KeyMaterialProvider before signing, a defined straggler path (offline devices stay on the old group until retirement, then are removed and re-added), and a re-issue path after a rejected Welcome.
- **G-V19-7 Per-suite leaf keys and classical-only authentication.** 0x004F keeps the Ed25519 roster key as leaf key, but every ML-DSA or ECDSA suite changes it (matrix column `roster_ed25519_key_is_new_leaf_key`). The roster/DeviceCredential must bind the per-suite MLS leaf key, and the production transition should carry both Ed25519 and ML-DSA-65 signatures like the V-001 DeviceIdentity binding.
- **G-V19-8 Equal-generation profile replacement is impossible.** The strict-greater floor rejects a same-generation successor (`NotAboveFloor`). The draft code point 0x004F will be replaced by a final one, which is a new immutable profile (ADR-0165). Proposal: either bump `security_generation` for any profile replacement, or register an explicit `successor_of` edge permitted at equal generation.

## What the parent should record

- **V-019 pre-engineering criterion: `met` as an OM-level authenticated migration; record `partial` if the gate is read as literal RFC 9420 ReInit.** Reason: the mechanism is `reboot` plus OM signature, because OpenMLS 0.9.0 has no ReInit. Docs that say the group wire "must carry a tested ReInit path" (GROUP_CHAT_ARCHITECTURE.md, CRYPTOGRAPHY_AND_AGILITY.md) should say "authenticated migration (reboot-based today, ReInit when libraries implement it)".
- Pre-engineering independence stays `open`: needs `independent_reviewer` (single-vendor evidence, ADR-0167). Pre-release interoperability criterion stays open.
- Feasibility criterion F0 is unaffected (already evidenced).
- Dependency facts for the ADR: openmls 0.9.0 with `draft-ietf-mls-pq-ciphersuites` (provisional code points), `openmls_rust_crypto` 0.6.0, `openmls_basic_credential` 0.6.0, `openmls_traits` 0.6.0, `hpke-rs(-rust-crypto)` 0.7.0, `x-wing` 0.1.0, `ml-kem` 0.3.2, `ml-dsa` 0.1.1 (all pre-1.0 RustCrypto-ecosystem crates), rustc 1.98.1.
