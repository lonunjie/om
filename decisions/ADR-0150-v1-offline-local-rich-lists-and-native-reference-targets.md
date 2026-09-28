# ADR-0150 — Promote offline-local delivery, rich lists and native reference targets into V1

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** Run 8A normative V1-scope promotion; executable/adversarial evidence deferred to Run 8B
- **Builds on:** ADR-0130 LAN privacy hardening, ADR-0148 V1 product-base promotion and ADR-0149 promoted-surface hardening
- **Amended by:** ADR-0151 clarifies registered-schema rejection versus schema-valid rich-list semantic fallback; ADR-0152 promotes desktop into the V1 reference family; ADR-0155 makes registered-schema failures ignore the enhancement and binds list annotations to the carrying event's text
- **Does not amend:** OM-Crypto-v1, AccountControlState authority, the frozen W0 ConversationEvent body grammar, MLS/group security, relay authority, provider trust semantics, or the rule that platform capabilities are discovered rather than assumed

## Context

After Run 7B, the remaining deferred product items were re-audited under a stricter rule: move something into V1 only when it adds meaningful daily-driver/operatorless value without creating a new protocol authority, new cryptographic primitive, or a second canonical message model.

Three items pass that bar:

1. The privacy-preserving LAN rendezvous design is already mature enough that keeping **actual offline-local messaging/file transfer** as merely a long-term goal would defer a property central to OM's operatorless/local-first philosophy. Calls already prefer LAN/direct routes. The same transport independence can apply to ordinary messages and encrypted blobs.
2. `RichTextV1` intentionally omitted list blocks from the frozen W0 body. ADR-0149 now provides a hardened registered non-critical extension mechanism with object/criticality/schema binding and safe fallback semantics. Bounded list presentation can therefore be standardized without reopening the W0 RichText grammar.
3. Android and iOS are already architectural peers of the PWA through shared `om-core` + `om-ui` + `PlatformAdapter`, and Android is already scheduled early to catch browser assumptions. Treating both native shells as merely future/post-V1 weakens that validation boundary. They should be explicit V1 reference-family targets while the PWA remains the first vertical slice and no simultaneous-launch promise is created.

The remaining deferred items—SPQR/Triple Ratchet, PQ MLS, live location, guaranteed cloud scheduling, stories/status, payments, mini-apps, massive private-broadcast crypto, erasure-coded storage, HQC diversity and similar work—remain deferred for substantive security/ecosystem/implementation reasons.

## Decisions

### 1. Offline-local message and file delivery is an optional V1 capability

V1 clients SHOULD support direct same-LAN delivery for already-related peers and short-lived QR/invite-linked peers when the platform can establish a local route. Automatic discovery is **not** a universal V1 requirement.

The route-selection rule is:

```text
relationship / temporary invite capability
        + privacy policy
        + runtime LocalDiscovery / known-endpoint capability
                         ↓
                 candidate local route
                         ↓
             ordinary OM encrypted bytes
```

Normative boundaries:

1. **Transport, not authority.** A LAN endpoint/token/locator is only an ephemeral transport candidate. It cannot create a RelationshipBinding, make a device eligible, lower a security floor, change AccountControlState, or become a canonical identity/routing address.
2. **Same canonical semantics.** Local delivery carries the same logical ConversationEvent/EventID, ordinary one-to-one/group cryptographic state and authenticated authorization semantics that would be used over WAN/provider routes. Route-specific transport wrapping may differ, but route selection cannot change message meaning, sender authentication or the security level.
3. **No plaintext fallback.** Failure to establish a local route causes queue/fallback to another allowed encrypted route; it never falls back to plaintext/local unauthenticated messaging.
4. **Deduplication across route changes.** A logical message delivered over LAN and later retried/reconciled through WAN/provider routes materializes once by authenticated EventID/session dedupe. A transport ACK is not a second event or authority record.
5. **Existing-contact discovery remains opt-in.** ADR-0130/R-270/R-271 still apply: relationship-derived rotating tags are per-contact opt-in, disabled by default under Hardened/Maximum Privacy, fixed-cardinality/querier-private as applicable, and retired immediately on block/removal/relationship end.
6. **Temporary invite rendezvous is narrow.** QR/invite local capabilities are short-lived, scoped to that introduction, and do not expose stable AccountID/DeviceID/usernames/device names.
7. **Platform capability boundary.** Native clients may use permission-gated mDNS/DNS-SD/local-network APIs. A PWA may use manual/QR/known-endpoint/WebRTC-style rendezvous or report LocalDiscovery unavailable. V1 promises interoperable local-mode semantics where a route exists, not identical automatic discovery on every platform.
8. **Resource bounds apply before expensive work.** Discovery answers and local connections are rate/budget constrained; an unauthenticated LAN peer cannot force unbounded crypto, blob allocation or connection state.

