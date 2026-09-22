# OM Service Principals, Bots, Agents and LLMs

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:56 PHT
- **Scope:** typed service principals, service identity/control, installation, commands, selected-context disclosure, local/remote LLMs, delegated agency, tool calls, streaming, automation, service-to-service behavior and revocation

## Decision summary

OM does not make a brittle human-versus-AI classification. It standardizes typed authority principals with explicit installation/data-access semantics. "bot", "assistant", "agent" and "LLM" are product/presentation categories over the `ServicePrincipal` model; they do not create hidden authority classes and they do not claim how content was cognitively produced.

Permissionless service creation, decentralized discovery/hosting, and delegated personal agency are frozen in `SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`.

The normal safe interaction is:

```text
user explicitly invokes service
 -> client shows/derives bounded selected context
 -> only that context is disclosed
 -> service returns response/stream/tool requests
 -> host/user decides what becomes canonical chat state
```

A service that needs continuous/full Room plaintext must instead become an explicit Room service member/subscriber under Room policy. That is a visible trust-boundary change and, for a private Room, is cryptographically represented rather than hidden inside the client.

## Principal types

OM authority subjects use typed principal references:

```text
PrincipalRef
  account(AccountID)
  service(ServiceID)
  bridge(BridgeID)      # explicit external-network trust/provenance principal
```

`ServiceID` is random stable 256-bit provider-independent data. It is not a username, domain, endpoint URL or bot token.

A service may be controlled by:

- one AccountPrincipal;
- an OrganizationIdentity/governance set;
- a Space/Room governance policy where appropriate;
- a standalone multi-party service-control set.

Control affiliation is evidence/governance; it does not change `ServiceID`.

## ServiceControlState

A service needs continuity without sharing a human/root private key with every worker.

Conceptually:

```text
ServiceControlState
  service_id
  state_version
  previous_state_hash
  controllers[]
  approval_threshold
  endpoint_credentials[]
  profile_ref
  manifest_release_ref?
  authorization_evidence
```

The state authorizes service endpoints/agents and key rotation. Losing a worker key does not replace `ServiceID`; revoking a worker does not revoke the organization/human controller.

A signature proves control-state continuity. It does not make the service safe, truthful or "OM verified".

Service-control authorization follows a no-self-bootstrap rule: the target ServicePrincipal cannot count its own service endpoints, automation output, or authority derived from that same installation toward the quorum that creates/replaces its controllers or expands its endpoint authority. If Room/Space governance is a controller, controller evaluation uses independently authorized governance principals under the existing delegation/quorum rules; a service cannot obtain more authority merely by first gaining a Room role. Initial ServiceControlState creation is authenticated by its declared independent controller set, and later states are authorized from the previous accepted state plus any explicitly defined controller-transition policy.

## Service endpoint credentials

A remote/full-member service has one or more typed endpoint credentials analogous to device endpoints but explicitly marked `service_endpoint`.

They may participate in OM encrypted sessions/MLS only when authorized by current `ServiceControlState` and the target relationship/Room policy.

They are **not** `DeviceIdentity` entries under a human `AccountID`, and service endpoints do not count toward human-device governance/majority rules.

## Conversation authorship becomes principal-typed

Canonical conversation events use:

```text
author_principal : PrincipalRef
author_endpoint_id
```

instead of assuming every event has `author_account_id + author_device_id`.

For an AccountPrincipal, `author_endpoint_id` is its authorized DeviceIdentity.

For a ServicePrincipal, it is an endpoint/agent credential authorized by `ServiceControlState` and any relevant installation grant.

A bridge uses the same typed-author boundary under `INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`.

This removes the architectural pressure to make autonomous services masquerade as AccountPrincipals.

## Human assistance versus autonomous service authorship

The protocol classifies **authority**, not intelligence or biology. See `SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md` for the canonical Account-confirmed / autonomous-service / represented-account cases and `RepresentationGrant`.

OM does not attempt to detect whether a human used a spell checker, local LLM, accessibility tool or drafting assistant before pressing Send. A user-confirmed send may remain authored by the AccountPrincipal.

When a service itself is authorized to post autonomously or respond as an installed bot, the event is authored by the `ServicePrincipal` and is visibly labeled as automation/service content.

A client must not silently rewrite autonomous service output as if the represented/user AccountPrincipal cryptographically authored it.

