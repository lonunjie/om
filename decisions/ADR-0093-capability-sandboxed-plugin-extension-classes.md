# ADR-0093 — Executable plugins are capability-sandboxed and cannot define hidden canonical semantics

## Status
Accepted — 2026-09-23

## Decision
OM distinguishes protocol extensions, inert content bundles, sandboxed local executable plugins, isolated web/mini-app surfaces, remote services and provider adapters. Official-client local plugins use a versioned OM WebAssembly Component Model/WIT capability boundary with no ambient authority. Plaintext-read and network egress are distinct; draft and send are distinct; key/governance access is unavailable. Permission expansion on update requires approval. Shared interoperable state must have published protocol semantics independent of plugin code.
