# OM Service Creation, Discovery and Delegated Agency

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:56 PHT
- **Scope:** permissionless ServicePrincipal creation, decentralized discovery/hosting, human/AI authorship boundary, representation grants, easy secure product flows, and anti-spam implications

## 1. Decision summary

OM does **not** have a BotFather, mandatory bot registry, mandatory bot cloud, or protocol-global bot username authority.

Creating a bot/agent/assistant is a local cryptographic operation:

```text
create random ServiceID
 -> create initial ServiceControlState
 -> create signed ServiceDefinition
 -> optionally authorize one or more execution endpoints
 -> optionally publish/share discovery descriptors
 -> install/invoke under explicit capability grants
```

No SeedLife server, global directory, app store, blockchain, phone number, domain, or third-party hosting account is required to create the identity.

The protocol also does not attempt to decide whether an actor is biologically human, AI, partly AI-assisted, or manually operated behind automation. OM distinguishes **authority and agency semantics**, not intelligence.

## 2. Why Telegram's ease is worth learning from—but not its authority root

Telegram's normal bot flow is intentionally easy: a developer talks to `@BotFather`, chooses a name/username and receives a token. Telegram also supports newer managed-bot creation flows through its own MTProto account system. This is excellent product ergonomics, but creation, naming, token issuance, reachability, API policy and account existence ultimately remain under Telegram's infrastructure/control.

OM should preserve the good UX lesson:

> creating a service should feel like creating a contact, not deploying a distributed system.

But OM rejects the architectural lesson that one network operator must issue the service identity.

## 3. Service identity is permissionless

`ServiceID` remains random stable 256-bit provider-independent data.

Initial creation is valid when a conforming client can verify:

```text
ServiceGenesis
  service_id
  control_state_v1
  service_definition_ref
  created_at_hint?          # display/audit only, never identity authority
```

`ServiceControlState v1` is approved by its declared independent controller set. The controller may be:

- one AccountPrincipal;
- several AccountPrincipals;
- organization governance;
- Room/Space governance where policy permits;
- another explicitly defined non-self controller set.

The target service never counts its own endpoint or powers derived from its installation toward creating its own root control.

### No root bot token

OM does not define a single bearer token that permanently equals ownership of a service.

Execution endpoints receive separately revocable credentials authorized by current `ServiceControlState`. A leaked worker credential can be revoked without changing `ServiceID`; moving hosts does not change `ServiceID`; losing a hosting provider does not transfer ownership to that provider.

## 4. Creation and discovery are different systems

Creation is local and permissionless. Discovery is optional and plural.

A service can be found through any combination of:

```text
direct signed service card / QR / om: link
peer-to-peer sharing from an existing installation
Space/community service list
domain-scoped alias, e.g. assistant@example.org
user-selected DirectoryProvider
organization directory
independent public catalog/search provider
creator website/source repository
local-only service list
```

No discovery provider becomes the authority for `ServiceID`.

A directory can rank, label, refuse, delist, moderate or verify entries in **its own** catalog. Another directory can make different choices. Existing installations continue to identify the service cryptographically even if every catalog removes it.

## 5. Human-readable names without a global bot namespace

OM reuses the existing discovery principle:

> identity is cryptographic; names are aliases; discovery is rendezvous.

A service may advertise aliases such as:

```text
weather@community.example
assistant@example.org
@helper               # UI shorthand retaining hidden namespace provenance
```

Aliases are optional claims and can move or expire. They do not define `ServiceID` and cannot silently transfer installed trust if recycled.

There is no protocol-global rule that a service name must end in `bot`, and no central party owns all short names.

## 6. Easy creation UX

The reference product should be able to present a flow like:

```text
Create assistant
  1. Name and icon
  2. Choose how it runs
       - On this device
       - Use a hosting provider
       - Connect my own endpoint
  3. Choose what it may do
       - Respond when invoked
       - Read selected context
       - Post autonomously in chosen Rooms
       - Optional network/tool permissions
  4. Optional public alias / directory listing
  5. Create
```

Under the surface, the client generates `ServiceID`, control state, endpoint credentials and signed definitions.

For a nontechnical user, a hosting provider may offer one-click deployment, templates and uptime. The host is an implementation/service provider, not the issuer of the service identity.

A creator may later move from Host A to Host B by authorizing a new endpoint and revoking the old endpoint, preserving `ServiceID`, installs, reputation/provenance and Room grants where still valid.

## 7. Cheap creation must not mean cheap unsolicited reach

Permissionless identity creation makes Sybil creation inexpensive. OM does **not** solve this by making identity creation centrally scarce.

Instead:

> **identity is cheap; unsolicited delivery is permissioned/rate-limited.**

A newly created ServicePrincipal has no ambient right to:

- discover private users;
- open arbitrary conversations;
- join Rooms;
- read transcripts;
- send bulk messages;
- obtain aliases in another party's namespace;
- appear in a directory;
- use another provider's compute/relay resources.

IntroGate, Room admission, user consent, provider quotas, directory reputation and local blocking remain the places where abuse cost is imposed. Service creation itself remains free of a universal gatekeeper.

## 8. OM does not define a human-versus-AI truth bit

The following are all possible:

- a human writes every character manually;
- a human uses autocomplete or accessibility assistance;
- a human asks a local LLM for a draft, edits it and presses Send;
- a remote LLM drafts a message and the human confirms it;
- a personal agent autonomously replies under delegated authority;
- a company account has human operators plus automation;
- a ServicePrincipal is manually operated by a person;
- an AccountPrincipal uses extensive automation locally.

