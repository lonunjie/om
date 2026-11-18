# Home load and soak results (2026-11-18)

Readiness Stage 1, "first load test and per-relay quota behaviour". Both reference Homes (the Cloudflare Home as the real Worker
bundle in workerd through Miniflare, and the SQLite Home as its own Node process on loopback) were driven by the same raw wire
client with many mailboxes, deposits, drains, acks, wake sockets, blob spool and recovery-inbox traffic, at increasing rates and
at every documented limit. Plus a 32-member Room at the Core level. Numbers only; the harness and how to re-run it are in
[`conformance/load/README.md`](../../conformance/load/README.md).

## Design sources read

Read: [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md) ("Free-plan and billing guardrails") and [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md)
("Operational budget behavior", "Required conformance tests"), the limits and usage rows of
[`2026-10-28-relay-status.md`](2026-10-28-relay-status.md) (D29 usage read, D31) and the code of both Homes. Not read in full: the
other documents the design gate lists for these paths (constitution, identity, discovery, crypto, wake channel, groups) beyond what
the code comments cite. Nothing here changes a design document; the two fixes below make code match behaviour the design already
states (a Home that starts after a crash; a removed device that can be invited again, ADR-0207).

## Result

Both Homes held every invariant in every scenario: no acknowledged deposit was lost or altered, no acked item came back, no
sequence went backwards, only the documented typed errors appeared, and no stated bound was exceeded. The load run found two real bugs, both fixed
with a failing-first test: the SQLite Home could refuse to start after a crash (kill during a write), and a Room member removed and
later invited again could not rejoin on the real MLS engine.

Environment of this run: one laptop (Apple silicon, Node 24.14, Miniflare 4.20260611 / workerd), client and Home on the same
machine and the same loopback, so absolute figures are a floor for the Home, not a capacity statement; the relative picture and the
behaviour at the limits are the result. Scale "full": 300 mailboxes per ramp stage, 10 s per stage, 150 mailboxes and 180 s
soak, 100 wake queues.

## Throughput and latency (ramp)

Mixed workload per client: deposit (1 KiB or 16 KiB ciphertext), and on every second visit a full drain and ack of that mailbox.
Requests per second counts every request (deposit, drain, ack). Deposits beyond a capability's burst answer `rate_limited` (documented, 30 burst and 1 per second per
deposit capability) and are not retried here, so the accepted-deposit rate is the useful throughput figure. Latency in ms, p50 / p95 / p99 / max.

| Home | clients | requests/s | deposits accepted/s | deposit | drain | ack |
|---|---:|---:|---:|---|---|---|
| Worker | 1 | 1774 | 872 | 0.53 / 1.06 / 3.4 / 11 | 0.33 / 0.54 / 2.4 / 11 | 0.37 / 0.62 / 1.5 / 10 |
| Worker | 4 | 1826 | 899 | 1.65 / 3.8 / 8.3 / 1070 | 1.37 / 2.9 / 7.8 / 202 | 1.44 / 2.9 / 7.1 / 1520 |
| Worker | 16 | 1593 | 785 | 6.3 / 14 / 30 / 3532 | 5.9 / 13 / 26 / 3529 | 6.7 / 14 / 31 / 3533 |
| Worker | 64 | 1266 | 622 | 31 / 89 / 167 / 3316 | 30 / 82 / 193 / 3315 | 31 / 79 / 157 / 3292 |
| SQLite | 1 | 943 | 457 | 1.17 / 2.1 / 3.4 / 25 | 0.58 / 0.97 / 1.8 / 4.2 | 0.85 / 1.5 / 2.6 / 13 |
| SQLite | 4 | 1279 | 624 | 3.8 / 6.0 / 8.2 / 36 | 1.6 / 2.6 / 3.9 / 9.1 | 2.8 / 4.2 / 6.2 / 25 |
| SQLite | 16 | 1272 | 622 | 16.7 / 22 / 30 / 51 | 5.7 / 8.8 / 12 / 27 | 10.3 / 15 / 20 / 34 |
| SQLite | 64 | 1230 | 601 | 70 / 91 / 130 / 345 | 24 / 33 / 39 / 90 | 43 / 58 / 67 / 111 |

