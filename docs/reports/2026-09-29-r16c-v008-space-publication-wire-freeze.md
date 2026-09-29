# R16C — V-008 Space/Publication wire freeze

## Result

R16C closes the last repo-local pre-engineering criterion of **V-008**. The gate is now **6/6 repo-local criteria**, but remains `pre_engineering_status=open` because the required independent-review/phase evidence is still outstanding.

## Frozen wire surface

- `SpaceControlState` — object type 24; SignedEnvelope; `OM-SPACE-CONTROL-SIG-v1`; state hash `OM-SPACE-CONTROL-HASH-v1`.
- `SpaceDelegatedIntent` — object type 25; SignedEnvelope; `OM-SPACE-DELEGATED-INTENT-SIG-v1`.
- `PublicationControlState` — object type 26; SignedEnvelope; `OM-PUBLICATION-CONTROL-SIG-v1`; state hash `OM-PUBLICATION-CONTROL-HASH-v1`.
- `PublicationEvent` — object type 27; SignedEnvelope; `OM-PUBLICATION-EVENT-SIG-v1`; signature role 7 (`publication_signing_key`); chain hash `OM-PUBLICATION-EVENT-HASH-v1`.
- Existing `ConversationEvent` kinds 7, 11, 12, 13 and 14 now have typed bodies for `moderation.redact`, `topic.create`, `topic.update`, `topic.status.set` and `message.topic.set`.

No W0 object identity changed. W0 object types 1–23 remain numerically stable.

## Codec evidence

The bounded fixture uses two independently written minimal codecs:

- Python: [`verification/prototypes/v008-space-wire/python_codec.py`](../../verification/prototypes/v008-space-wire/python_codec.py)
- Node: [`verification/prototypes/v008-space-wire/node_codec.mjs`](../../verification/prototypes/v008-space-wire/node_codec.mjs)

They agree on:

- **9/9 positive vectors**;
- **10/10 negative vectors**;
- canonical CBOR body bytes for every accepted vector;
- canonical SignedEnvelope bytes for signed objects;
- exact rejection/acceptance verdicts.

The fixture covers Space state, populated Space state, delegated reduction, all four newly typed ConversationEvent bodies, Publication control, Publication post, missing fields, wrong type/version, invalid topic status, empty authority, and Publication sequence-chain violations.

This is repo-local evidence only. It does not constitute independent review or an independent implementation criterion.

## Important protocol normalizations

The logical JSON schemas remain modeling views. The canonical bytes are the CDDL + registry definitions in this checkpoint. Authorization/signature evidence represented semantically in the logical schemas is carried by the existing SignedEnvelope signature set rather than duplicated body-level signature authorities.

Publication operational signing keys are subordinate to `PublicationControlState` and are explicitly typed as signature role 7. They are not AccountID identities.

## Deliberate non-scope

R16C does not freeze:

- `GroupControlTransition` / MLS group wire encoding (V-004);
- BridgePrincipal/foreign-provenance wire encoding (V-013);
- archive format or archive encryption profile (V-014);
- call signalling/media-key wire objects;
- W4 service/plugin representations.

## Validation

- V-008 wire prototype: **PASS — 19/19 vectors**.
- Registry uniqueness/reference checks: **PASS**.
- Architecture validator and repository checks are rerun after registry/file-registry/manifest refresh.
- Engineering remains unauthorized.
