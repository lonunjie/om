# ADR-0244 — Operation 3: limits of historical-credential checks under a durable P pin

- **Status:** Accepted by delegation, ENABLEMENT GATED (2026-11-20). The design is accepted as an additive specification. Code may not ship or be enabled until vectors `CC3C-01` to `CC3C-04` exist from two independent codecs (including real hybrid-signature goldens), the CDDL and registry rows it names are added in the same change, and a specialist signs off. Qualifies ADR-0240 and ADR-0236 (additive pointers there). No audited text or crypto floor changes; operation 3 stays gated and nothing was emitted.
- **Class:** Security-preserving clarification of gated ADR-0240/0236; no existing wire-byte change.
- **Answers:** CC3B-03, verified parent P only versus predecessor reverification.
- **Priority:** High.

## Ruling

Separate two verifier modes, chosen **before** processing supplied credential bodies:

1. **Anchored-P mode.** The selected controller has an independently, durably accepted, unforked **exact P signed envelope/state pin** (or an independently authenticated state anchor sufficient under existing rules). It compares the signed P, body hash and generation to the locally accepted P; checks current-fork/current-parent eligibility; verifies the new child, origin and selected proofs against **P's** credential commitments; and enforces canonical key-10 encoding, 18-body and 100,000-byte bounds, pairwise `(DeviceID, credential digest)` uniqueness and order. Historical bodies are inert supplementary bytes. Without independently checked Q references it **cannot** know which historical digest should be present or whether the historical set is complete, and MUST NOT describe a wrong-old-digest rejection or signer-reverification as having occurred. Its authorization vote is justified by the previously accepted P, not these bodies.
2. **Predecessor-reverification mode.** The selected controller has a previously independently verified Q or a contiguous authenticated path to Q from an earlier local pin. It checks P's full controller-signature set against Q's exact credential references, and verifies each historic credential body and key-10 exact two-era union, including old/current key swaps, omissions and same-DeviceID deduplication. A freely supplied Q or summary is not an anchor. On missing authoritative references, it may switch to anchored-P mode **only if P is already independently accepted**; otherwise refuse before voting.

A retained `P_validation_receipt` is optional local, integrity-protected metadata, not wire authority. If one exists, it MAY contain `(P hash, verified Q hash, sorted [(P controller signer DeviceID, Q credential digest)])` captured as part of an actual Q→P verification and bound to the local P acceptance. Merely recording these values from an op-3 preflight provides no authority. P-only controllers need **not** retain such a receipt to run anchored-P mode; requiring it retroactively would contradict ADR-0240's admissible durable-P anchor.

A malformed raw key-10 object (wrong shape, duplicate exact pair, unsorted order, >18, over-budget) fails in either mode. “Extra historical body”, “wrong old hash”, “missing old body” and “collapsed same DeviceID from two eras” are **semantically rejectable** only when authenticated historical references/receipt exist. In anchored-P mode these are not tested as proven historical negatives: implementations may reject unknown supplementary material by conservative admission policy, but cannot claim to have identified its correct historical digest.

## CDDL, labels and registration

```cddl
; No new field. Retain op3-proposal-v2 key 10:
10: [2*18 bstr .cbor DeviceCredential]
; Order/uniqueness: lexicographic (DeviceID bytes, credential digest bytes),
; unique pairs; exact two-era membership ONLY in predecessor-reverification mode.
```

No new label or object type. Correct ADR-0240 §“Required vectors” CC3B-03's scope by an additive pointer. Existing authenticated current-parent proofs are still mandatory. No log or Home exposure of receipts.

## Required vectors

- `CC3C-01`: P-only with no Q, no historical receipt: exact P accepted, valid P→C vote permitted, supplied historical bodies explicitly **not** promoted to verified; malicious/untrusted Q never authorizes.
- `CC3C-02`: predecessor mode Q with old digest H_old versus P's H_new, real hybrid signatures: wrong H_old, old/current swap, omitted H_old, same-DeviceID collapse, and extra credential all refuse before vote.
- `CC3C-03`: optional locally authenticated receipt gives the same historical rejection as independently verified Q; forged, transplanted, stale or post-preflight-created receipt fails; clean fallback only to independently accepted P.
- `CC3C-04`: both modes reject unsorted/duplicate exact pairs, malformed bodies, >18 distinct bodies, over-100,000 preflight, stale/forked P, wrong P-current credential, wrong child proof; CC3A-01 remains bit-identical.

## Change-control and privacy

No audited W0 text or frozen object semantics change; ADR-0236/0240 **gated prose and vector expectations require additive qualification**. Optional local receipt retention links a predecessor and rotated device key; keep it local, encrypted, expiry/erasure governed by authenticated-state retention and never export it as a sender-authority assertion. No release approval.