## ServiceDefinition / manifest

A service publishes a portable signed definition describing capability needs and interaction surfaces, conceptually:

```text
ServiceDefinition
  service_id
  version
  display_metadata
  service_categories[]        # assistant/bot/agent/search/automation/etc.
  command_descriptors[]
  supported_install_scopes[]
  requested_capabilities[]
  supported_execution_modes[] # local_component, remote_om, remote_web_service
  data_handling_declarations?
  homepage/source refs?
```

This is declaration/provenance, not permission. Actual authority comes from installation grants and Room/Space/account policy.

## Installation scope and interaction scope are separate

A service may be installed at:

```text
personal/account-local scope
Room scope
Space scope
```

Installation says **where the service is available**. It does not automatically say which plaintext it may read or which actions it may perform.

A `ServiceInstallation` records:

```text
installation_id
service_id
scope_ref
granted_capabilities[]
data_access_mode
command/interaction surfaces
automation_rules[]?
approval_evidence
created_by
revocation_state
```

Space installation alone never grants child-Room plaintext. A child Room must explicitly bind/install the service or accept delegated installation authority under the existing SpaceAuthorityBinding rules.

## Four data-access modes

### 1. Explicit selected-context invocation — default

The service is **not** a Room member and receives no ambient transcript.

The user invokes a command/action and the client creates a bounded disclosure bundle containing only selected data, for example:

- the selected message;
- selected attachments;
- current draft;
- selected prior N messages;
- selected thread/topic context;
- explicit user text.

The UI can summarize what will leave the local trust boundary before a remote call.

### 2. Local personal processing

A local service/plugin may process plaintext on the user's device under local capability grants.

Local execution is not automatically a disclosure to Room peers because every client can already process plaintext it legitimately receives. However, local services receive **no network capability by default**. If a local service can send data to the network, the host treats that as a separate disclosure capability and warns/grants accordingly.

### 3. Scoped event subscription

A Room may authorize a service to receive selected event classes or selected Topic/context scope without making it a full semantic human member.

The subscription is explicit shared Room control state, visible to members, and limited by capabilities. The service gets only the event classes/scope declared by that installation.

For private encrypted Rooms, the implementation must still define how those selected plaintext events are encrypted/delivered to the service endpoint; the service is an intended recipient for those disclosed events.

### 4. Full Room service member

A service is admitted as a typed member/recipient and receives the Room plaintext its membership permits.

For private Rooms:

- its authorized service endpoints become MLS clients;
- membership/control state visibly contains the ServicePrincipal;
- adding it is a high-risk trust-boundary change that may require configured quorum;
- removal performs the normal Room control + MLS removal transition;
- prior plaintext legitimately received cannot be clawed back.

The roster/UI must clearly communicate that automation/service infrastructure can read the Room.

## ContextDisclosureBundle

Remote selected-context calls use a disclosure object, conceptually:

```text
ContextDisclosureBundle
  invocation_id
  service_id
  source_scope_hint?          # minimal by default
  selected_items[]
  disclosed_participant_refs[]?
  purpose/command_id
  expiry
```

One-shot personal invocation should not reveal stable `ConversationID`, full member roster or unrelated metadata unless needed and explicitly disclosed.

A Room-installed service may legitimately know its Room scope because it is installed there; that is a different trust relationship.

## Commands and interaction surfaces

Services declare typed commands, not arbitrary parser hooks:

```text
CommandDescriptor
  command_id
  localized_name/description
  parameter_schema
  invocation_contexts[]       # personal/room/message/thread/topic/etc.
  requested_input_scopes[]
  possible_output_actions[]
```

A host shows only commands valid for the current installation/context/permissions.

Command declaration never grants `message.send`, `room.manage_members` or other authority by itself.

## Streaming responses

Streaming is ephemeral invocation state:

```text
InvocationID
  -> chunk/partial response frames
  -> tool requests / progress
  -> final candidate result
```

Partial stream frames are **not** canonical ConversationEvents merely because they were displayed.

A final service-authored reply becomes canonical only through an authorized `message.create`. A user-invoked drafting assistant may instead return a draft that the user edits/confirms and then sends as the AccountPrincipal.

Cancellation stops future stream delivery where possible; it does not erase data already disclosed to a remote service.

## LLMs: local and remote are materially different

### Local LLM

