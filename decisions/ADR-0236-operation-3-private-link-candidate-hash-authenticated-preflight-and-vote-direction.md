# ADR-0236 — Operation 3: candidate hash in the private link, authenticated origin preflight and vote direction

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-19). The design is accepted as the specification. Code may not ship or be enabled until vectors `CC3A-01` to `CC3A-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registries named below are updated in the same change, and a specialist signs off. It carries an explicit admission: the operation-3 private link newly exposes the 32-byte candidate hash to anyone who sees the link (section 1 and "Metadata and authority"); if the controlling threat model disallows that exposure, operation 3 stays blocked. Amends ADR-0231 (additive pointer there). Amended by ADR-0240 (additive note: the credential bodies of the preflight are the exact two-era union keyed by `(DeviceID, credential digest)`, the signatures of an accepted parent are checked against its predecessor's committed credential references, and a durably verified parent is an admissible anchor; the "sorted by DeviceID" and "no duplicate or unreferenced credential" wording of section 3 is read with that rule; no field or bound changes; this ADR is gated and nothing was emitted). Qualified by ADR-0244 and ADR-0245 (additive note: a controller with a durably accepted, unforked P uses anchored-P mode and does not claim historical signature verification of the Q-to-P step, a recovery-role signature of P is verified against Q's committed recovery method and not a DeviceCredential, and optional private proposal key 11 may carry missing evidence bytes; no existing field or bound changes; gated, nothing emitted).
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** New operation-3-only private carrier and authenticated preflight, not a redefinition of operations 0 or 1. Treat as an additive extension to the *gated and never-emitted* operation 3, subject to ADR-0165 review. If any old operation-3 bytes have shipped, this is a new incompatible op-3 subversion, not an in-place patch.
- **Answers:** the operation-3 questions 1 and 2; the pre-key candidate-hash circularity and reversal of the ADR-0216 authenticated message direction.
- **Supersedes:** ADR-0231 §2's short operation-3 link, §3's request-carried proposal, the reserved `replacement-request` CDDL, the assertion in its metadata section that the child hash stays only in the envelope, and `CC3-01`'s short link. **Preserves:** frozen W0 ACS signatures and authority, ADR-0216 operations 0/1, its `controller-binding` field order/purpose/KDF, reply kind 2 and comparison code, and ADR-0227 staging/adoption gates. No change is proposed to audited W0 text. The *private-link metadata exposure* changes and needs explicit review against the audited threat model.

## Design sources read and not read

Read: [`AGENTS.md`](../AGENTS.md); ADR-0230–0232, ADR-0216, ADR-0225–0227; ADR-0199, ADR-0202, ADR-0203, ADR-0205 (relevant parts); [`spec/cddl/controller-change.cddl`](../spec/cddl/controller-change.cddl), [`spec/cddl/account.cddl`](../spec/cddl/account.cddl), [`spec/cddl/common.cddl`](../spec/cddl/common.cddl); `crates/om-core/src/flows/controller_change.rs` and `crates/om-core/src/control.rs` (relevant functions); recovery-extension Python/Rust fixture sources; [`docs/process/AGENT_ARCHITECTURE_RULES.md`](../docs/process/AGENT_ARCHITECTURE_RULES.md) and relevant protocol-evolution text. Not read in full: all constitution/domain documents in the design gate, all ceremony/transport implementation, or all registry contents. The full design gate and independent testing are still owed before implementation.

## Ruling

### 1. The missing candidate hash is physically carried by the *private* operation-3 link

Use the operation-3-only form:

```
om:controller-change/3/<origin-device-id-32-lowercase-hex>/<existing-ADR-0203-offer-token>/<candidate-hash-64-lowercase-hex>
```

`candidate-hash` is the **digest value** of `OM-ACS-HASH-v1` on the canonical **unsigned child body**, exactly 32 bytes. The offer token is still at most 200 base64url characters and byte-identical to ADR-0216's version-1 offer; the *whole link* length limit is increased for operation 3 by exactly 65 characters (slash plus hex). A parser rejects any other number of segments, upper-case/non-hex hashes, an op-3 link missing the hash, or a trailing parameter or fragment. Operations 0/1 keep their original links and limits. No hash is added to a Home URL, relay identifier, HTTP header, relay deposit metadata or request fan-out.

The selected controller reads the hash from this personally transmitted QR/private link **before** deriving the existing `controller-binding`, ceremony ID, direction keys and comparison code. It uses its own locally verified unforked parent ACS, origin/selected DeviceIDs and **parent-committed selected-device credential hash** to reconstruct the binding. If that parent is missing, not current or different, do not derive an authorizing vote; end restricted. A hash *from the link* is an untrusted KDF input, **not** evidence that a candidate is valid.

This explicitly changes ADR-0231's metadata claim: the hash is visible to anyone who sees the private link. It may become a correlator if the child is later disclosed. It contains no root, code, vote or write capability and is not visible to the Home unless another actor leaks the link. Never auto-open it through third-party link-preview/analytics services; apply secret-link clipboard/QR handling and a 300-second lifetime. If this metadata exposure is disallowed by the controlling threat model, **operation 3 remains blocked**: a different approved authenticated pre-key transfer must be designed, not a candidate-hash-free KDF.

### 2. Origin sends an *authenticated preflight* before selected device's kind-1 request

The origin already has the fresh child and parent when it makes the offer. Using the link's unchanged derived `K_resp` and origin-to-selected `ROLE_RESP` AEAD, the origin first deposits a new **operation-3-only preflight kind 3** to the link's response queue using the origin's corresponding deposit capability; the selected controller reads that queue using `B_recv`. The preflight transports the exact signed parent, unsigned child, new **and parent** RecoveryPolicy bodies, and the exact parent-committed credential bodies needed for both controllers and for the parent signed state's signers. The selected device must already have an independently verified matching parent; the offered evidence may fill missing *bytes*, never select the trusted parent or override a pin. Confirm its credentials hash to the parent's committed references, parent and child hashes recompute, old/new policies hash to respective commitments, the generation and epoch change is the sole permitted recovery-policy transition, and every signer belongs to the parent.

The preflight has an **origin-device proof**, verified against the parent's committed origin credential. Its signature uses `OM-CONTROLLER-OP3-PREFLIGHT-SIG-v1`, **not** `OM-ACS-SIG-v1`; it is proof of the offered bytes, **not** an ACS vote. The origin does **not** issue its W0 vote before the person confirms the matching code. The preflight, its sealed bytes, nonce and exact hashes are persisted before first deposit; repeating uses identical sealed bytes and the same private mailbox. No reply/adoption follows from receiving preflight alone.

After verifying the preflight and the user's explicit code comparison, the selected controller sends **request kind 1, selected → origin, over the existing request mailbox (`A_cap`)**. It carries a partially signed W0 child with **only the selected controller's ACS vote**, plus a separate selected-device request proof. That proof binds the exact preflight plaintext SHA-256 and exact signed-child bytes under `OM-CONTROLLER-OP3-REQUEST-PROOF-SIG-v1`. The request is not an origin-to-selected transport for the unsigned proposal.

After verifying that request, current head, selected eligibility, absence of an open recovery request and the matching code on the origin screen, the origin produces **its own W0 `controller` signature entry over the exact child body**. It returns **reply kind 2, origin → selected**, with the fully signed W0 child (selected entry and origin entry) and the existing `controller-reply-binding` signed by the **origin's committed device key**, binding the exact kind-1 request hash and the reply-body hash. The kit-transfer slot is `null`. The selected device independently verifies the origin's W0 entry, distinct signer quorum, parent policy, full successor and reply proof before any adoption. The origin's W0 vote first appears in this signed reply; the old policy and credential bodies first appear in the authenticated preflight. There is no unauthenticated intermediate vote.

The existing two-direction AEAD is reused with a **fresh, never-reused 24-byte nonce** for *each* origin→selected preflight and final response. An application-level kind check prevents a preflight being accepted as a final reply. The origin must not adopt before the code-custody confirmation and valid quorum. Replayed or late preflights/requests/replies, a changed parent, a fork or a cancellation authorize nothing. If requisite evidence cannot fit the existing 100,000-byte ceremony plaintext bound, **refuse op 3 before voting**; a separately specified authenticated ADR-0195-style continuation is required to support such accounts. Never silently omit evidence or exceed the bound.

### 3. Exact CDDL delta (within `controller-change.cddl`; not W0)

```cddl
; Existing promotion-request, removal-request and controller-reply remain unchanged.
; Dispatch only after deriving/verifying the operation=3 binding.
controller-binding-op = 0 / 1 / 3
controller-request = promotion-request / removal-request / op3-vote-request
controller-op3-message = op3-preflight / op3-vote-request / controller-reply

