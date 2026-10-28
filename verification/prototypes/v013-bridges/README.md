# v013-bridges: bridge adapter contract and loop/reflection model

Feasibility prototype for V-013. Not product code. Not for production.

Written by a Claude agent from [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../../../docs/architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) and
[`TRANSPORT_ADAPTERS_AND_INTEROP.md`](../../../docs/architecture/network/TRANSPORT_ADAPTERS_AND_INTEROP.md). The design package was authored by a ChatGPT design agent and Claude agents;
ADR-0117 (proposed) decides whether an AI reviewer counts as independent. The code is Rust and is not derived from
[`verification/bounded/wire_boundary_models.py`](../../bounded/wire_boundary_models.py).

- [`BRIDGE_ADAPTER_CONTRACT.md`](BRIDGE_ADAPTER_CONTRACT.md) is the candidate contract. Rule IDs (BAC-*) match the
  code, and residual risks are in its §10.
- `bridgesim/` is an explicit-state explorer. It covers every interleaving of deliveries and scripted actions for:
  - B1, from OM to a Matrix-like network (rich text, edits, reactions, metadata);
  - B2, from OM to an XMPP-like network (plain text, no edits, no reactions, metadata extension);
  - an optional OM-unaware foreign-side relay F between them, which is absent, preserves metadata, or strips it.

## Run

`./run.sh` takes about 2 s after the first build. It needs Rust 1.98.1 ([`rust-toolchain.toml`](../../../rust-toolchain.toml)) and `sha2`
(`Cargo.lock`). It writes [`verification/results/v013-bridges-latest.json`](../../results/v013-bridges-latest.json).

## Results

16 scenarios behave as expected. The 8 contract scenarios explore 43,936 states in total and hold every property:

- Bounded
- TypedAuthor
- ForeignProvenance
- IdentityLink
- E2EEBoundaryDisclosed
- Fidelity (against an independent oracle)
- TargetMapping
- NoReflection

The scenarios are:

- an OM-origin message, edit and reaction through both bridges;
- foreign messages, including display-name impersonation and a verified identity link;
- a foreign relay that preserves metadata (OM→M→F→X→OM loops);
- a duplicate import through two bridges;
- a metadata-stripping relay, for OM-origin and for foreign-origin events;
- a bridge removal and re-admission race;
- unmapped reply and reaction targets.

Each of the 8 fault injections (one contract rule disabled) is caught. The violated property is in brackets:

- relaying bridged events through a stripping relay (unbounded echo, `Bounded`);
- a bridge-scoped fingerprint (duplicate import, `NoReflection`);
- ignoring the trace (reflections, `NoReflection`);
- no echo table (`NoReflection`);
- retroactive export (`E2EEBoundaryDisclosed`);
- admission without disclosure (`E2EEBoundaryDisclosed`);
- trusting display names (`TypedAuthor`);
- attaching unmapped targets (`TargetMapping`).

## Design gaps found (proposed design updates)

The details are in contract §10.

1. **Fingerprint scope contradiction.** The metadata refinement says `origin_fingerprint` is "mapping/bridge-domain
   scoped". §14 requires deduplication across independent bridges. The two cannot both hold (`mut_fp_scope_bridge`).
   *Proposal:* scope to (Room, origin), without the BridgeID.
2. **Suppression depends on provenance surviving foreign relays.** It is not guaranteed. A relay that strips metadata
   (a third-party bridge) causes bounded reflections. *Proposal:* add an `origin_unverified` fidelity flag, and a Room
   policy that can refuse such topologies.
3. **Bridge-to-bridge relay through OM must be opt-in.** Without that rule, one stripping relay makes the echo
   unbounded (`mut_relay_bridged_strip`).
4. **No retroactive export.** A bridge may export only epochs from its own membership intervals. A re-admitted bridge
   keeps its interval history.
5. **Cross-bridge dedup of foreign-origin events** needs the foreign relay to expose a stable origin reference. This
   should be listed as an adapter capability (`carries_interop_metadata`).

Registry note: the new files need `python3 tools/om_registry.py sync` (this agent may not edit `machine/`).
