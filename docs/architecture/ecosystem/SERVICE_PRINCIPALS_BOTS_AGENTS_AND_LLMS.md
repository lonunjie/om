# OM Service Principals, Bots, Agents and LLMs

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:56 PHT
- **Scope:** typed service principals, service identity/control, installation, commands, selected-context disclosure, local/remote LLMs, delegated agency, tool calls, streaming, automation, service-to-service behavior and revocation

## Decision summary

OM does not make a brittle human-versus-AI classification. It standardizes typed authority principals with explicit installation/data-access semantics. "bot", "assistant", "agent" and "LLM" are product/presentation categories over the `ServicePrincipal` model; they do not create hidden authority classes and they do not claim how content was cognitively produced.

Permissionless service creation, decentralized discovery/hosting, and delegated personal agency are frozen in [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md).

The normal safe interaction is:

```text
user explicitly invokes service
 -> client shows/derives bounded selected context
 -> only that context is disclosed
 -> service returns response/stream/tool requests
 -> host/user decides what becomes canonical chat state
```

A service that needs continuous Room plaintext must instead become an explicit full Room member under Room policy. That is a visible trust-boundary change and, for a private Room, is cryptographically represented (it holds the Room's MLS keys) rather than hidden inside the client. There is no cryptographic mode between the two (ADR-0129).

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

A bridge uses the same typed-author boundary under [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md).

This removes the architectural pressure to make autonomous services masquerade as AccountPrincipals.

## Human assistance versus autonomous service authorship

The protocol classifies **authority**, not intelligence or biology. See [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md) for the canonical Account-confirmed / autonomous-service / represented-account cases and `RepresentationGrant`.

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

`local_component` means the service runs on the user's own device as a sandboxed plugin ([`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md)). Its service endpoint credential is sealed in that device's DeviceVault, is never readable by the component's code, and is used only by the host to sign the component's ServicePrincipal-authored events. A local plugin that acts without per-action confirmation must run in this mode (ADR-0129).

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

## Data-access modes

There are exactly two cryptographic positions a remote service or bridge can hold toward a conversation: **not a member** (invocation-only) or **full member**. Local processing (mode 2) is a third, device-local case. The former "scoped event subscription" is not a cryptographic mode; it is a trigger filter on full membership (ADR-0129).

### 1. Explicit selected-context invocation — default

The service is **not** a Room member and receives no ambient transcript.

The user invokes a command/action and the invoking member's client creates a bounded disclosure bundle containing only selected data, and sends it to the service over a 1:1 OM session with a service endpoint authorized by the current ServiceControlState. Each invocation is a one-shot, user-initiated disclosure (ADR-0124 rule 1) and appears in disclosure receipts. No other member's device forwards anything, and no member device is ever a standing forwarder for a service. Selected data may be, for example:

- the selected message;
- selected attachments;
- current draft;
- selected prior N messages;
- selected thread/topic context;
- explicit user text.

The UI can summarize what will leave the local trust boundary before a remote call.

### 2. Local personal processing

A local service/plugin may process plaintext on the user's device under local capability grants.

Local execution is not automatically a disclosure to Room peers because every client can already process plaintext it legitimately receives. However, local services receive **no network capability by default**.

~~If a local service can send data to the network, the host treats that as a separate disclosure capability and warns/grants accordingly.~~ The local user's grant is not enough for plaintext to leave. ADR-0124's taint rule applies to local services, personal-scope installations and personal `AutomationRule`s exactly as to Room-installed services (ADR-0129):

- an invocation that has read conversation plaintext is tainted;
- a tainted invocation gets no network egress unless the conversation's policy grants `plaintext_egress` (Room policy in a Room; the policy of both participants in a 1:1);
- a one-shot, user-initiated, per-action selected-context disclosure is the only exception, and it is handled like mode 1;
- continuous or automated egress of conversation plaintext is export under [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) §7 and needs that authorization.

### 3. ~~Scoped event subscription~~ Trigger filter on a full member

~~A Room may authorize a service to receive selected event classes or selected Topic/context scope without making it a full semantic human member.~~

A private Room cannot deliver a subset of its events to a non-member without either giving it the Room's keys or making some member device a disclosure agent. OM does neither for services (ADR-0129). A Room that wants a service to react only to selected event classes or a Topic installs it as a **full member** (mode 4) and records a **trigger filter** in the installation: which event classes or scopes wake the service and which it may act on. The filter limits automation, not reading. The roster and installation UI must say that the service **can read all Room content**, whatever its filter.

### 4. Full Room service member

A service is admitted as a typed member/recipient and receives all Room plaintext from the epochs of its membership. This is the only way a service or bridge gets continuous access.

For private Rooms:

- its authorized service endpoints become MLS clients;
- membership/control state visibly contains the ServicePrincipal;
- adding it is a high-risk trust-boundary change that may require configured quorum;
- removal performs the normal Room control + MLS removal transition;
- prior plaintext legitimately received cannot be clawed back.

The roster/UI must clearly communicate that automation/service infrastructure can read the Room. Bridges are always full members ([`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) §6).

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
- ~~Room subscription: remove subscription grant;~~ trigger filter change: an ordinary installation update (it never changed what the service could read);
- full private-Room service: revoke installation + remove service endpoints through Room control/MLS;
- endpoint compromise: rotate/revoke endpoint in ServiceControlState, then remove it from every Room (below).

Revocation cannot prove a remote service forgot plaintext it already received. UI and policy must not promise retroactive erasure.

### Endpoint revocation reaches every Room (ADR-0129)

A revoked service endpoint that stays in a Room's MLS tree keeps receiving plaintext. Revocation therefore has a defined path into Rooms, the service counterpart of AccountControlHead gossip and ADR-0128 rule 2:

1. **Head carriage.** A full-member service's installation in GroupControlState records the ServiceControlState head (version and hash) it was admitted under. The service's controllers must deliver every new head to each Room where it is installed, as the revoking transition's own delivery step. Members also gossip `ServiceControlHead` observations inside the Room, as for AccountControlHead. Recipients keep the highest validated head and reject rollback (ADR-0122 rule 6).
2. **Stop and remove.** A member that validates a head in which an endpoint is revoked (or replaced by `migrate_endpoint`) sends no further application messages until a Commit removing that endpoint's leaf is accepted, and proposes the Remove itself.
3. **Who commits, and by when.** Removing a revoked service endpoint needs no capability (basis: membership), like removing a revoked device. The coordinator sequences it; if it is not committed within the Room's `sequencing_timeout` (ADR-0128 rule 1: default 5 minutes, maximum 1 hour), any member may commit it directly. Competing identical removals are ordered by ADR-0121 and deduplicated by `(ServiceID, endpoint_id, ServiceControlState version)`.
4. **Monotone.** The removal is an authority reduction (ADR-0122 rule 1): it rebases onto any later head and is never frozen by a fork.
5. **Bound.** Exposure after revocation is bounded by the time for the first member to receive the head plus `sequencing_timeout`. A Room in which no member receives the head keeps delivering to the endpoint; that is the documented bounded staleness of ADR-0122 rule 8, and the service's controllers close it by delivering the head.

V-011 models this case and the threat register records it (T-073).

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
5. Full Room service access is visible shared trust-boundary state, and it is the only form of continuous access; the roster says the service can read all Room content.
6. Local LLM does not imply network disclosure; network is a separate capability.
7. LLM output/tool requests are untrusted and capability-mediated.
8. Autonomous service posts are authored by the ServicePrincipal.
9. Revocation stops future authorization but cannot claw back prior plaintext.
10. Service-control quorum cannot count authority derived from the target service itself; service installation cannot bootstrap its own controller or endpoint escalation.
11. No OM Project bot cloud/catalog is required for interoperability.
12. Creating a ServiceID requires no central issuer and grants no ambient reach/read/send authority.
13. A service acting for a person remains ServicePrincipal-authored with a scoped `RepresentationGrant`; it never becomes an Account device merely by representation.
14. Every executable extension that reads conversation plaintext is subject to taint; a local user's grant alone never permits plaintext egress (ADR-0129).
15. A revoked service endpoint is removed from every Room within the bound above, by any member if the coordinator does not (ADR-0129).

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0122](../../../decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md): the service, its endpoints and its hosts are ineligible controllers (no self-appointment). Recipients enforce anti-rollback on ServiceControlState. `migrate_endpoint` is one atomic transition, and endpoint revocations are monotone reductions.
- [ADR-0124](../../../decisions/ADR-0124-service-information-flow-and-bridge-provenance.md): the triggering message is a plaintext disclosure. An invocation that has read Room plaintext is tainted, and a tainted invocation gets no network egress without an explicit `plaintext_egress` grant. Content filtering is not a control.
- ADR-0129: ADR-0124's taint rule covers local plugins, personal installations and personal automations; in a 1:1 `plaintext_egress` needs both participants' policy. A service or bridge is either invocation-only (selected context over a 1:1 session) or a full member; "scoped subscription" is a trigger filter on full membership. Revoked service endpoints are removed from every Room with ADR-0128 rule 1 liveness. Autonomous local plugins are `local_component` ServicePrincipals.
