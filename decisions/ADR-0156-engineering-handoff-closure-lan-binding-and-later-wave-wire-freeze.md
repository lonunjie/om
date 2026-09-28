# ADR-0156 — Engineering-handoff closure: LAN wire binding, later-wave wire freeze, scheduled send and sync namespacing

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** independent architecture review (Claude Opus 5.5) of drop `20260928-2200`, findings F-04–F-07; no W0 wire change and no engineering authorization
- **Amends:** ADR-0150 §1 (offline-local delivery), ADR-0148 §2 / ADR-0139 (account-private sync boundary), MESSENGER_BASELINE_V1 scheduled send, RELEASE_WAVES scope table
- **Opens:** design item A-055 (scope `local_delivery`)

## Context

Four places would force engineers to invent protocol or client semantics that two conforming implementations could resolve incompatibly.

- **F-05 (MAJOR): the LAN delivery wire binding does not exist.** ADR-0150 made offline-local message and file delivery a V1 capability and froze its invariants: transport not authority, same EventID, no plaintext fallback, durability honesty. But there is no LAN channel profile, framing, rendezvous-token KDF/label/epoch, or route descriptor, and the QR invite's `lan_rendezvous` field is an opaque `bstr`. Worse, the design never says whether a LAN route carries `SealedDeliveryEnvelope` or the bare inner 1:1 envelope. `SealedDeliveryEnvelope` binds to a relay route (`relay_uri` plus deposit capability). The bare inner envelope starts with the sender's `EncodeEC(IK)` (A-054) and would hand a stable device correlator to every observer on a public Wi-Fi network, contradicting LAN §7 and ADR-0142's no-clear-sender rule. LAN delivery also sits in no engineering scope.
- **F-06 (MAJOR): later-wave wire encodings are unfrozen and no gate requires them.** Only W0 objects have frozen CDDL (ADR-0139: logical JSON schemas are modeling views only). GROUP_COMMIT §7 says groups are not frozen until the `GroupControlTransition` encoding exists, yet V-004 has no such criterion. The same holds for W1–W4 event bodies (`pin.set`, `poll.*`, `topic.*`, `moderation.redact`, which W0 carries as opaque kinds 6–14), GroupControlState/Intent/CommitBinding, SpaceControlState and Publication, call signaling and the SFrame sender-key package, ServicePrincipal/RepresentationGrant (event key 13), BridgePrincipal/foreign provenance (event key 14), plugin package manifests, `custom_emoji` inline type 4 and `reaction_asset`. The package also leaves open how a group-call sender key reaches a joined co-member it has no 1:1 relationship with ("pairwise device channel … or an equivalent … HPKE channel"). A scope could be authorized with engineers inventing these encodings.
- **F-04 (MINOR): scheduled send is underspecified.** "If no authorized device is online, nothing is sent" does not say whether an overdue item is sent late or dropped, which clock and epoch it carries, or what happens when the device loses eligibility.
- **F-07 (MINOR): implementation-owned sync codecs can collide.** ADR-0139/0148 leave the account-private sync and HistoryBundle codec to implementations. Two different clients linked to one account could reuse a record type name with different encodings and misparse each other's records.

## Decisions

### 1. LAN delivery: architectural rules now, concrete binding under A-055 and scope `local_delivery`

These rules follow from ADR-0142, ADR-0130/R-271 and LAN §7, and bind every future binding:

1. **No clear-sender LAN path.** An OM-native 1:1 message on a LAN route carries `SealedDeliveryEnvelope` protection, or an equivalent reviewed construction that hides stable sender identity, the sender device key and the Double Ratchet header from on-path LAN observers. A LAN route is never a clear-sender fallback. Group messages carry ordinary MLS `PrivateMessage` bytes.
2. **No stable LAN-visible identifier.** Every identifier a passive LAN observer can see (connection id, delivery-key id, route token) rotates at least with the rendezvous epoch and never survives across epochs or link-layer address changes.
3. **No LAN-only relationship path.** First contact over a LAN without WAN uses the ordinary MessageRequest/IntroGate semantics and the frozen route-binding abstraction. A LAN endpoint can serve as a reply route, but it never creates a relationship, device eligibility or first-contact shortcut.
4. **Registered derivation.** The rendezvous-token KDF, its domain-separation label and its epoch length are registered in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) before implementation.

The concrete profile (local route descriptor and route binding for `SealedDeliveryKeyRecord`, channel security and framing including a browser-feasible variant, rendezvous derivation, offline serving of async key material for first contact, file chunking/backpressure, and the fixed-cardinality constant K) is design item **A-055**. It blocks the new W1 scope **`local_delivery`** (depends on `core_kernel`). ADR-0150's invariants and the route-independent dedupe/backfill semantics stay in `core_kernel`: W0 must materialize one EventID whatever route delivered it. The new scope ships only the LAN binding. This keeps offline-local delivery a V1 commitment while preventing incompatible LAN protocols. V-003 and V-015 list `local_delivery` in `unlock_scopes`.

### 2. Every scope must freeze its own wire objects before authorization

Each later-wave gate gains a pre-engineering criterion: every wire object, event body and extension the scope introduces has frozen CDDL, registered numeric fields and domain-separation labels, and positive/negative vectors on which two independently written codecs agree (the V-005 method), before the scope can be authorized. The criteria name the objects listed in F-06. For `calls`, the criterion also requires choosing and specialist-reviewing the key-delivery channel to joined participants without a 1:1 relationship. This is a cryptographic-construction choice that needs a reviewed design, not invention during engineering. `portability` (V-014) and the ContentPack part of `expressive_media` (V-012) already carry equivalent criteria.

### 3. Scheduled send semantics

1. The scheduled queue is **device-local** and profile-bound (it is not in the V1 account-private sync allowlist) and each item captures its `ProfileExecutionContext`.
2. At send time the item becomes an ordinary new `message.create`. Its EventClock, sending epoch, authorization and `expiry` are taken **at actual send time**, never backdated to compose or due time.
3. If the device was not running at the due time, the item is sent at the next opportunity after it, and the sender's UI marks it "sent late". Recipients see an ordinary message.
4. If the sending device or profile loses eligibility before sending (device revoked, profile removed, member left or removed from the room, relationship ended or blocked), the item is discarded with a local notice. It is never migrated to another device automatically.

### 4. Implementation-namespaced account-private sync and HistoryBundles

Every account-private sync record and HistoryBundle carries an implementation/codec identifier bound inside its authenticated E2EE framing. A receiving device that does not recognize the identifier ignores the record entirely and never interprets part of it. Cross-implementation sync of drafts, preferences and history is **not** a V1 interoperability promise. When another linked device cannot share them, the Devices screen SHOULD say so rather than letting state silently diverge. Conversation delivery, ACS and device-link notices remain fully interoperable.

## Consequences

`design_open` becomes 1 (A-055), and that item blocks only `local_delivery`. No `core_kernel` gate, criterion or W0 wire object changes because of it. Later-wave scopes can no longer be authorized with engineers inventing their wire formats. Scheduled-send and cross-implementation sync behavior become deterministic. This ADR is Anthropic-authored, so ADR-0117 requires a non-Anthropic reviewer.
