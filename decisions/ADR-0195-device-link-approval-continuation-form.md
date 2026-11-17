# ADR-0195 — Device-link approval: a continuation form for accounts whose chain and credentials exceed one message

- **Status:** Accepted
- **Date:** 2026-11-17
- **Decided by:** operator delegation 2026-11-17 ("make the product work for real accounts without friction"), to fill silence. This ADR fills silence only and amends no audited text.
- **Class:** `additive_noncritical_extension` ([`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) section 2, strongest applicable class; ADR-0165). The ADR-0192 approval, its binding input, its labels, its link version (2) and its message keys 0 to 11 are unchanged and still produced for every account whose approval fits one message. Two new approval keys (12, 13), one new ceremony message kind (7) and two new labels are added. Nothing a published construction authenticates changes meaning, so no link version bump is needed.
- **Amends:** nothing. **Does not amend:** [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md) (`DeviceHistorySnapshot` chunking is implementation-private; the same chunk mechanism is reused, not changed), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) (4.1, 12), ADR-0181, ADR-0192 (its `OM-DEVICE-LINK-CEREMONY-MAC-v1` entry and sections 1 to 7 stand as written), ADR-0193 (the owed-notice window split and its bound of 32 stand).

## Design sources read

Read in full: [`AGENTS.md`](../AGENTS.md) "Design gate"; [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md); [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md); [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md); ADR-0192; ADR-0193; the registry entries and rules of [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) for the ceremony labels; the approval, history-chunk and join code of `crates/om-core/src/flows/ceremony.rs` and all of `crypto/link_ceremony.rs`.

Read in part: ADR-0181 (headings and the sections that mention first contact limits and confirmation nonces; the Core-issued single-use confirmation nonce of section 8 is unchanged here).

Not read (stated, not claimed): the other documents and ADRs that `python3 tools/design_gate.py needs crates/om-core/src/flows/ceremony.rs crates/om-core/src/crypto/link_ceremony.rs spec/registries/om-domain-separation.json` prints for `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events` (about 30 documents and 49 ADRs), notably `CRYPTOGRAPHY_AND_AGILITY`, `ASYNC_KEY_MATERIAL`, `ONE_TO_ONE_SESSION_MANAGEMENT`, `TIME_AND_EXPIRY_SEMANTICS`, `MULTI_RELAY_AND_DELIVERY_RESILIENCE`, ADR-0126, ADR-0129, ADR-0143, ADR-0144, ADR-0154, and the V-021 and TLA+ models. If any of those fixes a construction for this ceremony, that text wins and this ADR must be corrected by a follow-up.

## Context

ADR-0192 sends the approval as one sealed message carrying the whole account chain (key 2) and every credential body (key 3). A message is at most 100,000 bytes. One credential body is 2,085 bytes and a signed state is about 3.4 kB plus 48 bytes per listed credential, so the approval stops fitting at about 47 credentials or about 25 transitions, well below the protocol's own limits (a chain of 65 states; up to 272 credential bodies for a chain a verifier can reach). Today such an approval is refused as too large and nothing changes, so a person whose account has a long history cannot add a device at all. The audited text fixes what the new device must verify before it adopts anything and is silent on how a set larger than one message travels.

## Decision

### 1. When the continuation form is used

The approving device builds the ADR-0192 approval exactly as before. If its plaintext is at most 98,976 bytes (`MAX_MSG` minus 1,024 of headroom) it is sent unchanged (sections 2 to 7 never apply). Otherwise it is sent in the continuation form. Which form is used is the sender's deterministic size rule; a receiver accepts either and verifies either completely. An account that fits is untouched, byte for byte.

### 2. Labels

| Label | Use | Construction |
|---|---|---|
| `OM-DEVICE-LINK-CEREMONY-MANIFEST-HASH-v1` | `hash` | `manifest_digest = DS-HASH(label, OM-CBOR [ [state, ...], [credential, ...] ])`: the chain states and then the credential bodies, each as bstr, in the order the approver holds them |
| `OM-DEVICE-LINK-CEREMONY-CONTINUATION-MAC-v1` | `mac` | `cont_binding = HMAC(K_bind, OM-CBOR [ label, OM-CBOR [ binding, digest, n_states, n_creds, n_chunks ] as bstr ])`, where `binding` is the ADR-0192 approval binding (key 11) |

`K_bind` is the ADR-0192 key; the label inside the MAC input separates this tag from every ADR-0192 MAC (whose input has a different shape and label). Both labels are `frozen` in the registry in the same change as the vectors; the ADR-0192 entries are not edited.

### 3. The approval in continuation form

The approval message is the ADR-0192 message with keys 2 and 3 removed and two keys added:

- key 12, `manifest`: `[digest: bstr(32), n_states: uint, n_creds: uint, n_chunks: uint]`;
- key 13, `cont_binding`: the tag of section 2.

Keys 0, 1, 4 to 11 are exactly as in ADR-0192, including the binding of key 11, which is computed over the same inputs (request digest, approver, successor state hash, notice digest, roster count, history count). The approval therefore stays one small message (a few kilobytes) and the new device can check the tag before it buffers anything.

### 4. Continuation chunks

After the approval the approver sends `n_chunks` ceremony messages, sealed exactly as every ADR-0192 response (same key, associated data, padding bucket and envelope): `{0: 7, 1: index: uint, 2: [state bstr, ...], 3: [credential bstr, ...]}`. The states come first, then the credentials, each in order, packed greedily to about 50,000 bytes of items per chunk (the same size the history snapshot uses), at least one item per chunk, at most 64 chunks. A set that would need more than 64 chunks is refused as too large with nothing changed, as an oversized approval is today. The chunks use the same ceremony queue as the history snapshot and are deposited after the approval; the queue's order is relied on in the same way (a chunk that is read before its approval is dropped and the set stays incomplete).

### 5. What the new device does

1. **On the approval.** It checks the shape and bounds of the manifest (2 to 65 states, at most 320 credentials, 1 to 64 chunks, no inline keys 2 or 3, key 13 present) and compares `cont_binding` in constant time with the value it recomputes from key 11 and the manifest. A failure ends the ceremony with nothing adopted. Otherwise it stores the approval and the manifest and waits; the join stays in its awaiting state and the device has no account.
2. **On a chunk.** Only while a manifest is pending. A chunk whose index is out of range or already held, or which is malformed or has an oversized item, changes nothing (a replayed or duplicated chunk is a no-op). Chunks may arrive in any order; the index, not the arrival order, places them. A chunk that does not open under the ceremony key and associated data is dropped without effect.
3. **When every index is present.** It assembles the lists in index order, checks the counts against the manifest and `manifest_digest` against the manifest's digest, and only then runs the unchanged ADR-0192 verification on the rebuilt approval: the chain, its own credential in the head, the approver, the signed notice, the key 11 binding in constant time. Any failure ends the ceremony as Failed, deletes the buffer and adopts nothing. Only a complete, verified approval adopts anything, in the one step ADR-0192 already defines.
4. **Expiry and restart.** The buffer is part of the stored join, so a restart resumes where it was. The ADR-0192 expiry applies unchanged: at or after expiry no message is accepted, the buffer is deleted and the ceremony is over. The buffer is also deleted at Linked, Rejected, Failed and Expired, and holds no secret (every item is public account state).

### 6. Bounds

At most 64 chunks, 65 states, 320 credentials, 65,536 bytes per item, and one pending manifest per join, so the stored buffer is bounded by the account's own protocol limits. The existing 64-credential cap of an inline approval is unchanged; the larger cap applies only to a set that was checked against its manifest.

### 7. Not changed

The roster, introductions, history snapshot, owed notices (ADR-0193: they already carry only what a receiver cannot hold, in windows that fit one message), the notice signer, the ADR-0181 confirmation nonce, the ADR-0192 expiry and the link format. The ADR-0193 bound of 32 owed notices still decides how many other active devices an approving account may have at once; the continuation form addresses an account with a long history of removed devices, not more active ones.

## Where did authority go?

Nowhere new. Authority to add the device is still exactly one parent-authorized `link_messaging` successor, verified by the new device from the signatures in the chain. The manifest and its tag are not authority: they let the new device know, before it buffers anything and again before it adopts, that the pieces it received are the ones the approving device meant for this request. The chunks carry public account state that the chain's own signatures already authenticate; the digest adds completeness and order.

## Consequences

Accounts of any size the protocol allows can link a device, in the form that suits them, with no new step for the person. A new device whose transfer is incomplete adopts nothing and the ceremony ends at expiry, as for any other failure; the person makes a new link. The approving side holds nothing extra (the chunks are sealed and deposited in the same transaction as the approval). Residual: the two new labels and the tag are OM's own composition over standard primitives (HKDF-SHA-256 keys, HMAC-SHA-256, SHA-256), in the same position as the ADR-0192 construction: [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md) section 2 asks for independent review of such a composition before any production claim, and this ADR does not claim it. An older build that does not know key 12 refuses a continuation approval and adopts nothing; before this ADR the same account could not link at all.

## Work list for a builder

1. Code: `crypto/link_ceremony.rs` (`manifest_digest`, `cont_binding`, the two labels); `flows/ceremony.rs` (the size rule, `cont_chunks`, the manifest approval, `on_manifest_approval`, `on_chain_chunk`, `verify_and_adopt`); `state.rs` (`LinkJoin.cont`).
2. Registry: the two labels, `frozen`, with the vectors.
3. Vectors: `verification/vectors/device-link-ceremony-vectors.json.gz` gains a `continuation` section (digest and tag over a fixed set; negatives for a reordered, dropped, moved or changed item and for each tag input), made by the independent Python generator and cross-checked by the Node checker, replayed by `tests/device_link_vectors.rs`.
4. Tests: a 65-state chain with 272 credentials links end to end; a missing, repeated, reordered (by arrival), index-swapped, altered or late chunk adopts nothing or links exactly once as specified; a wrong tag or out-of-bound manifest is refused; expiry; restart mid-transfer; an account that fits keeps the inline form.
5. UI: none. The new device already shows an awaiting state until the approval completes and the existing failure copy when it does not.

## UX line

Adding a device works the same for every account, however long its history.

## Traces

Requirement R-435, threat T-179, decision D-212. Related: ADR-0192, ADR-0193, relay status D125.
