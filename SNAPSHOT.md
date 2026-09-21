# OM Master Planning & Design Snapshot

Generated: **2026-09-21 21:32 PHT (UTC+08:00)**

This snapshot supersedes the 2026-09-21 10:08 PHT checkpoint, preserved under `history/20260921-1008/`. Earlier snapshots remain under `history/20260921-0801/` and `history/20260818/`.

## Phase

Architecture consolidation is mature enough for implementation prototypes. Wire/security-sensitive claims still require concrete cryptographic selections, canonical encodings, test vectors, adversarial tests and independent review.

## Major additions in this checkpoint

1. Recovery is pluggable and off by default; disposable identities are first-class.
2. Identity recovery, history recovery and provider recovery are distinct authorities.
3. Added explicit `AccountControlState` monotonic authenticated control chain.
4. Added two-stage quarantine/permanent revocation and safety-freeze semantics.
5. Control-device promotion is constitutional, preventing attacker-manufactured quorums.
6. Added `AccountControlHead` gossip/witness direction for stale-state/fork detection.
7. Initial deployment premise is user-owned Cloudflare accounts with automated provisioning.
8. `workers.dev` is the zero-domain v1 personal discovery locator; FQDN owners/companies form natural grouped directories.
9. Reduced zero-payment Cloudflare core to Worker + SQLite Durable Object; R2/D1/Pages are optional.
10. Added subject-signed `ContactCapsule` Share Contact flow and optional non-transitive referral context.
11. Added software supply-chain/update trust architecture from first public release.
12. Added Standard/Hardened/Maximum Privacy policy presets with one common cryptographic floor.
13. Added opaque push/notification architecture and explicit iOS delivery tradeoff when APNs is disabled.
14. Formalized backup anti-rollback: history restores, authority does not.
15. Explicitly rejected uploaded/hashed contact-book discovery for v1; local Contacts permission only.
16. Added disappearing-message/backup lifecycle direction.
17. Added critical/non-critical protocol extension semantics before wire freeze.
18. Reserved endpoint-assurance and nation-state anonymity transports as later pluggable layers.

## Highest-priority unresolved gates

- AccountControlState formal transition spec and test vectors.
- CryptoProfile v1 concrete algorithms/library boundary and stable MLS implementation/profile.
- Canonical signed-object/event/control encodings and golden vectors.
- Relay wire protocol and multi-device sync/history bootstrap.
- Blob/backup manifest and restore/repair protocol.
- Report-evidence cryptographic construction.
- Concrete release/update toolchain and key ceremony.

See `CHECKPOINT_20260921_2132.md` and `machine/open_questions.jsonl`.