#### Direct encrypted file transfer

A locally transferred attachment uses the same canonical random `BlobID`, `ciphertext_digest`, decryption material and immutable encrypted blob bytes referenced by the ConversationEvent. A LAN route is a transfer path, not a second attachment format.

- The receiving endpoint verifies digest/authentication before exposing plaintext.
- Direct receipt may populate the recipient's local encrypted blob cache immediately.
- When WAN/provider service returns, ordinary BlobLocationRecord/storage synchronization may mirror/backfill the **same encrypted object** without rewriting EventID, BlobID or attachment semantics.
- A client MUST distinguish **locally delivered** from **provider/storage durability satisfied** when the configured StoragePool policy has not yet been met. It must not imply that another linked device or an offline recipient can retrieve the file until normal replication/repair makes that true.
- Provider backfill failure does not invalidate a file already authenticated and received locally, but it can reduce later availability; the UI may surface pending/failed replication.

Calls keep their existing RoutePolicy behavior and are not redesigned by this ADR.

### 2. `om.rich-list.v1` standardizes bounded lists without changing RichTextV1

V1 registers a non-critical ConversationEvent extension **`om.rich-list.v1`**. It is presentation/structure metadata over existing W0-valid RichText paragraph blocks; it does not duplicate item text and does not add a W0 block type.

Canonical value:

```text
RichListV1Extension = {
  0: [1*256 RichListItemV1]
}

RichListItemV1 = {
  0: block_index uint 0..4095,
  1: depth uint 0..3,
  2: style uint 1..2,       # 1 unordered, 2 ordered
  ? 3: ordinal uint 1..9999 # REQUIRED iff style=ordered; forbidden iff unordered
}
```

Additional validity rules:

- the extension is meaningful only on `message.create` (indexes `content.blocks`) or `message.edit` (indexes `replacement.blocks`); on another ConversationEvent kind it is ignored as unusable non-critical presentation metadata;
- `block_index` entries are strictly increasing and unique;
- each referenced block exists in that event's applicable RichText value and is a W0 `paragraph` block;
- no more than 256 blocks are annotated and nesting depth is at most four levels (`0..3`);
- the first annotation and the first annotation after any gap in `block_index` have `depth=0`; within a contiguous run depth may increase by at most one between adjacent annotated blocks (it may decrease by any amount);
- an ordered item carries `ordinal`; an unordered item does not;
- list annotations grant no execution capability and contain no HTML/CSS/Markdown source.

#### Deterministic W0 fallback binding

Each annotated paragraph already carries its readable list marker in W0 RichText. Its first inline MUST be an unmarked `InlineText` whose text starts with the exact canonical prefix:

```text
"  " * depth + "- "                 # unordered
"  " * depth + decimal(ordinal) + ". " # ordered
```

The decimal ordinal has no leading zeroes.

A supporting client verifies every annotation against that exact fallback before rendering richer list indentation/numbering. If any annotated block is missing, not a paragraph, out of bounds, duplicated, structurally invalid, or has a mismatched fallback prefix, the client ignores the **entire non-critical `om.rich-list.v1` interpretation** and renders the ordinary W0 RichText unchanged.

A supporting renderer may suppress the verified marker prefix visually while drawing native bullets/numbers, but links, mentions, marks and item text after the prefix remain the same authenticated RichText semantics old clients receive. An unannotated block ends the current list run; a later annotated block starts a new run at depth 0. Plain-text copy/export/accessibility fallback is derived from the W0 RichText (including the canonical markers), not from a second list-only text source. This prevents old and new clients from authenticating different list text.