Both Homes saturate at about 1.2k to 1.8k requests per second on this machine; past saturation extra clients only add queueing
(latency grows about linearly with clients, throughput flat to slightly falling). The SQLite Home is steady (max latency under
350 ms at 64 clients, 25 ms at one); every write is one transaction with synchronous commit, so it pays about 1 ms of
commit per deposit. The Worker is faster at the median but shows rare multi-second stalls (max 1.1 to 3.5 s at 4, 16 and 64 clients, and
a handful of requests per stage) that the SQLite Home does not; these are in the local runtime (not seen at one client; p99 stays
under 200 ms) and were not chased further. After each stage every mailbox was audited in full against the ledger: nothing missing, altered or repeated.
Queue creation: Worker 19 to 22 ms p50, SQLite 12 to 20 ms p50 (one new storage object each).

Deposit to wake frame, 100 queues with one socket each, one deposit per queue at once, five rounds (500 deposits, 0 missed):
Worker 38.8 / 111 / 132 / 135 ms, SQLite 115 / 207 / 212 / 213 ms. The wake adds nothing measurable to the deposit's own latency (the two lines are within 1 ms at the median); the SQLite figure is the 100 concurrent deposits
queueing behind each other's commits. One socket costs no measurable memory at this scale (the difference is inside the noise of
about 20 MB).

Blob spool, 4 MiB object: put p50 148 ms (max 579) on the Worker, 89 ms (max 117) on SQLite; get p50 19 ms and 8 ms. A 1 MiB recovery record: put 15 ms, read back byte for byte. 64 KiB blob puts in the soak: p50 3.0 ms and 9.9 ms.

## Behaviour at the limits

Identical on both Homes (they run the same handlers).

