# Groups host driver: status (2026-11-14)

## Design sources read

Read in full in this session (with the Read tool or `cat`): [`AGENTS.md`](../../AGENTS.md) "Design gate"; [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md);
[`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) lines 40 to 130 (sections 3.1, 3.2, 4, 5: coordinator, same-parent ordering, binding, journal);
[`2026-11-13-groups-plan.md`](2026-11-13-groups-plan.md) (all 333 lines, including Q1, Q2, G9 and the slice 2b and wasm/TS sections);
[`2026-11-14-groups-ui-status.md`](2026-11-14-groups-ui-status.md) and [`2026-11-14-groups-ui-questions.md`](2026-11-14-groups-ui-questions.md); [`ADR-0187`](../../decisions/ADR-0187-mls-engine-on-wasm-unforked-openmls-with-host-entropy-and-clock.md);
ADR-0128 rules 5 to 7 (lines 45 to 66); and the code of `runtime.ts`, `groups.ts` (platform-api), the harness `groups.ts`, `groups.test.ts`, `groups/flow.rs` (the wire, `Target`, the Commit and Welcome steps), `loadCore.ts`, `RealApp.tsx`, `main.e2e-core.tsx`, `vite.config.ts`.

The committing session then read every document and ADR that `python3 tools/design_gate.py needs` lists for the commit's file paths (pass the file paths: the directory arguments `packages/om-core-wasm apps/web` match no `code_paths` glob and return fewer domains): all eight domains `client_endpoint_distribution`, `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events` and `ux_accessibility_personalization` (about 50 documents and 67 ADRs, roughly 690 KB), plus ADR-0177 and ADR-0187 (Accepted by the operator, 2026-11-14).

## Answers to the open questions (what the design says)

**Q2 (which 1:1 object type carries `GroupWire`, which delivery kind carries Commit, LeaveRequest and App): still genuinely silent.** What the text does say:
a Room invitation, the accept with the KeyPackage release, and the Welcome travel "over the existing 1:1 relationship" (GROUP_CHAT_ARCHITECTURE "Creating a group", "Adding a person"; ADR-0128 rule 5);
every application message is one MLS ciphertext wrapped in an independently encrypted per-recipient envelope sent to that device's opaque delivery capability, never a raw MLS message on a relay (ADR-0128 rule 7).
[`om-object-types.json`](../../spec/registries/om-object-types.json) has no object for a Room invitation, accept, decline, Welcome, or a group transport wrapper; the groups documents and registries name none. Building a carriage means choosing an object type, a CDDL
shape and a delivery kind, which is wire the audited text does not give. Not built, not invented.

*Recommendation (constitution-aligned, for the operator to confirm):* one additive W1 object, `GroupTransport`, whose plaintext is the Core-internal wire, carried **inside the existing sealed 1:1 session of each destination device** (so the per-recipient envelope, fresh outer wrapper and
opaque capability of rule 7 are the 1:1 machinery that already exists, and the MLS ciphertext rides inside it: this is not a pairwise-fanout group mode, because the group key agreement stays MLS). No Home endpoint, no directory, no OM-run service (OPERATORLESS_CORE: no indispensable institution). It needs an ADR that
allocates the object type and CDDL, a Core change to seal and open that object for a relationship and for a named member device, and the `0xF04D` GroupContext extension code point (still unregistered, plan "Slice 2b committed").

**G9 (signed intent envelope, ACS-validated device-to-account mapping, MLS credentials instead of the released `pk`): still open and not answerable from the text.** ADR-0176 freezes the `GroupControlIntent` body but no signature envelope for objects 28 to 31 is described
in the documents read; the ACS device lookup is Core's. Recommendation: the signed envelope is the existing device-role composite signature over the intent body with a new domain label (as the binding already does), and the committer's device key comes from the receiver's validated ACS head, never from the sender. Needs the same ADR.

**New, G10: when may a committer report its own Commit accepted (`commit_accepted`)?** GROUP_COMMIT_AND_STATE_CONVERGENCE section 5 rule 3 says a relay or storage acknowledgement is never acceptance, and section 3.2 makes
the canonical successor the minimum of `(non-reducing?, direct?, committer DeviceID, binding hash)`, merged by every member as it sees it. The text does not say what evidence lets the committer itself treat its Commit as the canonical one (the committer cannot see a competing Commit that is still in transit). The driver therefore
takes the policy as an injected `CommitAcceptance`; **the default is none**, so a prepared Commit is reported `commit_published` once every wire is out and then stays prepared: the group is paused (`canSend` false) and the Welcome is not sent. A test pins this. *Recommendation:* a coordinator-sequenced Add (the committer is first in `coordinator_succession`)
is accepted once the Commit has been handed to every member device and no better Commit for the same parent has been received within `sequencing_timeout`; reductions follow the same rule. Needs an audited-text answer; the loopback and the e2e use `acceptAfterDelivery`, which is explicitly not that rule.

## What was built

A host driver, a loopback network, the wiring in the web client behind the existing capability flag, and tests. Nothing in Rust changed.

- `packages/om-core-wasm/src/groups-driver.ts` (new): `createGroupDriver`, `GroupCarriage` (the transport port: `available`, `send`, `onReceive`), `CommitAcceptance`, `acceptAfterDelivery` (test only).
  The driver never reads a wire. After each Core step (`do`, `receive`) it writes the step's outbounds as opaque sealed blobs in the profile's EncryptedKv (`gdrv/out/`, outside Core's `core/` records) **before the step is returned**, sends them in order, deletes a blob only after the transport took it (`sent`), keeps order across an
  outage (`unavailable` stops the pass and retries the same bytes; `undeliverable` drops), and walks a Commit through `commit_published` (after every wire tagged with that intent is out) and `commit_accepted` (only if a policy says so, then sends the Welcome the step releases). Markers (`gdrv/pend/`) make a half-reported Commit resume after a restart.
  `available()` is true only when the engine is present **and** a carriage is present and available; otherwise every step refuses `groups_unavailable` and reads are empty, so the UI shows no group control.
- `packages/om-core-wasm/src/runtime.ts`: option `groupDriver: { carriage, acceptance? }`. Given, `client.groups` is the driver; the runtime flushes owed outbounds when a profile opens, includes the driver in `idle()` and disposes it. Absent, `client.groups` is the bare engine as before (all existing tests unchanged).
- `packages/om-core-wasm/src/groups-loopback.ts` (new, exported through `test-support` so the existing production guards cover it): `createGroupLoopback` (in-process network with down, duplicate-delivery and a log) and `createClientPairCarriage` (two identities in one runtime, for the browser e2e).
- Web client: `loadCore.ts` loads `om_wasm_groups.wasm` instead of the shipped module only when `VITE_OM_GROUPS=1` (a statically dead dynamic import otherwise: verified that a plain `pnpm build` bundle holds no groups module and passes `check:no-harness`), and passes a carriage supplied by the host.
  `RealApp` takes an optional `groups` prop; **no product carriage exists, so a real build has none and `groups.available()` stays false**. [`vite.config.ts`](../../apps/web/vite.config.ts) puts `VITE_OM_GROUPS` on the same refuse-in-production list as the test flags (ADR-0187 is now accepted, but no product carriage exists, Q2) (verified: `VITE_OM_GROUPS=1 vite build` fails with the R-072 message).
  `pnpm --filter @om/web build:groups-e2e` builds `dist-groups-e2e` (served on port 4180 by the Playwright config), [`ensure-core-wasm.mjs`](../../apps/web/build/ensure-core-wasm.mjs) builds the groups variant when asked, [`package.json`](../../package.json) of `@om/core-wasm` exports `om_wasm_groups.wasm`, `.gitignore` lists `dist-groups-e2e/`.

## Verification

- `vitest run --dir src` in `packages/om-core-wasm` (`groups-driver.test.ts`: 7 scripted-engine tests and 4 over two real wasm Cores with OpenMLS): create, invite, accept (Bob is shown as invited, not a member), Commit published and accepted, Welcome over the 1:1, text both ways with every application message going only to devices (never over a relationship),
  an outage with durable ordered retry, a duplicate delivery shown once, a restart of the driver and of the whole runtime over the same store (an owed message is still delivered), Bob leaves and his device stops sending, no carriage means unavailable, and no policy means a paused group.
- Chromium e2e `apps/web/e2e/groups-live.spec.ts` on `dist-groups-e2e` (two identities in one browser, the real Core with OpenMLS, the real UI): Alice creates a group with Bob, Bob gets the invitation and joins, a message each way. Passed (about 6.5 s).
- Size (`dist-groups-e2e`, production-optimised wasm): `om_wasm.wasm` 5,131,394 bytes (gzip 1,308,330); `om_wasm_groups.wasm` 7,300,940 bytes (gzip 1,961,280); delta +2,169,546 bytes raw, +652,950 gzip (plus 42 percent raw, 50 percent gzip). The ADR-0187 budget (about 3x) is respected; no budget was set by the operator.
- Final gate run: `vitest run --dir src` om-core-wasm 37 passed (6 files), apps/web 138 passed (20 files); `tsc -b apps/web` clean; `eslint` clean on every file touched here (om-core-wasm `runtime.ts` and `groups.test.ts` keep their older unused-variable and assertion lint errors, not caused by this work);
  Chromium e2e `groups-live`, `groups` (harness) and `core` passed; `cargo test -p om-wasm --locked` and `cargo test -p om-core --locked` pass with no Rust change; plain `pnpm build` plus `check:no-harness` ok; `python3 tools/om_docs.py fmt` 0 unresolved links; `make check` OK.

## Deviations

Numbering continues the relay-status table (D1 to D29) and the groups UI report (D30, D31); the next free number was D32.

| ID | Item | What was done | Needs |
|---|---|---|---|
| D32 | **The group wire is carried by a host-supplied transport, not by a specified 1:1 object type** (GROUP_CHAT_ARCHITECTURE "Creating a group": "over the existing 1:1 relationship"; ADR-0128 rule 7: per-recipient envelopes). | The driver defines only the `GroupCarriage` port. The only implementations are the in-process loopbacks (test and e2e). A real build has none, so groups are unavailable there. The wire stays Core-internal JSON (Q2). | The Q2 ADR and Core change above. |
| D34 | **`commit_accepted` policy (G10).** | Injected, default none (a prepared Commit stays prepared). Test and e2e builds use `acceptAfterDelivery`. | An audited-text answer to G10. |

`Design-Departures` for the commit: `D32, D34, ADR-0187`. D33 (host-persisted outbound blobs) was dropped after the full read: ADR-0120 K1 and ADR-0163 sections 2 and 4 require the exact Commit bytes and the outbox durable before transport, so persisting them is what the design asks; the platform-api comment "do not persist it" is only an engineering note about the type and is not a design rule.

## What remains (for the operator)

1. **Q2** (the carriage: object type, CDDL, Core sealing for a relationship and a member device) and **G9** need an ADR from the design owner. Until then groups cannot be on in a real build, and that is intended.
2. **G10** (accepted policy) needs an answer; with none, no real group can complete an Add.
3. ADR-0187 is accepted. Once Q2 is answered, remove `VITE_OM_GROUPS` from the refuse list, decide how the groups module ships (a second signed asset: its digest must join the release manifest, [`release-delegation.json`](../../apps/web/release-delegation.json) and the reproducible-build check; the supply-chain and distribution documents were not read here) and set a size budget (measured above).
4. The `0xF04D` Room context extension code point has no registry entry.
5. `Q1` and the UI questions (names, capability flags, change feed) are unchanged.

## Commit

Committed with `Design-Read` for all eight domains above and `Design-Departures: D32, D34, ADR-0187`. Files: `packages/om-core-wasm/{package.json,src/{groups-driver.ts,groups-driver.test.ts,groups-loopback.ts,runtime.ts,index.ts,test-support.ts}}`, `apps/web/{package.json,vite.config.ts,build/ensure-core-wasm.mjs,e2e/groups-live.spec.ts,e2e/playwright.config.ts,src/core/{loadCore.ts,RealApp.tsx},src/dev/main.e2e-core.tsx}`, [`.gitignore`](../../.gitignore), [`machine/file_registry.json`](../../machine/file_registry.json), this report.
