# v011-services: service authority model and tool-capability broker

Feasibility prototype for V-011. Not product code. Not for production.

Written by a Claude agent from `SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md` and
`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`. The design package was authored by a ChatGPT design agent and
Claude agents; ADR-0117 (proposed) decides whether an AI reviewer counts as independent. The work is in TLA+ and Rust
and is not derived from `verification/bounded/authority_models.py`.

## (a) Authority model: [`ServiceAuthority.tla`](../../tla/ServiceAuthority.tla)

The model covers:

- a `ServiceControlState` with independent account controllers, a threshold, and endpoint credentials;
- host migration from e1 on host A to e2 on host B;
- recipients whose view of the control state lags;
- a hostile former host that keeps its credential and replays old state;
- a service that holds a Room role through its installation;
- the Room's MLS roster of service endpoints;
- alias recycling and catalog replacement.

The configs are listed in `verification/tla/models-v011.json` and take about 35 s:

- **3 scenarios pass:** 199,108, 203,008 and 32,700 distinct states. They include two liveness properties. The first
  says a revocation eventually reaches every recipient and the Room roster. The second says a sealed revocation always
  takes effect.
- **6 mutations are caught:** self-bootstrap through the Room role or endpoints, host approvals counting, rollback
  acceptance, installs following a recycled alias, a host adding an endpoint to the roster without a visible Room
  transition, and a fork blocking revocation.
- **3 golden vectors:** migration completes, the bounded window in which a stale recipient still accepts a revoked
  endpoint, and an alias-recycled install binding the new ServiceID.

## (b) Tool-capability broker: `broker/` (Rust)

- `broker.rs` is the only authority. It exposes typed host tools, each mapped to a required capability. Tools such as
  keys, shell, files, governance, bridges and send-as-account have no host tool at all.
- **Scope checks:** the target room must be in scope, and `read_context` sees only the disclosure bundle (or the
  service's own Room when it is a full member).
- **Taint and egress:** disclosed plaintext taints the invocation. A tainted invocation cannot use the network unless
  it holds `plaintext_egress`.
- **URL checks:** https only, allowlisted host, no userinfo, no port.
- **Authorship:** messages are always authored by the ServicePrincipal. `on_behalf_of` needs a matching
  `RepresentationGrant`.
- **Loop limits:** a hop limit, a per-root send budget, a per-invocation call budget, and repeated-output detection.
  Service-originated mentions trigger other bots only when they opted in.
- **Capability requests** are queued for approval and never granted by the broker.
- `model.rs` is a deterministic stub "LLM" that obeys *every* directive it sees: in the prompt, in messages, in
  documents, base64 or double-base64 encoded, and inside tool results (chained). So the results do not depend on model
  behavior.
- `oracle.rs` independently re-checks the effect log against the installation records.
- [`injection-corpus.json`](injection-corpus.json) has 48 cases: 13 direct, 7 indirect-via-message, 8 via-document,
  9 chained, 5 loop and 6 benign controls. All pass.
- A random-model sweep sends 10,000 invocations (65,177 arbitrary tool requests) with 0 oracle violations.
- Loops: two bots that answer each other stop after 6 messages, versus 400 (the harness cap) with broker limits
  disabled.

Run `./run.sh` (about 45 s). It needs Rust 1.98.1 and `serde`, `serde_json` and `base64` from `Cargo.lock`.
`./run.sh --long` runs a 100,000-invocation sweep. Results go to `verification/results/v011-services-latest.json`.

## Design gaps found (proposed design updates)

1. **Honest host migration freezes the service.** Authorizing e2 and revoking e1 are sealed concurrently, and the
   same-parent fork rule then freezes control and drops the revocation. *Proposal (modelled):* endpoint revocations
   only reduce authority, so they rebase onto the new head, never count as a fork, and stay executable while control
   is frozen. Alternatively, define migration as one atomic transition.
2. **No-self-bootstrap covers approvals but not appointment.** The docs forbid the service counting toward its own
   quorum, but they do not forbid legitimate controllers *appointing* the service, its endpoint or its host as a
   controller. That appointment makes the service self-governing. *Proposal (modelled):* the service, its endpoints and
   its hosts are ineligible controller principals.
3. **Recipients need anti-rollback for ServiceControlState.** Without it, a former host can replay an older state in
   which its endpoint is active (`mut_accept_rollback`). The docs imply this but never state it for services.
4. **Stale-recipient window.** A recipient that has not yet synced accepts a revoked endpoint (`trap_stale_window`).
   Document this as bounded staleness, the same as the Space-mirror race in V-008.
5. **The triggering message is disclosed plaintext.** A mention-triggered bot receives Room plaintext even in a
   "respond when mentioned" mode. It must count as a disclosure for egress control. Before this fix, the harness had
   let a mention-triggered bot fetch with Room text.
6. **Plaintext-read vs. egress needs an information-flow rule, not content filtering.** Exfiltration can be encoded
   (base64, URL paths). The broker therefore denies all network egress from a tainted invocation unless
   `plaintext_egress` is granted. *Proposal:* state this rule normatively. The docs say only that "network is a
   separate capability".

Registry note: the new files need `python3 tools/om_registry.py sync` (this agent may not edit `machine/`).
