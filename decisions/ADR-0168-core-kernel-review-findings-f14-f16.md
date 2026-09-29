# ADR-0168 — core_kernel review findings F-14..F-16

- **Status:** Accepted
- **Date:** 2026-09-29
- **Review:** [`2026-09-29-claude-opus-5-5-core-kernel-review.md`](../verification/reviews/2026-09-29-claude-opus-5-5-core-kernel-review.md) (ADR-0167 review model)
- **Amends:** ADR-0137 §2 (safety-freeze resolution), ADR-0160 (W3 signature role), ADR-0119 registry rules (enum value waves)
- **Does not authorize engineering by itself.**

## F-14 — one stolen device could permanently lock a two-controller account

ADR-0137 made freeze resolution device-only and restore-only. In a two-controller account with recovery on (the common
case), a thief holding one device could quarantine the owner's other device. The account froze, resolution needed both
signatures, and recovery was barred from freezes, so the owner was locked out permanently. In normal mode the same owner
wins: their device plus the RecoveryRoot form a majority. The owner could also trigger the lockout themselves by
"suspending" the stolen phone, because any quarantine that drops below the threshold freezes a two-controller account.
TLC did not catch this, because no invariant stated that an owner with a device and the recovery method can always
regain control.

**Decision.** A freeze resolution has exactly normal-mode majority authority:

1. signers are parent controllers; quarantined ones sign only with their freeze-challenge capability;
2. the threshold is the parent majority, counting device signatures plus **at most one recovery vote, only co-signed**
   by at least one parent controller (R-185). A RecoveryVeto rejects it, and recovery alone never resolves a freeze;
3. the child may restore challenge-signing quarantined controllers and/or **revoke non-signing controllers**
   (tombstoned). At least one is restored or revoked, and no other constitutional field changes;
4. the child's active, non-quarantined controllers must reach the child's majority.

A thief without the recovery method still reaches only one vote. An attacker holding a device and the recovery method
already had a normal-mode majority, so no attacker gains power. Fork bridges are unchanged (device-only).

**Evidence.** The ACS authorization table has 16,200 bounded cases and 63 vectors, including the owner-escape and
thief-denial vectors. The table and TLA agree over 98,496 freeze cases (847 accepted by each). The new TLA configs are a
`thief_freeze` pass check and a `trap_thief_freeze` golden trace. The general config passes with 1,952,289 distinct
states. Invariants `LiftIsDeviceOnlyMajority`, `FrozenBlocksControllerChanges` and `FreezeExitOnlyByResolve` are
narrowed exactly to the amended rule.

## F-15 — V-005 evidence was stale and could not run on the current tree

R16C registered W3 object types 24–27, but the V-005 generator and CDDL cross-check iterated every registered type. The
V-005 harness therefore crashed on the current tree, and `make verify` did not run it. R16A had also added 23
`om.critical.` downgrade vectors to the generator without regenerating the committed corpus, and changed the Rust decoder
in a sandbox without a Rust toolchain.

**Decision.** V-005 tools cover the frozen W0 wire only (`wave == "W0"`); later-wave objects carry their own codec
evidence. The corpus is regenerated to 758 vectors (the 735 existing ones unchanged, plus 23 critical-prefix negatives).
Rust and Node agree on 758/758, and both CDDL tools pass.

## F-16 — a W3 signature role widened frozen W0 validity

R16C added `publication_signing_key` (7) to the shared `sig_role` enum and to `sig-role` in [`common.cddl`](../spec/cddl/common.cddl). Every W0
decoder that reads the registry therefore began accepting role 7 on W0 objects such as AccountControlState. That is an
in-place change to frozen W0 validity, which ADR-0165 forbids. It surfaced as 19 "signature-role-out-of-range" vectors
that both decoders now accepted.

**Decision.** [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json) gains `enum_value_waves` (`sig_role` 7 → W3) and an appended rule: enum values
with a later-wave entry are invalid in objects of an earlier wave. [`common.cddl`](../spec/cddl/common.cddl) restores `sig-role = 1..6` for W0 and
adds `w3-sig-role` / `w3-signature`, used only by the Publication envelopes. The W0 Node and Rust decoders drop
later-wave enum values when loading the registry. Future waves extend enums this way, never in place.

## F-17 — canonical geo token boundaries were only in a harness

MESSENGER_BASELINE_V1 required "no canonical-form `geo:` token for any other point" but did not define where a token
starts and ends; only a Python regex did. The token grammar and boundaries are now normative text, and the clean-room
Rust implementation (written from that text) agrees with the Python/Node pair on every location vector.

## Consequences

- R-387 (freeze resolution authority) and R-388 (wave-scoped enum values) are added, with T-156 and T-157.
- V-002 gains an ADR-0168 criterion evidenced by the table, cross-check and TLC results above. V-005 gains an F-16
  criterion evidenced by the regenerated corpus and two-codec run.
- This ADR is Anthropic-authored and passes under ADR-0167's review model, not vendor-independent review.
