# ADR-0266 — W4 plugins-wire canonical fixture and codec

Run from any directory, with Python 3 standard library only:

```sh
python3 verification/prototypes/plugins-wire/run.py
```

The runner uses the **independently authored**, self-contained `python_codec.py` (not the OM or bridges implementation) to encode/decode deterministic OM-CBOR-v1, perform schema validation using the **complete** existing-plus-W4 registries in `spec/registries/`, replay every exact byte fixture and check registries. This package contains **one** codec; a **second independently written** codec and independent review are still required by ADR-0259 / the November 23 build plan and remain **open**.

## Frozen top-level IDs

`PluginPackageManifest` 59, `PluginControlState` 60, `PluginCapabilityGrant` 61, `PluginHostRequest` 62, `PluginHostResponse` 63, `PluginCapabilityCall` 64, `PluginUiTreeUpdate` 65, `PluginEvent` 66 and `PluginError` 67. All have `wave: W4`. Every W0 receiver rejects them, including when their otherwise well-formed bytes are supplied as a typed top-level object. W4 stays flag-off. Existing bridge IDs 53–58 and service IDs 47–52 remain unchanged.

## Exact-byte vectors

`vectors.json` has **24 positive** and **116 negative** vectors, **140 total**. Each value has an `object`, diagnostic `value` (numeric map keys represented by decimal strings, byte strings by `{"$bytes":"hex"}`), `canonical_hex`, and **actual** `wire_hex`. For semantic negatives, the two hex strings match and the validator must fail on the requested semantics. For malformed CBOR and over-size objects, `wire_hex` is intentionally not the canonical value. Each negative has its exact required `error_class`; the runner refuses class substitution, not merely acceptance/rejection.

Stateful fixture attributes may include `prior_value` (previous authenticated control/grant head), `prior_release_value` (prior manifest), `prior_release_record_value` (authenticated prior release continuity-ceiling record), `control_value` (authenticated control), `manifest_value` (current authenticated manifest), `grants_values` (authenticated active device grants), `pending_call_value` (host-validated exact pending action), `now` (host time), `tainted` and `policy` (host-authenticated context), `channel_hex` (channel binding), `origin_bound` (exact-origin host binding independently verified), and `receiver_wave`. These are **assumed trusted external fixtures**, not assertions the plugin can author or prove. Any product implementation must verify their provenance and current state before invoking the validator.

Negative classes exercised: `unknown_field`, `missing_field`, `non_canonical`, `set_order`, `null_forbidden`, `max_bytes`, `wrong_role`, `value_range`, `capability_not_granted`, `capability_escalation`, `release_not_authorized`, `stale_chain`, `forked_chain`, `grant_expired`, `origin_channel_mismatch`, `tainted_egress_denied`, `bilateral_consent_required`, `user_confirmation_required`, `confirmation_mismatch`, and `w0_receiver_rejection`. Stateful checks include a publisher release with **more requested capabilities than the prior release** (valid, **not automatically active**), a malicious *automatic continuity ceiling* expansion (reject), a new-cap call without a **fresh grant approved for the new release** (reject), expired and revoked capability grants, control/release/grant rollback and same-version forks, key/release/grant tombstone resurrection, channel mismatches and missing per-send/1:1 authorization.

Signatures are **dummy byte strings** in valid existing `Signature` / `SignedEnvelope` structures. They let the validator check eligible **role numbers and signer IDs** and digest-chain references, but **are not valid cryptographic signatures**. `digest(name, body)` uses the preexisting DS-HASH construction; `confirmation_digest(call)` binds canonical exact argument bytes and destination with the registered `OM-PLUGIN-CONFIRMATION-HASH-v1` label. None of this substitutes for trusted host prompts, policy consent, MLS state or cryptographic signature verification.

## Example codec invocation

```sh
# run.py exercises the codec on every exact positive/negative hex string
python3 verification/prototypes/plugins-wire/run.py
```

For programmatic use:

```python
from python_codec import validate_wire, digest, confirmation_digest, CodecError
# manifest_wire: bytes, control: independently validated control state.
# validate_wire('PluginPackageManifest', manifest_wire, control=control)
# validate_wire('PluginCapabilityCall', call_wire, manifest=approved_manifest,
#               grants=trusted_grants, now=host_unix_seconds, policy=host_policy)
```

The code is intended as a public **wire/fixture prototype only**. It does not provide a browser host, Wasm sandbox, site separation, resource enforcement, the plugin update engine, external Room/1:1 consent or a production authorization oracle. The `PluginUiNode` vocabulary is intentionally minimal; a future host must prevent UI spoofing and protect all reserved chrome. An opaque argument/event payload is not self-authorizing: the individually named WIT operation and host broker must decode and validate its own capability-specific arguments before invoking any canonical OM API. Sensitive taint must persist through `storage.plugin_private`; parsing a host policy flag is not proof of real taint enforcement.

ADR-0259 engineering authorization and this design freeze do **not** authorize releasing or activating plugins. The release toggle remains off and V-010 including independent external sandbox review remains open.
