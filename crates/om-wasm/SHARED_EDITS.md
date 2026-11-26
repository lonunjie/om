# Design sources read

The OM design gate, Constitution Digest, plugin and client extension architecture, and ADR-0093, ADR-0124,
ADR-0129, ADR-0132, ADR-0259 and ADR-0266 define this binding. The Core host, publisher chain and grant table
are the implementation sources. The engineering-only bridge room and room services commands are the JSON
and handle precedents. The gate maps the touched source paths to `crypto_1to1`, `discovery_first_contact`,
`identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events`. I used the
Constitution Digest for the constitution entries.

## Shared edits

- `crates/om-wasm/src/lib.rs` declares the private `plugins` module under `scope-engines`.
- `packages/om-core-wasm/src/scope-engines.ts` re-exports the typed plugin factory and types from the scope
  entry.
- The new paths are pending file registry registration as part of lead integration.

## Integration question

1. May the existing JSON dispatcher in `crates/om-wasm/src/bridge.rs` also be edited? The module declaration in
   `lib.rs` alone cannot register a command. The required additions are a feature-gated `PluginsDo { request: Value }`
   variant, a per-Bridge `crate::plugins::Handles` field initialized with `Default::default()`, and a match arm
   that returns `Ok(crate::plugins::plugins_do(&mut self.plugin_handles, request))`. The dispatcher edit is pending.
