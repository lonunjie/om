# ADR-0175 — V-019: the v1-to-hybrid-PQ group migration is an OM-level authenticated transition

- **Status:** Accepted
- **Date:** 2026-10-01
- **Decided by:** design agent (Claude, sole design agent, ADR-0167), from the V-019 prototype below
- **Source:** [`verification/prototypes/v019-hybrid-pq-migration/`](../verification/prototypes/v019-hybrid-pq-migration/README.md),
  [`verification/results/v019-hybrid-pq-migration-latest.json`](../verification/results/v019-hybrid-pq-migration-latest.json)
- **Amends:** [`CRYPTOGRAPHY_AND_AGILITY.md`](../docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) §2 (the "ReInit/migration" wording), [`GROUP_CHAT_ARCHITECTURE.md`](../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md) (security section), R-174
- **Does not decide:** the wire encoding of the migration transition (not frozen here), the final hybrid suite code point, or when ADR-0107 adoption triggers fire.

## Context

R-174 and V-019 say the v1 group wire must carry a tested, downgrade-safe **ReInit** path to a hybrid-PQ MLS profile that preserves the
ConversationID and the group authority. The chosen library does not offer one: OpenMLS 0.9.0 states "`ReInit` is not yet implemented"
(`src/messages/external_proposals.rs:5`, upstream issue 751) and has no resumption PSK in its key schedule. It does offer `MlsGroup::reboot`
(feature `fork-resolution`): a new group with the same members and no key-schedule link to the old one, and a provisional
`draft-ietf-mls-pq-ciphersuites` feature (X-Wing KEM, suite `0x004F`). The prototype therefore proves the *property* with an OM-level mechanism.

## Decision

1. **The v1 group migration path is an OM-level authenticated transition**, built on a library "new group, same members" primitive until a
   library implements RFC 9420 ReInit. "ReInit" in R-174, CRYPTOGRAPHY_AND_AGILITY §2 and GROUP_CHAT_ARCHITECTURE means this migration;
   an implementation that has a conforming ReInit may use it, because the OM-level properties below are what is normative.
2. **Properties** (verified by 20 vectors on native and wasm32, byte-identical): the transition is signed by an authorized roster device; the
   ConversationID and the GroupControlState hash are unchanged and only the MLS group id and ciphersuite change; the new group admits exactly the
   roster, and every joiner checks the Welcome against the signed transition (group id, suite, binding, roster) before joining; a downgrade, an
   equal-generation replacement, a replay, a stale parent, an unauthorized author and any tampered field are each rejected, by the monotone
   `security_generation` floor (ADR-0165) and the signature.
3. **Supersession.** An applied migration is ordered like any other non-reducing transition under ADR-0173 (canonical successor; the loser is
   SUPERSEDED), and the old MLS group becomes SUPERSEDED: it sends and commits nothing, and keeps only tail decryption within `past_epoch_window`.
   The old group's secrets, including the classical ones, are deleted at a retirement bound set by the migration (time and acknowledgement), because
   after migration an epoch-count window never expires on its own (G-V19-1, G-V19-2).
4. **Binding and readiness.** The signed transition binds a hash of the new group's initial GroupContext or Commit, not only its group id, and the
   binding travels as a critical GroupContext extension (reserved prefix `om.critical.`, ADR-0157). A migration needs a fresh KeyPackage in the new
   suite for every roster device before the transition is signed; a device without one stays on the old group until retirement and is then removed and
   re-added (G-V19-3, G-V19-6).
5. **Open for the migration wire** (recorded, not decided): where the security-generation floor lives relative to the hashed state (G-V19-4), the
   per-suite leaf key binding and a dual Ed25519 plus ML-DSA-65 transition signature (G-V19-7), and how a final code point replaces a draft one at
   equal generation (G-V19-8; a generation bump or an explicit successor edge). The migration transition is an additive W1 object (type 32 and a
   later `binding_kind` value are unassigned in ADR-0176), so deciding it later changes no frozen byte.

## Consequences

- Requirements R-404 (rule 2), R-405 (rule 3), R-406 (rule 4). V-019's pre-engineering criterion is met as an OM-level migration; if it is read as the literal RFC 9420 ReInit proposal it is not met and cannot be until a library implements it, which the pre-release interoperability criterion (ADR-0107 triggers) still requires.
- The two docs that said "tested ReInit path" now say "authenticated migration".
- Pre-release: an interoperable migration between two implementations, and a real wire encoding.
