# v004-group-wire

Feasibility prototype for V-004 criteria 6 and 7 (the W1 `groups` wire freeze and the `GroupHistoryBundle` freeze). Not product code. Not for production.
The frozen wire is [`spec/cddl/group.cddl`](../../../spec/cddl/group.cddl) plus the registries; the decision is [ADR-0176](../../../decisions/ADR-0176-v004-groups-wire-freeze.md).
Gate status is recorded in the ledger, not here.

## What is checked

| Step | Evidence |
|---|---|
| Registry changes are additive | [`additive.py`](additive.py) compares `om-numeric-fields`, `om-object-types`, `om-domain-separation` and `om-room-capabilities` with the pre-freeze commit `31e35af`; it also checks that [`group.cddl`](../../../spec/cddl/group.cddl)'s copy of `SpaceDelegatedIntent` still equals the Space rule |
| W1 event-body overlay | [`overlay.py`](overlay.py) merges [`om-w1-event-bodies.json`](../../../spec/registries/om-w1-event-bodies.json) (kinds 6-14) into a copy of the registries; both codecs read it through `OM_REG`. W0 decoders ignore the overlay and keep those bodies opaque |
| Object vectors | [`gen_group_vectors.py`](gen_group_vectors.py) builds the four signed objects (positive: every operation, every binding kind, genesis/typical/minimal states, bundles) and the typed pin/poll/redact/topic bodies, plus single-fault negatives (generic CBOR faults per type, set order, ranges, reserved keys, dispatch mismatch, size limits). Positive bytes come from the unmodified [`verification/cbor/python_canonical.py`](../../cbor/python_canonical.py) encoder |
| Two strict codecs agree | the V-005 Rust codec and Node harness (registry driven, no per-type code) decode, validate and re-encode every vector; `gen/compare.py` checks verdict, exact error class, byte-identical re-encoding and the signature/hash inputs |
| Frozen W0 corpus unchanged | the committed 758-vector W0 corpus regenerates byte for byte and both codecs pass it with the default registries |
| Cross-field rules | [`gen_semantic.py`](gen_semantic.py) builds wire-valid objects that break exactly one ADR-0176 §3 rule; [`semantic.py`](semantic.py) and [`semantic.mjs`](semantic.mjs) (written separately from the rule list) must agree with the expectation and with each other |
| Bundle admission and isolation (criterion 7) | 19 scenarios: Room, recipient and incarnation binding, state and epoch, removed or re-added author, quarantined author device, signer, replay and nonce duplicates, structure, and live-state isolation (a claimed EventID never enters the live dedupe set, never suppresses a later live event, never touches the pin reducer; the label names the bundle author) |
| Two CDDL tools agree | the Rust `cddl` 0.10.7 and Ruby `cddl` 0.12.14 validate every CDDL-expressible vector, and Ruby-generated examples of each root are fed to the Rust codec in lenient mode |

```sh
verification/prototypes/v004-group-wire/run.sh            # about 15 minutes, almost all of it the CDDL tools
verification/prototypes/v004-group-wire/run.sh --no-cddl  # about 1 minute
verification/prototypes/v004-group-wire/run.sh --update-vectors   # accept regenerated committed vectors
```

Committed vectors: [`v004-group-wire-object-vectors.json.gz`](../../vectors/v004-group-wire-object-vectors.json.gz) and
[`v004-group-wire-semantic.json.gz`](../../vectors/v004-group-wire-semantic.json.gz). Results: [`v004-group-wire-latest.json`](../../results/v004-group-wire-latest.json).
Needs what `../v005-encoding/run.sh` needs (python3, node, rustup, ruby and the two CDDL tools under `../v005-encoding/.tools`).

## Design findings (ADR-0176)

- **F-27** The W0 `SignedEnvelope.body` limit (64 KiB) bounds every signed W1 object; the draft caps (2048 members, 4096 items) were unreachable. Membership is capped at 512 and a bundle at 1024 items.
- **F-28** The draft `transition_binding_hash` in the state is circular and was dropped.
- **F-29** No text said which intents are reductions; ADR-0176 §3 classifies them (and `coordinator.replace` is one).
- **F-30** The room-operation registry had no `invite_link.redeem` or `admission.set`; appended.

## Limits

One agent family wrote the generator, both codecs' registries, both semantic checkers and the scenarios. The two codecs and the two checkers share no code, which is what ADR-0134
section 5 calls an independent implementation; this is not independent review. Signatures are the V-005 insecure test profile, so no signature, MLS or network behaviour is
exercised; the bundle admission is a model of the receiver (Room view, live dedupe set, one pin reducer), not an implementation. The Rust `cddl` crate cannot match named or
constrained map keys, so (as in V-005) it validates a copy with the extension key inlined.
