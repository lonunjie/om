# ADR-0262 W4 services wire — local canonical-vector verifier

Run from any working directory:

```sh
python3 verification/prototypes/services-wire/run.py
```

Only the Python standard library is used (tested with Python 3.11+). The runner reads the **complete** updated registries at `spec/registries/` relative to this file; they are deliberately not forked into a second registry. `python_codec.py` independently implements the OM-CBOR-v1 deterministic binary encoder and decoder, registration-based numeric shape validation, and service-specific semantic rules. It does **not** import or reuse the V-008 Python implementation, CBOR packages or network services.

`vectors.json` contains **21 positive** and **80 negative** vectors, including every new object form plus W4 `ConversationEvent` representation key 13. `value` uses diagnostic JSON with integer keys as strings and byte values as `{"$bytes":"..."}`. `canonical_hex` is the exact canonical CBOR for the logical value, including a full `SignedEnvelope` when registered; `wire_hex` is the exact on-wire input. In intentionally noncanonical CBOR vectors, these differ. Signatures are deterministic **structural placeholders**, not cryptographic proof. Optional `signatures`, `prior_value`, `grant_value` and `receiver_wave` provide the required context. Negatives declare an exact `error_class`; the replay fails on any wrong class or accepted invalid case.

The replay verifies exact hex, shortest-form/length-first sorted map keys, nested bounds, absence/null, set order/uniqueness, signed role restrictions, service-control and grant/installation head-chain checks, and **W0 rejection** of key 13. It re-encodes accepted bytes and checks domain-label/type assignments. The core API is `encode(value)`, `decode(bytes)`, `validate_wire(name, bytes, prior=..., grant=..., receiver_wave=...)`, `object_digest(name, body)` and `sign_wire(name, body, ...)` for test fixtures.

## Important limits

This is **repo-local one-codec** evidence, not a two-independent-codec convergence result and **not independent review**. `validate_wire` checks signature CBOR form, role, identity and counts, but **does not cryptographically verify signature bytes or map signer devices to authenticated controller authority**. Production receivers must verify `DS-SIG`, control/Room/Space/Account governance proof, known validated heads, real endpoint credentials and MLS roster, represented grant scope/actions/expiry, user/account/counterparty notices and acceptance, and actual one-shot disclosure/receipt transport. A service-reading plaintext invocation remains subject to ADR-0124/0129 taint and egress policy and a separate trusted tool-capability broker. No prototype green result implies permission, crypto safety, complete W1 group body support or release readiness.

The V-011 two independently written codecs, independent reviewer, implementation model-vector/injection-corpus replay and ADR-0258 pre-release gate remain separate obligations.
