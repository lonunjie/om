# OM Agent Instructions

OM is SeedLife's open-source, operatorless, end-to-end-encrypted messenger. This repository is the **single source of
truth** for its design: documents, decisions, machine ledgers, specs and verification evidence. It is private on GitHub
until the operator makes it public. Write everything as if a stranger will read it, because they will.

**Engineering is NOT authorized.** Research, specification, formal models and bounded feasibility prototypes
(`verification/`) are allowed. Product code is not. The architecture rules below say exactly what is locked.

**Current verification handoff:** follow [`verification/CLAUDE_OPUS_5_5_HANDOFF.md`](verification/CLAUDE_OPUS_5_5_HANDOFF.md) and ADR-0134. The verification executor must exhaust repo-local gate work before requesting independent-review-only sign-off; do not bounce the repository between agents one criterion at a time.

## Part 1: Working in this repository

### Where things live

| Path | Purpose |
|---|---|
| [`SNAPSHOT.md`](SNAPSHOT.md) | current checkpoint state; update it with every checkpoint |
| `docs/constitution/`, `docs/overview/`, `docs/architecture/<domain>/`, `docs/process/`, `docs/threat-model/` | canonical design prose; basenames are unique, so refer to a doc as `` `NAME.md` `` and `tools/om_docs.py fmt` turns it into a link |
| `docs/reviews/`, `docs/checkpoints/`, `docs/research/`, `docs/reports/` | dated records: reviews, checkpoint records, references, working reports |
| `decisions/` | ADRs; every ADR is listed in [`decisions/INDEX.md`](decisions/INDEX.md) |
| `spec/` | normative schemas (`spec/protocol/`, `spec/client/`); frozen CDDL/registries go here too |
| `machine/` | machine truth (contract, file registry, ledgers, traceability, threats) |
| `verification/` | evidence: `tla/`, `bounded/`, `cbor/`, `prototypes/<gate>/`, `vectors/`, `results/` |
| `tools/` | [`validate_architecture.py`](tools/validate_architecture.py), [`om_docs.py`](tools/om_docs.py), [`om_drop.py`](tools/om_drop.py), [`build_manifest.py`](tools/build_manifest.py), [`layout.json`](tools/layout.json) |
| `design-drops/` | **gitignored** drop zone for design zips going out to, or coming in from, other agents |

A new file must be added to [`machine/file_registry.json`](machine/file_registry.json), or the validator fails. New doc folders need a line in the table
in [`docs/README.md`](docs/README.md).

### Checks (run before every commit)

```sh
make verify      # validator + docs check + phase-1 evidence + TLC
make check       # fast subset: validator + docs link check
```

### Design drops (zip <-> repository)

External design agents (for example the ChatGPT design agent) work on zips; this repository is canonical.

```sh
python3 tools/om_drop.py export                   # HEAD -> design-drops/om-design-<stamp>-<sha>.zip
python3 tools/om_drop.py import design-drops/X.zip # merge a returned drop into the current branch
```

- `export` packages a committed ref (never uncommitted work). It adds [`OM_EXPORT.json`](OM_EXPORT.json) (the base commit) and a
  [`MANIFEST.json`](MANIFEST.json), and validates the result before zipping.
- `import` applies the drop on a branch cut from the drop's base, commits it, tags it `checkpoint/<stamp>`, and
  **merges** it into the current branch, so concurrent repository work is kept and real conflicts are surfaced. It
  accepts both this layout and the legacy flat layout (drops up to 2026-09-24 02:29), mapped by [`tools/layout.json`](tools/layout.json).
  It then runs `om_docs.py fmt` and the validator.

**If you are an agent editing an exported drop:** edit files in place, keeping this layout. Do not create `history/`
or nest zips; git is the history. Leave [`OM_EXPORT.json`](OM_EXPORT.json) unchanged. Record the checkpoint in [`SNAPSHOT.md`](SNAPSHOT.md),
[`CHANGELOG.md`](CHANGELOG.md) and a new `docs/checkpoints/CHECKPOINT_YYYYMMDD_HHMM.md`. Register new files, run
`python3 tools/validate_architecture.py` and `python3 tools/om_docs.py check`, then zip the folder as
`om-design-YYYYMMDD-HHMM-PHT.zip`.

