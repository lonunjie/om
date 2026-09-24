# OM Decentralized Abuse, Moderation, and Safety Architecture

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 08:21 PHT
- **Resolves:** A-014
- **Scope:** blocking, first-contact spam, Room/Space moderation, shared advisories, reports, appeals, provider enforcement, malicious administrators, service/bot abuse and operator exposure

## 1. Decision summary

OM needs effective safety controls without creating a universal OM police force.

The governing rule is:

> **Enforcement belongs to the scope that has custody or authority; advice may be shared, but no shared safety service becomes protocol truth.**

OM therefore uses layered moderation:

```text
user-local control
relationship/introduction control
Room/Space governance
provider operational policy
public-directory/catalog policy
optional subscribed safety advisories
external legal/reporting recipients
```

SeedLife/the OM Project is not the mandatory recipient of reports, issuer of bans, owner of a global reputation score or universal identity revocation authority.

## 2. Distinguish moderation actions from safety advice

Two concepts must not be conflated.

### Authoritative scoped action

An actor with authority in a specific object can perform an action there:

```text
Room moderator -> redact / timeout / remove / ban in that Room
Space authority -> manage delegated Space-scoped admission/moderation policy
Provider -> deny its own relay/storage/directory service
User -> local block/mute/hide
```

The action has effect because the actor has authority over that scope.

### Safety advisory

A third party may publish signed information such as:

```text
"this ServiceID is reported as credential-phishing"
"this public sender is associated with spam"
"this blob hash is reported malware"
```

An advisory has **no authority by itself**. A user, Room, Space, directory, catalog or provider may subscribe and choose a response.

This enables collaborative defense without a universal blocklist.

## 3. Local user safety controls

Every user can locally:

- block an AccountPrincipal/ServicePrincipal/BridgePrincipal they know;
- mute/hide a conversation or sender;
- reject/delete a MessageRequest;
- revoke accepted-relationship delivery capabilities where applicable;
- suppress calls/service invocations;
- hide content based on locally selected safety advisory sources;
- export selected evidence.

Local block state is account/device-private and need not be published to the blocked party.

A local block does not claim that the blocked party is deleted from OM or banned elsewhere.

## 4. Blocking is not Sybil elimination

Permissionless identity means a determined attacker can create another AccountID/ServiceID.

OM does not promise impossible “block this human forever” semantics without a universal real-world identity system.

Instead:

- known relationship capabilities are revoked;
- future introductions remain gated through IntroGate policy/rate limits;
- repeated abuse can increase admission cost or require referrals/challenges;
- public aliases/invite links can be rotated;
- Room/Space admission can use trusted invitations, approval or locally chosen advisories;
- provider operational defenses may use temporary network/device signals under their own privacy policy.

This is deliberate. Perfect cross-identity blocking would require exactly the global linkability 0M and metadata privacy reject.

## 5. First-contact abuse

`IntroGate` remains the primary unsolicited-contact defense.

The cheap path is:

```text
bounds -> ticket/capability validation -> replay check -> quota
       -> optional privacy-preserving admission proof -> expensive work
```

Controls may include:

- per-recipient quotas;
- per-ticket/per-capability quotas;
- source/session/network rate limits;
- short expiry/single-use tickets;
- proof-of-work during automated attack;
- privacy-preserving entitlement/admission tokens;
- contact/referral/invite-only policy;
- CAPTCHA/device attestation only as optional provider/user policy, not protocol identity.

Creating a new AccountID/ServiceID remains cheap. **Unsolicited reach is the scarce permission.**

## 6. Room moderation semantics

Room policy may grant separate capabilities such as:

```text
moderate_content
set_slow_mode
timeout_member
remove_member
ban_member
manage_admission
manage_topic
manage_pins
manage_service_installation
```

These are not collapsed into one magical Administrator bit.

Moderation actions are authenticated as the moderator/governance principal that performed them. `moderation.redact` never pretends the original author deleted the message.

