---------------------------- MODULE ServiceAuthority ----------------------------
(***************************************************************************)
(* V-011 (a) feasibility model: ServiceControlState endpoint authority     *)
(* under host migration, endpoint revocation races with lagging            *)
(* recipients, the no-self-bootstrap rule, full-Room service membership    *)
(* (no hidden plaintext recipient) and alias/catalog replacement.          *)
(*                                                                         *)
(* Derived from the public design prose, not from any existing OM model:   *)
(*   SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md: "ServiceControlState"     *)
(*     (no-self-bootstrap), "Service endpoint credentials", "Four          *)
(*     data-access modes" (full Room service member), "Revocation",        *)
(*     security invariants 5, 9, 10                                        *)
(*   SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md sections 3 (no     *)
(*     root bot token), 5 (aliases are replaceable claims), 11 (hosting is *)
(*     replaceable), 15 (invariants 2, 3, 9)                               *)
(* Written by a Claude agent (feasibility evidence, not product code).     *)
(***************************************************************************)
EXTENDS Integers, FiniteSets

CONSTANTS
  Accounts,       \* independent controller principals
  Endpoints,      \* service endpoint credentials (e1 on host A, e2 on host B, ...)
  FirstEp,        \* the endpoint authorised by ServiceGenesis
  Host,           \* [Endpoints -> host name]
  Recipients,     \* honest peers verifying service-authored events
  Menu,           \* tuple of control templates [kind, who]
  InitCtrl, InitThr,
  MaxSteps, MaxVersion,
  Mutation        \* "none" | "self_bootstrap" | "host_counts" | "accept_rollback" | "alias_binds_trust"
                  \* | "host_adds_endpoint" | "freeze_blocks_revocation"

SVC == "svc"

VARIABLES
  ctrl, thr, ep, version, props, frozen,
  svcHasRoomRole,   \* the service obtained a Room role via its installation
  view,             \* [Recipients -> accepted ServiceControlState version]
  epAt,             \* [0..MaxVersion -> [Endpoints -> status]] endpoint status per version
  accepted,         \* set of <<recipient, endpoint, view version>> accepted service events
  roster,           \* service endpoints in the Room's MLS roster
  visibleAdds,      \* ghost: endpoints added through a visible Room commit
  alias,            \* the ServiceID an alias currently points to
  installs,         \* set of [sid, via] installations
  counted,          \* approvals counted by the last executed transition
  steps

vars == <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, view, epAt, accepted, roster,
          visibleAdds, alias, installs, counted, steps>>

Env == steps < MaxSteps /\ steps' = steps + 1

\* No-self-bootstrap: only independently authorised controllers count. The
\* service's own endpoints, its host and any authority derived from the
\* service's installation (its Room role) never count.
Approvers(A) ==
  IF Mutation = "self_bootstrap"
  THEN (A \cap ctrl) \cup (IF svcHasRoomRole THEN A \cap ({SVC} \cup Endpoints) ELSE {})
  ELSE IF Mutation = "host_counts" THEN (A \cap ctrl) \cup (A \cap {Host[e] : e \in Endpoints})
  ELSE A \cap ctrl \cap Accounts

Init ==
  /\ ctrl = InitCtrl /\ thr = InitThr
  /\ ep = [e \in Endpoints |-> IF e = FirstEp THEN "active" ELSE "none"]
  /\ version = 0 /\ props = {} /\ frozen = FALSE
  /\ svcHasRoomRole = FALSE
  /\ view = [r \in Recipients |-> 0]
  /\ epAt = [v \in 0..MaxVersion |-> [e \in Endpoints |-> IF e = FirstEp THEN "active" ELSE "none"]]
  /\ accepted = {}
  /\ roster = {FirstEp} /\ visibleAdds = roster
  /\ alias = "S" /\ installs = {[sid |-> "S", via |-> "alias"]}
  /\ counted = {}
  /\ steps = 0

\* Any principal may sign an approval (including the service's endpoints or a
\* hostile host); only Approvers() count.
\* A template is only worth approving if it would change the control state.
Effective(m) ==
  CASE m.kind = "authorize_endpoint" -> ep[m.who] = "none"
    [] m.kind = "revoke_endpoint"    -> ep[m.who] # "revoked"
    [] m.kind = "add_controller"     -> m.who \notin ctrl
    [] m.kind = "remove_controller"  -> m.who \in ctrl

Approve(p, t) ==
  /\ Env /\ (~frozen \/ Menu[t].kind = "revoke_endpoint")
  /\ Effective(Menu[t])
  /\ LET cur == {x \in props : x.t = t /\ x.base = version} IN
       IF cur = {} THEN props' = props \cup {[t |-> t, base |-> version, appr |-> {p}]}
       ELSE LET x == CHOOSE y \in cur : TRUE IN
              /\ p \notin x.appr
              /\ props' = (props \ {x}) \cup {[x EXCEPT !.appr = @ \cup {p}]}
  /\ UNCHANGED <<ctrl, thr, ep, version, frozen, svcHasRoomRole, view, epAt, accepted, roster, visibleAdds, alias, installs, counted>>

Apply(m) ==
  CASE m.kind = "authorize_endpoint" -> ep' = [ep EXCEPT ![m.who] = "active"] /\ UNCHANGED <<ctrl, thr>>
    [] m.kind = "revoke_endpoint"    -> ep' = [ep EXCEPT ![m.who] = "revoked"] /\ UNCHANGED <<ctrl, thr>>
    [] m.kind = "add_controller"     ->
         \* Proposed rule: the service itself, its endpoints and its hosts are never eligible controllers.
         /\ Mutation \in {"self_bootstrap", "host_counts"} \/ m.who \notin {SVC} \cup Endpoints \cup {Host[e] : e \in Endpoints}
         /\ ctrl' = ctrl \cup {m.who} /\ UNCHANGED <<ep, thr>>
    [] m.kind = "remove_controller"  -> /\ Cardinality(ctrl) > 1 /\ ctrl' = ctrl \ {m.who}
                                        /\ thr' = IF thr > Cardinality(ctrl') THEN Cardinality(ctrl') ELSE thr
                                        /\ UNCHANGED ep

\* Proposed rule: endpoint revocations only reduce authority, so they commute with
\* other transitions. A revocation sealed on an older head is rebased onto the
\* current head (never a fork), and it still applies while control is frozen.
RevokeRule(x) == Menu[x.t].kind = "revoke_endpoint" /\ Mutation # "freeze_blocks_revocation"
Execute(x) ==
  /\ ~frozen \/ RevokeRule(x)
  /\ x \in props /\ (x.base = version \/ RevokeRule(x)) /\ version < MaxVersion
  /\ Cardinality(Approvers(x.appr)) >= thr
  /\ Apply(Menu[x.t])
  /\ version' = version + 1
  /\ epAt' = [epAt EXCEPT ![version + 1] = ep']
  /\ props' = {y \in props : y # x /\ ~(frozen /\ y.base = x.base)}
  /\ counted' = Approvers(x.appr)
  /\ UNCHANGED <<frozen, svcHasRoomRole, view, accepted, roster, visibleAdds, alias, installs, steps>>

\* Two sealed successors of one head (e.g. a compromised controller) freeze
\* further control changes (same rule as AccountControlState / Space control).
Fork(x) ==
  /\ ~frozen /\ x \in props /\ x.base < version /\ ~RevokeRule(x)
  /\ Cardinality(Approvers(x.appr)) >= thr
  /\ frozen' = TRUE /\ props' = {y \in props : y # x}
  /\ UNCHANGED <<ctrl, thr, ep, version, svcHasRoomRole, view, epAt, accepted, roster, visibleAdds, alias, installs, counted, steps>>

\* The service is given a Room governance role through its installation.
GrantRoomRole ==
  /\ Env /\ ~svcHasRoomRole /\ svcHasRoomRole' = TRUE
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, view, epAt, accepted, roster, visibleAdds, alias, installs, counted>>

\* Recipients learn control-state versions in order (never roll back).
RecvSync(r) ==
  /\ view[r] < version
  /\ view' = [view EXCEPT ![r] = @ + 1]
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, epAt, accepted, roster, visibleAdds, alias, installs, counted, steps>>

\* A hostile former host replays an older, validly signed control state.
Rollback(r) ==
  /\ Mutation = "accept_rollback" /\ Env /\ view[r] > 0
  /\ view' = [view EXCEPT ![r] = 0]
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, epAt, accepted, roster, visibleAdds, alias, installs, counted>>

\* Any holder of an endpoint credential (including a host whose endpoint was
\* revoked) emits a service-authored event; recipients accept per their view.
EndpointSend(e, r) ==
  /\ Env /\ epAt[view[r]][e] = "active"
  /\ accepted' = accepted \cup {<<r, e, view[r]>>}
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, view, epAt, roster, visibleAdds, alias, installs, counted>>

\* The Room (a member's client) commits the service's endpoint set into the
\* MLS roster: a visible "service endpoint changed" transition.
RoomSync ==
  /\ roster # {e \in Endpoints : ep[e] = "active"}
  /\ roster' = {e \in Endpoints : ep[e] = "active"}
  /\ visibleAdds' = visibleAdds \cup roster'
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, view, epAt, accepted, alias, installs, counted, steps>>

\* Mutation: a host inserts an endpoint into the Room roster without the
\* visible Room transition (hidden plaintext recipient).
HostAddsEndpoint(e) ==
  /\ Mutation = "host_adds_endpoint" /\ Env /\ e \notin roster
  /\ roster' = roster \cup {e}
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, view, epAt, accepted, visibleAdds, alias, installs, counted>>

\* The alias namespace owner re-points (recycles) the alias to another ServiceID.
RepointAlias ==
  /\ Env /\ alias = "S" /\ alias' = "S2"
  /\ installs' = IF Mutation = "alias_binds_trust" THEN {[i EXCEPT !.sid = alias'] : i \in installs} ELSE installs
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, view, epAt, accepted, roster, visibleAdds, counted>>

\* A new installation made through the alias binds whatever the alias names now.
InstallViaAlias ==
  /\ Env /\ [sid |-> alias, via |-> "alias_new"] \notin installs
  /\ installs' = installs \cup {[sid |-> alias, via |-> "alias_new"]}
  /\ UNCHANGED <<ctrl, thr, ep, version, props, frozen, svcHasRoomRole, view, epAt, accepted, roster, visibleAdds, alias, counted>>

Next ==
  \/ \E p \in Accounts \cup Endpoints \cup {SVC} \cup {Host[e] : e \in Endpoints}, t \in DOMAIN Menu : Approve(p, t)
  \/ \E x \in props : Execute(x) \/ Fork(x)
  \/ GrantRoomRole
  \/ \E r \in Recipients : RecvSync(r) \/ Rollback(r)
  \/ \E e \in Endpoints, r \in Recipients : EndpointSend(e, r)
  \/ RoomSync
  \/ \E e \in Endpoints : HostAddsEndpoint(e)
  \/ RepointAlias \/ InstallViaAlias

ExecRevoke == \E x \in props : Menu[x.t].kind = "revoke_endpoint" /\ Execute(x)
Spec == Init /\ [][Next]_vars /\ \A r \in Recipients : WF_vars(RecvSync(r)) /\ WF_vars(RoomSync) /\ WF_vars(ExecRevoke)

-----------------------------------------------------------------------------
\* Only independent account controllers ever authorised a control change.
NoSelfBootstrap == counted \subseteq Accounts /\ ctrl \subseteq Accounts \cup InitCtrl

\* A host never becomes a controller merely by running an endpoint.
HostNeverController == \A e \in Endpoints : Host[e] \notin ctrl

\* Recipients never move backwards in the service's control-state chain.
NoRollback == [][\A r \in Recipients : view'[r] >= view[r]]_vars

\* Every accepted service event came from an endpoint active in the control
\* state the recipient held; once a recipient's view includes a revocation,
\* that endpoint is never accepted by it again.
AcceptedOnlyActive == \A a \in accepted : epAt[a[3]][a[2]] = "active"

\* No hidden plaintext recipient: every service endpoint in the Room roster
\* entered through a visible Room transition and was authorised by the service.
NoHiddenRecipient == roster \subseteq visibleAdds /\ \A e \in roster : \E v \in 0..version : epAt[v][e] = "active"

\* Alias replacement never transfers existing installed trust.
AliasNoTrustTransfer == [sid |-> "S", via |-> "alias"] \in installs

\* Liveness: revocation eventually reaches every recipient and the Room roster.
RevocationDominates ==
  \A e \in Endpoints : [](ep[e] = "revoked" => <>(e \notin roster /\ \A r \in Recipients : epAt[view[r]][e] = "revoked"))

SealedRevoke(e) == ep[e] # "revoked" /\ \E x \in props : Menu[x.t] = [kind |-> "revoke_endpoint", who |-> e]
                                     /\ Cardinality(Approvers(x.appr)) >= thr
\* Liveness: a revocation sealed by quorum on the current head always takes effect (a fork cannot block it).
RevocationNotBlocked == \A e \in Endpoints : [](SealedRevoke(e) => <>(ep[e] = "revoked"))

\* Safety form of the same rule: a sealed revocation is always executable (never stranded by a fork freeze).
RevocationNeverStranded == \A e \in Endpoints : SealedRevoke(e) => ENABLED ExecRevoke

(* Golden-witness traps *)
TrapHostMigration == ~(\E e1, e2 \in Endpoints : e1 # e2 /\ ep[e1] = "revoked" /\ ep[e2] = "active" /\ roster = {e2})
TrapStaleAcceptWindow == ~(\E a \in accepted : ep[a[2]] = "revoked")
TrapAliasRecycledNewInstallIsImpostor == ~([sid |-> "S2", via |-> "alias_new"] \in installs)

-----------------------------------------------------------------------------
T(k, w) == [kind |-> k, who |-> w]
HostOf == [e \in {"e1", "e2"} |-> IF e = "e1" THEN "hostA" ELSE "hostB"]
\* Migration from host A (e1) to host B (e2); host A and B try to become controllers.
MenuMigrate == << T("authorize_endpoint", "e2"), T("revoke_endpoint", "e1") >>
\* A compromised controller forks control state to block revoking its endpoint.
MenuFork == << T("revoke_endpoint", "e1"), T("authorize_endpoint", "e2") >>
MenuBootstrap == << T("add_controller", "svc"), T("authorize_endpoint", "e2") >>
=============================================================================
