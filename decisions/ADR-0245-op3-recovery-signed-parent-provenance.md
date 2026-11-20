# ADR-0245 — Operation 3: a recovery-signed P is not verified by device credentials

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-20). The design is accepted as an additive specification. Code may not ship or be enabled until vectors `CC3D-01` to `CC3D-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registry rows it names are added in the same change, and a specialist signs off. Qualifies ADR-0236 and ADR-0240 (additive pointers there). The optional private proposal key 11 is not registered until approved with the private CDDL; no W0 authority change.
- **Class:** Clarification and conditional private evidence extension to gated ADR-0236/0240. No W0 authority change.
- **Answers:** operation 3 when signed P contains a recovery-role AccountID signature.
- **Priority:** Critical.

## Ruling

A signed P's `role=recovery, signer=AccountID` signature is verified **only** by the recovery-method public key identified by the independently verified **Q-committed RecoveryPolicy**. It is not a DeviceCredential; key 10's 18-credential two-era union applies only to device signers and op-3 parties, and MUST NOT contain a fabricated AccountID “device”. P's recovery-role signature may be accompanied by controller signatures; those controller signatures are separately verified against Q's credential commitments. For verification of Q→P, apply the historical recovery verifier, including exact signed RecoveryRequest, target credential, next policy, request-veto identity, applicable parent authorization, and verifier-local delay/veto evidence. A root-authenticated RecoveryRecord dossier **is not a substitute** for an independently authenticated recovery acceptance or local elapsed-time evidence.

**Branch decision:** a controller with a previously independently accepted, durably pinned P (including an accepted recovery-signed P) takes **anchored-P mode** and checks op-3 P→C against P; it does **not** claim to reverify Q→P. A controller without a verified P may use predecessor-reverification only if it independently trusts Q and has all Q→P authority evidence, including *its own* legitimate historical observation or the pre-existing authorized immediate co-sign path. An offered Q, policy body, signed request, or AEAD dossier is merely proposed bytes. No Q-policy, missing original request-to-child identity, borrowed timer, unresolved veto, or fork → refuse before vote, not a shortcut through predecessor mode.

## Optional private evidence carrier (gated, no new authority)

If a predecessor re-verification mode requires carriage of missing **bytes**, extend the not-yet-emitted `op3-proposal-v2` with the following optional field, applicable iff P has a recovery-role AccountID signature. The recipient MUST bind Q to a prior independently verified anchor and compare the policies to their respective committed digest values. This field is never allowed to convey an authoritative elapsed-time decision.

```cddl
; Existing keys 0..10 of op3-proposal-v2 unchanged.
? 11: op3-parent-recovery-evidence,
op3-parent-recovery-evidence = {
  0: bstr .cbor signed-AccountControlState, ; exact signed predecessor Q
  1: bstr .cbor RecoveryPolicy,             ; Q-committed policy body
  2: bstr .cbor signed-RecoveryRequest,     ; candidate Q->P request
  3: bstr .cbor DeviceCredential,          ; request target credential
}
; P's next policy is already proposal key 9. Parent P itself is key 1.
; Existing 100,000-byte preflight bound applies to entire plaintext.
```

The optional field is a **transport of bytes only**. A previously verified local Q may supply Q/policy/request without key 11; an existing authenticated sibling `RecoveryLinkEvidence`/RecoveryRecord key-5/key-6 route may supply missing bytes under its own bounds, never a remote timing fact. Do not enlarge the preflight limit or reclassify a valid request merely because its other fields match the child; the exact nonce binding remains unresolved under ADR-0243.

## Labels, vectors and compatibility

No new cryptographic label. Use the existing `OM-ACS-SIG-v1`, `OM-RECOVERY-POLICY-HASH-v1`, `OM-RECOVERY-REQUEST-SIG-v1`, `OM-RECOVERY-REQUEST-HASH-v1` and op-3 proof labels. Key 11 is reserved for this unshipped op-3 private version **only after registration and new goldens**; if any op-3-v2 plaintext was emitted, allocate `op3-proposal-v3` instead.

- `CC3D-01`: accepted recovery-signed P used as an exact durable parent anchor: P→C passes, no assertion that Q→P was freshly reverified.
- `CC3D-02`: independently trusted Q, actual Q policy/method, signed request and target credential; parent recovery-role signature verifies using Q recovery key, any co-signer using Q device reference, never P keys.
- `CC3D-03`: fake AccountID DeviceCredential, wrong Q method, mismatched Q policy, wrong epoch/target/nonce, changed request dossier, absent independent Q, borrowed wait, known veto and fork all refuse, no op-3 W0 vote.
- `CC3D-04`: preflight too large even with optional key 11; fail before vote; exact P-only mode unchanged and existing CC3A links stable.

## Audited-text and metadata note

No audited W0 changes authorized. The optional field changes a gated private carrier and requires updated private CDDL/registry, ADR-0236/0240 pointers, independent hybrid-signature vectors and security review. Q, old recovery policies and signed requests are disclosed only to an authenticated controller, never to Home or logs; the exposure is greater than anchored-P mode and must be included in metadata review. This does not approve op-3 enablement.