Default:

- model/runtime runs on the user's device;
- no network access unless separately granted;
- selected-context read scope unless the user grants broader local indexing/search;
- model output is untrusted data;
- no automatic tool/system capability merely because the model can produce a tool-looking string.

### Remote LLM

Default:

- remote service is visibly identified;
- only explicit disclosure bundle is sent;
- room keys/DeviceVault/raw MLS secrets are never handed to the provider;
- transport encryption does not change the fact that the service receives plaintext;
- data retention/training promises are provider policy, not cryptographic OM guarantees.

A Room can deliberately install a remote LLM as a full service recipient, but the UI must say so plainly.

## Tool use and prompt-injection boundary

LLM text cannot directly become authority.

A service may request a typed host tool action such as:

```text
create_draft
search_selected_local_scope
fetch_url_via_scoped_network_provider
propose_room_action
```

Actual execution is mediated by the installation's granted capabilities and, where configured, user/governance approval.

No service receives by default:

- raw private keys;
- DeviceVault access;
- unrestricted shell/process execution;
- unrestricted filesystem;
- unrestricted network;
- account-control authority;
- Room/Space governance authority;
- permission to approve its own privilege escalation.

Prompt injection is therefore treated as untrusted input attempting to exercise capabilities, not as permission.

## Automation rules

Autonomous behavior must be explicit and bounded.

```text
AutomationRule
  rule_id
  service_id
  trigger_scope
  trigger_types[]
  filter
  allowed_actions[]
  rate/budget bounds
  enabled
  approval evidence
```

Examples:

- post a summary every day;
- respond when explicitly mentioned;
- moderate messages matching a Room rule;
- mirror a selected feed.

Automation is not equivalent to "read everything forever". Trigger visibility and data access must match the installation grant.

## Bot-to-bot / agent-to-agent loop safety

Service-originated output does **not** implicitly trigger every other service.

Automation/invocation frames carry lineage such as:

```text
root_invocation_id
parent_invocation_id?
origin_principal
hop_count
```

Default automation rules ignore service-originated events unless explicitly configured otherwise. Hosts enforce a bounded hop/rate budget so two bots cannot create an unbounded reply loop merely by mentioning one another.

## Revocation

Revocation stops future authority:

- personal selected-context service: delete/revoke installation and credentials;
- Room subscription: remove subscription grant;
- full private-Room service: revoke installation + remove service endpoints through Room control/MLS;
- endpoint compromise: rotate/revoke endpoint in ServiceControlState.

Revocation cannot prove a remote service forgot plaintext it already received. UI and policy must not promise retroactive erasure.

## Creation, discovery and catalogs

Service creation is local and permissionless: generate `ServiceID`, initial independently controlled `ServiceControlState` and a signed definition. No BotFather/global issuer exists.

Services may be discovered via direct links, QR, organization pages, domain aliases, independent catalogs, peer sharing or Space recommendations. Hosting is a replaceable endpoint role rather than identity ownership.

There is no mandatory OM bot store or SeedLife approval gate. Catalog signatures/reviews are optional attestations, not protocol authority. Cheap identity creation does not imply unsolicited-delivery authority: reachability/admission remains consent-, Room-, IntroGate-, provider- and policy-scoped.

A conforming third-party client can interact with a service from published interoperable definitions without needing a secret official-client API.

## Security invariants

1. Autonomous/delegated actors are typed principals; OM does not attempt to infer whether content is human- or AI-generated.
2. Installation scope and data-access scope are separate.
3. Selected-context invocation is the default remote-agent path.
4. Space installation never silently grants child-Room plaintext.
5. Full Room service access is visible shared trust-boundary state.
6. Local LLM does not imply network disclosure; network is a separate capability.
7. LLM output/tool requests are untrusted and capability-mediated.
8. Autonomous service posts are authored by the ServicePrincipal.
9. Revocation stops future authorization but cannot claw back prior plaintext.
10. Service-control quorum cannot count authority derived from the target service itself; service installation cannot bootstrap its own controller or endpoint escalation.
11. No OM Project bot cloud/catalog is required for interoperability.
12. Creating a ServiceID requires no central issuer and grants no ambient reach/read/send authority.
13. A service acting for a person remains ServicePrincipal-authored with a scoped `RepresentationGrant`; it never becomes an Account device merely by representation.
