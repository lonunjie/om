# OM Engineering Repository Layout

## Goal

One monorepo is recommended initially so protocol, core, UI, reference infrastructure, vectors and formal models change atomically. The protocol remains independently implementable.

```text
om/
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock
├── package.json
├── pnpm-lock.yaml
├── rust-toolchain.toml
├── crates/
│   ├── om-types/           bounded IDs/value types; no I/O
│   ├── om-codec/           deterministic CBOR + limits
│   ├── om-protocol/        wire/control/event objects
│   ├── om-core/            identity/relationship/event/application state machine
│   ├── om-control/         AccountControlState implementation
│   ├── om-crypto-api/      CryptoProvider/Profile traits only
│   ├── om-storage-api/     DeviceVault/local/Blob/Backup abstractions
│   ├── om-discovery/       descriptors/invites/contact capsules
│   ├── om-relay-client/    capability delivery client
│   └── om-wasm/            narrow WASM bridge
├── packages/
│   ├── om-ui/              shared React/TS product UI
│   ├── om-design/          tokens/theme/motion engine
│   ├── om-platform-api/    TS capability interfaces
│   └── om-web-platform/    browser implementations
├── apps/
│   └── web/                PWA shell only
├── infra/
│   └── cloudflare-home/    Worker + SQLite DO reference personal Home
├── formal/
│   └── account-control/    TLA+ spec/config/traces
├── fuzz/
│   ├── codec/
│   ├── rich-text/
│   ├── descriptors/
│   └── attachments/
├── conformance/
│   ├── vectors/
│   ├── negative/
│   └── runner/
├── spec/
│   ├── cddl/
│   ├── registries/
│   └── profiles/
├── tools/
├── docs/
└── third_party/
    └── REVIEW.md
```

## Dependency direction

```text
om-types
   ↓
om-codec
   ↓
om-protocol
   ↓
om-core
   ↓
om-wasm / native bridges
   ↓
om-ui
   ↓
platform shells
```

Provider implementations depend on public interfaces; `om-core` must not import Cloudflare/browser/native UI code.

## Rust rules

- stable Rust; edition 2024 unless a current stable toolchain gives a documented reason otherwise;
- `#![forbid(unsafe_code)]` in types/codec/protocol/core/control where feasible;
- unavoidable FFI/crypto unsafe isolated into tiny reviewed modules;
- no panics on network/untrusted input paths;
- checked arithmetic for lengths/counters;
- bounded allocation before parsing attacker-controlled lengths;
- secrets use zeroizing containers where meaningful, but do not claim language/runtime guarantees that cannot be delivered (especially WASM/browser copies);
- OS CSPRNG only for security randomness;
- no home-grown primitive implementation.

## TypeScript/web rules

- TypeScript strict mode;
- no `any` crossing Core/Platform boundaries;
- Trusted Types + restrictive CSP in PWA production configuration;
- no `dangerouslySetInnerHTML` for messages/themes;
- no remote runtime CDN dependencies/fonts/themes;
- service worker app shell does not own cryptographic semantics;
- CSP failures, theme validation failures and malformed deep links fail closed.

## UI framework default

Start with current stable React + TypeScript + Vite in `om-ui`/`apps/web`.

This is an engineering convenience, not an OM compatibility layer. Do not introduce React types into Core/Protocol APIs. Replace only if measured prototypes show unacceptable low-end performance, accessibility limitations or WebView behavior.

## Core-to-UI API

Prefer coarse typed calls/events over chatty FFI:

```text
Command -> Core transaction -> ViewDelta/ViewState
```

Do not expose raw private keys or mutable internal structs to JS.

## Storage

PWA v1 may use IndexedDB behind a storage adapter, with application-layer encryption and explicit lifecycle classes. Do not make IndexedDB schema the portable backup format.

Native clients may use SQLite/SQLCipher-like storage behind the same semantic storage API after separate review.

## Cloudflare reference Home

Worker/DO code is deliberately dumb/untrusted relative to content:

- capability/ticket validation;
- quotas/rate limits;
- opaque encrypted-envelope storage;
- push trigger;
- public discovery descriptor serving.

No conversation plaintext, message indexing or identity master keys.
