# ADR-0119 — Freeze the W0 CDDL and registries (V-005)

- **Status:** Accepted. `spec/cddl/` and `spec/registries/` are frozen.
- **Decided by:** operator, 2026-09-24 ("i lean to agree ... check it and accept it if good")
- **Date:** 2026-09-24
- **Source:** V-005 prototype ([`verification/prototypes/v005-encoding/`](../verification/prototypes/v005-encoding/README.md))

## Decision

Freeze the CDDL for the 18 W0 signed or hashed objects (`spec/cddl/*.cddl`), and the six registries (numeric fields,
object types, domain separation, crypto profiles, time fields, CBOR profile). After the freeze, assigned values are
immutable and never reused (`tools/continuity_bundle.py check-registries`, in CI). Entries that depend on open crypto
items A-023..A-028 (KDF/AEAD/MAC labels, per-prekey signatures, DeviceCredential key fields) stay **provisional**
inside the frozen registries until those items resolve. Mechanically, such an entry's value starts with `reserved`.
`check-registries` lets a `reserved…` value be filled in **once**; every other assigned value is immutable.

## Evidence

- Two CDDL tools (Ruby `cddl` 0.12.14 and Rust `cddl` 0.10.7) agree on 246 schema-level checks. A Rust-tool bug with
  bounded/named map keys is worked around only for that tool.
- Independent strict codecs in Rust and Node have 0 mismatches on 540 object vectors (36 positive, 504 negative),
  covering bytes, error classes and signature/hash inputs. Rust also matches the 14 earlier fixtures.
- Codec cost, median on an M3: decode + validate 1.3 µs native and 1.5 µs wasm32 (budget impact negligible, V-022).

## Rulings on contradictions found while writing the CDDL (accepted with this ADR)

1. **Map-key order.** The earlier Python and Node encoders used RFC 7049 length-first order. OM uses RFC 8949 §4.2.1
   bytewise order, as [`CANONICAL_ENCODING.md`](../docs/architecture/crypto/CANONICAL_ENCODING.md) says. The profile also requires every key in a map to have one type,
   so both orders coincide.
2. **Recovery default.** Recovery is on by default (ADR-0111). [`IDENTITY_PROTOCOL.md`](../docs/architecture/identity/IDENTITY_PROTOCOL.md) is corrected.
3. **`expires_at` type.** Always unsigned POSIX seconds. Four logical schemas that used strings are superseded by the CDDL.
4. **RecoveryPolicy `policy_commitment`.** Removed from the policy object, because it was circular. The commitment is
   the hash of the policy, stored in AccountControlState.
5. **ContactCapsule referral.** `referral_attestation` becomes a separate object signed by the referrer. It cannot sit
   inside the subject's signed body.
6. **BlobID.** A random identifier. Integrity comes from `ciphertext_digest`. [`REACHABILITY_AND_OBJECT_REPAIR.md`](../docs/architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md)'s
   "content identity" wording means the digest.
7. **AccountControlState device keys.** New key 13 `device_credentials` binds each device id to its public keys
   (related to A-024).
8. **Recovery labels.** The [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) labels are recorded verbatim alongside the `OM-*-v1` style;
   the naming is unified at freeze.

## Consequences

When accepted, V-005 pre-engineering criterion 1 is met. Criterion 2 needs ADR-0117 or a second author, because one
agent wrote both codecs.
