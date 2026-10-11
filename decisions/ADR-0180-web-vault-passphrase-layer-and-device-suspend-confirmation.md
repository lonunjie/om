# ADR-0180 — Web vault passphrase layer; device suspend keeps its confirmation

- **Status:** Accepted (operator direction, 2026-10-11; recommendations adopted)
- **Date:** 2026-10-11
- **Clarifies:** [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) §4-5 rule 3 (passphrase factor); [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §2.2 rule 2 ("Suspend is one tap")
- **Does not amend:** wire formats, OM-Crypto-v1, AccountControlState, the DeviceVault wrapping order, or any suspension/revocation authority rule

## Context

The `reference_client_ui` scope (ADR-0172) implemented the optional passphrase factor that the DeviceVault design already allows ("an additional wrapping factor ... Argon2id with versioned calibrated parameters"). The design text fixes the intent but not the construction, so the construction needed a record. Separately, the Devices screen was built with a confirmation step on Suspend, while the spec says Suspend is "one tap".

## Decision

1. **Passphrase layer construction (web client, KDF version 1).**
   - Optional, user-selected, and layered over method 1 (WebAuthn PRF) or method 2 (non-extractable key). It is never the sole wrapper; the record parser and the types both refuse that shape.
   - Argon2id v0x13 (RFC 9106) from the audited `@noble/hashes`, 32-byte output, 16-byte random salt, passphrase normalised as Unicode NFKC and encoded UTF-8.
   - Parameters are versioned and stored in the wrapper header, which is part of the AEAD associated data of both layers, so cost or salt cannot be edited or downgraded. Default m = 64 MiB, t = 3, p = 1. Floor m = 19 MiB, t = 2 (records below it are `corrupt`); maximum m = 256 MiB, t = 16. A calibration request can only raise the cost.
   - Construction: the PVK is encrypted under a passphrase KEK (HKDF-SHA-256 over the Argon2id output, labelled `om:v1:vault-passphrase-kek`, AAD label `om:v1:pvk-wrap-passphrase`); that inner ciphertext is then encrypted under the method 1/2 KEK. A guess therefore needs the authenticator or device key first, and `rotateWrapping` keeps the inner layer unchanged without asking for the passphrase.
   - Setup requires at least 12 code points; unlock reports a wrong passphrase with its own reason code. Format 1 (no passphrase) records stay readable.
   - **Honest limits, user-visible:** a copy of the browser profile can be attacked offline at Argon2id cost; nothing is rate limited and no such claim may be made. A forgotten passphrase makes the local data unreadable (the recovery key restores the account). Pure-JS Argon2id on the main thread is slow on low-end phones.
2. **Suspend keeps one confirmation.** "Suspend is one tap" means Suspend is a single deliberate action reachable directly from the device's details, with no multi-step flow and no authorization from another device. It still uses the destructive-intent confirmation token required by UI-11 (`SuspendDevice` is in `DESTRUCTIVE_INTENT_KINDS`), because an accidental suspension of one of two controllers freezes the account (ADR-0168). Permanent revocation stays a distinct action with its own wording and confirmation.

## Consequences

- R-409 and R-410 are testable in the reference client: vault unit tests for the layer (including hand-built format 1 records, header tampering and floor rejection) and UI tests for the opt-in and unlock screens; Devices UI tests for the confirmation.
- A cross-vendor or human review of the nested wrap is a **pre-release** item, because under ADR-0167 the only review so far is single-vendor. Registered on V-006's pre-release criteria with the measured unlock time on low-end phones.
- Camera QR scanning, the PRF passkey path and the service worker could not be exercised in the in-app browser (no camera or authenticator); they are pre-release checks on the five first-release browsers (also on V-006).
- Changing, adding or removing a passphrase after setup is not built; it needs a new vault method and its own tests.

## Where did authority go?

Nowhere new. The passphrase is a local at-rest factor and has no protocol or account authority; Suspend remains an AccountControlState transition decided by Core.
