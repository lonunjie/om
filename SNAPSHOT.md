# OM Master Planning, Design & Engineering Snapshot

Generated: **2026-09-22 14:34 PHT (UTC+08:00)**

This snapshot supersedes the 2026-09-22 13:42 PHT UI/UX checkpoint, preserved under `history/20260922-1342/`.

## Phase

**Architecture candidate -> engineering prototype + adversarial validation.**

The architecture is sufficiently complete to begin implementation. Production-security claims remain blocked on deterministic wire/profile freeze, AccountControlState formal validation, production crypto integration, fuzzing/conformance, supply-chain gates and independent review.

## Major additions

- engineering-agent start/handoff/repository instructions;
- ClientHost + DeviceVault + origin migration model;
- built-in multi-identity/profile isolation, with UI switcher later;
- safe semantic RichTextV1;
- hostile decrypted-content/link-preview security boundary;
- message-deniability vs durable-control-signature direction;
- LocalDataClass and local-first diagnostics;
- deterministic-CBOR direction;
- formal AccountControlState validation gate;
- production crypto integration gate and 2026 security/crypto re-audit;
- theme light/dark/Auto requirements and optional per-profile theme suggestions;
- ADR-0059 through ADR-0068.

## Engineering source of truth

Begin with `ENGINEER_START_HERE.md`. `ENGINEERING_HANDOFF.md` defines the execution work packages.

## Highest-priority engineering gates

1. repository/trust-root bootstrap;
2. deterministic encoding + golden vectors;
3. AccountControlState TLA+ model;
4. DeviceVault/Profile isolation;
5. shared UI shell;
6. personal Home/discovery vertical slice;
7. event/rich-text vertical slice with dev-only crypto;
8. hostile-content/fuzz/local-data lifecycle;
9. dedicated production CryptoProfile integration/review.
