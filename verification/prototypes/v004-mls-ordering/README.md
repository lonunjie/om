# V-004 OpenMLS ordering probe

Feasibility prototype for gate V-004 ([`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../../../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) §3-§5, ADR-0163 §4,
[ADR-0173](../../../decisions/ADR-0173-group-commit-ordering-fork-detection-and-recovery.md)). Not product code. Not for production.

It asks whether a real MLS library can do what the design needs, not whether MLS is correct. The crate is pinned to
OpenMLS 0.9.0 (rust_crypto provider, in-memory storage, `fork-resolution` feature) and drives 4 or 3 devices
through six scenarios. Every result is a boolean or a count, so the JSON is identical on every runtime.

| Scenario | What it shows |
|---|---|
| `s1_ordered_same_epoch_race` | Two members each prepare a direct reducing commit from one epoch and keep it **pending, un-merged**. The application ordering picks a winner (run both ways); the loser's author discards its pending commit, every member accepts the winner and converges; the loser's commit is dead at the new epoch; the loser's reduction is rebased as a fresh commit and converges. |
| `s1b_mutual_removal` | A removes C while C removes A: whichever wins, the loser's author learns it was removed and goes inactive. |
| `s2_partition_fork_and_readd` | Both partitions **merge** their own direct commit. The branches share an epoch number but not state, cannot cross-process each other, and the winner cannot read a message sent on the loser branch. Fork resolution (`recover_fork_by_readding`) re-joins the in-roster loser; a device removed on the winning branch cannot process the winner; the loser's reduction is rebased and everything converges. |
| `s3_external_commit_policy` | A removed device builds an external commit. MLS stages it; the staged commit exposes the joiner credential, so the OM roster check can refuse it before merge, leaving the group unchanged. |
| `s4_crash_restart` | A prepared commit survives a restart (storage copy), the restored node promotes the same commit and converges, a duplicate delivery is rejected by MLS at the new epoch, and a pending commit can be discarded after restore. |
| `s5_past_epoch_window` | With `max_past_epochs = 2`, a message from epoch e decrypts 1 and 2 epochs later and not 3 or 4 (ADR-0121 rule 4 semantics). |

Runtimes: native macOS arm64 and `wasm32-unknown-unknown` run under Node/V8 through wasm-bindgen (OpenMLS needs the
`js` feature on every wasm32 target, so `wasm32-wasip1` is not a target for it). They must agree byte for byte. The
crate also type-checks for `aarch64-apple-ios` and `aarch64-linux-android`; that is a compile check, not a run.

```bash
verification/prototypes/v004-mls-ordering/run.sh
```

## What it does not show

- It is one library at one version, with the in-memory test storage. A production build needs durable storage and the
  OM credential type, and must repeat these scenarios against them.
- Android and iOS were not executed.
- No performance numbers; those belong to V-022.
- Independence: written by a Claude agent from RFC 9420 and the public OpenMLS API. This is single-vendor evidence (ADR-0167).

## Proposed design updates

See [ADR-0173](../../../decisions/ADR-0173-group-commit-ordering-fork-detection-and-recovery.md): scenario 2 shows that a merged MLS epoch cannot be rolled back,
so "accepted" for a direct reduction in a partition means the fork must be recovered, not prevented.
