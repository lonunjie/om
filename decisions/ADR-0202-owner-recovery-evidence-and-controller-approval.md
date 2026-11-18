# ADR-0202 — Owner recovery evidence, stale requests and controller approval

- **Status:** Accepted
- **Date:** 2026-11-18
- **Decided by:** operator delegation for Q-REC-13 through Q-REC-15. This decision fills gaps in the owner flow and preserves audited authority, wire semantics and security floors.
- **Class:** The verifier rules apply the existing account-control and recovery semantics. The owner-held checkpoint and optional controller ceremony are `additive_noncritical_extension` work under ADR-0165. Each requires its own versioned representation and conformance evidence before interoperability.
- **Answers:** Q-REC-13 (starting point and reconciliation), Q-REC-14 (expiry), Q-REC-15 (controller approval round trip).

## Design sources read

Read in full: [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md), [`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../docs/constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md), [`AGENT_ARCHITECTURE_RULES.md`](../docs/process/AGENT_ARCHITECTURE_RULES.md), [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md), ADR-0111, ADR-0141, ADR-0154, ADR-0163, ADR-0192, and ADR-0199 through ADR-0201.

Read in part: [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md), [`IDENTITY_PROTOCOL.md`](../docs/architecture/identity/IDENTITY_PROTOCOL.md), ADR-0190, ADR-0195, and the recovery entries in [`account.cddl`](../spec/cddl/account.cddl). The remaining Core, client and UI design-gate sources are outside this ADR's reading record. The implementation gate applies separately.

## Context

ADR-0199 allows a bounded RecoveryRecord to supply a candidate parent for a signed RecoveryRequest. The record's encryption authenticates the bytes to holders of the RecoveryRoot or RecoveryKit. A first read, a tail link or a self-signed genesis in those bytes cannot independently bind the device to the pre-existing account. ADR-0201 carries signed requests and vetoes. Its success responses and empty boards give no account-head or freshness proof.

The owner explicitly starts an attempt. Recovery policy remains the account's choice, including the explicit off option. ADR-0111 superseded the off-by-default setup rule in ADR-0026, so this ADR applies the audited setup default.

## Decision

### 1. Establish a starting point and reconcile before enabling authority

Start from the latest independently authenticated AccountControlState pin available in live, profile-bound storage. A fresh device with no pin can establish the first one from the verified chain of the owner's surviving controller through the personally authenticated ceremony in section 3. The device checks the chain from genesis, or extends a pin already held, with the normal ACS verifier and the applicable recovery evidence for prior links. Use the same checks for evidence retrieved over any other carrier. A contact or witness can supply an observation or chain bytes under the existing head-gossip rules. Their observation can reveal a missing head or fork. Every transition still needs its own account authorization.

If no controller survives, use an independently authenticated checkpoint retained by the owner. At issuance a trusted device captures the AccountID, generation and ACS digest of its verified head. The owner keeps that checkpoint with the recovery material in the same deliberate save action. A future sheet version can carry it in the same scan or paste. Its provenance comes from the owner's custody of the material issued by that device. A checksum or a MAC keyed by the RecoveryRoot cannot authenticate a substitute checkpoint delivered by a provider. The checkpoint fixes a minimum known head and grants no vote. Check the signed parent evidence against that pin and verify every intervening transition, including any previous recovery link under the applicable verifier rules. The missing signed chain or link evidence can travel over any available route because Core checks it against the pin.

With that evidence, recovery alone can use the strict verifier-local interval and the existing veto rules. Reconcile at request preparation and again before adoption. Compare authenticated head observations from the available, revalidated routes, enforce the durable floors, and resolve a known conflict only by the audited fork rules. A validated newer head makes the old request stale. A known but unreconciled newer or divergent head keeps the device restricted, including an observed different hash at the same generation. There is no proof of global freshness during a partition. An unanswered route supplies no affirmative evidence, and wall-assisted credit requires the current-segment authenticated sweep in ADR-0154. If no controller can answer that sweep, only the strict interval can qualify under ADR-0190.

A fresh device with only a current-format sheet and a store record, with no independently authenticated pin or controller, stays restricted even after the timer. AccountID is random. A holder of the root could construct another chain using that identifier and the root-derived recovery key. A signed genesis delivered in the same root-authenticated record cannot distinguish the histories. A restore archive supplies historical evidence and cannot establish a live pin. The existing sheet and record formats stay valid for lookup and request preparation. The new checkpoint representation and any chain or dossier carriage beyond existing routes require their own specification and vectors before implementation.

Persist the record high-water mark per scope and record lineage under ADR-0141, separately from the ACS pin. Persist the validated pin, exact request, target credential, proposed successor, verifier-local timer and observed vetoes in live local recovery state before use. Network retries reuse the signed request and its digest. Restart revalidates the evidence, preserves known vetoes, checks the same stale conditions and credits only time allowed by ADR-0147 and ADR-0154. Loss of the timer checkpoint restarts the interval. Loss of the authenticated pin returns the attempt to restricted reconciliation. Never restore timer credit from a backup or accept a peer's time. Adopt the verified successor, recovery evidence and revocations atomically before enabling account capabilities.

### 2. Staleness ends the request

A request ceases to be usable when its authenticated parent or policy epoch is superseded or its target is no longer fresh. A valid, previously observed veto for that digest remains terminal even if the Home prunes its copy. An explicit cancellation closes the local attempt. No owner-attempt deadline is defined. The signed request carries no timestamp, and the constitution supplies no independent deadline or time authority for this path.

Home inbox retention is transport housekeeping. Refresh delivery of the same signed request where needed without resetting the verifier's timer or creating a second attempt. The policy activation delay remains a verifier-local lower bound. It may exceed the Home's retention. Transport expiry or inability to re-establish delivery cannot be interpreted as the absence of a veto.

### 3. Exchange the proposal and approval with a surviving controller

The Home inbox alerts a surviving controller. The owner uses that device to open a short, single-use recovery approval ceremony with the restricted device. Reuse the owner-mediated QR or private-link exchange, comparison code, direction-specific encryption and transcript authentication of ADR-0192. Use a distinct, authenticated recovery purpose and version, with fresh ceremony secret and queues. An ordinary enrollment offer cannot be interpreted as a recovery offer in either direction. Existing sessions remain the path for the controller's notifications to contacts.

The restricted device sends the exact signed RecoveryRequest and RecoverySuccessorBundle over the ceremony. The controller checks its own current reconciled parent, policy, target credential, successor shape and known vetoes before asking the owner to approve. A co-signature is issued only after the explicit matching-code confirmation and the existing parent authorization checks, including the required controller votes. It signs the same child body. The authenticated reply binds the ceremony transcript, exact request digest, parent hash, target credential hash, approving controller and child hash. It carries the signed parent evidence, the co-signed successor and any applicable veto or newer state the device knows. Large evidence uses an authenticated manifest and bounded chunks as in ADR-0195. A reply for another ceremony or target, incomplete evidence, or an expired approval exchange cannot advance recovery.

The restricted device verifies the transcript, chain, controller eligibility and recovery successor, checks known vetoes and reconciles the parent again before adoption. The QR and transcript authenticate the exchange with the owner's device. The account's signed states and recovery verifier still decide authority. Ceremony expiry requires a fresh ceremony and leaves the signed recovery request subject to section 2. No messaging keys, history keys or RecoveryRoot are sent in the ceremony.

## Where did authority go?

AccountControlState and its committed policy retain authority. The owner holds the recovery secret. Existing contacts and witnesses supply observations. Homes and relays carry opaque or signed bytes. No new operator, global registry or mandatory witness is required.

## Work list for a builder

1. Core: persist and enforce the record floor, pin and restricted attempt state. Exercise replay after restart, fork and stale-target refusal, veto retention, strict timer completion only with an independently authenticated parent, and refusal with only an advisory record. Specify and test the owner-held checkpoint representation and carriage of any missing chain and dossier evidence.
2. Core and host: register, drain and acknowledge controller inbox items, fan out requests, read and verify vetoes, and publish the ADR-0190 notices. Exercise independent verification by a contact. Preserve the separate read-only history import.
3. Controller approval: specify the versioned ceremony payloads, bounds, registry entries and two-codec vectors before wiring the transport. Include cross-purpose replay, substituted target, missing chunk, expiry, stale parent and old-client safe failure. An unsupported ceremony cannot fall back to enrollment or bypass the wait.
4. Client: expose the restricted, waiting and terminal states through Core. Show a simple waiting or evidence-needed state when verification cannot finish.

## Traces

Decision D-219, requirement R-441 and threat T-185. Related: R-438 through R-440, V-002, ADR-0131, ADR-0141, ADR-0147, ADR-0154, ADR-0190 and ADR-0192.