; first authenticated origin -> selected application plaintext, sealed ROLE_RESP
op3-preflight = [3, bstr .cbor op3-proposal-v2, bstr .size 3373]

op3-proposal-v2 = {
  0: 2,                                        ; proposal version, not the link version
  1: bstr .cbor signed-AccountControlState,    ; signed parent (must match local pin)
  2: bstr .cbor AccountControlState,           ; exact unsigned candidate child
  3: bstr .cbor RecoveryPolicy,                ; new committed policy
  4: bstr .size 32,                           ; parent ACS hash value
  5: bstr .size 32,                           ; child ACS hash value = private-link suffix
  6: uint,                                    ; parent recovery epoch
  7: uint,                                    ; candidate recovery epoch
  8: [1*16 op3-controller-ref],                ; complete parent eligible-controller references
  9: bstr .cbor RecoveryPolicy,                ; parent committed policy body
  10: [2*18 bstr .cbor DeviceCredential],      ; canonical, sorted, exact signer/party credentials
}
op3-controller-ref = [bstr .size 16, bstr .size 32] ; DeviceID and credential digest

; selected -> origin, sealed ROLE_REQ, after code comparison
op3-vote-request = [
  1, bstr .size 3373,                         ; selected-device proof under OP3-REQUEST label
  bstr .cbor signed-AccountControlState,      ; one selected-controller W0 signature only
  null, bstr .size 32                         ; SHA-256 of exact preflight plaintext
]

