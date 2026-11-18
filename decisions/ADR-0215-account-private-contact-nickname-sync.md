# ADR-0215 — Sync contact nicknames between eligible devices

- **Status:** Accepted by delegation
- **Date:** 2026-11-18
- **Scope:** `core_kernel`, implementation-private account sync
- **Basis:** ADR-0196 section 5 calls for a separate decision when nickname sync is available. ADR-0148 section 2 allows additional typed, non-authoritative preferences. ADR-0149 governs causality, eligibility and restore.

## Design sources read

Read in full: [Multi-Device Sync and History](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md), [ADR-0196](ADR-0196-local-contact-nickname.md) and the preference, restore and profile-context sections of [ADR-0148](ADR-0148-v1-product-base-promotion-without-kernel-expansion.md) and [ADR-0149](ADR-0149-promoted-v1-adversarial-hardening.md). The implementation handoff records the remaining design-gate reading.

## Decision

The nickname is a typed record on authenticated, sealed sibling-device sessions. Core writes its local display state and the causal preference mutation in one transaction. The account's background device-sync tick queues the mutation and retries it until the eligible sibling acknowledges it. Receipt is application-level and authenticated. Providers carry opaque ciphertext.

The record binds the authenticated account profile namespace and origin DeviceID, the origin's verified ACS generation, a durable sequence, the canonical ConversationID and contact AccountID, an optional nickname, and a bounded causal vector. The frame has a random 32-byte codec ID and major version. Unknown codec versions are ignored whole. The recipient verifies eligibility in both the stated generation and its current verified ACS and only merges into a matching bound contact. Acknowledgements refer to the same item and sequence under the sibling session. Revocation stops new delivery and discards queued traffic to the ineligible device.

The scalar preference uses causal order first. Among concurrent values, authenticated generation, logical time and DeviceID give a deterministic last writer. A clear is a tombstone. A candidate value must descend from each retained clear. Core retains the highest tombstone and latest mutation for each origin per key, including when a clear arrives out of order. Logical time has no wall component. Limits are 256 synchronized contact keys, 64 origins per key and eight outbound records per tick. At capacity a new local nickname continues to work on that device.

This treatment leaves the contact record, nickname validation, presentation and backup format of ADR-0196 in place. Pre-existing device-local nicknames and restored history stay local until edited on a current bound relationship. A new eligible device can receive the current records of their originating devices once the sibling path is ready. The enrollment roster carries the contact relationship separately from its preference records.

## Where did authority go?

ACS remains the authority for sibling eligibility. Nickname convergence only changes the owner's display state. Contacts, Rooms and provider APIs have no nickname field. No sync record approves an action or changes an identity binding, session authority, or security floor.

## Verification

Exercise two linked devices and a separate contact, write and clear, restart, revocation, concurrent edits, stale replay and causal recreation. Reuse the existing OMB-1 backup and contact-wire privacy checks.
