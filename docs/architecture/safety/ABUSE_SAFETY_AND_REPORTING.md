# Abuse, Safety, and Reporting

## Goal

Reduce spam, harassment, fraud, and resource abuse without creating universal plaintext access or a global social-control identity system.

## First-contact boundary

Unknown senders use constrained IntroductionEndpoints / MessageRequests:

- tiny bounded payloads;
- strict sender/capability rate limits;
- no unsolicited large attachments;
- no unsolicited group addition;
- no direct calls by default;
- cheap validation before expensive storage/fanout/crypto work;
- optional escalating proof/resource requirements if attacks demand it.

Acceptance creates private relationship delivery capabilities.

## Enforcement boundary

Controls that a hostile modified client could remove must be enforced by recipients/relays/providers. UI-only spam limits are not security controls.

## Blocking

Blocks are primarily local/private. Where a provider needs a deny rule to avoid resource consumption, use scoped opaque capability/endpoint identifiers rather than mandatory global identity publication.

## Reporting

A user may intentionally disclose selected abusive content through a **Report Evidence Bundle**. The design goal is to provide enough authenticated evidence to evaluate the report while not granting the platform passive access to all conversations.

Logical report bundle may include:

```text
selected message plaintext chosen by reporter
corresponding authenticated event/envelope evidence
conversation/account/device context strictly required for validation
report category and local annotations
optional media objects explicitly selected by reporter
```

The exact cryptographic evidence mechanism requires careful design so a malicious reporter cannot trivially fabricate another user's authorship while also preserving deniability properties where expected.

## No mandatory global reputation score

OM should not create a universal numeric trust/reputation score. Provider/community-local abuse decisions can exist, and users may subscribe to optional safety services, but no score becomes part of root identity or protocol validity.

## Provider denial

A relay/media/storage operator may deny service according to its terms. Because providers are replaceable, provider denial is not equivalent to erasing a user's identity from OM.

## Public/discovery spaces

If OM later adds public directories, public groups, or searchable services, stronger moderation can apply there because those spaces have different expectations from private accepted relationships. Do not use “private E2EE” as a reason to make public discovery ungoverned.

## Children/vulnerable users

Any future safety features for minors should avoid universal scanning backdoors. Prefer account/device policy, contact approval, known-adult supervision models where legally/ethically appropriate, constrained discovery, and user-visible reporting controls.

## Abuse-resistance tests

- malicious client cannot bypass IntroductionEndpoint quotas;
- reporting reveals only intentionally selected evidence;
- report mechanism cannot silently become bulk conversation export;
- relay blocking does not require plaintext;
- resource exhaustion is bounded before expensive work;
- provider ban does not mutate AccountID;
- no global trust score is required for ordinary delivery.

## IntroGate before MessageRequest

The public discovery plane now terminates at an IntroGate. A public handle does not expose a reusable mailbox token. The sender obtains a short-lived/single-use IntroductionTicket and can submit only the bounded MessageRequest allowed by that ticket. This gives providers a cheap point for replay protection, rate limits, per-recipient quotas, adaptive proof-of-work or optional privacy-preserving admission proofs before expensive recipient work.

Famous/public users can tighten policy to referrals/contacts/challenges without changing their AccountID or normal accepted-relationship delivery.

## Resource admission economics

Per-user infrastructure reduces the blast radius of global relay abuse but creates a direct cost-exhaustion target against each recipient's Worker/relay. The request path therefore rejects cheaply before allocating storage/expensive work:

```text
parse bounds -> capability/ticket validity -> replay check -> quota -> optional admission proof -> expensive work
```

Privacy Pass-style unlinkable tokens are a candidate for proving limited entitlement without stable global identity. Provider-local quotas/budgets remain independent of any universal reputation system.

## 2026-09-22 operatorless reporting refinement

The operatorless core does not require an OM AbuseDesk. The baseline response is:

1. local block / capability revocation;
2. optional selective `EvidencePackage` export;
3. user chooses the external recipient (authority, organization, administrator, lawyer, etc.).

A store-distributed client or managed provider may add a `ReportTarget` integration to meet its own obligations. Reports are explicit disclosures and must not require ambient server plaintext access.


## 2026-09-23 08:21 decentralized moderation resolution

`DECENTRALIZED_ABUSE_MODERATION.md` is now canonical for A-014.

The earlier principles above remain valid but are sharpened as follows:

- enforcement is scoped to the user/relationship/Room/Space/provider authority that actually holds the capability/custody;
- optional signed `SafetyAdvisory` sources may share warnings, but subscribers choose enforcement and no advisory becomes protocol truth;
- non-public subjects should use issuer-scoped pseudonymous moderation references where feasible rather than creating a universal cross-labeler identifier;
- `ModerationDecision` separates authenticated action/reason/appeal state from private report evidence;
- provider denial/delisting cannot mutate AccountID/Room/Service/Bridge authority outside provider custody;
- permissionless identity means OM does not promise impossible permanent human-level blocking across new Sybil identities; it constrains unsolicited reach through IntroGate/admission/rate/resource controls instead;
- SeedLife/the OM Project remains neither mandatory AbuseDesk nor universal moderator.

## 2026-09-24 public-surface illegal content (PR-014, R-181)

The "no universal client-side scanning" stance (`DECENTRALIZED_ABUSE_MODERATION.md`) covers **private E2EE conversations**. Reference clients do not scan private plaintext for any authority, and they do not ship hidden hash-matching.

**Public or provider-readable surfaces** are different and must say so explicitly:

- public Publications and their mirrors;
- directories, Home lists, content-pack and service catalogs;
- blob hosts serving public (non-E2EE) content;
- public link-preview/unfurl services.

Each operator of such a surface, including any project-controlled one, is responsible for the content it can read and serve. It must publish a role-scoped illegal-content process (reporting intake, takedown, preservation where legally required, and referral to competent bodies such as child-protection hotlines) appropriate to its jurisdiction. That process is provider-scoped (R-154). It never becomes a protocol-wide authority, and it cannot rewrite AccountIDs, Rooms or history outside that surface. PR-014 blocks any project-controlled public surface until its process exists.

Users always keep the existing tools: block, leave, selective EvidencePackage reports to a chosen ReportTarget, and Room/Space moderation.
