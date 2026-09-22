# ADR-0097 — Portable archives restore data, not authority

- **Status:** Accepted
- **Date:** 2026-09-23

## Decision

OM defines a public `om-archive-v1` logical portability format that preserves exact native cryptographic objects where available and records provenance/fidelity for imported foreign history.

Ordinary archives exclude long-term private authority keys. Device/account authority is established through AccountControlState/recovery/enrollment, never by importing history. Foreign exports become ImportedHistoryRecords rather than fabricated historical OM ConversationEvents.
