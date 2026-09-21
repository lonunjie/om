# ADR-0038 — Push is an opaque optional wakeup

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

APNs/FCM or alternate push only wake a device; relays retain message ciphertext and local clients decrypt/render notifications. Maximum-privacy users may disable vendor push with an explicit immediacy tradeoff.
