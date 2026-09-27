# ADR-0141 — Recovery freshness, capability hygiene and frozen-spec consistency

- **Status:** Accepted
- **Date:** 2026-09-27
- **Scope:** architecture remediation only; no engineering authorization
- **Supersedes/amends:** the contradictory LegacyPolicy criticality encoding in ADR-0138-era machine specs; stale A-035 call-key text; provisional markers inside already-frozen W0 registries

## Context

The post-ADR-0140 semantic review found no need to change OM's authority model, but it found several places where the machine-readable specification did not faithfully express already-intended security boundaries. This ADR closes those gaps before implementation.

## Decisions

### 1. `om.acs.legacy_policy` is critical and fail-closed

`om.acs.legacy_policy` is a **critical** AccountControlState extension. A parser/verifier that does not implement its semantics MUST reject an ACS carrying it; it MUST NOT preserve-and-ignore the extension while continuing normal account processing. W0 still rejects `authorization_kind=legacy_policy` and LegacyPolicy activation remains later-wave/off by default. Criticality prevents a client that does not understand the committed irreversible-retirement semantics from silently accepting a state it cannot interpret.

### 2. Bearer capabilities have normative transport hygiene

Provider/resource capabilities are bearer secrets. A conforming transport MUST:

- carry them only inside an authenticated encrypted transport session;
- use a protected authorization field (for HTTP, the `Authorization` header) or the operation's frozen encrypted/authenticated CBOR authorization slot; never place bearer capability bytes in a URL path, query string, fragment or cookie;
- send `Referrer-Policy: no-referrer` on browser-facing capability-bearing surfaces and never propagate a capability through a cross-origin redirect;
- mark authenticated responses that can contain capability-derived/private resource data `Cache-Control: no-store` (and equivalent policy on non-HTTP transports);
- redact capability bytes from ordinary access/application/proxy logs, analytics, telemetry, crash reports and diagnostics. Debugging may log only a non-reversible, deployment-local fingerprint specifically designed not to be a bearer credential.

Opaque public resource locators such as a hash of a read capability are not bearer credentials, but remain metadata and receive ordinary privacy treatment.

### 3. RecoveryRecord v1 has authenticated freshness state

The decrypted logical RecoveryRecord v1 contains at least:

- `record_generation`: a monotonically increasing unsigned generation for that RecoveryStore scope/record lineage;
- `recovery_epoch`: the recovery epoch observed when the snapshot was written; and
- `last_known_acs_head`: the last authenticated AccountControlState generation and state hash known to the writer.

Clients durably remember the highest accepted RecoveryRecord generation per RecoveryStore scope and merge this high-water mark monotonically across private sync/backup. A lower generation is stale and rejected when a fresher observation survives. The same generation with different authenticated plaintext is fail-closed equivocation. A restored old backup never lowers the high-water mark.

The RecoveryRecord is **advisory recovery metadata, not authority**. Its ACS head is a remembered observation used to detect rollback/partition; it cannot authorize a newer ACS. Routes, provider capabilities, descriptors and other externally mutable data recovered from the snapshot MUST be revalidated before reuse. A record claiming a newer recovery epoch or ACS head is not trusted merely because its ciphertext authenticates; the normal AccountControlState/recovery chain remains authoritative.

The record encryption key is domain-separated from RecoveryRoot per RecoveryStore scope. The authenticated plaintext carries the epoch/generation; key derivation does not depend on untrusted record contents.

### 4. RecoveryRoot confidentiality limits are explicit; Maximum Privacy may split relationship custody

Recovery activation delay/veto protects **account authority**. It does not make RecoveryRoot compromise harmless for confidentiality: in the normal profile, a party with RecoveryRoot plus the RecoveryStore rendezvous can derive the record read/decryption material and learn provider-recovery metadata and the RelationshipSnapshot.

Maximum Privacy MAY use an independently generated `RelationshipRecoveryKey` that is neither derived from nor wrapped by RecoveryRoot. The RecoveryRecord then contains only a second-layer encrypted relationship snapshot; its key is kept by a separately selected custodian (for example another recovery key or guardian set). Losing that separate key loses convenience relationship restoration but does not affect AccountID authority. The normal profile remains one-custodian/domain-separated for usability.

### 5. Passkey privacy claims are narrowed, not usability-broken

A random WebAuthn `user.id` RecoveryHandle means OM does not directly encode AccountID in that field. It does **not** imply the credential manager cannot correlate the credential with the user's OM identity: the RP ID is visible, `user.name` remains the human recovery locator required by the current recovery UX, and the platform may have its own telemetry. Documentation and metadata descriptors MUST state this honestly. Maximum Privacy avoids platform passkey custody.

### 6. Already-frozen W0 registry semantics stop saying “proposed”

Every assigned W0 domain-separation label that was still marked `proposed` is reclassified `frozen`; no label value or semantic assignment changes. AccountControlState `device_credentials` key 13 is likewise described as frozen rather than a proposed addition. Reserved recovery-record KDF/AEAD labels are filled once under ADR-0119, and `OM-RECOVERY-RECORD-KEY-v1` is added for the outer encrypted record.

The `om-time-fields` default wall-clock tolerance is normatively frozen at **300 seconds** wherever a row says `skew_policy=default`. A row with an explicit override remains authoritative. This skew tolerance never supplies account/recovery authority.

### 7. A-035's resolved call isolation semantics replace stale text

Group-call media keys are per-sender SFrame keys delivered only to joined participant DeviceIDs and rotated on joined-participant-set generation changes, as resolved by ADR-0132/R-283. Room MLS epoch possession alone does not grant media-key access. Older ADR-0128/logical-schema text describing the MLS-exporter-only construction as still open is amended.

### 8. Design closure checks semantic agreement

`design_open=0` is insufficient if frozen registries/CDDL/current machine requirements contradict accepted ADR semantics. The architecture validator and a bounded ADR-0141 consistency checker therefore fail on at least:

- LegacyPolicy criticality drift;
- W0 domain labels reverting to `proposed`;
- the frozen 300-second default becoming provisional again;
- stale A-035 call isolation text;
- missing RecoveryRecord freshness fields; and
- removal of the bearer-capability hygiene rules.

## Consequences

This ADR changes no operatorless-core authority, creates no new central service, does not activate LegacyPolicy, and does not pull sealed sender, SPQR, PQ MLS, group deniability or later-wave product systems into v1. It closes specification/security-consistency defects and adds design-side verification evidence only. Existing V-* implementation/formal/independence obligations remain open unless separately satisfied.
