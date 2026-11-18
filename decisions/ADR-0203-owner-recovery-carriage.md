# ADR-0203 — Owner recovery checkpoint and approval carriage

- **Status:** Accepted by delegation, 2026-11-18.
- **Class:** `additive_noncritical_extension` under ADR-0165. Independent review of the composition remains a production gate.
- **Scope:** Representation and carriage for ADR-0202 sections 1 and 3. The account-control rules, recovery policy, timer, veto and Home API remain as published.

## Design sources read

Read in full for this representation: OM and lonunjie agent instructions, [`AGENT_ARCHITECTURE_RULES.md`](../docs/process/AGENT_ARCHITECTURE_RULES.md), [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md), [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](../docs/process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md), [`ENGINEERING_HANDOFF.md`](../docs/process/ENGINEERING_HANDOFF.md), [`IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md`](../docs/process/IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md), [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md), [`recovery-sheet.md`](../spec/protocol/recovery-sheet.md), ADR-0192, ADR-0195 and ADR-0199 through ADR-0202. Read the shared OM completion topic and local recovery handoff. Read the relevant recovery passages of ADR-0190, the recovery notice CDDL and the existing registry entries. The wider mapped Core, provider, client and UX source lists were not read in full. The CDDL for the new objects is included below. Product implementation has its own design gate.

## Decision

### 1. Owner checkpoint and sheet version 2

`OwnerRecoveryCheckpoint` v1 is a deterministic OM-CBOR map with the AccountID, ACS generation and digest
(`OM-ACS-HASH-v1`, SHA-256 Digest). A trusted device captures it from the verified head as part of the deliberate
save action. The owner retains the complete sheet under the same handling rules as the recovery root.
Only that custody establishes its provenance. A checkpoint from a provider, record or archive cannot replace it.

A full sheet of version `0x02`, kind `0x01`, uses the ADR-0200 layout, with the canonical checkpoint bytes inserted
immediately after the hostname and before the four-byte whole-sheet check. The remaining bytes before that check
are exactly one checkpoint (82–90 bytes). The host length determines its start. The check uses the existing
ADR-0200 construction over the entire body, including the version and checkpoint. The same compact base32, line
checks and QR wrapper apply. The longest binary sheet is 416 bytes. A reader rejects an unsupported version or kind,
missing checkpoint, checkpoint extensions, extra bytes, bad check, or malformed checkpoint without using any field. Version 1 full and
place-only sheets and the legacy key remain readable under their original rules. A place-only update changes the
lookup location only. A client requires version 2 support to use a version 2 sheet.

The pin fixes a minimum known head. Verify that the supplied signed state has the pinned AccountID, generation and
digest, and verify each later transition and its applicable recovery dossier using the account verifier. The
checkpoint contains no policy, credentials, time or vote. A RecoveryRecord with a tail starting after the pin
requires the missing signed states, credential bodies and applicable dossiers. An available revalidated carrier can
supply them as evidence. If no carrier has them, the device remains restricted. A known conflicting observation
requires the ordinary reconciliation rules even when the checkpoint matches one branch.

### 2. Offer and transcript

The controller opens the offer and displays the QR or private link. The link is `om:recover-approve/` followed by
canonical unpadded base64url of the ADR-0192 offer layout with version byte `0x01` for this purpose: four-byte
big-endian expiry in POSIX seconds, 16 random secret bytes, a one-byte request-capability length and that capability,
a one-byte response-capability length and that capability, then the exact UTF-8 Home URI. Each capability is 16–64
bytes, the URI and request capability must pass the ordinary relay-entry rules, and the base64url token is at most
200 characters. Each offer uses fresh queues and secret. Reuse the 300-second lifetime, 360-second link acceptance
window, single-use comparison confirmation and terminal cleanup of ADR-0192. The expiry is local to the exchange.

Use ADR-0192's direction keys, XChaCha20-Poly1305, random 24-byte nonce and D24 padding and type-23 wrapping, with
the following recovery-specific inputs. All occurrences of `CBOR` below are deterministic OM-CBOR.

| Alias | Domain label |
|---|---|
| KDF | `OM-RECOVERY-APPROVAL-KDF-v1` |
| AD | `OM-RECOVERY-APPROVAL-AEAD-AD-v1` |
| MAC | `OM-RECOVERY-APPROVAL-MAC-v1` |
| MAILBOX | `OM-RECOVERY-APPROVAL-MAILBOX-HASH-v1` |
| EVIDENCE | `OM-RECOVERY-APPROVAL-EVIDENCE-HASH-v1` |

- `id = DS-HASH(KDF, CBOR ["recovery_approval", 1, expiry_s, home_uri:bstr, A_cap, B_recv])`.
  Derive `K_req`, `K_resp`, `K_code`, `K_bind` by ADR-0192 HKDF with that salt and
  `CBOR [KDF, purpose:tstr]` as info. The polling id is the first 16 bytes of `DS-HASH(MAILBOX, B_recv)`.
- AEAD associated data is `CBOR [AD, id:bstr, role]`. Role 1 is restricted device to controller and role 2 the reverse.
- `MACIN(role, bytes) = CBOR [MAC, role, bytes:bstr]`. Use the ADR-0192 six-digit derivation with
  `HMAC(K_code, MACIN(1, proposal_bytes))`. The comparison covers the entire exact proposal.
