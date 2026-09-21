# OM Master Planning & Design Snapshot

Generated: **2026-09-22 07:34 PHT (UTC+08:00)**

This snapshot supersedes the 2026-09-21 21:32 PHT checkpoint, preserved under `history/20260921-2132/`. Earlier snapshots remain under `history/20260921-1008/`, `history/20260921-0801/`, and `history/20260818/`.

## Phase

Architecture consolidation remains implementation-ready at the component/prototype level. Wire/security-sensitive claims still require concrete cryptographic selections, canonical encodings, test vectors, adversarial tests and independent review.

## Major additions in this checkpoint

1. OM Protocol/Core is explicitly operatorless: no OM company or central runtime service is required.
2. Defined OM Protocol, OM Core, OM Project, deployments, providers and distributions as separate roles.
3. Added Creator-Disappearance Gate: existing independent users must continue communicating if the original project/author/provider disappears.
4. OM is PWA-first, not PWA-only; native Android/iOS shells remain planned.
5. Security/state logic should converge on one portable memory-safe OM Core shared through WASM/native FFI rather than independent protocol reimplementations.
6. PWA Web Push can originate from each recipient's own deployment; no universal OM push gateway is required.
7. Native push/signing/store obligations belong to a particular client distribution, not the protocol.
8. Cloudflare Deploy-to-Cloudflare/templates become a preferred operatorless provisioning path; OAuth provisioning remains optional and replaceable.
9. Central OM AbuseDesk is not required; core reporting becomes local Block + selective `EvidencePackage` export to user-chosen recipients.
10. App-store or managed-service distributions may add their own report/moderation services without making those universal protocol dependencies.
11. Added explicit legal/operational role boundaries and retained Philippine legal-classification questions as pre-public-launch review items.
12. Optional convenience providers/businesses may sell relay/storage/media/directory/support while remaining fully replaceable and non-privileged.
13. Added coherent reference client-family policy to avoid fragmented-client ambiguity while retaining third-party interoperability.

## Highest-priority unresolved gates

- AccountControlState formal transition spec and test vectors.
- CryptoProfile v1 concrete algorithms/library boundary and stable MLS implementation/profile.
- Canonical signed-object/event/control encodings and golden vectors.
- Portable OM Core language/FFI/WASM/key-provider boundary.
- PWA Web Push/VAPID ownership and notification flow for per-user deployments.
- PWA incoming-call/background behavior and native migration path.
- Relay wire protocol and multi-device sync/history bootstrap.
- Blob/backup manifest and restore/repair protocol.
- EvidencePackage cryptographic construction and deniability semantics.
- Concrete release/update toolchain and key ceremony.
- Philippine legal classification of project publisher vs user deployment vs app distributor/provider before broad public release.

See `CHECKPOINT_20260922_0734.md` and `machine/open_questions.jsonl`.
