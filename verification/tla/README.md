# OM TLA+ models

Run everything with `make tlc` (or `python3 tools/run_tlc.py`). It uses the digest-pinned TLC 1.7.4
(`tools/tlc.sh`, [`TLAPLUS_TOOL_PIN.md`](../TLAPLUS_TOOL_PIN.md)), checks each config against its expected outcome in
[`models.json`](models.json), writes [`verification/results/tlc-latest.json`](../results/tlc-latest.json), and converts every expected
counterexample into a regression vector in `verification/vectors/tla-*.json`.

## AccountControl (V-002)

[`AccountControl.tla`](AccountControl.tla) replaces `AccountControlState.tla`. The old model is kept in git
history (tag `checkpoint/20260924-0229`). It never parsed, its `generation` was unbounded, one invariant was a
tautology, and it let any actor unfreeze when recovery was enabled.

The new model has an owner and an attacker. The attacker holds its own devices, can compromise some of the owner's
devices, and can hold 0-2 recovery custodians. It covers majority authorization with non-shrinking quarantine,
linking with notices, revocation, recovery rotation, ADR-0111 recovery (delay, veto, one vote, co-signed immediate
completion, two-method requests that only a majority act can cancel), fork detection and freeze, terminal retirement,
LegacyPolicy claim/contest/activation, devices going offline or being lost, and device compromise.

**Environment assumption (explicit):** a live owner device reacts to a recovery notice before the delay elapses.
It is encoded as the guard on `Tick`. The `residual` config drops it (the owner can go offline) and shows the T-041
residual risk.

| Config | Expected | What it shows |
|---|---|---|
| [`AccountControl.cfg`](AccountControl.cfg) | pass (402,192 states) | structural invariants and action properties with every actor enabled |
| `_live_owner`, `_live_owner2` | pass | a custodian holder, even with both custodians, never takes over from a live owner |
| `_one_vote`, `_one_vote_nocustody` | pass | a custodian is worth exactly one vote (R-185) |
| `_off_no_recovery` | pass | with recovery off, losing every device is final |
| `_residual` | violation | T-041: an offline owner loses the race to a custodian holder |
| `_trap_recover`, `_trap_two_device`, `_trap_legacy`, `_trap_fork` | violation | golden traces: recovery after total loss, 2-device reconstitution, legacy activation, fork freeze |

**Design gaps found by TLC** (now normative in [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §13): (1) a revocation could leave
only quarantined controllers, so nobody could vote; (2) a recovery could complete onto a device that had meanwhile been
linked, so it was both kept and revoked.

Bounds: 2-3 owner devices, 1 spare, 1 attacker device, delay 2 ticks, at most 3-4 transitions. These are small-scope
results, not proofs for all N. V-002 still needs review by someone other than the model's author.