- The reply tag is `HMAC(K_bind, MACIN(2, CBOR [id:bstr, SHA-256(proposal_bytes):bstr, descriptor:bstr]))`.
  The descriptor bytes are the canonical encoding of the entire reply descriptor. Compare tags in constant time.
  Its request digest is the existing `OM-RECOVERY-REQUEST-HASH-v1` digest of the RecoveryRequest body, and its
  parent, target and child hashes are the raw digest values from the existing ACS and credential constructions.

Enrollment and recovery differ in link dispatch, transcript, keys, associated data and plaintext type. A client
rejects an unsupported prefix or version. The exchange has no enrollment interpretation.

### 3. Proposal, reply and evidence

The first message is `RecoveryApprovalProposal`. It carries the exact signed RecoveryRequest and
RecoverySuccessorBundle. Each nested signed object is carried as its original bytes. The controller checks the
proposal against its reconciled current head and the applicable verifier rules before soliciting approval. It makes
a controller signature only after explicit matching-code confirmation and authorization under the existing
threshold. The reply binds the exact proposal, controller, request, parent, target and proposed child body.

Evidence in the response carries the signed states through the controller's verified head, the referenced credential
bodies, and any applicable dossiers, vetoes and co-signed successor. With an independently held pin, a receiver may
walk from the state that matches it. Without one, the ceremony supplies the controller's evidence from genesis
and the receiver checks each transition. Dossiers carry the committed policies and exact signed requests. They
carry no time. Any recovery-only link after the starting point still requires the recipient's own applicable
verification. A veto or newer state can be returned as evidence with the co-signed successor absent. The receiver
reconciles that evidence before any activation. Receipt of an authenticated reply or completion of a transfer is
insufficient by itself.

The reply announces a single manifest. Its evidence digest is `DS-HASH(EVIDENCE, evidence_bytes)`. Split the
canonical evidence bytes into consecutive 50,000-byte slices, with only the final slice shorter. The number of
chunks is the ceiling of the byte length over 50,000. Each `RecoveryApprovalChunk` includes that digest and a
zero-based index, and uses the reply direction. Accept chunks only for a pending authenticated manifest, at most
one chunk per index. An identical duplicate is a no-op, a conflicting duplicate aborts the ceremony. An early
chunk may be dropped. Arrival order can differ from index order. Assemble only when all indexes are present,
check the length, digest, canonical encoding and bounds, then run the full account verifier. A missing chunk,
expired exchange, stale parent or changed target cannot approve anything. Retrying the ceremony leaves the exact
signed recovery request and its verifier-local attempt rules intact.

Use the limits of ADR-0195 and ADR-0190: up to 65 signed states, 320 credential bodies, four dossiers and 16 vetoes.
A state item is at most 65,536 bytes, a credential body at most 32,768, a policy body at most 24,576, and a signed
request or veto at most 16,384. Maximum encoded sizes are 98,976 bytes for a proposal, 1,024 for a reply, 50,128 for a chunk and
3,200,000 for the complete evidence. There are at most 64 chunks. Truncating a required
chain, credential or dossier to fit leaves the transfer incomplete. No ceremony object contains the RecoveryRoot,
session or history keys, or verifier-local elapsed credit.

## CDDL

Uses [`common.cddl`](../spec/cddl/common.cddl), [`account.cddl`](../spec/cddl/account.cddl) and [`recovery-notice.cddl`](../spec/cddl/recovery-notice.cddl). Bounds on total size, exact byte lengths and sums,
and semantic checks described above also apply. Key 23, where accepted by the registry, follows common extension
rules and is included in the original bytes when covered by a hash or MAC.

```cddl
OwnerRecoveryCheckpoint = {
  0: 39, 1: 1, 2: account-id, 3: uint, 4: digest,
}
RecoveryApprovalProposal = {
  0: 40, 1: 1,
  2: recovery-approval-signed-request,
  3: recovery-successor-bundle,
  ? 23: extensions,
}
RecoveryApprovalReply = {
  0: 41, 1: 1, 2: recovery-approval-descriptor, 3: bstr .size 32,
  ? 23: extensions,
}
recovery-approval-descriptor = {
  0: bstr .size 32,                    ; RecoveryRequest digest value
  1: bstr .size 32,                    ; parent ACS hash
  2: bstr .size 32,                    ; target DeviceCredential hash
  3: device-id,                        ; controller
  4: bstr .size 32,                    ; proposed child ACS hash
  5: bstr .size 32,                    ; evidence digest
  6: 1..3200000,                       ; evidence length
  7: 1..64,                            ; chunk count
}
RecoveryApprovalChunk = {
  0: 42, 1: 1, 2: bstr .size 32, 3: 0..63,
  4: bstr .size (1..50000), ? 23: extensions,
}
RecoveryApprovalEvidence = {
  0: 43, 1: 1,
  2: [1*65 recovery-approval-signed-state],
  3: [1*320 recovery-approval-credential],
  4: [0*4 recovery-dossier-entry],
  5: [0*16 recovery-approval-signed-veto],
  ? 6: recovery-successor-bundle,
  ? 23: extensions,
}
recovery-approval-signed-request = bstr .cbor signed-RecoveryRequest
recovery-approval-signed-state = bstr .cbor signed-AccountControlState
recovery-approval-credential = bstr .cbor DeviceCredential
recovery-approval-signed-veto = bstr .cbor signed-RecoveryVeto
```

## Authority and evidence

The checkpoint anchors a known head. The ceremony authenticates an exchange with the owner's device. The committed
policy and account verifier govern every transition. Vectors cover wire shape, version and bounds using two codecs.
Runtime tests must independently cover crossed purpose, changed target, missing chunks, expiry, stale parent and
restarts before enabling the exchange. Related: D-219, R-441, T-185 and V-002.