| Limit | Result |
|---|---|
| Largest conforming envelope (132,267 bytes) | stored |
| Ciphertext one byte over its shape bound | `invalid_request` |
| Body over the 133,000-byte bound | `payload_too_large` |
| 24 MiB body | Worker: `payload_too_large`, memory +2 MB. SQLite: connection cut after the 16 MiB read bound (by design), memory unchanged |
| Queue items | exactly 512 accepted, then `quota` (class items, not hard); after acking 10 exactly 10 more accepted |
| Queue bytes | 126 envelopes, 16,665,642 bytes stored (bound 16,777,216), then `quota` |
| Deposit capabilities per queue | 64 (the queue's own plus 63 minted), then `quota` |
| Per-capability rate | 80 sent back to back: 30 accepted (the burst), 50 `rate_limited` with `retry_after` 1 s; accepted again after the stated wait |
| Wake sockets per queue | 8 open; the ninth answered `rate_limited` (HTTP 429) before the upgrade |
| Blob spool | 8 blobs of 4 MiB (the 32 MiB bound) then `quota`; 4 MiB + 1 byte `payload_too_large`; all 8 read back intact; deleting one frees its space; with small blobs exactly 64 then `quota` |
| Recovery store | 16 inboxes then `quota`; Home-wide post budget: 10 accepted then `rate_limited`; every accepted request drained once, unchanged; ack removes |
| Retry table per mailbox (limit 8192 rows) | 9,000 remembered requests written to one mailbox: growth slows after row 8192 (about 480 KiB per 1000 rows before, 270 KiB after); one mailbox peaks near 4.2 MiB of database |

### Usage states (personal profile, ceiling 3000)

Run twice, once per metric, with sequential traffic so the request index of each transition is exact. Same on both Homes.

| Metric | soft at request | hard at request | shed ratio | At hard |
|---|---:|---:|---:|---|
| Worker requests | 2400 / 2402 (80.0%) | 2856 / 2857 (95.2%) | 0.952 / 0.953 | deposit `quota`+hard, queue create `quota`, blob put `quota`; drain, health check, usage read and recovery register served |
| Durable Object calls | 1201 / 1202 | 1429 / 1430 | 0.953 / 0.954 | same |

(Worker / SQLite where two values.) States went healthy, soft, hard and never backwards. The Home's own count equalled the client's count exactly at the end (2867 = 2867, 1440 = 1440 on the Worker; 2868 and 1441 on SQLite), healthz and the read included, as the documented rule says. Acked data was still readable at hard.
The day rollover back to healthy was not exercised (it is the clock; unit-tested).

## Soak, memory and disk

Eight clients, 150 mailboxes, mixed deposit, drain, ack, 64 KiB blob put and delete, for 180 s, then audit, items left behind, kill, restart, audit, drain all, clean stop.

| | Worker | SQLite |
|---|---:|---:|
| Operations | 404,824 (2,249/s) | 189,062 (1,050/s) |
| Throughput first third / last third | 2,441 / 2,269 per s | 1,144 / 1,035 per s |
| Deposits accepted, acked | 31,500, 31,496 (4 held) | 31,408, 31,408 |
| Home memory idle before | 37.7 MB | 62.6 MB |
| Home memory during load (after warm-up / end / peak) | 215 / 322 / 407 MB | 146 / 162 / 195 MB |
| Home memory 5 s after the load stops | 29.3 MB | 60.6 MB |
| Disk during and after drain, clean stop | 642.5 MB (database 38.1, write-ahead log 599.4, index 5.0) | 40.7 MB (no log file) |

Memory rises with load and falls back to idle once the load stops; there was no sustained growth in either (the series goes up and down as the runtime collects; idle memory after the load equals idle memory before). Throughput fell by 7 to 10% over the three minutes on
both, with no cliff. The disk figure for the Worker is dominated by the write-ahead logs Miniflare keeps per storage object and does not fold in on a clean stop; its databases total 38 MB for 150 mailboxes, the same size as the SQLite Home's 40.7 MB, so the stored size is the second figure, and what Cloudflare itself holds is not measured here. The SQLite Home's database size is bounded by the retry tables (about 270 KB per mailbox after 400 operations each, 4.2 MiB at the 8192-row bound) and stays flat once mailboxes are empty (`auto_vacuum` on).
Memory and disk of the Worker and the SQLite Home are not comparable to each other beyond that.

## Never lose acked data, never corrupt

- Ledger: every deposit the Home acknowledged is checked after every ramp stage and after the soak; every drained item is hashed and matched, order and uniqueness checked, every acked item checked not to return. 36,469 deposits and 29,248 acks on the Worker ramp and 23,095 and 18,735 on SQLite: zero violations.
- Crash during load: the Home was stopped at a random moment under 8-way traffic, three times per Home (SQLite: SIGKILL of the process; Worker: stop of the Miniflare host, since a Worker has no process to kill from outside). Restart in 36 to 115 ms; every acknowledged deposit was present and nothing acked came back (SQLite: 394 to 1,829 acknowledged deposits per round, Worker: 2,389 to 3,926). Plus the post-soak kill with items left behind: 600 of 600 (SQLite) and 604 of 604 (Worker) found after restart.

## Bugs found and fixed

1. **SQLite Home failed to start after a crash.** Start-up read each storage file's pending alarm by opening it read-only; a file with a rollback journal left by a kill in the middle of a write cannot be opened read-only ("attempt to write a readonly database"), so the Home refused to start until the file was opened for writing by something else, which nothing does before start. Found by the crash scenario (first in 1 of 3 early rounds, then reproduced). Fix: open for writing at start (a writer rolls the journal back). Test PC-H05 builds the exact post-crash state (database partly rewritten plus journal), failed before the fix and passes after; the crash scenario then passed 8 repeats (24 kills) and the full run.
2. **A removed Room member could not be invited again.** On the real MLS engine a device that learned it had been removed kept an inactive copy of the group under the same id, so the Welcome of a later ordinary re-invitation (ADR-0207) answered `welcome_rejected`. Found by the 32-member Room churn. Fix: the engine drops the group state when the device learns it is out (its epoch secrets go with it); behaviour seen from Core is unchanged (`unknown_group` for any later use). Tests: `groups_openmls::a_removed_device_keeps_no_group_state_and_can_be_added_again` and `groups_loopback::a_removed_device_is_admitted_again_by_an_ordinary_invitation`, both failed before. The stand-in engine used by the default test run already allowed this; the two engines now agree. The groups module grew 8,112 bytes (size budget delta +1,883,997 of +2,278,023).

Harness fixes found along the way are not product bugs (a hang on stopping a dead child, a 64 KiB random limit in the test kit).

## Groups at the Core level

32 accounts, each one device, on the real MLS engine (stock OpenMLS), built as in `groups_loopback.rs`: every commit through the journal and the committer-signed binding, every other member applying it through `group_receive_commit`. Release build; one process, so memory is the whole test process with 32 engines and 32 states. Run: 500 messages per burst, 20 churn rounds of 4 removals and 4 re-admissions (80 and 80).

| Operation | n | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|---:|
| Admission commit, prepare and accept (admin) | 31 | 1.88 ms | 2.68 | 3.05 | 3.05 |
| Welcome join (joiner) | 31 | 0.77 | 1.33 | 1.36 | 1.36 |
| Commit apply (receiver), build | 465 | 1.10 | 1.33 | 1.41 | 1.44 |
| Commit apply (receiver), all, with churn | 5,025 | 1.16 | 1.40 | 1.72 | 5.88 |
| Encrypt | 500 | 0.18 | 0.19 | 0.21 | 0.36 |
| Decrypt (31 readers per message, 15,500 reads) | 15,500 | 0.19 | 0.21 | 0.22 | 10.6 |

Building the Room: 0.60 s for 31 admissions. About 160 messages per second sent and 4,900 reads per second. After 191 epochs the engine snapshot is 54 KiB per device (max 57) and the app state 99 KiB (the admin 419 KiB: its bounded journal tail). Memory 17.5 MiB when built, 14.0 after the burst, 15.9 at the end: no growth through 80 removals and 80 re-admissions. Invariants asserted throughout: all members on one epoch, one head hash and one exporter secret after every phase; every message read by every other member with the right sender; no removed member reads anything sent after its removal (80 of 80 refused). Not covered: the transport frames of group messages (`group_frame_send`) and the Home's behaviour under 32 concurrent group members (the Home is exercised above with the same kinds of requests).

## Not covered, and caveats

- No real Cloudflare account was used (it must not be a dependency); Cloudflare's own limits, billing and storage were not measured. The plan ceilings in the usage meter are the ones in `limits.ts`, to be re-checked on the day of a deploy.
- The Worker ran in workerd under Miniflare: no 128 MB isolate limit, no network latency, local storage. The SQLite Home ran with `synchronous=FULL` on a laptop SSD; a slower disk lowers its numbers.
- Client and Home share one machine; the client is part of the measured CPU.
- Power-loss behaviour (as opposed to a killed process) is not tested, and the Worker has no process to kill; its crash rounds stop the host cleanly.
- Day rollover of the usage meter, 7-day and 14-day retention and a multi-hour soak were not run (the soak is three minutes; the full scale is set in `load.load.ts`).
- Real separate-host run of the SQLite Home, the block drill and the Docker image remain open (see the SQLite Home status entry).

## Re-run

```sh
pnpm load:home --full                 # both Homes, about 12 minutes
pnpm conformance:provider             # the 104 public-API cases on both Homes (unchanged, 104 of 104)
cargo test -p om-core --release --features groups-openmls --test groups_load -- --ignored --nocapture
```
