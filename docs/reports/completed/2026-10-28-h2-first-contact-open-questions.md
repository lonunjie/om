# H2 first contact: open questions (blocked-item)

- **Date:** 2026-10-28
- **Status:** resolved and applied (2026-10-28). The operator delegated these decisions to the engineering agents; ADR-0181 answers all seven and the work is built. The questions below are kept as the record of what was open.
- **Source finding:** H2 in [`2026-10-28-core-kernel-opus-audit.md`](../2026-10-28-core-kernel-opus-audit.md). Spec: [`INVITES_INTROGATE_AND_SPAM.md`](../../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) sections 5.1, 6 and 11; wire objects in [`spec/cddl/discovery.cddl`](../../../spec/cddl/discovery.cddl).

## What the spec does define

- The wire shapes: `signed-ContactDescriptor`, `signed-IntroductionTicket`, `MessageRequest`, `signed-MessageRequestPayload`, `signed-IntroIdentityBinding`, and signature role 4 `intro_identity` ([`spec/cddl/common.cddl`](../../../spec/cddl/common.cddl)).
- The sealing construction (ADR-0131 PQ/T HPKE), the bootstrap reply key rules (ADR-0142 to ADR-0147) and the relationship states `ACCEPTED_UNBOUND` then `ACTIVE` after both `IntroIdentityBinding`s verify.
- Qualitative abuse controls: cheap validation and replay rejection first, then endpoint, ticket and source rate limits, per-recipient quotas, short expiry.

## What it leaves open (each blocks the item)

1. **Intro identity lifecycle.** Section 5.1 says the request key is trusted only from a ContactDescriptor "signed by the alias-specific intro identity". The spec does not say how Core derives or stores that key, whether it is per invite, per alias or per contact descriptor, which signature scheme the `bstr .size (32..2592)` key uses (Ed25519, ML-DSA-65 or the dual construction of the device binding), or when it rotates. Without this, no signature can be produced or verified.
2. **Can an invite link commit to the request key directly?** Pass 1 and 2 pin the request-key digest in the link instead of an intro-identity-signed descriptor (status report, question 4). The audit offers "build the IntroGate set, or record an ADR that allows a link-committed request key". That is an architecture decision, not an implementation detail.
3. **Account disclosure before acceptance.** Section 11 says acceptance "keeps the requester's AccountID hidden before acceptance". The built request carries the requester's ACS head and the whole chain in the sealed payload, so the inviter sees the account before accepting. The specified payload carries only a `sender_descriptor` (intro identity), with account binding deferred to `IntroIdentityBinding`. Conforming means a new relationship state (`ACCEPTED_UNBOUND`), a new `RelState` and UI states in `packages/om-platform-api`, and a re-ordered Hello. Is the state change in scope for `core_kernel`?
4. **IntroductionTicket issuer.** Core currently issues a bare invite id and stores a descriptor at an introduction service. Who issues and signs the ticket (the recipient's Core, a delegated anti-abuse service, the IntroGate), and what are `payload_limit_bytes`, the anti-replay nonce and `admission_proof` for a ticket minted by Core? RFC 9578 blind-RSA referral tokens (ADR-0132) need an IntroGate adapter that does not exist yet (`provider_reference` scope).
5. **Rate limits are unquantified.** Section 6 gives an ordering, not numbers. Open: the per-invite and per-source budgets, the window and its clock class, whether the limiter lives in Core (device, persists across restart) or only at the IntroGate (section 10: "enforceable server-side"), and what a refused request does (silent drop, or the section 11 user-visible "you are receiving a lot of requests from strangers; people you invited can still reach you").
6. **Chain-length cap at first contact.** The audit suggests "genesis plus a few links". The protocol caps are 64 links and 16 signatures (`control.rs`). A legitimate account with many rotations would be cut off by a lower cap, which also changes interoperability. Is a lower first-contact cap a protocol rule (needs a spec line) or a local resource policy?
7. **"Unverified" display.** `IntroductionRequest` has no verification field. Showing the requester as unverified until a binding exists is an API change to the UI contract (`packages/om-platform-api/src/core` and `crates/om-core/src/api.rs`), not a styling choice.

## Cost today (why it matters)

Each request costs one HPKE decapsulation plus up to 65 chain links x 16 dual-signature verifications before any admission decision, and the invite is marked used only after that work (`flows.rs`, `on_message_request`). Since B1 each of those verifications is also stricter, so the cost did not shrink. Cheap checks that already run first: invite exists, not used, not revoked, not expired, request size at most 70,000 bytes, W0 `MessageRequest` shape, ticket and request-key digest equality. M4 (applied) bounds the requester's bootstrap key lifetime.

## Suggested next step

Settle questions 1 to 3 in one ADR (intro identity, link-committed request key or not, when the account becomes visible). Questions 4 to 7 are then implementable; 5 and 6 are the part that can ship earlier on their own once the numbers are chosen.

## Applied (2026-10-28)

Decided and built under [ADR-0181](../../../decisions/ADR-0181-first-contact-intro-identity-and-confirmation-nonces.md) (commits `47098d9`, `318d6bb`, `ee42d2c`). Answers: (1) per-invite composite intro identity derived in Core from a profile secret and the invite id; (2) the link commits to the intro key digest and the request key is trusted only from a descriptor that key signed; (3) the requester's account is hidden until acceptance, `ACCEPTED_UNBOUND` then `ACTIVE` after both bindings verify; (4) Core issues the ticket, `admission_proof` empty; (5) persisted limiter, 20 per hour and 100 per day, before any decapsulation, silent drop with one aggregated notice; (6) 16-entry chain cap as local policy; (7) `verification` field and an "Unverified" label. Detail and caveats: the "H2 and M2 applied" section of [`2026-10-28-core-kernel-opus-audit.md`](../2026-10-28-core-kernel-opus-audit.md).
