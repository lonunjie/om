# SQLite Home: real-host run, packaging and capacity of a per-person relay (2026-11-22)

Readiness slice 6. Evidence for PR-013 (a second Home implementation with no cloud-vendor dependency passes the shared provider
conformance suite) and a written capacity statement for a relay run by one person or one small group. It moves PR-013 evidence. It does
not claim resilience: one machine, one operator, one disk, no failover, and no vendor-wide block drill.

## Design sources read

Read in full: [`PROVIDER_CONFORMANCE_SUITE.md`](../architecture/providers/PROVIDER_CONFORMANCE_SUITE.md), [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md) (free-plan and billing guardrails), [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md) (operational budget, conformance tests) and the Home wake channel document sections 1 to 4, 6 and 7. Read in part: the tiers and provider-diversity document (the PR-013 passages). Constitution: digest rules for continuity and maturity claims only. Not read: the identity, crypto and groups documents the design gate also lists for these paths; this change touches packaging, test harnesses and a report, not their subjects. Nothing here changes a design document.

## What was run, and where

All heavy runs were on a Linux x86-64 build host (16 cores, Node 24.15, ext4 on NVMe), under a 24 GB memory cap and idle I/O priority, one run at a time. The host also runs unrelated services, so figures are a floor for the Home and the disk-commit figures in particular depend on its load.

| Check | Result |
|---|---|
| Shared provider conformance, SQLite Home as its own operating-system process on a real TCP listener (new target `sqlite-process`) | 10 files, 116 of 116 cases pass, three consecutive runs after the fix below |
| Same suite, SQLite Home in-process (`sqlite`) and the Worker (`worker`), same host, final tree | 116 of 116 each |
| systemd unit, installed as a user service with the unit's directives that an unprivileged manager can apply | active, serves the capabilities document, refuses creation without the owner value (401), data directory mode 0700, `NoNewPrivs` and a seccomp filter applied, restarts after SIGKILL and serves again, stops on SIGTERM with a clean exit |
| Docker image | Not run. Neither the build host nor the development machine has a container runtime, so the Dockerfile and its README commands are unexercised in this slice |

`systemd-analyze verify` on the system unit is clean. The directives that need a privileged manager (`DynamicUser`, `ProtectSystem`, `ProtectHome`, the kernel and clock protections, the capability bounding set, `StateDirectory`) were not applied in the user-service run, so the full sandbox is verified for syntax only. Run `systemd-analyze security om-home.service` on the target machine.

One finding in the shared suite: the replay case for minted deposit capabilities compared whole responses including HTTP headers. A Node server adds a `date` header, so the comparison failed whenever the replay crossed a second boundary (about 1 run in 12 in-process, 8 in 12 as a process). The Worker never sends one. The case now compares the outcome, not the headers; 15 of 15 repeats pass on the process target. No Home behaviour changed.

## Unit cost on the usage meter

