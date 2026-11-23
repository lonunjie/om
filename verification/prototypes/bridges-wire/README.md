# ADR-0264 — W4 bridges-wire canonical test corpus

Run, from **any** working directory:

```sh
python3 verification/prototypes/bridges-wire/run.py
```

The standard-library-only `python_codec.py` implements deterministic RFC 8949 OM-CBOR-v1 canonical encode/decode **without importing third-party CBOR libraries or other OM codec code**, then validates W4 bridge forms against the **complete** three registries at `spec/registries/` (located relative to this directory):

- `BridgePrincipal` object 53 (unsigned random BridgeID wrapper)
- `BridgeControlState` object 54 (`SignedEnvelope`, controller role)
- `BridgeMapping` object 55 (`SignedEnvelope`, controller role, external Room approval still required)
- `InteropTrace` object 56 (unsigned top-level wrapper over `InteropTraceBody`)
- `ForeignProvenance` object 57 (unsigned top-level wrapper over `ForeignProvenanceBody`)
- `ForeignIdentityLink` object 58 (`SignedEnvelope`, native Account control authority)
- Existing `ConversationEvent` type 10 plus **W4-only key 14** carrying `ForeignProvenanceBody` directly; **no new event kind**.

`ConversationEvent` key 13 is still the separate W4 services representation form. A W0 receiver **must reject** either later-wave key as an unknown critical field. Both keys together reject. For a bridged inbound event, the author must be `PrincipalRef` kind **3** (BridgePrincipal) with the active bridge endpoint as `author_endpoint_id`; foreign identities and links never rewrite native authorship. A bridge is always a **full, visibly rostered MLS member** of every private Room it continuously bridges; export filters do not narrow what it can decrypt. The private Room-scoped origin fingerprint key **never** travels in a public object.

## Vector format

`vectors.json` contains **27 positive and 98 negative** cases (125 total). Each `value` is a complete diagnostic JSON representation of a typed OM object: unsigned integer map keys appear as JSON decimal strings; binary values appear as `{"$bytes":"hex"}`. `canonical_hex` is the full deterministic CBOR encoding of that value, including `SignedEnvelope` for the three signed types. `wire_hex` is the actual wire input. They match for all positives and semantic negatives, but intentionally differ in raw noncanonical/oversized rejection cases. `error_class` is the **exact** required rejection class; the runner checks equality, not merely failure. `signatures` optionally supplies structural signatures; `prior_value`, `control_value`, `mapping_value`, `receiver_wave` and `now` supply independently trusted-state **fixtures** for contextual checks.

The error classes are `non_canonical`, `max_bytes`, `missing_field`, `null_forbidden`, `value_range`, `unknown_field`, `set_order`, `wrong_role`, `loop_detected`, `provenance_mismatch`, `stale_chain`, `forked_chain` and `w0_receiver_rejection`. Examples include duplicate/unsorted CBOR map keys and set members, forbidden CBOR major types, unknown critical extension, missing and null fields, wrong signature roles, wrong Room scope/direction or control head, duplicate adapter namespace, hop/path mismatch, exhausted hop limit, inconsistent key-14 fingerprint, cross-identity claims, W0 key-14 rejection and exact version-chain/endpoint-tombstone checks.

`run.py` re-encodes each accepted wire, requires canonical byte equivalence, runs every semantic reject, checks digest labels and uniqueness, verifies object IDs 53–58 after untouched services 47–52, checks typed key 14 carries `"wave": "W4"`, and checks that secret-keyed fingerprints cannot be correlated between different Room keys from the same foreign origin. The new HMAC uses an **existing** OM-used primitive (HMAC-SHA256) and domain label `OM-INTEROP-FP-MAC-v1`; no secret is in the registry, trace, provenance or event.

## Prototype API

```python
from python_codec import encode, decode, validate_wire, object_digest, origin_fingerprint, sign_wire
body = validate_wire('BridgePrincipal', bytes.fromhex('a30018350101025820' + '33'*32))
encoded = encode(body)
# For stateful cases: validate_wire('BridgeControlState', wire, prior=authenticated_prior_state)
# For imports: validate_wire('ConversationEvent', wire, control=trusted_control,
#                            mapping=trusted_mapping, receiver_wave='W4')
```

`origin_fingerprint(room_secret_32, origin_namespace, stable_foreign_origin_ref)` deliberately rejects invalid key sizes and uses a canonical length-delimited MAC input; the test key used in fixtures is **not** a real Room secret. Test signatures are **placeholder blobs** containing valid `Signature` structures, not real Ed25519 or other cryptographic attestations.

## Critical evidence limits

This is **one** Python codec, not the independently implemented second-codec agreement required by ADR-0165/V-013. The validator enforces shapes, signed-envelope role labels, and some semantic invariants but **cannot prove actual signature validity, eligible controller/account devices, Room-level approval/quorum, MLS roster/epoch/recipient state, foreign adapter identity proofs or genuine external-leg behavior**. Context passed to it is assumed already authenticated. Room key establishment/derivation/rotation is an external security requirement. A fingerprint MAC does not itself authenticate a foreign event or give a bridge permission to export. Origin metadata stripped by a foreign relay cannot be magically reconstructed; mark `origin_unverified`, preserve user-visible fidelity loss and deduplicate only with adequately strong provenance.

Real implementations also need durable echo and target mappings, safe foreign-side idempotency/reconciliation on unknown send result, the protected `room.add_bridge` governance transition, explicit visible OM E2EE termination, relay-off default, membership-interval export restriction and nonhuman quorum exclusion. ADR-0260 authorizes **engineering only**; the V-013 pre-release gate, independent review and second-codec comparison remain open. [`NOTES.md`](../../../crates/om-archive-reader/NOTES.md) is **AI design review**, not independent review or a production-release sign-off.
