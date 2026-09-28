# ADR-0157 — Independent-review remediation: extension convergence, history-bundle boundary and scheduled-send late binding

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** independent non-Anthropic review of ADR-0154–0156 and their canonical consequences
- **Amends:** ADR-0155 extension handling; ADR-0156 §§3–4
- **Does not authorize engineering.**

## Context

The 2026-09-28 review fixed registry-version-dependent validity for non-critical extensions, but its residual T-133 still allowed a future **critical** extension to be downgraded on the wire: a malicious sender could clear `ExtEntry.critical`. A client that knows the later registration would reject the mismatch while a baseline client would see an unknown non-critical extension and accept the object. Because V1 has not shipped, the baseline can close that convergence hole now.

ADR-0156 also grouped every `HistoryBundle` with implementation-owned account-private sync. That is too broad. A new device restoring its owner's local history is implementation-private state, but a **group history bundle sent to a newcomer** is a group interoperability feature with security-visible provenance (`shared by <bundle author>`). The latter must be frozen with the `groups` scope and cannot be silently implementation-namespaced.

Finally, ADR-0156 says scheduled messages become normal events at send time, but does not explicitly prohibit precomputing session/MLS ciphertext or recipient fanout at compose time. Doing so would retain stale recipient/device/epoch state across revocation or membership changes.

## Decisions

### 1. Future critical extensions are self-identifying to the V1 baseline

V1 reserves the ASCII prefix **`om.critical.`** for mandatory-to-understand extension IDs.

1. Every newly allocated canonical critical extension after this ADR MUST use `om.critical.<public-name>`.
2. Experimental/private critical extensions MUST use `om.critical.private.<collision-resistant-namespace>.<name>`; they gain no canonical authority by doing so.
3. A non-critical extension MUST NOT use the reserved prefix.
4. A V1 decoder treats any **unknown** ID beginning `om.critical.` as effectively critical **regardless of the sender-supplied boolean** and rejects the containing object as `unknown_critical_extension`.
5. For a registered extension, the registry's criticality remains authoritative and a wire criticality mismatch rejects as before.
6. The two already-allocated critical IDs, `om.acs.fork_ack` and `om.acs.legacy_policy`, are grandfathered. They are safe for V1 because the baseline registry already knows them; they MUST NOT be renamed on the frozen W0 wire.

This makes a future critical registration fail closed on both old and new V1 clients even if a sender lies about the boolean. Registry growth can add meaning but cannot change whether the baseline accepts the containing object.

### 2. Split implementation-private device history from interoperable group history

Two different artifacts now have different names and contracts:

- **`DeviceHistorySnapshot`** is account-private same-owner history transfer/restore. It is implementation-owned in V1, uses the account-private sync namespace, and may be unavailable across implementations.
- **`GroupHistoryBundle`** is a group member's independently encrypted history gift to a newcomer. It is an interoperable `groups`-scope object. It preserves ADR-0128/R-223 provenance and exclusion rules and MUST have frozen CDDL/vectors before `groups` authorization under V-004.

`GroupHistoryBundle` MUST NOT carry or depend on an implementation/codec namespace to be understood. Its author is the only authenticated provenance of the bundle as a whole; claimed original message authorship remains non-authenticating historical content.

For implementation-private account sync, the namespace is concrete: every record family carries a **32-byte random `codec_id` chosen once for that codec family plus a `codec_major` uint** inside the authenticated E2EE framing. A codec ID is never reassigned to incompatible semantics. An unrecognized `(codec_id, codec_major)` is ignored whole. No central registry is required and a market-leading implementation receives no protocol privilege from its codec ID.

### 3. Scheduled-send queues hold intent, never stale cryptographic output

A queued scheduled item is a local compose intent, not a partially sent protocol object. Before its actual due/late send, it MUST NOT persist:

- a signed `ConversationEvent` intended for later release;
- a Double Ratchet or MLS ciphertext/header;
- a `SealedDeliveryEnvelope` or delivery-key selection;
- a frozen recipient-device fanout or group epoch;
- a provider/relay deposit wrapper.

At actual send time the client re-resolves current eligibility, relationship/group state, recipient devices/routes and cryptographic epoch, constructs the new `message.create`, then encrypts and fans out normally. Locally staged attachment plaintext/ciphertext may be retained under ordinary local protection, but authorization, expiry and recipient visibility are rechecked before upload/delivery. A revoked/removed/blocked target cannot be reached merely because it was eligible when the schedule was created.

## Consequences

T-133's criticality-downgrade residual is closed for the V1 baseline. The `groups` wire-freeze criterion now explicitly includes `GroupHistoryBundle`. Account-private history remains implementation-private without weakening group interoperability. Scheduled messages cannot bypass revocation or membership changes through stale pre-encryption.

This ADR is authored by an OpenAI reviewer. Its new normative changes require a different-vendor `independent_reviewer` before any affected engineering scope is authorized; this review does not self-approve its own amendments.
