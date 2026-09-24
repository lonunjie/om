--------------------------- MODULE ScopedPermissions ---------------------------
(***************************************************************************)
(* V-008 feasibility model: Room capability grants, delegation ceilings,   *)
(* explicit restrictions, governance quorum bound to the prior state,      *)
(* same-parent fork freeze, bilateral Space<->Room links, a single         *)
(* authority-parent SpaceAuthorityBinding, and the asynchronous room-local *)
(* mirror of Space roles (revocation races).                               *)
(*                                                                         *)
(* Derived from the public design prose, not from any existing OM model:   *)
(*   SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md  (SP)             *)
(*     "Capability model", "No ordinary-role deny maze", "Delegation       *)
(*     ceiling prevents self-escalation", "High-risk changes may require   *)
(*     quorum"                                                             *)
(*   SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md  (SR)                     *)
(*     "Space control-state convergence", "Child links are affiliation,    *)
(*     not possession", "Space-managed rooms", "No live transitive         *)
(*     permission magic", "Nested Spaces", "Governance"                    *)
(* Written by a Claude agent (feasibility evidence, not product code).     *)
(*                                                                         *)
(* Mutation = "none" is the proposed-correct semantics. Other values       *)
(* disable exactly one rule so TLC must find a counterexample (see         *)
(* verification/tla/models-v008-permissions.json).                         *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
  P,              \* principals (typed AccountPrincipals in this model)
  Caps,           \* room capabilities
  HighRisk,       \* subset of Caps that only governance quorum may root
  Spaces,         \* Spaces that may list / govern the Room
  GovMenu,        \* governance proposal templates (records, see Apply)
  RoleMenu,       \* ordinary role grants a role manager may attempt
  SpaceRoleMenu,  \* Space role assignments a Space may make
  RestrictMenu,   \* capabilities a moderator may restrict / unrestrict
  InitGov, InitThr, InitOfficial, InitBinding, InitSpaceParent, InitGrants,
  MaxSteps, MaxVersion, MaxGrants,
  Mutation        \* "none" | "no_ceiling" | "self_lift" | "stale_approvals"
                  \* | "accept_subdelegated" | "no_cascade" | "unilateral_binding"

NoSpace == "none"
GOV == "GOV"

VARIABLES
  gov, thr,           \* Room governance set and threshold (governance root)
  version,            \* accepted Room control-state version
  govAt, thrAt,       \* governance in force at each accepted version
  execAt,             \* template executed at each version (fork detection)
  props,              \* pending proposals [t, base, appr]
  grants,             \* Seq of [to, caps, deleg, parent, by, live]
  restr,              \* explicit restrictions P -> SUBSET Caps
  spaceSide, roomSide,\* the two halves of each bilateral link
  binding,            \* [space, deleg] single authority-parent binding
  boundEver,          \* ghost: Spaces the Room ever bound by its own quorum
  spaceRole, spaceRoleBy,  \* Space-side role state and its authoring governance
  mirror, mirrorCeil, \* room-local materialisation of Space roles
  influence,          \* ghost: Spaces whose governance changed Room authority
  frozen, forkBase,
  steps, last

vars == <<gov, thr, version, govAt, thrAt, execAt, props, grants, restr,
          spaceSide, roomSide, binding, boundEver, spaceRole, spaceRoleBy,
          mirror, mirrorCeil, influence, frozen, forkBase, steps, last>>

-----------------------------------------------------------------------------
Official(S) == S \in spaceSide /\ S \in roomSide
Idx == 1..Len(grants)

RECURSIVE Rooted(_)
\* A grant is rooted when its whole issuance chain is still live: a governance
\* grant, a live grant from a still-rooted role manager, or a grant issued from
\* Space-derived authority that the Room still materialises.
Rooted(i) ==
  LET g == grants[i] IN
    /\ g.live
    /\ \/ g.parent = 0
       \/ /\ g.parent > 0
          /\ Rooted(g.parent)
          /\ "manage_roles" \in grants[g.parent].caps
       \/ /\ g.parent = -1
          /\ "manage_roles" \in mirror[g.by]

\* The validity rule the enforcing implementation uses.
Valid(i) == IF Mutation = "no_cascade" THEN grants[i].live ELSE Rooted(i)

LocalCaps(p)  == UNION {grants[i].caps  : i \in {j \in Idx : grants[j].to = p /\ Valid(j)}}
LocalDeleg(p) == UNION {grants[i].deleg : i \in {j \in Idx : grants[j].to = p /\ Valid(j)}}
RootedCaps(p) == UNION {grants[i].caps  : i \in {j \in Idx : grants[j].to = p /\ Rooted(j)}}
MirrorDeleg(p) == IF "manage_roles" \in mirror[p] THEN mirrorCeil[p] \ {"manage_roles"} ELSE {}

Eff(p)     == (LocalCaps(p) \cup mirror[p]) \ restr[p]
Deleg(p)   == LocalDeleg(p) \cup MirrorDeleg(p)
Closure(p) == Eff(p) \cup Deleg(p)
\* Authority held before explicit restrictions are applied (restrictions are
\* temporary and lifting another principal's restriction restores, not creates).
RawClosure(p) == LocalCaps(p) \cup mirror[p] \cup Deleg(p)
AllClosure == UNION {RawClosure(q) : q \in P}

\* What the Room would materialise for p from its authority parent right now.
Author(p) == IF binding.space = NoSpace THEN NoSpace ELSE spaceRoleBy[binding.space][p]
Target(p) ==
  IF Mutation = "unilateral_binding" /\ binding.space = NoSpace /\ spaceSide # {}
  THEN LET S == CHOOSE s \in spaceSide : TRUE IN spaceRole[S][p]
  ELSE IF /\ binding.space # NoSpace
          /\ Official(binding.space)
          /\ (Mutation = "accept_subdelegated" \/ Author(p) = binding.space)
       THEN spaceRole[binding.space][p] \cap binding.deleg
       ELSE {}
TargetAuthor(p) ==
  IF Mutation = "unilateral_binding" /\ binding.space = NoSpace /\ spaceSide # {}
  THEN LET S == CHOOSE s \in spaceSide : TRUE IN spaceRoleBy[S][p]
  ELSE Author(p)
TargetCeil ==
  IF Mutation = "unilateral_binding" /\ binding.space = NoSpace THEN Caps ELSE binding.deleg

Stale(p) == ~(mirror[p] \subseteq Target(p))

-----------------------------------------------------------------------------
Init ==
  /\ gov = InitGov /\ thr = InitThr /\ version = 0
  /\ govAt = [v \in 0..MaxVersion |-> IF v = 0 THEN InitGov ELSE {}]
  /\ thrAt = [v \in 0..MaxVersion |-> IF v = 0 THEN InitThr ELSE 0]
  /\ execAt = [v \in 0..MaxVersion |-> 0]
  /\ props = {}
  /\ grants = InitGrants
  /\ restr = [p \in P |-> {}]
  /\ spaceSide = InitOfficial /\ roomSide = InitOfficial
  /\ binding = InitBinding
  /\ boundEver = IF InitBinding.space = NoSpace THEN {} ELSE {InitBinding.space}
  /\ spaceRole = [s \in Spaces |-> [p \in P |-> {}]]
  /\ spaceRoleBy = [s \in Spaces |-> [p \in P |-> s]]
  /\ mirror = [p \in P |-> {}]
  /\ mirrorCeil = [p \in P |-> {}]
  /\ influence = {}
  /\ frozen = FALSE /\ forkBase = 0
  /\ steps = 0
  /\ last = [kind |-> "init", actor |-> "none", counted |-> {}, govNow |-> InitGov, thrNow |-> InitThr, detail |-> "none"]

Env == steps < MaxSteps /\ steps' = steps + 1
Rec(k, a, d) == last' = [kind |-> k, actor |-> a, counted |-> {}, govNow |-> gov, thrNow |-> thr, detail |-> d]

\* ---- governance (SP "High-risk changes may require quorum"; SR "Space control-state convergence")
Approve(p, t) ==
  /\ Env /\ ~frozen
  /\ p \in gov
  /\ LET cur == {x \in props : x.t = t /\ x.base = version} IN
       IF cur = {}
       THEN props' = props \cup {[t |-> t, base |-> version, appr |-> {p}]}
       ELSE LET x == CHOOSE y \in cur : TRUE IN
              /\ p \notin x.appr
              /\ props' = (props \ {x}) \cup {[x EXCEPT !.appr = @ \cup {p}]}
  /\ Rec("approve", p, t)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, grants, restr, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

\* Mutation stale_approvals: an approval signed at an older base still counts.
LateApprove(p, x) ==
  /\ Mutation = "stale_approvals"
  /\ Env /\ ~frozen /\ x \in props /\ x.base < version
  /\ p \in gov /\ p \notin x.appr
  /\ props' = (props \ {x}) \cup {[x EXCEPT !.appr = @ \cup {p}]}
  /\ Rec("approve", p, x.t)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, grants, restr, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

Counted(x) == IF Mutation = "stale_approvals" THEN x.appr ELSE x.appr \cap govAt[x.base]
Sealed(x)  == Cardinality(Counted(x)) >= (IF Mutation = "stale_approvals" THEN thr ELSE thrAt[x.base])

ApplyTemplate(m) ==
  CASE m.kind = "grant" ->
         /\ Len(grants) < MaxGrants
         /\ grants' = Append(grants, [to |-> m.who, caps |-> m.caps, deleg |-> m.deleg, parent |-> 0, by |-> GOV, live |-> TRUE])
         /\ UNCHANGED <<gov, thr, roomSide, binding, boundEver>>
    [] m.kind = "revoke" ->
         /\ grants' = [i \in Idx |-> IF grants[i].to = m.who /\ grants[i].parent = 0
                                     THEN [grants[i] EXCEPT !.live = FALSE] ELSE grants[i]]
         /\ UNCHANGED <<gov, thr, roomSide, binding, boundEver>>
    [] m.kind = "gov_add" ->
         /\ gov' = gov \cup {m.who} /\ UNCHANGED <<thr, grants, roomSide, binding, boundEver>>
    [] m.kind = "gov_remove" ->
         /\ Cardinality(gov) > 1 /\ m.who \in gov
         /\ gov' = gov \ {m.who}
         /\ thr' = IF thr > Cardinality(gov') THEN Cardinality(gov') ELSE thr
         /\ UNCHANGED <<grants, roomSide, binding, boundEver>>
    [] m.kind = "link_accept" ->
         /\ roomSide' = roomSide \cup {m.space} /\ UNCHANGED <<gov, thr, grants, binding, boundEver>>
    [] m.kind = "link_revoke" ->
         /\ roomSide' = roomSide \ {m.space}
         /\ binding' = IF binding.space = m.space THEN [space |-> NoSpace, deleg |-> {}] ELSE binding
         /\ UNCHANGED <<gov, thr, grants, boundEver>>
    [] m.kind = "bind" ->   \* SR: at most one authority parent; requires an official link
         /\ binding.space = NoSpace /\ Official(m.space)
         /\ binding' = [space |-> m.space, deleg |-> m.deleg]
         /\ boundEver' = boundEver \cup {m.space}
         /\ UNCHANGED <<gov, thr, grants, roomSide>>
    [] m.kind = "unbind" ->
         /\ binding' = [space |-> NoSpace, deleg |-> {}]
         /\ UNCHANGED <<gov, thr, grants, roomSide, boundEver>>

Execute(x) ==
  /\ ~frozen /\ x \in props /\ version < MaxVersion
  /\ Mutation = "stale_approvals" \/ x.base = version
  /\ Sealed(x)
  /\ ApplyTemplate(GovMenu[x.t])
  /\ version' = version + 1
  /\ govAt' = [govAt EXCEPT ![version + 1] = gov']
  /\ thrAt' = [thrAt EXCEPT ![version + 1] = thr']
  /\ execAt' = [execAt EXCEPT ![version] = x.t]
  /\ props' = props \ {x}
  /\ last' = [kind |-> "execute", actor |-> GOV, counted |-> Counted(x), govNow |-> gov, thrNow |-> thr, detail |-> x.t]
  /\ UNCHANGED <<restr, spaceSide, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase, steps>>

\* A different authority-changing successor of an already-superseded head was
\* sealed concurrently (offline replica): SR says freeze, never pick a winner.
StaleSeal(x) ==
  /\ ~frozen /\ x \in props /\ x.base < version
  /\ Sealed(x) /\ execAt[x.base] # x.t
  /\ frozen' = TRUE /\ forkBase' = x.base
  /\ props' = props \ {x}
  /\ Rec("fork_detected", GOV, x.t)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, grants, restr, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, steps>>

\* Resolution must be authorised by the common pre-fork governance.
Resolve(A) ==
  /\ Env /\ frozen
  /\ A \subseteq govAt[forkBase] /\ Cardinality(A) >= thrAt[forkBase]
  /\ frozen' = FALSE /\ props' = {}
  /\ last' = [kind |-> "resolve", actor |-> GOV, counted |-> A, govNow |-> govAt[forkBase], thrNow |-> thrAt[forkBase], detail |-> "none"]
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, grants, restr, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, forkBase>>

\* ---- ordinary role management (SP "Delegation ceiling prevents self-escalation")
Bases(a) == {i \in Idx : grants[i].to = a /\ Valid(i) /\ "manage_roles" \in grants[i].caps}
            \cup (IF "manage_roles" \in mirror[a] THEN {-1} ELSE {})
Ceil(a, b) == IF b = -1 THEN MirrorDeleg(a) ELSE grants[b].deleg

Grant(a, to, r) ==
  /\ Env /\ Len(grants) < MaxGrants
  /\ "manage_roles" \in Eff(a)
  /\ \E b \in Bases(a) :
       /\ Mutation = "no_ceiling" \/ (r.caps \cup r.deleg) \subseteq Ceil(a, b)
       /\ grants' = Append(grants, [to |-> to, caps |-> r.caps, deleg |-> r.deleg, parent |-> b, by |-> a, live |-> TRUE])
  /\ Rec("grant", a, to)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, restr, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

RevokeOwnGrant(a, i) ==
  /\ Env /\ i \in Idx /\ grants[i].by = a /\ grants[i].live
  /\ grants' = [grants EXCEPT ![i].live = FALSE]
  /\ Rec("revoke_grant", a, grants[i].to)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, restr, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

\* ---- restrictions (SP "No ordinary-role deny maze": restrictions win)
Restrict(a, t, c) ==
  /\ Env /\ "moderate" \in Eff(a) /\ c \notin restr[t]
  /\ restr' = [restr EXCEPT ![t] = @ \cup {c}]
  /\ Rec("restrict", a, t)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, grants, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

Unrestrict(a, t, c) ==
  /\ Env /\ "moderate" \in Eff(a) /\ c \in restr[t]
  /\ Mutation = "self_lift" \/ a # t
  /\ restr' = [restr EXCEPT ![t] = @ \ {c}]
  /\ Rec("unrestrict", a, t)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, grants, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

\* ---- Space-side actions (SR "Child links", "Nested Spaces")
SpaceLink(s) ==
  /\ Env /\ s \notin spaceSide /\ spaceSide' = spaceSide \cup {s}
  /\ Rec("space_link", s, s)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, grants, restr, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

SpaceUnlink(s) ==
  /\ Env /\ s \in spaceSide /\ spaceSide' = spaceSide \ {s}
  /\ Rec("space_unlink", s, s)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, grants, restr, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase>>

\* author = the Space's own governance, or its own authority parent acting
\* through a Space-to-Space binding (sub-delegation).
SpaceSetRole(s, author, p, r) ==
  /\ Env
  /\ author = s \/ author = InitSpaceParent[s]
  /\ author # NoSpace
  /\ spaceRole[s][p] # r \/ spaceRoleBy[s][p] # author
  /\ spaceRole' = [spaceRole EXCEPT ![s][p] = r]
  /\ spaceRoleBy' = [spaceRoleBy EXCEPT ![s][p] = author]
  /\ Rec("space_set_role", author, p)
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, grants, restr, spaceSide, roomSide,
                 binding, boundEver, mirror, mirrorCeil, influence, frozen, forkBase>>

\* ---- Room-local materialisation (SR "No live transitive permission magic")
NeedsSync(p) == mirror[p] # Target(p) \/ (Target(p) # {} /\ mirrorCeil[p] # TargetCeil)

\* The Room applies all pending Space-role materialisations in one accepted
\* control transition (state-space reduction: per-principal interleavings of
\* the same transition add no new authority behaviour).
SyncAll ==
  /\ \E p \in P : NeedsSync(p)
  /\ mirror' = [p \in P |-> Target(p)]
  /\ mirrorCeil' = [p \in P |-> IF Target(p) = {} THEN {} ELSE TargetCeil]
  /\ influence' = influence \cup {TargetAuthor(p) : p \in {q \in P : Target(q) # {}}}
  /\ Rec("sync", "room", "none")
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, grants, restr, spaceSide, roomSide,
                 binding, boundEver, spaceRole, spaceRoleBy, frozen, forkBase, steps>>

\* A Space withdrawing its half of the link ends the authority binding.
DropBinding ==
  /\ binding.space # NoSpace /\ ~Official(binding.space)
  /\ binding' = [space |-> NoSpace, deleg |-> {}]
  /\ Rec("drop_binding", "room", "none")
  /\ UNCHANGED <<gov, thr, version, govAt, thrAt, execAt, props, grants, restr, spaceSide, roomSide,
                 boundEver, spaceRole, spaceRoleBy, mirror, mirrorCeil, influence, frozen, forkBase, steps>>

Next ==
  \/ \E p \in P, t \in DOMAIN GovMenu : Approve(p, t)
  \/ \E p \in P, x \in props : LateApprove(p, x)
  \/ \E x \in props : Execute(x) \/ StaleSeal(x)
  \/ \E A \in SUBSET P : Resolve(A)
  \/ \E a, t \in P, r \in RoleMenu : Grant(a, t, r)
  \/ \E a \in P, i \in Idx : RevokeOwnGrant(a, i)
  \/ \E a, t \in P, c \in RestrictMenu : Restrict(a, t, c) \/ Unrestrict(a, t, c)
  \/ \E s \in Spaces : SpaceLink(s) \/ SpaceUnlink(s)
  \/ \E s \in Spaces, au \in Spaces, p \in P, r \in SpaceRoleMenu : SpaceSetRole(s, au, p, r)
  \/ SyncAll
  \/ DropBinding

Spec == Init /\ [][Next]_vars /\ WF_vars(SyncAll) /\ WF_vars(DropBinding)


-----------------------------------------------------------------------------
(* Scenario values bound from the .cfg files with `<-`. *)
T(k, w, c, d, s) == [kind |-> k, who |-> w, caps |-> c, deleg |-> d, space |-> s]
G(to, c, d) == [to |-> to, caps |-> c, deleg |-> d, parent |-> 0, by |-> GOV, live |-> TRUE]
Unbound == [space |-> NoSpace, deleg |-> {}]

\* Main: 2-of-2 governance {a,b}; c is an ordinary role manager; S2 officially
\* linked but not yet an authority parent; S1 is S2's own authority parent.
MenuMain == << T("grant", "b", {"add_bridge"}, {}, NoSpace),
               T("bind", "none", {}, {"moderate", "manage_roles"}, "S2"),
               T("gov_remove", "a", {}, {}, NoSpace),
               T("link_revoke", "none", {}, {}, "S2") >>
RolesMain == { [caps |-> {"post"}, deleg |-> {}],
               [caps |-> {"moderate", "manage_roles"}, deleg |-> {"post"}],
               [caps |-> {"add_bridge"}, deleg |-> {}] }
SpaceRolesMain == { {}, {"moderate", "manage_roles"} }
GrantsMain == << G("c", {"manage_roles", "moderate"}, {"post", "moderate"}) >>
ParentMain == [s \in {"S1", "S2"} |-> IF s = "S2" THEN "S1" ELSE NoSpace]
BoundS2 == [space |-> "S2", deleg |-> {"moderate", "manage_roles"}]

\* Governance race scenarios: 3 governors, threshold 2 (or 1 for forks).
MenuGov == << T("gov_remove", "a", {}, {}, NoSpace),
              T("grant", "c", {"post"}, {}, NoSpace),
              T("gov_add", "c", {}, {}, NoSpace) >>
MenuTransfer == << T("gov_add", "b", {}, {}, NoSpace), T("gov_remove", "a", {}, {}, NoSpace) >>
NoGrants == << >>
MenuRooms == << T("grant", "b", {"add_bridge"}, {}, NoSpace), T("gov_remove", "a", {}, {}, NoSpace) >>
MenuSpaces == << T("bind", "none", {}, {"post", "moderate", "manage_roles"}, "S2"), T("link_revoke", "none", {}, {}, "S2") >>
RolesSpaces == { [caps |-> {"post"}, deleg |-> {}] }
ParentNone == [s \in {"S1", "S2"} |-> NoSpace]
NoRoles == { }

-----------------------------------------------------------------------------
(* Safety properties *)

TypeOK ==
  /\ gov \subseteq P /\ gov # {} /\ thr \in 1..Cardinality(gov)
  /\ \A p \in P : restr[p] \subseteq Caps /\ mirror[p] \subseteq Caps
  /\ binding.space \in Spaces \cup {NoSpace}

\* No ordinary action (anything except an accepted governance transition or a
\* Room-accepted materialisation of the bound Space) can create a capability or
\* grantable scope that nobody held before: the delegation-ceiling property.
CoalitionClosed ==
  [][ last'.kind \notin {"execute", "sync"} => \A p \in P : RawClosure(p)' \subseteq AllClosure ]_vars

\* No principal's own action enlarges its own effective or grantable authority.
NoSelfEscalation ==
  [][ \A q \in P : last'.actor = q => Closure(q)' \subseteq Closure(q) ]_vars

\* High-risk capabilities are only ever held through a governance-rooted chain
\* or a quorum-approved binding ceiling.
HighRiskRooted ==
  \A p \in P : \A c \in HighRisk \cap Eff(p) :
     \/ \E i \in Idx : grants[i].to = p /\ Rooted(i) /\ c \in grants[i].caps
     \/ c \in mirror[p] /\ c \in mirrorCeil[p]

\* Approvals that counted came from the governance in force at execution and met its threshold.
QuorumBoundToState ==
  last.kind = "execute" => /\ last.counted \subseteq last.govNow
                           /\ Cardinality(last.counted) >= last.thrNow

\* Only a Space the Room itself bound by quorum, acting through its own
\* governance, ever changed Room authority (no transitive / unilateral authority).
NoTransitiveAuthority == influence \subseteq boundEver

\* Space-derived authority never exceeds the binding ceiling it was materialised under.
MirrorWithinCeiling == \A p \in P : mirror[p] \subseteq mirrorCeil[p]

\* Authority in force never outlives the chain that justified it (cascade).
NoLaunderedAuthority == \A p \in P : LocalCaps(p) \subseteq RootedCaps(p)

\* Liveness: a revoked / unbound Space role is eventually removed from the Room.
RevocationDominates == \A p \in P : [](Stale(p) => <>(~Stale(p)))

(* Golden-witness traps: each is violated by a short trace that becomes a vector. *)
TrapForkFreezes == ~frozen
TrapHighRiskByQuorum == \A p \in P : HighRisk \cap Eff(p) = {}
TrapSpaceDelegatedModeration == \A p \in P : "moderate" \notin mirror[p]
TrapCascadeRevocation == ~(\E i \in Idx : grants[i].parent = -1 /\ grants[i].live /\ ~Rooted(i))
TrapGovernanceTransfer == gov # {"b"}
=============================================================================