### Timeout

A timeout is a bounded authorization restriction with explicit start/expiry clock semantics. ~~Expiry is convenience/policy time, not constitutional authority; clients/Room control define deterministic behavior when clocks disagree.~~ A timeout ends only through a committed `untimeout` transition (ADR-0122).

### Remove vs ban

- **remove/kick** ends current Room membership but does not create a durable re-admission prohibition unless policy says so;
- **ban** creates Room-scoped admission-deny state against the identified principal until revoked/expired;
- neither action mutates AccountID globally.

A new Sybil identity is a new subject and may require separate admission defense.

## 7. Space/community moderation

A Space may coordinate policy across child Rooms only through explicit authority bindings accepted by those Rooms.

There is no invisible global cascade.

A Space-wide moderation action can be represented as a signed `ModerationDecision` from Space governance. A child Room enforces it only if its `SpaceAuthorityBinding` delegates the relevant capability.

This means a community can offer “ban from all our Rooms” UX without turning the Space into a hidden provider superuser.

Rooms that did not delegate that authority remain independent.

## 8. ModerationDecision

Logical form:

```text
ModerationDecision
  decision_id
  scope_ref                 # Room/Space/provider-local object
  actor_principal
  action                    # redact/timeout/remove/ban/admission-deny/etc
  subject_ref
  target_event_id?
  reason_code?
  public_reason_text?
  starts_at?
  expires_at?
  evidence_commitment?
  appeal_descriptor?
  previous_decision_id?
  authorization_evidence
```

Private report details/reporter identity are **not** automatically embedded in the public/shared decision.

The Room/Space may show a bounded reason while keeping sensitive evidence/reporting material separately access-controlled.

## 9. Appeals and review

OM does not define one universal appeals court.

A Room/Space/provider can publish an `appeal_descriptor` identifying its own review process or a delegated independent reviewer.

Appeal outcomes are new signed decisions such as:

- uphold;
- shorten/expire;
- revoke decision;
- restore future admission eligibility;
- annotate error.

A moderator cannot cryptographically erase evidence participants already possess.

If governance becomes abusive or irrecoverably captured, participants retain the right to leave, export what they possess, and fork/create a successor Room/Space. That is not a guarantee of continued access to the captured community; it is the anti-capture escape path.

## 10. Malicious administrators

OM assumes administrators/moderators can be malicious.

Mitigations include:

- capability-scoped authority and delegation ceilings;
- authenticated moderation/control actions;
- configurable quorum for high-risk governance transitions;
- no permanent protocol owner/master administrator;
- visible changes to moderation roles/policy;
- local evidence/export;
- Space/Room fork/migration path when governance cannot recover;
- optional subscribed watchdog/safety services that can warn about a community/provider without acquiring authority over it.

Ordinary moderation need not require quorum; doing so could make urgent abuse response unusable. Quorum is reserved for constitutionally significant actions as Room/Space policy chooses.

## 11. Report EvidencePackage

Reporting remains explicit disclosure.

A reporter selects what to disclose:

```text
selected plaintext/content
corresponding authenticated session/event evidence where available
selected media/blob material
minimal principal/conversation context needed for validation
report category
reporter statement
```

A report does not silently upload the entire Room, contact graph, recovery state or unrelated history.

Message protocols may provide participant authentication/deniability rather than permanent public non-repudiation. `EvidencePackage` states what is independently verifiable and what is reporter-held evidence.

## 12. Report targets

A user may send a report to any chosen target:

- Room/Space moderators;
- provider abuse contact;
- client-distributor safety service;
- employer/school/organization;
- law enforcement/authority;
- lawyer/advocate;
- independent safety labeler;
- nowhere (local evidence only).

The OM Project is not a mandatory hop.

Store policies or local law may require a particular distribution/provider to expose a report flow. That is a role-local obligation, not a protocol-global AbuseDesk.

