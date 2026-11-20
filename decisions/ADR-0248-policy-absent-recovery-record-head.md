# ADR-0248 — Policy-absent head in an explicit new RecoveryRecord lineage

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-20). The design is accepted as an additive specification. Code may not ship or be enabled until vectors `RK2E-01` to `RK2E-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registry rows it names are added in the same change, and a specialist signs off. Qualifies ADR-0242 (additive pointer there). The version-2 record lineage, its CDDL and its AEAD label are registered only in the same change; the version-1 record is unchanged.
- **Class:** Versioned RecoveryRecord plaintext/AEAD evolution; no redefinition of W0 `RecoveryPolicy` or v1 record bytes.
- **Answers:** RK2B-02, mandatory `acs_evidence[3]` while the signed head has no `recovery_policy_commitment`.
- **Priority:** High.

## Ruling

Never invent a `RecoveryPolicy` for a state that has **no** key-10 commitment. Recovery policy is optional in W0 ACS. The current `recovery-acs-evidence` key 3 is a mandatory `bstr .cbor RecoveryPolicy`, so it cannot faithfully encode a policy-free head. For a **new, explicitly versioned private record** retain key 3 but allow the value **CBOR null iff the signed head has no recovery commitment**. A present head commitment requires an exact canonical policy body and digest match, including when the active policy is off. Do not substitute `null` for an `off_explicit` policy: “off” is a valid signed policy body, not absence of a commitment.

```cddl
; legacy RecoveryRecord 0:36, 1:1 remains unchanged.
RecoveryRecord-v2 = {
  0: 36, 1: 2,
  2: 1..18446744073709551615, 3: uint,
  4: recovery-acs-head-ref, 5: recovery-acs-evidence-v2,
  6: recovery-provider-snapshot,
  ? 7: relationship-snapshot, ? 8: bstr .size (28..1048000),
  ? 22: bstr .size (0..1048000), ? 23: extensions,
}
recovery-acs-evidence-v2 = {
  0: bstr .cbor signed-AccountControlState,
  1: [0*8 bstr .cbor signed-AccountControlState],
  2: [0*320 bstr .cbor DeviceCredential],
  3: (bstr .cbor RecoveryPolicy) / null, ; required; exact iff head commits
  ? 4: [0*56 bstr .cbor signed-AccountControlState],
  ? 5: [0*64 recovery-record-link-dossier],
  ? 6: [0*65 recovery-record-policy-run],
}
; Optional key 6 carries exactly one run for each committed policy in the
; checkpoint->head window and no run for absence; retain prior runs across
; a removal, and represent the absence by the signed ACS commitment's absence.
RecoveryRecordEnvelope-v2 = {
  0: 35, 1: 2, 2: profile-id, 3: bstr .size 12,
  4: bstr .size (16400..1048016), ? 23: extensions,
}
; v2 ciphertext buckets/nonce/tag match the established envelope bounds;
; authentication is under OM-RECOVERY-RECORD-AEAD-AD-v2.
```

Use a **new authenticated plaintext/AEAD profile** with explicit record/envelope version dispatch before parsing and a distinct `OM-RECOVERY-RECORD-AEAD-AD-v2` label so legacy v1 plaintext is never silently reinterpreted. Proposed envelope version `0:35,1:2` retains the existing ciphertext/nonce and size limits; no new Home field/endpoint, locator or record-key derivation. The domain label must be registered before emission; if migration requires a different safe nonce or profile construction, resolve it with the crypto specialist instead of recycling v1 associated data as v2. Existing v1 records lacking a valid mandatory policy body remain invalid v1; a valid old record remains readable as v1.

For the v2 sheet-only walk, at every signed state check each policy run against the exact commitment from that state until changed or removed. A `null` final policy with a present head commitment, or a non-null final policy with no head commitment, is `integrity_failure`. Missing key 3, key 6 when needed, or unavailable predecessor policy evidence is `needs_evidence` if merely unavailable, and `integrity_failure` for a proven mismatch. Recovery-signed Q→P always requires an actually committed valid Q policy (not null); otherwise reject it as invalid recovery authorization. A current policy-free head does **not** authorize issuing/writing a new RecoveryRecord under an ineligible kit: writer ability and historical read-only classification are separate.

## Conformance and metadata

- `RK2E-01`: v2 checkpoint with policy P0, an ordinary policy removal, policy-free head: exact signed links, key-6 P0 run retained, key-3 null, all other predicates valid.
- `RK2E-02`: policy-free checkpoint and head with no policy-run entries; no invented key-3 body; compare with a valid `off_explicit` policy which **does** require the body.
- `RK2E-03`: absent key3, null-with-commitment, body-without-commitment, fabricated policy-run, missing historical body, nonce/AD version swap and v1/v2 cross-decoding fail appropriately.
- `RK2E-04`: 65-state/65-run, 1,048,000 plaintext caps, high-water floors, crash/restart and old v1 compatibility refusal; record writer cannot call a policy-free account recovery-ready.

A no-policy marker leaks no new Home-visible account information within the existing encrypted and padded record, though v2 issuance and bucket changes can be traffic-observed. **Audited W0 semantics and the original v1 CDDL are unchanged; new v2 CDDL/registry, AEAD label, profile/version negotiation and audit-owner review are required.** No release approval.