### Workspace rules (lonunjie control plane)

This repository lives at `~/dig/lonunjie/om`, inside the SeedLife control plane (`~/dig/lonunjie`, repo `lonunjie/hq`).
When working there, read `../AGENTS.md` first for org-wide rules and registry.

- **Push cadence: auto-push.** Push to `origin` after each verified commit. Never force-push without explicit approval.
- **Memory:** there is no repo-local `memory/`. The shared store is `../memory/`; OM facts go in `../memory/topics/om-*.md`.
- **Reports:** dated writeups go in `docs/reports/YYYY-MM-DD-<slug>.md`, and move to `docs/reports/completed/` once
  executed (see [`docs/reports/README.md`](docs/reports/README.md)).
- **Artifacts stay local:** no hosted artifacts, gists or pastebins unless the operator asks for a shareable link.
- **Secrets:** never commit secrets or credentials.
- CLI tools before GUI tools; small single-purpose tools; shared logic in `tools/omlib/`.

## Part 2: Architecture rules (from the design package; edit with care)

### Mission

Build OM as a fast, simple, reliable secure messenger whose identity and canonical conversations survive provider and cryptographic replacement.


### Read before any work

1. [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) — constitutional intent.
2. [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](docs/process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md) — executable process/phase lock.
3. [`machine/repository_contract.json`](machine/repository_contract.json) — machine authority for checkpoint state.

**Current rule:** product engineering is not authorized. Research, specification, formal modeling and bounded feasibility experiments may be used to harden architecture, but do not begin the product implementation plan or let a prototype silently freeze a wire/security choice.

SeedLife/lonunjie is the initial steward, not protocol authority. Businesses may participate commercially but receive no hidden/mandatory protocol privilege.

### Absolute rules

1. Provider names never become wire-protocol semantics.
2. AccountID is never used as a relay/media routing address.
3. No novel cryptographic primitive/protocol unless an explicit research ADR says otherwise; prefer reviewed standards/designs.
4. Crypto agility means approved **CryptoProfiles**, not arbitrary primitive negotiation.
5. Crypto upgrades are automatic and normally invisible; never add a user-facing cipher selection flow for ordinary users.
6. Never silently downgrade an established relationship/group/transport security level.
7. Conversation semantics are transport-agnostic; SMS/MMS/RCS are future adapters.
8. Media E2EE must remain independent of direct/TURN/SFU route.
9. Use infrastructure only when it adds value; prefer LAN/P2P for calls when privacy policy permits.
10. Unknown senders are constrained before expensive work and before direct calls.
11. Open-source clients are adversarially modifiable; enforce abuse/resource limits at recipient/provider boundaries.
12. No global trust score is required for protocol validity.
13. Storage classes remain separate; do not turn relay DB or R2 into universal state.
14. Provider migration and restore require conformance tests.
15. Public specs/test vectors define compatibility, not official-client private behavior.
16. Normal users must not need infrastructure/crypto vocabulary.
17. Advanced provider choices live behind policy interfaces; default is automatic.
18. Cloudflare is a reference implementation, never a dependency.
19. Store/carrier approval work must not block OM-native product delivery.
20. Every architecture addition must answer: **Where did authority go?**

### Core interfaces

At minimum:

- `CryptoProvider` / `CryptoProfileRegistry`
- `DirectoryProvider`
- `KeyMaterialProvider`
- `RelayProvider`
- `PushProvider`
- `BlobStore`
- `BackupStore`
- `RecoveryStore`
- `StoragePool`
- `MediaProvider`
- `RoutePolicy`
- `TransportAdapter`
- scoped bot/service capabilities

### User-facing design test

For any implementation proposal ask:

> Could a user who does not know how to configure Wi-Fi still use this correctly and securely?

If no, the complexity probably belongs in automatic policy/provider code or advanced settings.

### Change discipline

Architecture change requires:

- update relevant canonical doc;
- ADR under `decisions/`;
- machine decision/requirement/open-question records;
- threat-model consequences;
- negative/conformance tests;
- migration/backward-compatibility story when wire/storage state changes.

### External facts