## 13. SafetyAdvisory and composable moderation

OM standardizes an optional signed `SafetyAdvisory` shape so trusted organizations can share warnings without receiving universal enforcement power.

Logical form:

```text
SafetyAdvisory
  advisory_id
  issuer_principal_or_key
  subject_ref
  category
  severity_hint?
  scope_hint?               # spam, malware, public-content, service, etc.
  issued_at
  expires_at?
  evidence_reference?
  policy_reference?
  supersedes?
  signature
```

A subscriber decides what each category means locally:

```text
ignore
warn
hide locally
require manual admission
block in this Room/Space
provider review
```

The issuer cannot force the subscriber's response.

## 14. Labeler-scoped subject references

Publishing raw stable AccountIDs in every shared moderation feed can create a correlation database.

For non-public principals, a SafetyAdvisory SHOULD prefer a labeler-scoped pseudonymous reference where feasible:

```text
ScopedModerationRef = H(
  "OM-SAFETY-REF-v1" || issuer_scope_id || canonical_principal_id
)
```

Because OM principal IDs are high-entropy, subscribers that already know a candidate principal can compute/match the reference, while the feed does not itself reveal the raw principal identifier or create one cross-labeler universal handle.

For already-public entities such as a public ServiceID/PublicationID, direct subject references may be appropriate.

The exact hash/domain/canonical encoding is subject to V-016/conformance review; implementations must not invent incompatible ad-hoc encodings.

## 15. No global reputation score

OM does not define:

- one universal numeric reputation;
- one canonical “trusted/untrusted user” bit;
- one master spam list;
- mandatory proof-of-personhood;
- one required moderation labeler;
- one global ban authority.

A provider/community may compute local risk signals. A user may subscribe to multiple advisory sources. Those remain policy inputs, not root identity or protocol validity.

## 16. Portability of moderation policy

A Room/Space may export:

- its own moderation decisions;
- local policy/rule definitions;
- which external advisory sources it subscribes to;
- public ban/admission rules it intentionally shares.

Importing these into another community creates **new local policy**. It does not transfer the original Room's authority.

Shared safety lists are therefore advisory subscriptions, not “portable universal bans.”

## 17. Provider enforcement

A relay, BlobStore, directory, catalog, push sender or managed service may enforce its own terms/law/resource policy.

It may:

- reject new queues/uploads;
- stop serving a blob or locator;
- rate-limit/disable a provider-local capability;
- delist a public descriptor/catalog entry;
- terminate its service relationship;
- preserve/respond to evidence according to applicable law/policy.

It may **not** thereby mutate the user's AccountID, Room governance, ServiceID or protocol history outside the provider's custody.

Provider denial is not universal OM erasure.

## 18. Illegal/harmful content and encrypted storage

Private encrypted providers often cannot inspect plaintext they do not possess. OM does not add universal client-side scanning/backdoors merely to make every relay a content moderator.

Response paths include:

- recipient/local blocking;
- explicit evidence report;
- Room/Space moderation after participant disclosure;
- provider removal of a specifically identified stored object/capability under its policy;
- public-content moderation where Publication/directory content is intentionally public;
- lawful process against the role holding relevant data.

The architecture does not claim this eliminates all legal/operator risk. Operators need jurisdiction-specific policy and legal review for services they actually run.

## 19. Public Publications/directories/catalogs

Public reach justifies stronger policy than private accepted relationships.

A Publication host/directory/catalog may:

- refuse hosting/indexing;
- add its own warning/label;
- rank or downrank according to published policy;
- require age/safety classification where applicable;
- remove public listings.

Because these are optional replaceable roles, delisting does not delete the underlying OM identity/content from every other provider.

Public providers should document policy and appeal/contact routes when practical.

## 20. ServicePrincipal / bot / agent abuse

Service creation remains permissionless, but services acquire no ambient reach/read/send authority.

Safety controls include:

