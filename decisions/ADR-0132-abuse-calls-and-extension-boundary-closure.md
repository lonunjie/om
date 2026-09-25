# ADR-0132 — Abuse, calls and extension-boundary closure

- **Status:** Accepted
- **Date:** 2026-09-25
- **Decided by:** integrator under operator delegation 2026-09-25
- **Resolves:** A-032, A-035 and the remote-mini-app plaintext boundary gap
- **Amends:** ADR-0128, ADR-0129

## Decision

### 1. Referral admission tokens use publicly verifiable Privacy Pass tokens

The W0 referral-token profile uses **RFC 9578 token type 0x0002**, publicly verifiable Blind RSA (2048-bit), not the privately verifiable VOPRF token. This keeps the recipient-controlled issuer key separate from the IntroGate verifier: the IntroGate needs only the issuer public key and therefore cannot mint referral capacity.

For each referral-issuer epoch the recipient client generates an issuer key pair and publishes the public key/key id to its selected IntroGate inside authenticated recipient-controlled configuration. Tokens are issued only over an already accepted relationship, with local per-relationship issuance budgets. The Privacy Pass challenge is domain-separated and commits to the IntroGate identity, recipient request-key digest, issuer epoch and admission class. Tokens are single-use at the IntroGate; spent token nonces are retained for the issuer epoch's validity horizon.

Revocation is by issuer-epoch rotation: the recipient publishes a new referral issuer key and asks the IntroGate to stop accepting the prior key after a bounded overlap. Individual issued tokens are intentionally unlinkable and not individually revocable. Short validity epochs bound exposure. `admission_proof` remains opaque bytes on the W0 wire; concrete profile id/version is inside the proof framing.

A ReferralAttestation remains separate client-visible provenance and never becomes admission authority or a reputation score.

### 2. Group-call confidentiality uses per-sender SFrame keys distributed only to joined devices

A Room's MLS epoch authenticates call control but **does not derive media keys**. For each call participant-set generation, every sending device generates fresh SFrame sender key material and distributes it only to the authenticated DeviceIDs currently joined to the call, over existing OM 1:1 device sessions (or an equivalent pairwise HPKE channel already authenticated to those DeviceIDs).

Each key package binds CallID, Room ConversationID, MLS epoch, participant-set generation, sorted participant DeviceIDs, sender DeviceID and SFrame key id. Join, leave, device revocation/quarantine or participant-set correction increments the generation and forces fresh sender keys before further media. The SFU receives ciphertext and routing metadata only.

A Room member, service or bridge that is not joined does not receive sender keys and cannot decrypt merely because it holds the Room MLS epoch. Services/bridges joining a call are visible participants. V-007 must test silent-SFU-listener, stale-generation, leave/rejoin and device-revocation cases.

### 3. Remote mini-app plaintext disclosure is egress at the boundary

ADR-0129's host-enforced plaintext taint model applies to code whose network authority OM can actually confine (for example the local Wasm capability host). A remote-origin mini-app is different: once plaintext crosses into that origin, OM must treat the disclosure itself as network egress because the remote origin has its own ambient network authority.

Therefore remote mini-apps receive no conversation plaintext by default. Plaintext can cross the frame boundary only as:

- a one-shot user-confirmed selected-context disclosure; or
- an explicit conversation-authorized recipient/service path that is represented to every affected participant under ADR-0129.

Granting a remote frame `content.read` without such disclosure authority is invalid. The host MUST NOT claim it can prevent later exfiltration by withholding a host `fetch` capability from remote-origin code.

### 4. Bilateral 1:1 plaintext-egress policy is signed relationship state

Where continuous personal automation needs plaintext egress in a 1:1, each participant signs an independent `RelationshipPolicyConsent` over relationship id, policy generation, egress principal/service id, scope and expiry/revocation generation. Effective permission is the intersection of the latest valid consent from both participants.

Either participant can revoke unilaterally; revocation is a monotone reduction and targets the consent/principal incarnation under ADR-0131. Devices sync their own account's consent state, and the relationship protocol gossips the latest signed peer consent. Absence, staleness or disagreement fails closed. This is protocol state, not merely local UI preference.

## Consequences

- A-032 and A-035 close as design questions; V-016 and V-007 retain the proof burden.
- Remote mini-apps cannot inherit the stronger containment promises made for local capability-sandboxed plugins.
- Later service/plugin wire work has an explicit 1:1 bilateral-consent interface and need not retrofit ambient local preferences.