; origin -> selected final reply, sealed ROLE_RESP:
; existing controller-reply = [bstr .cbor controller-reply-body, device-proof]
; existing controller-reply-body = [2, signed-control-state, null]
; For op3 signed-control-state is canonical signed-AccountControlState
; containing both selected and origin W0 votes over the same child body.
```

**Semantic constraints beyond CDDL.** Preflight proof input is `DS-SIG(OM-CONTROLLER-OP3-PREFLIGHT-SIG-v1, profile=1, role=1, signer=origin_DeviceID, body=OM-CBOR[ceremony_id:bstr32, SHA-256(op3-proposal-v2-bytes):bstr32])`. Selected request proof input is `DS-SIG(OM-CONTROLLER-OP3-REQUEST-PROOF-SIG-v1, profile=1, role=3, signer=selected_DeviceID, body=OM-CBOR[ceremony_id:bstr32, SHA-256(exact-preflight-plaintext):bstr32, SHA-256(signed-child-bytes):bstr32])`. All nested CBOR is canonical; reject duplicates/extra fields. `op3-controller-ref` is sorted by DeviceID, not credential hash; the credential bodies are sorted by DeviceID and are the union of signers of the parent signed state plus the origin and selected, with no duplicate or unreferenced credential. The selected must verify its own pinned parent and all credentials needed to verify a parent state. If a parent's required signing set exceeds this form, refuse pending a separately approved continuation.

### 4. Domain labels and vectors

Register exactly two **new** domain separation labels as `signature` constructions:

- `OM-CONTROLLER-OP3-PREFLIGHT-SIG-v1` — origin proof on the two-element preflight binding body above.
- `OM-CONTROLLER-OP3-REQUEST-PROOF-SIG-v1` — selected proof on the three-element request binding body above.

Both use the existing `DS-SIG(label, crypto_profile, role, signer, body)` construction and existing hybrid signature profile; neither is an ACS vote. **No new KDF, AEAD, reply, ACS or comparison label.** The existing `om.controller_change.v1` controller binding still contains the exact candidate hash; the six-digit comparison code's existing context remains byte-identical after the selected reads the suffix.

Required independent Python/Rust goldens:

- `CC3A-01`: amend the `CC3-01` link by appending `/` + 64 lowercase hex digits for the fixed `64*32` candidate hash. Its existing binding, ceremony ID, keys, proof and comparison code must remain **bit-identical** (`411048` in the committed fixture). The old op3 short link must fail; old operation-0/1 links/vectors remain byte-identical.
- `CC3A-02`: with a real fixed W0 parent, both policy bodies, child, parent-signer credentials and fixed signature RNG, publish complete hex of canonical preflight CBOR, origin DS-SIG input and signature, selected request CBOR and signature, both W0 signature entries, reply CBOR and origin reply proof. The selected W0 signature must not exist before the preflight is verified and the code compared; the origin W0 signature must not exist until after request validation and origin confirmation.
- `CC3A-03`: mutate QR suffix, child, either policy, a signer credential, preflight digest, either W0 entry, both proof signatures, request/reply hashes, operation digit, expiry and nonce. Each yields fail-closed and **zero ACS adoption**; test code mismatch and a stale parent separately.
- `CC3A-04`: preflight, request, reply delivered in duplicate/reordered fashion; restart at each durable boundary; origin/selected simultaneous cancellation; missing 100,000-byte fit; cross-operation replay and altered encrypted role. Neither side adopts a partially signed child. Verify the old code survives a noncommit.

## Metadata and authority

The origin and selected controller already know their own AccountID and controller roles. The recipient learns the candidate policy before consent. The private link newly exposes one 32-byte **candidate identifier** to anyone who reads it, and can connect the screenshot to a later published child. A Home carries encrypted, padded preflight/request/reply deposits and can still correlate queue timing, number and size buckets; it cannot see policy, hash or votes from conforming envelopes. Do not claim unlinkability against anyone holding the QR or against the Home's ordinary traffic observations. The new origin proof attests to a proposed byte string; only W0 signatures under the verified parent give authority. Any metadata claim that the candidate hash is *always encrypted* must be expressly retired before approval.

## Gate

Do not enable operation 3 on this text alone. Amend ADR-0231 and its vectors and the private carrier CDDL/registries together; obtain specialist sign-off on the new private-link identifier and proof composition; test two independent codecs and end-to-end loopbacks; satisfy all four ADR-0216 gates and ADR-0227 confirmation rules. No release or security claim is made.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision.** Preflight kind `3` in the controller-change ceremony: [`controller-change.cddl`](../spec/cddl/controller-change.cddl) and ADR-0216 register only request kind 1 and reply kind 2, so 3 is free. The proposal-version `2` inside `op3-proposal-v2` is a field value of this ADR's own map, not a link version. The two labels `OM-CONTROLLER-OP3-PREFLIGHT-SIG-v1` and `OM-CONTROLLER-OP3-REQUEST-PROOF-SIG-v1` do not exist in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) or in any ADR; they are reserved here and registered when the gate opens. Vector ids `CC3A-01` to `CC3A-04` are unused. No existing number changes.
- **Relation to ADR-0231.** This ADR replaces ADR-0231 section 2 (short link), section 3 (request-carried proposal), its reserved `replacement-request` CDDL, its statement that the child hash stays only inside the envelope, and vector `CC3-01`'s short link; vectors `CC3-02` to `CC3-04` stay as intent and are re-expressed as `CC3A-02` to `CC3A-04`. Both ADRs are gated and nothing is emitted, so no wire compatibility is affected. ADR-0231's decisions on the immutable child, the pre-vote screen, vote acceptance and commit order stand.
- **Private-link metadata exposure kept.** The text stating that the candidate hash is visible to anyone holding the private link is kept as written; it is not softened. The operator's standing direction does not allow a metadata floor to be weakened, so the specialist sign-off in the gate must expressly accept or reject this exposure.
- **Link length.** The operation-3 link limit is 65 characters longer than ADR-0216's; operations 0 and 1 keep their links and limits. This is additive for a gated operation.
- **Edits.** Review-numbering references in the header were reworded to neutral wording; the status, decided-by and amends lines were rewritten; this section was added. No ruling text was changed.