Cloud/provider quotas, app-store policies, drafts, APIs, library support, and pricing are volatile. Verify current official sources before implementation or budgeting. Do not encode current free-tier numbers into protocol behavior.

### Discovery/resilience checkpoint

Treat [`DISCOVERY_FABRIC.md`](docs/architecture/discovery/DISCOVERY_FABRIC.md), [`INVITES_INTROGATE_AND_SPAM.md`](docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](docs/architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](docs/architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md), and [`CENSORSHIP_AND_NETWORK_RESILIENCE.md`](docs/architecture/network/CENSORSHIP_AND_NETWORK_RESILIENCE.md) as canonical. Do not introduce a protocol-global username root, publish stable relationship mailboxes in directories, reuse a capability across multiple relays, or broadcast stable identity on LAN. New network/circumvention transports must remain below canonical OM semantics.

### 21:32 checkpoint implementation rules

- Do not implement account authority using chat timestamps, CRDT merge, or last-write-wins.
- Do not let one existing controller promote arbitrary new control devices without parent-state constitutional authorization.
- Do not make backup content authoritative for device/recovery/security state.
- Do not require R2/payment setup for the v1 zero-payment personal Cloudflare bootstrap.
- Do not upload/hash the OS address book for discovery.
- Do not place sender/message plaintext in APNs/FCM payloads.
- Do not interpret Recommended/Hardened/Maximum Privacy as weaker/stronger cipher selections.
- Do not ship a public binary whose trust reduces to one mutable CI workflow/one online update key if the documented trust-root gate is incomplete.

### 2026-09-22 operatorless constraints

Agents must preserve these invariants:

- do not add a mandatory OM-operated control plane merely for convenience;
- do not make a reference client/distribution technically privileged;
- treat provisioning, push, reporting and managed hosting as replaceable provider/distribution capabilities;
- preserve Creator-Disappearance Gate;
- do not infer “no legal obligations” from decentralization; flag jurisdiction-specific operator-role questions instead.

### 2026-09-22 shared UI constraints

- Official PWA/iOS/Android/desktop product screens belong in shared `om-ui`; do not create independent product/security semantics in platform shells.
- Platform adapters expose capabilities/system surfaces, not messaging/security business logic.
- UI components send typed intents to Core and render returned state; never mutate cryptographic/control/provider state directly.
- Motion must be semantic, interruptible where interactive, and reduced-motion compatible; never delay core actions for animation.
- Themes are declarative data-only; no arbitrary CSS/HTML/script/WASM/native code or remote tracking assets.
- Themes cannot suppress/spoof critical security UI, focus, accessibility or external-link destination semantics.
- Accessibility regressions in shared primitives are release-blocking.
- Default UI prioritizes ergonomics/functionality and low visual noise; advanced capabilities use progressive disclosure.

### Future engineering constraints — retained, currently inactive

- Read [`ENGINEER_START_HERE.md`](docs/process/ENGINEER_START_HERE.md) before coding.
- Dev/test crypto MUST be unmistakably non-production and impossible to ship in a production configuration.
- Production crypto integration is blocked by [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md); never homebrew primitives or an ad-hoc hybrid combiner.
- Use strict deterministic CBOR for canonical signed/wire objects; never sign JSON serialization.
- Treat PWA ClientHost/origin as an endpoint trust boundary; provider migration is device linking/transfer, not IndexedDB copying.
- Namespace local state by IdentityProfile from the first migration/schema even if v1 UI shows one identity.
- Do not create a hidden device/person identifier linking profiles.
- Rich messages use RichTextV1 semantic data only; no message/theme HTML/script execution.
- Decrypted files remain hostile; validate before preview and fuzz parsers.
- Default diagnostics stay local and redact plaintext, keys, capabilities, auth tokens and push endpoints.
- AccountControlState implementation does not merge until the formal model/invariants have corresponding CI coverage.
- Every built-in theme has light+dark variants and Auto mode; theme/profile appearance cannot modify security semantics.

### 2026-09-22 23:02 architecture-hardening rules