Trying to infer which messages are “really human” is unreliable and would create surveillance and false assurances.

OM therefore defines protocol semantics from **who possessed authority to cause the canonical action**, not from how content was cognitively produced.

## 9. Three authorship/agency cases

### 9.1 Account-authored / user-confirmed

If an authorized AccountPrincipal device reviews/accepts the actual outgoing action and sends it under Account authority, the event may simply be authored by that AccountPrincipal.

OM does not require disclosure that spellcheck, translation, an LLM, accessibility software or another drafting tool helped produce the text.

### 9.2 Service-authored autonomous action

If a service has authority to act without per-action Account confirmation, the canonical event is authored by its `ServicePrincipal`.

The client presents it as automation/service content. The protocol does not need to claim whether the service uses an LLM, deterministic code or human workers.

### 9.3 Service acting on behalf of an AccountPrincipal

A personal agent may be explicitly delegated authority to represent a person without being allowed to impersonate the person's cryptographic authorship.

OM defines a revocable `RepresentationGrant`:

```text
RepresentationGrant
  grant_id
  represented_account_id
  service_id
  scope_refs[]
  allowed_actions[]
  optional_constraints
  issued_by_account_authority
  not_before?
  expires_at?
  revocation_state
```

An autonomous event remains signed/authored by the `ServicePrincipal` and may carry:

```text
representation
  represented_account_id
  representation_grant_ref
```

The UI can render:

```text
Alice · via Personal Assistant
```

or another clear equivalent.

The service does **not** become an Account device and cannot vote in `AccountControlState` merely because it represents Alice in a Room.

## 10. Representation is scoped, not impersonation

A representation grant may allow, for example:

- reply in one Room;
- manage a calendar integration;
- post a daily status;
- answer direct messages from an allowlist;
- draft but not send;
- moderate within a bounded Space role.

It does not imply:

- account recovery;
- device enrollment;
- alias transfer;
- key export;
- voting on AccountControlState;
- broad access to all Rooms;
- permission to re-delegate itself unless separately authorized.

Recipients can distinguish:

```text
Alice                    # AccountPrincipal authored
Alice · via Assistant    # ServicePrincipal + valid RepresentationGrant
Assistant                # ServicePrincipal acting as itself
```

This remains meaningful even if the assistant is sometimes driven by a human and Alice sometimes uses AI directly.

## 11. Hosting is replaceable

A `ServiceEndpointDescriptor` may advertise one or more execution locations/capabilities:

```text
ServiceEndpointDescriptor
  service_id
  endpoint_id
  execution_mode           # local_component / remote_om / remote_web_service
  rendezvous/endpoints[]
  protocol_versions[]
  capability_hints[]
  expiry
  authorization_proof_from_ServiceControlState
```

Endpoint descriptors are routable/replaceable state, not identity.

A remote `ServiceHost` may:

- deploy service code/model;
- keep a worker online;
- expose webhooks/OM-native endpoints;
- meter compute;
- charge money;
- impose its own acceptable-use policy.

It may not silently become controller of the ServiceID unless the user explicitly changes `ServiceControlState` to grant that authority.

## 12. Catalogs, verification and reputation are plural evidence

The reference client may make discovery easy by shipping a default directory/catalog set, but it must preserve source provenance and allow alternatives.

Possible evidence includes:

- controller signature continuity;
- domain alias proof;
- source-code/release signature;
- independent catalog review;
- community endorsement;
- provider attestation;
- user-local trust/history;
- transparency logs/witnesses.

No single “verified bot” badge becomes a universal OM trust oracle.

Signatures mean provenance/control continuity, not truthfulness, safety or endorsement.

## 13. Installation remains the security choke point

Permissionless service creation is safe only because creation itself grants no receiver-side privilege.

At installation/invocation, the client shows the actual requested capabilities and data-access mode. The architecture from [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) remains authoritative:

- selected-context invocation is default;
- local plaintext access and network egress are separate;
- scoped subscription is explicit;
- full Room membership is a visible plaintext-recipient change;
- draft and send are distinct capabilities;
- tool calls are host-mediated;
- capability expansion after update requires approval.

## 14. Bot-to-bot and agent-to-agent systems

No central bot registry is needed for service-to-service interaction. Services address one another by cryptographic identities/installations and typed capabilities.

Automation lineage/hop/rate controls remain mandatory. A service cannot make another service process arbitrary output merely by mentioning it unless the receiving installation's trigger policy permits it.

## 15. Security invariants

1. Creating a ServiceID requires no central issuer.
2. No directory/catalog/host owns or can replace a ServiceID merely by listing/hosting it.
3. Endpoint compromise is recoverable through ServiceControlState rotation.
4. Service creation grants no unsolicited-delivery or plaintext rights.
5. OM never promises to detect AI-generated content.
6. User-confirmed Account actions may remain Account-authored regardless of drafting assistance.
7. Autonomous delegated actions remain ServicePrincipal-authored.
8. Representation grants are visible, scoped, revocable and cannot bootstrap Account control.
9. Human-readable aliases are replaceable claims, not service identity.
10. No OM Project bot store/cloud/registry is required for protocol correctness.

## 16. Verification obligation

V-011 must now include:

- permissionless ServiceGenesis verification;
- controller/no-self-bootstrap transitions;
- endpoint-host migration/revocation;
- representation grants and attempted impersonation;
- distinction between Account-confirmed and autonomous service actions;
- installation/data-access boundaries;
- catalog/alias replacement without identity replacement;
- spam/Sybil tests proving cheap creation does not imply unsolicited delivery authority.
