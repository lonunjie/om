# ADR-0133 — Provider, release-transparency and conformance closure

- **Status:** Accepted
- **Date:** 2026-09-25
- **Decided by:** integrator under operator delegation 2026-09-25
- **Resolves:** A-045, A-046, A-047
- **Amends:** ADR-0130

## Decision

### 1. Home API is a versioned capability API, not a vendor API

[`spec/protocol/home-api.md`](../spec/protocol/home-api.md) and [`spec/cddl/home-api.cddl`](../spec/cddl/home-api.cddl) are the normative W0 Home API surface. Version negotiation is explicit; unknown major versions fail closed and additive minor capabilities are advertised through `ProviderCapabilities`. Requests authenticate with the narrowest capability appropriate to the operation; AccountID bearer authentication is forbidden for ordinary relay/blob/IntroGate operations.

The API freezes operations for relay deposit/drain/ack/capability rotation, per-contact deposit capability mint/revoke, descriptor publish/fetch, IntroGate ticket/request handling, async key material publish/fetch, sponsorship redemption, RecoveryRecord store/fan-out, encrypted blob spool/location repair, and forwarding/moved responses. Typed errors come from [`spec/registries/om-home-errors.json`](../spec/registries/om-home-errors.json).

`ProviderCapabilities` is replaced by an actual JSON Schema and includes `durability_class`. Missing implementation interfaces are made explicit in [`IMPLEMENTATION_INTERFACES.md`](../docs/architecture/providers/IMPLEMENTATION_INTERFACES.md). V-020 tests the API; architecture no longer leaves its shape to provider engineering.

### 2. Release transparency uses a Rekor-v2/C2SP-checkpoint compatible append-only log profile

OM does not invent a transparency log. The reference profile uses Rekor v2-style entries/checkpoints and the C2SP signed-checkpoint format with inclusion and consistency verification. A release entry commits to distribution id, release digest, TUF target metadata digest, source/provenance digest when present, and release generation.

The release log is evidence, not update authority: TUF/root signatures remain authority. At least two independently operated monitors verify append-only consistency and inclusion and publish signed `ReleaseObservation`s over the release digest and observed checkpoint. Automatic ClientHost deployment requires two valid observations from distinct configured monitor operators/failure domains after the owner's delay. An owner may manually approve when observations are unavailable, but the UI must state that transparency corroboration is incomplete; a signed SecurityAdvisory may shorten delay under ADR-0129 but never fabricate observations.

Monitor snapshot retrieval is cacheable, epoch/batch addressed and mirrorable so clients need not query by private user identity or installed version. Missing monitors pause auto-update but never disable the already installed client or messaging. Origin compromise remains an acknowledged web-platform limit: code already served by a hostile origin cannot be trusted to police itself. Migration away from a compromised origin is new-device enrollment.

### 3. Every W0 conformance suite has a concrete case list

The existing owning documents contain normative `CONFORMANCE-*` case tables for identity, discovery, attachments, security floor, accessibility, identifier spoofing, performance, UI intents and UI themes. [`machine/conformance_suites.json`](../machine/conformance_suites.json) marks those W0 suites `specified`.

A case list defines observable input, expected outcome and fail condition. It does not mark the case passed. Scope authorization still requires its named V-gates, independence and executable evidence.

## Consequences

- A-045..A-047 are resolved.
- `provider_reference` remains blocked by V-020 evidence, not by an undefined API.
- `reference_client_ui` remains blocked by V-023/V-021/V-022 evidence, not by an undefined transparency mechanism or case list.
- A compromised PWA origin is never described as cryptographically constrained by JavaScript delivered from that same origin.