- Canonical AccountID is random 256-bit stable data; never derive it from a device key or algorithm.
- `AccountControlState` is the canonical account-authority object; do not resurrect a competing generic account-authority state.
- Treat asynchronous handshake/MLS key publication as a first-class replaceable provider role; do not assume a crypto library/server hides it safely.
- Do not mirror the same consumable one-time prekey/KeyPackage across independent providers without a reviewed atomic-consumption construction; default to disjoint provider pools.
- 1:1 multi-device session management is a distinct state machine above the ratchet.
- MLS does not choose a canonical same-epoch Commit for OM; define and test application ordering.
- OM group-policy membership/role transitions and MLS epoch transitions must be cryptographically/atomically bound.
- Relay migration must document the all-known-routes-dead case; do not publish ordinary private relay capabilities globally to paper over it.
- Blob identity is stable; provider locators are mutable authenticated repair state.
- Every TTL/time field must declare its clock class and skew/failure semantics. HLC is never constitutional authority.
- PWA self-hosting requires an update/deployment lifecycle; same-origin JavaScript is not an independent trust root after origin compromise.
- Link-preview fetchers are SSRF-sensitive infrastructure; private/link-local/metadata destinations, rebinding and redirect-to-private paths must be blocked.
- Provider `failure_domain` metadata is an operational hint, never cryptographic proof of independence.
- PWA LAN discovery/reachability is platform-capability-gated; native mDNS behavior is not promised universally on web.
- Licensing follows [`LICENSING_AND_CONTRIBUTION_POLICY.md`](docs/constitution/LICENSING_AND_CONTRIBUTION_POLICY.md); normal contributions use DCO/inbound-equals-outbound, not broad copyright assignment.
- Run `python3 tools/validate_architecture.py` before accepting a checkpoint.

### 2026-09-23 architecture-resolution rules

- The former Q-001..Q-061 choices are resolved in [`ARCHITECTURE_RESOLUTIONS_20260923.md`](docs/reviews/ARCHITECTURE_RESOLUTIONS_20260923.md); do not reopen them casually because an implementation is easier.
- Product engineering is still **not authorized**.
- Before proposing engineering, clear every open blocking item in [`machine/architecture_design_queue.jsonl`](machine/architecture_design_queue.jsonl) and pass every blocking gate in [`machine/architecture_verification_gates.jsonl`](machine/architecture_verification_gates.jsonl).
- [`machine/public_release_gates.jsonl`](machine/public_release_gates.jsonl) applies to the affected public service/distribution/claim; do not silently turn one of those roles into mandatory protocol infrastructure.
- Preserve all six OM meanings plus 0M in [`NAMING.md`](docs/constitution/NAMING.md) and the rule that capability support does not imply SeedLife operation.
- Do not introduce OM-operated mandatory payment, marketplace, content catalog, bot cloud, public feed, account service or provider merely to make a feature convenient.

### 2026-09-24 hardening-review rules

- Read [`HARDENING_REVIEW_20260924.md`](docs/reviews/HARDENING_REVIEW_20260924.md) for the findings register (F-01..F-17) and their fixes.
- Engineering unlock is scope-granular ([`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md)). Never set a scope `authorized: true` without an ADR naming it and passed or ADR-waived `pre_engineering_status` on every gate listing it. Global `engineering_authorized` stays `false` until then.
- Never mark a gate `passed` on evidence produced solely by the agent/author who designed the thing under test; follow the gate's `independence` field.
- Under ADR-0134, `pre_engineering_criteria` / `criteria_met` are the canonical checklist; free-text "remaining" notes are not. Complete all non-review evidence first, then batch review-only blockers.
- For any gate with `interface_impact`, use its explicit `feasibility_criteria`; do not require full later-wave passage merely to unlock an earlier scope.
- Every new requirement needs a [`machine/requirement_traceability.json`](machine/requirement_traceability.json) entry, and every new threat needs a [`machine/threat_register.jsonl`](machine/threat_register.jsonl) entry.
- Do not claim post-quantum confidentiality for v1 groups or group calls.
- Do not make any single cloud vendor the only zero-signup onboarding path. T0 sponsorship and T1 hosted Homes are first-class.
- Device linking must never be silent. Do not design hidden "duress"/deniable profiles without a research ADR.
- There is no remote kill switch. Security response uses signed SecurityAdvisories and peer-local floors.