### 3. Android and iOS are explicit V1 reference-family targets

OM remains **PWA-first, not PWA-only**.

The V1 implementation sequence is now explicit:

1. build the first operatorless PWA vertical slice over portable `om-core` + shared `om-ui`;
2. start the Android reference shell early enough to expose browser assumptions and validate native lifecycle/storage/push/share/local-network boundaries;
3. bring the iOS reference shell into the same V1 reference-family cycle once the core and push/device lifecycle contract is stable enough to exercise Keychain/Secure Enclave/APNs/CallKit-class boundaries;
4. ~~keep desktop native shells~~ **desktop is promoted into the V1 reference family by ADR-0152**; carrier/SMS/RCS integrations remain separately sequenced.

This is a **V1 scope/architecture commitment, not a simultaneous-release gate**. A usable PWA may ship before native binaries are public. However, V1 architecture and shared APIs must not take shortcuts that knowingly make the documented Android/iOS shells a V2 redesign.

Native shells remain non-privileged PlatformAdapters:

- no native-only crypto/session/conversation semantics;
- no native-only identity or provider authority;
- the same conformance vectors and canonical Core state apply;
- platform differences are reported as capabilities and honest UX limits;
- automatic LAN discovery may be available natively while web falls back to manual/QR/known-endpoint paths.

### 4. Deferred boundary remains deliberate

This ADR does **not** pull the following into V1: SPQR/ML-KEM Braid Triple Ratchet, PQ MLS, live/continuous location, guaranteed provider-side scheduled sending, stories/status, payment/wallet authority, executable mini-app ecosystem, massive private-broadcast sender-key crypto, erasure-coded StoragePools, cryptographic group deniability, extra PQ algorithms merely for diversity, or nation-state anonymity/attestation systems.

They remain later work because their cost/risk is qualitatively different from the three promotions above.

## Run 8B verification obligations

Run 8A changes normative scope only. Before external review is reactivated, Run 8B must add/reconcile bounded evidence for:

1. **V-003 — route reconciliation:** LAN delivery followed by WAN retry/failover preserves the same logical EventID and materializes once; route changes cannot change peer eligibility/session security; direct-file receipt plus later provider backfill preserves BlobID/digest semantics.
2. **V-015 — LAN privacy/resource path:** opt-in relationship tags, removal/block retirement, fixed-cardinality/rotation, QR-temporary rendezvous, no stable identity broadcast, no route authority promotion and bounded unauthenticated work remain true when local delivery is actually used rather than merely advertised.
3. **V-005 — rich-list extension:** canonical registry/object/criticality/schema, positive/negative deterministic-CBOR vectors, strict bounds/unique ordering, ordered/unordered ordinal rules, paragraph-only references, exact prefix equivalence and safe whole-extension fallback; two independent codec/check paths must agree.
4. **V-021 — V1 native target/capability matrix:** this Run-8 obligation originally covered PWA/Android/iOS; ADR-0152 extends the current V1 reference-family obligation to Desktop as well. LocalDiscovery/background/push/key-storage capabilities differ honestly; no platform may introduce protocol/security meaning.
5. Re-run the full portable evidence/integrity suite and sweep current normative prose for stale “long-term LAN messaging”, “list blocks later-wave”, “future native Android/iOS”, or “future profile switcher” claims that contradict the current V1 boundary.

Until Run 8B supplies this evidence, the consolidated different-vendor review brief is paused and engineering remains unauthorized.

## Consequences

The V1 base gains more of the operatorless/local-first value already latent in the architecture while retaining the same authority model:

- local networking becomes a real optional delivery route rather than only a discovery/call aspiration;
- ordinary list UX becomes interoperable without destabilizing the frozen W0 RichText grammar; and
- PWA-first development is forced to remain honest about Android/iOS from the beginning.

The cost is a small amount of additional conformance surface. The benefit is that these concerns are frozen before implementation habits make them expensive to retrofit.