Measured by reading the Home's own usage figure before and after one operation (the read's own cost subtracted). Units are logical: a Worker request, and a Durable Object call (the meter's own call plus the routed call).

| Operation | Worker requests | Durable Object calls |
|---|---:|---:|
| deposit, drain (with or without items), acknowledge, mint, create a queue, usage read | 1 | 2 |
| health check | 1 | 1 |
| wake socket upgrade | 1 | 1 |
| wake pings and wake frames after the upgrade | 0 | 0 |

The Durable Object figure is the binding one: with equal daily ceilings the Home reaches its limit on Durable Object calls at half the request count.

## Idle accounting after the wake channel

The real client wake hub drove 10 idle mailboxes for 7 minutes at the runtime's 3-second tick, with the client's 5-minute safety interval; the same mailboxes polled on the timer for comparison.

| Mode | Worker requests per mailbox per hour |
|---|---:|
| polling on the visible cadence (3 s) | 1,200 (measured, 100 requests for 10 mailboxes in 31 s) |
| polling idle (15 s) / hidden (60 s) | 240 / 60 (the same arithmetic) |
| wake hub, steady state | 12 (one safety drain per 5 minutes) |
| wake hub, including opening drain and socket upgrade in the first 7 minutes | 25.7 |

1,380 of 1,400 ticks were answered locally without a request. All 10 sockets stayed live. The wake channel is only used while the page is on screen; a tab left open but hidden polls at 60 s and is the largest remaining idle cost.

## N simulated users on one relay

Each user holds one mailbox with a wake socket and up to 8 sender capabilities. Messages arrive at random moments at a steady offered rate (0.2 messages per user per second, capped at 3 per second in total, far above real use), for 180 seconds; each message is a deposit, a drain triggered by the wake frame, and an acknowledgement. The Home's meter was reconciled with the client's own request count.

| Users | Messages | Worker requests | Durable Object calls | Per message (W / DO) | Deposit p50 / p95 / max (ms) | Lost, duplicated | Meter vs client |
|---:|---:|---:|---:|---|---|---|---|
| 1 | 36 | 109 | 218 | 3.03 / 6.06 | 46 / 55 / 75 | 0, 0 | equal |
| 5 | 180 | 541 | 1,082 | 3.01 / 6.01 | 44 / 61 / 157 | 0, 0 | equal |
| 25 | 540 | 1,619 | 3,238 | 3.00 / 6.00 | 46 / 168 / 578 | 0, 0 | equal |
| 100 | 540 | 1,621 | 3,242 | 3.00 / 6.00 | 46 / 116 / 238 | 0, 0 | equal |

Every accepted deposit arrived exactly once, nothing was left behind, no deposit was refused, and the usage state stayed healthy. Home memory 78 to 88 MB, disk 0.2 to 5.9 MB. The meter and the client agreed exactly in every run. A message therefore costs 3 Worker requests and 6 Durable Object calls, independent of the number of users.

Throughput is bound by the disk, not by the process. Every write is one transaction with a synchronous commit, and under the usage meter each request commits twice (the meter is one more storage object). On this shared build host, with idle I/O priority, an informal probe measured about 14 ms per deposit commit and 68 deposits per second without the meter (default profile) and about 20 requests per second with it (personal profile), against 450 to 620 deposits per second on a laptop SSD in the earlier load report. A first attempt that offered 125 deposits per second to 25 users on this host exceeded what it could commit and timed out; the offered load above was lowered to fit. The probe is not a committed scenario. A per-person relay needs about 0.001 requests per second at 100 messages a day, so this ceiling matters only to a large shared relay.

## At the quota

Ten users on a personal-profile Home with a Worker ceiling of 6,000, driven past it:

| State | At request | Share of ceiling |
|---|---:|---:|
| soft | 4,843 | 80.7% |
| hard | 5,739 | 95.7% (read every fifth round) |

At the hard state: new deposits are refused with the typed `quota` answer carrying the hard flag (10 of 10 refused deposits did); queue creation is refused; drains, acknowledgements, the usage read and the health check keep working; a wake socket still opens and authenticates. 1,890 deposits were accepted before the stop, 0 lost, 0 duplicated, 0 unknown, none accepted after the first refusal. The day rollover back to healthy is the clock and was not exercised here (the limits report covers its unit test).

## Capacity statement for a per-person relay

Assumptions, all adjustable in `conformance/load/relay-capacity.load.ts`: 100 messages received a day per user; one mailbox per user at this relay; the app on screen 3 hours a day with a wake socket (3 sessions); a tab left open but hidden 8 hours a day; ceilings of 100,000 Worker requests and 100,000 Durable Object calls a day, the figures last known for a free plan and to be checked on the day of use, with the Home reporting the soft state at 80%.

| Per user, per mailbox, per day | Worker requests | Durable Object calls |
|---|---:|---:|
| messages (3 / 6 each) | 300 | 600 |
| on screen with wake (3 opens, 12 safety drains an hour) | about 40 | about 80 |
| hidden tab polling at 60 s | 480 | 960 |
| total with a hidden tab | about 820 | about 1,640 |
| total without one | about 340 | about 680 |

- **One person** on one relay uses 1.6% of the Durable Object ceiling a day with a hidden tab and 0.7% without. Spare capacity is large; the cost is dominated by a tab left open, not by messages.
- **A shared relay** stays under the soft state for about 48 users with a hidden tab each and about 117 with none (Durable Object calls bind; by Worker requests the figures are 97 and 235). A user with several mailboxes at the relay divides these figures by the mailbox count for the polling part.
- Each additional message costs 3 Worker requests. At the ceiling, the relay stops accepting new deposits, keeps delivering what it holds, and recovers at the next day's reset.
- A SQLite Home run by its owner has no plan or bill. The same meter is only a self-imposed guard; the owner chooses the ceiling (`OM_USAGE_CEILING_WORKER`, `OM_USAGE_CEILING_DO`) or runs the default profile with no meter.

## What this does not show

- No resilience claim: one host, one disk, no replication or failover. Restoring an older copy of the data directory can lose accepted items. PR-013's survivability and block-drill obligations stay open.
- The Docker path is untested. The systemd sandbox is verified in part (above).
- The real browser client was not run against this host; the wake hub, the drain and the acknowledgement are exercised by the load client with the real hub.
- Cloudflare's current limits and billing are not measured here, and the Worker target was not put through the capacity scenarios.
- Per-message cost assumes the wake channel is live. A network that drops sockets sends clients back to polling, which is the polling row above.
- The TURN adapter parity test for the SQLite Home is a separate slice.

## Re-running

```sh
pnpm conformance:provider sqlite-process        # the real-process target
pnpm load:home sqlite --capacity --full         # about 20 minutes; JSON in conformance/load/out/
```
