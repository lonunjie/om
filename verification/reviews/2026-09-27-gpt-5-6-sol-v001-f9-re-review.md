# V-001 F-9 focused independent re-review

- **Reviewer:** OpenAI GPT-5.6 Sol
- **Date:** 2026-09-27
- **Base commit:** `fa8370d665eb9e3e6659040f332fb3305b8b2299` (from `OM_EXPORT.json`)
- **Reviewed drop:** `om-design-20260927-1644-PHT.zip`, SHA-256 `32fcabffe6034a61d0676d64a50944d609b5a38b394f404470b41f07fb3f5f25`
- **Scope:** only the V-001 independent re-review criterion reopened by finding F-9: ADR-0135 §7 / ADR-0136 rule 4 responder-side initiator binding. This is not a verdict on the other seven items in [`BRIEF-2026-09-26-core-kernel-re-review.md`](BRIEF-2026-09-26-core-kernel-re-review.md), and it does not substitute for the separate ADR-0140 or ADR-0142 Rust/native/wasm runtime criteria.

## F-9 re-review

**Verdict: accept.**

The remediation closes the authority gap identified by F-9.

1. **The relationship is a mandatory input.** `RelationshipContext` has no unbound/default form. The receive/decrypt APIs require it for inbound traffic, including established sessions.
2. **`IK_A` is bound before secret-key or session work.** `Device::decrypt` parses the sender key and any initial PQXDH header, rejects a sender/header mismatch, then calls `binding::verify_initiator` before session lookup, Double Ratchet decryption, `accept_initial`, PQXDH responder operations, one-time-key consumption, replay-ledger mutation, plaintext release, or session insertion.
3. **The binding is to the intended relationship, not merely to any trusted key.** `verify_initiator` requires the X25519 identity key to match exactly one committed DeviceCredential whose AccountID equals `relationship.peer_account_id`. Zero matches, multiple matches, and inactive/quarantined devices fail closed.
4. **The credential is anchored in the trusted ACS head.** `peer_devices` accepts only credential bodies committed by the trusted ACS body/hash. `trust_peer` rejects rollback to an older ACS generation and rebuilds the derived peer-device index from the stored trusted heads after reload.
5. **The requested negative surface is present.** The selftest source covers unknown identity keys, relationship-route mismatch, revoked devices, quarantined devices, stale credentials after rotation, an uncommitted substituted identity key, same-account ambiguity, same-key cross-relationship routing, established-session wrong-route handling, reload persistence of the route pin, and crossed initiation. Rejections are written to require unchanged device state where relevant. The deterministic `initiator_binding` corpus contains 11 positive/negative cases including unknown, route mismatch, revoked, quarantined, stale, substituted, ambiguous and untrusted-head cases; the independent Python verifier implements the same ACS/credential/account/active checks from the normative rule rather than calling the Rust code.
6. **The composed authentication argument in ADR-0135 §7 is acceptable.** OM is not claiming the original XEdDSA proof unchanged. It explicitly composes (a) trusted ACS commitment to a DeviceCredential carrying the X25519 identity key and composite signing key, (b) mandatory Ed25519 + ML-DSA-65 signatures over recipient prekey material, and (c) PQXDH key establishment, with both parties' identity keys bound at their respective acceptance boundaries. This resolves the asymmetry that F-9 identified.

### Execution note

This reviewer environment has Node and Python but no Rust/Cargo/Wasmtime and no outbound network. I therefore did **not** relabel any Rust/native/wasm execution as current evidence. The repository's ADR-0140 bounded checker and ADR-0142 sealed-delivery bounded checker both pass in this environment, as do [`tools/validate_architecture.py`](../../tools/validate_architecture.py) and `tools/om_docs.py check`, but the two separate V-001 runtime criteria still require a Rust-capable runner.

## Gate impact

The non-Anthropic **F-9 independent re-review criterion is satisfied by this review** once the executor integrates it into the evidence/ledger. No new F-10+ finding is raised in this focused scope.

V-001 must remain open until the executor also obtains current runtime evidence for:

- ADR-0140 AsyncKeyBundle freshness/persistence/provider-migration and one-time-prekey context binding; and
- ADR-0142 SealedDeliveryEnvelope HPKE route-bound info/AAD/padding/negative behavior on the required runtime path.
