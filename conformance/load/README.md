# Home load and soak harness

Repeatable load, soak and limit checks for both reference Homes: the Cloudflare Home (the real Worker bundle in workerd, through
Miniflare) and the SQLite Home (its own Node process on loopback). The same raw wire client drives both, so numbers compare.
Results of the last full run: [`docs/reports/2026-11-18-home-load-results.md`](../../docs/reports/2026-11-18-home-load-results.md).

## Run

```sh
pnpm load:home                       # both Homes, quick scale (about 1 minute each)
pnpm load:home worker                # one Home
pnpm load:home sqlite --full         # full scale (about 8 minutes each)
pnpm load:home --full --out /tmp/om-load   # keep the JSON somewhere else
```

One JSON file per Home is written to `conformance/load/out/` (git-ignored): `<home>-<scale>.json`. The run fails if any
invariant is violated (the list is printed and stored under `violations`). It is not part of `pnpm test` or `pnpm conformance:provider`.
Needs Node 24, a free loopback, and about 1 GiB of free memory at full scale. Run it on a quiet machine: the client and the
Home share the CPU, so absolute numbers are a floor, not a capacity statement.

## What each scenario checks

| Scenario | Load | Must hold |
|---|---|---|
| ramp | many mailboxes; deposit, drain, ack at 1, 4, 16, 64 concurrent clients | throughput and latency percentiles; only documented typed outcomes (`rate_limited`, `quota`); after every stage each stored, unacked item is present byte for byte, once, in order; no acked item comes back |
| limits | one mailbox at its bounds | largest envelope accepted and one over refused; a 24 MiB body refused without memory growth; 512 items then `quota`, frees exactly what is acked; 16 MiB queue bytes never exceeded; 64 deposit capabilities; 30-deposit burst then `rate_limited` with `retry_after` that is honoured |
| blob spool | 4 MiB blobs | 32 MiB spool bound, 4 MiB object bound, 64 object bound, read back intact, delete frees space |
| recovery store | inboxes, requests, a 1 MiB record | 16 inboxes, whole-Home post budget answers `rate_limited`, every accepted request drains once and unchanged |
| wake | one socket per queue, a deposit per queue at once | every deposit wakes its socket; 8 sockets per queue, the ninth answered `rate_limited` before the upgrade; memory per socket |
| usage (two runs: Worker requests, Durable Object calls) | personal profile with a small ceiling | healthy, soft, hard in order; hard sheds deposits, queue creation and blob puts with `quota` and the hard flag and still serves drains, acks, usage read, health check and recovery; the Home's count equals the client's exactly |
| soak | mixed traffic for minutes, 8 clients | memory and disk over time, throughput trend; then every mailbox audited, the Home killed (SIGKILL for the SQLite Home) and restarted, audited again, drained, disk measured |
| crash | traffic with the Home stopped at a random moment, three times | everything the Home acknowledged is present after the restart; nothing acked comes back |

Scale is set by `OM_LOAD_SCALE` (`quick` or `full`, see `P` in `load.load.ts`). A Worker keeps its write-ahead log until a clean
stop, so its disk figure during a run includes it; the figure after a clean stop is the stored size.

## Files

- `run.mjs`: the command line; `load.load.ts`: the scenarios (vitest, run through `vitest.config.ts`); `lib.ts`: Home launchers, latency recorder, memory and disk samplers, the no-loss ledger; [`home-process.mjs`](home-process.mjs): runs the SQLite Home as its own process.
- Groups at the Core level are a separate check: `crates/om-core/tests/groups_load.rs` (a 32-member Room, bursts and churn). Report: `cargo test -p om-core --release --features groups-openmls --test groups_load -- --ignored --nocapture` (`OM_GROUP_BURST`, `OM_GROUP_ROUNDS` scale it).
