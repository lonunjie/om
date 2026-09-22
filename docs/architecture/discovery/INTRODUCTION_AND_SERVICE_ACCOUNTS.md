# OM Introduction, Message Requests, Organizations, and Service/Bot Accounts

- **Status:** Phase 0 selected direction
- **Timestamp:** 2026-08-18 13:53 PHT (UTC+08:00)

## Message-request gate

Unknown users do not immediately enter a normal conversation.

```text
public alias / QR / domain
        ↓
IntroductionEndpoint
        ↓
MessageRequest
        ↓
Accept / Ignore / Block / Report
        ↓
RelationshipBinding
        ↓
normal private delivery
```

The first-contact plane is intentionally weaker and more constrained than an accepted relationship.

### Default unknown-sender limits

Before acceptance:

- small introductory payload only;
- no arbitrary file/video upload;
- no unsolicited calls;
- no repeated high-rate messaging;
- no automatic addition to groups;
- rate limits and abuse controls at the recipient's chosen introduction provider;
- rotatable/revocable introduction capability.

Established contacts bypass the request gate through private relationship capabilities.

## Why this belongs in architecture

Spam defense cannot depend on the official client's UI because an attacker can modify the client.

The relay/introduction protocol must make abusive first contact cheap to reject before expensive storage, fanout, or cryptographic work.

## Business identity model

Businesses should not run their permanent identity from one bot token or one employee account.

Use:

```text
OrganizationIdentity
       │
       ├── DomainClaim: example.com
       │
       ├── ServiceIdentity: support@example.com
       │       │
       │       ├── BotAgent A
       │       ├── HumanAgent B
       │       └── HumanAgent C
       │
       └── ServiceIdentity: orders@example.com
               └── BotAgent D
```

The organization's root/administrative authority delegates a stable service role.

Bots and human operators receive scoped, revocable operational authority. They do not receive the organization's root identity key merely to answer messages.

## ServiceIdentity

A stable public business endpoint:

```text
ServiceIdentity
  service_id
  organization_id
  display_name
  domain_role_alias
  service_policy
  authorized_agents[]
  automation_disclosure
  signatures
```

Examples:

- support@example.com
- reservations@example.com
- orders@example.com

The user sees the stable business/service identity even when the underlying bot/vendor/employee changes.

## Agent delegation

```text
AgentAuthorization
  service_id
  agent_principal_ref
  endpoint_or_delegation_ref
  scopes[]
  issued_at
  expires_at?
  revoked_at?
  authorization_signature
```

Possible scopes:

```text
read_new_requests
reply
send_attachments
handoff
manage_conversation
initiate_message_when_consented
```

Service bots SHOULD NOT be permitted to initiate arbitrary unsolicited conversations merely because they exist.

## Automation disclosure

Users must be told when an autonomous ServicePrincipal is acting, but OM does not attempt to infer whether content itself was produced by AI or a human. Authority/disclosure follows the principal that caused the canonical action.

Examples:

```text
Example Bank Support
✓ example.com
Automated assistant
```

or

```text
Example Bank Support
✓ example.com
Maria joined the conversation
```

Do not disguise autonomous ServicePrincipal actions as AccountPrincipal authorship. A human-confirmed Account action may remain Account-authored even if drafting tools or an LLM assisted it.

## E2EE boundary

If a service bot must process message plaintext, that service/bot is an intended cryptographic endpoint or explicit service trust boundary.

The UI must not imply that plaintext is available only to one human when an automated service can read it.

The organization may rotate bot infrastructure without changing ServiceIdentity.

## Human handoff

A business conversation can preserve one stable `ServiceIdentity` while changing agents:

```text
bot triage
 -> human support agent
 -> specialist
```

The conversation identity remains "Example Bank Support".

Agent additions/removals are auditable service-state events.

## Business inbound scale

A business may configure a more permissive public IntroductionEndpoint than a private person.

The scalable part belongs behind the provider interface:

- rate limiting;
- queueing;
- proof/challenge policies;
- bot triage;
- abuse scoring based on local/provider policy;
- operator tools.

OM core should define the request semantics, not one mandatory anti-spam vendor.

## Personal account bots

Personal users may authorize assistants, but the same principles apply:

- explicit;
- scoped;
- revocable;
- autonomous actions remain visibly ServicePrincipal-authored;
- optional on-behalf-of behavior uses a `RepresentationGrant`;
- no root AccountID key sharing;
- least access needed.

## Architectural rule

> Services may act for accounts only through delegated capabilities. They do not become the Account identity root, and OM does not require a human-versus-AI classifier.

## 2026-09-21 IntroGate refinement

`IntroductionEndpoint` is refined into an `IntroductionService` / `IntroGate` boundary.

Public search records MUST NOT publish a reusable relationship mailbox capability. A sender first obtains a short-lived/single-use `IntroductionTicket` scoped to a small MessageRequest. The IntroGate can enforce replay protection, per-recipient quotas, rate limits and optional privacy-preserving admission proofs before expensive storage/fanout work.

Invites carry separate revocable capabilities. Revoking an invite does not affect accepted relationships. See `INVITES_INTROGATE_AND_SPAM.md`.

## 2026-09-23 bot/LLM direction

Bots and LLM agents are explicit service principals. Default interaction should be command/scoped-context delivery; a bot admitted as a full room/group member is visibly a plaintext recipient for the data it can decrypt. Local on-device inference and remote-agent disclosure are distinct capabilities and must not be conflated.

## 2026-09-23 07:18 principal-model refinement

`ServiceIdentity` in this document is the **product/business-facing continuity profile**. Its protocol actor is the provider-independent typed `ServicePrincipal` identified by `ServiceID` and governed by `ServiceControlState`. Employee accounts may be delegated agents of that service; autonomous bot/LLM posts are authored by the ServicePrincipal itself, not by a synthetic AccountID.

The canonical `ConversationEvent` envelope therefore carries `author_principal: PrincipalRef` plus an authenticated `author_endpoint_id`. Human-authored events bind the endpoint to `DeviceIdentity -> AccountControlState`; autonomous service-authored events bind it to `ServiceEndpoint -> ServiceControlState`. Neither service endpoints nor their controllers become members of the user's account-device majority.

Default service interaction is selected-context invocation. Continuous scoped subscription and full-Room membership require explicit grants and visible disclosure; a full-Room service is an actual plaintext recipient. Local model execution and network egress are separate capabilities. Detailed rules live in `SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`.


## 2026-09-23 07:56 creation/delegated-agency refinement

Service identity creation is permissionless and local; no BotFather/global bot issuer exists. Discovery/catalogs/hosting are optional replaceable roles. A personal autonomous agent acting for an AccountPrincipal remains authored by its ServicePrincipal and may attach a scoped `RepresentationGrant`, allowing clear UI such as “Alice · via Assistant” without making the service an Account device. See `SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`.
