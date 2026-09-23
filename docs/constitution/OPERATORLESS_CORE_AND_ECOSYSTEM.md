# Operatorless Core and Ecosystem Model

## Status

Canonical architecture checkpoint: **2026-09-22 07:34 PHT**.

## 1. Goal

OM should be able to exist as a protocol and open-source software project without requiring an OM company, OM account service, OM relay fleet, OM master directory, OM push gateway, OM recovery service, or OM abuse desk.

The project may publish software and specifications. Users and organizations operate their own endpoints. Optional businesses may later sell convenience, but no such business becomes a protocol authority.

This is an architectural property, not a claim that developers, distributors, users, or providers have no legal obligations in their own jurisdictions.

## 2. Separate the layers

```text
OM Protocol / Commons
  specs
  crypto/state core
  schemas and registries
  conformance tests
  reference source
       |
       +-------------------------------+
       |                               |
User-owned deployments           Client distributions
  personal relay/IntroGate         reference PWA
  personal discovery               native Android/iOS later
  optional blob storage            third-party clients
  personal Web Push sender         enterprise clients
       |                               |
       +-------------------------------+
                       |
                Optional providers
          managed relay/storage/media
          directory/enterprise/support
```

### OM Protocol / Commons

Owns or stewards only public artifacts:

- protocol specifications;
- portable OM Core implementation;
- test vectors and conformance suites;
- reference client source;
- security advisories;
- release/update metadata for a particular reference distribution;
- documentation and deployment templates.

It does **not** need to possess:

- user accounts;
- AccountIDs or private keys;
- canonical user directories;
- message plaintext;
- universal traffic logs;
- personal relay mailboxes;
- user contact graphs;
- backup keys;
- a universal blocklist;
- an abuse-report database.

## 3. User deployments

A personal user may own a Cloudflare account or another provider account and deploy:

```text
PersonalDiscoveryPublisher
IntroGate
Relay/mailbox
bounded temporary blob spool
optional BlobStore
optional Web Push sender
```

The provider account is the user's infrastructure account. OM code is merely one deployable implementation.

Cloudflare `workers.dev` is a convenient v1 locator, not an OM identity namespace.

## 4. No mandatory OM network operator

Existing accepted communication must not require contacting an OM-operated service.

After relationship establishment, Alice and Bob should be able to communicate using their chosen relays/transports even if:

- the original OM author disappears;
- the original project website goes offline;
- a source forge removes the repository;
- a reference-client distributor shuts down;
- an optional convenience provider ceases operation.

This is the **Creator-Disappearance Gate**.

## 5. Convenience businesses are allowed, but never privileged

A future company may offer:

- one-click managed relay hosting;
- managed encrypted blob storage;
- TURN/SFU capacity;
- enterprise/domain directories;
- managed push for a native distribution;
- support/SLA/MDM/compliance tooling;
- account migration assistance.

Such a provider may be easier to use than self-hosting, but it receives **no protocol privilege**. A user must be able to migrate away without replacing AccountID or canonical conversation meaning.

## 6. Foundation/company governance

An Ethereum-Foundation-like stewardship organization may exist later, but OM must not require one to remain functional.

A foundation may fund:

- audits;
- standards work;
- public test infrastructure;
- reference implementations;
- grants and interoperability work.

It must not become the mandatory identity, relay, directory, recovery, or policy authority.

## 7. Project terminology

Use these terms precisely:

- **OM Protocol** — interoperable technical specification.
- **OM Core** — portable reference implementation of protocol/security/state logic.
- **OM Project** — source, specs, tests, reference distributions and governance activity.
- **OM deployment** — a user's or organization's independently operated endpoint/infrastructure.
- **OM provider** — optional operator of a replaceable service.
- **OM distribution** — a particular packaged client/release channel.

Avoid implying that there is one centrally operated “OM Network.” OM is an interoperable ecosystem of independent endpoints.

## 8. Legal posture principle

The architecture aims for **scoped custody + scoped responsibility**:

- a relay operator is responsible for the relay it operates;
- a directory operator is responsible for its directory;
- a client distributor is responsible for its distribution/update/store obligations;
- a managed provider is responsible for the service it sells;
- the protocol project does not obtain operational custody merely because implementations conform to OM.

Exact legal classification is jurisdiction-specific and must not be inferred from architecture alone.
## 2026-09-22 anti-capture constitution

`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`, `LICENSING_AND_CONTRIBUTION_POLICY.md`, and `PROJECT_STEWARDSHIP_AND_CONTINUITY.md` are canonical.

SeedLife/lonunjie is the initial project steward, not a protocol service or identity authority. Commercial providers/clients are welcome but receive no hidden compatibility privilege. The legal/technical right to implement and fork published OM versions must survive capture or disappearance of the canonical steward.


## 10. 2026-09-23 08:21 metadata/safety/longevity refinement

Operatorless does not mean providers learn zero metadata or communities lack moderation. It means no provider/safety/project role becomes universally mandatory.

- metadata exposure is role/path-specific and documented by `METADATA_THREAT_MODEL.md`;
- optional safety labelers/advisory services are replaceable subscriber-selected policy inputs, never protocol authority;
- published protocol generations are mirrorable/interpretable without a live SeedLife/OM Project service;
- a captured/disappeared canonical project can be forked operationally without acquiring or rewriting user identity authority.