- install consent and scoped capabilities;
- explicit Room admission for subscriptions/full plaintext access;
- per-installation and provider rate budgets;
- automation lineage/hop limits;
- no self-bootstrap of service control;
- Room/user revocation;
- catalog delisting without ServiceID destruction;
- optional SafetyAdvisory targeting a ServiceID;
- provider host termination without identity transfer.

A service that changes host keeps its ServiceID only if its legitimate control state authorizes the new endpoint.

## 21. Bridge abuse

A BridgePrincipal can be a powerful exfiltration/spam surface.

Room policy controls:

- direction;
- external scope;
- allowed event classes;
- send/fanout rates;
- membership/event translation;
- revocation.

A bridge may be removed without deleting the foreign network or OM Room. Bridge-originated inbound spam remains BridgePrincipal-authored with foreign provenance, allowing local policy to distinguish bridge/foreign actor observations from native AccountPrincipal authorship.

## 22. Denial of service

Abuse controls must reject cheaply before expensive operations.

Every exposed provider role defines:

- bounded parser/input sizes;
- pre-auth/pre-capability cost ceilings;
- quotas/budgets;
- replay resistance;
- storage/fanout limits;
- cancellation/expiry;
- circuit breakers/failover.

Open-source clients cannot be trusted to self-rate-limit; resource admission is enforced at the recipient/provider boundary.

## 23. Safety service capture resistance

A dominant safety/advisory provider may become socially influential. 0M requires technical escape:

- users/communities can select multiple or no labelers;
- advisory format is public;
- clients can implement independent sources;
- issuer identity/signature is visible;
- labels can expire/supersede;
- no advisory changes protocol validity;
- clients can export/import subscription choices;
- a bundled default source can be replaced without AccountID migration.

A client distributor may ship defaults, but must not describe distributor policy as universal OM truth.

## 24. Security invariants

1. No universal OM moderator, reputation score or mandatory blocklist exists.
2. Room/Space/provider enforcement is scoped to the authority/custody actually held.
3. Shared SafetyAdvisories are signed advice; subscribers choose enforcement.
4. Reports disclose selected evidence only.
5. Local block/revocation does not pretend to solve all Sybil identities.
6. Provider denial cannot rewrite AccountID/Room/Service authority.
7. Moderation actions are authenticated as the actor who performed them.
8. Reporter identity/evidence can remain separately protected from the public moderation decision.
9. Service/bridge abuse controls preserve typed authorship/provenance.
10. Abuse defenses may introduce scoped linkability but not mandatory global person tracking.

## 25. Verification obligation

V-016 must adversarially/model test:

- block/revoke and new-Sybil behavior;
- IntroGate spam/resource attacks and privacy-preserving admission token replay;
- Room timeout/remove/ban authorization under reorder/races;
- Space delegated moderation and revocation;
- malicious moderator attempts to exceed delegation ceilings;
- report minimization and forged-evidence attempts;
- SafetyAdvisory signature/supersession/expiry;
- labeler-scoped subject reference collisions/correlation;
- multiple conflicting advisory sources and local policy resolution;
- provider denial without identity/control mutation;
- service/bot spam and automation-loop budgets;
- bridge abuse and provenance handling;
- fork/escape behavior under captured Room/Space governance.

A passing V-016 is required before engineering authorization because moderation and anti-spam controls touch authority, privacy and resource-exhaustion boundaries.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0122](../../../decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md): removing, banning, timing out or restricting a governance principal is a constitutional transition that needs governance authority or quorum. A timeout ends only through a committed, epoch-bound `untimeout` transition (auto-committed by moderators' clients after expiry). The subject cannot lift its own timeout.
- [ADR-0123](../../../decisions/ADR-0123-moderation-evidence-access-and-appeal-hold.md): evidence is encrypted to the moderator set at the report's epoch, without clawback after a demotion. Appeal holds are capped at `hold_max`, at most 180 days after the original retention expiry.
