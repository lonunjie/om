---------------------------- MODULE AccountControl ----------------------------
(***************************************************************************)
(* V-002 formal model of AccountControlState (ACCOUNT_CONTINUITY_AND_       *)
(* CONTROL.md) with ADR-0111 default recovery (RECOVERY_AND_BACKUP_POLICY  *)
(* §10), retirement and LegacyPolicy (ACCOUNT_LIFECYCLE_RETIREMENT_AND_    *)
(* LEGACY.md) and the device-link notice rule (ENDPOINT_COERCION_AND_      *)
(* PLATFORM_LEAKAGE.md §2).                                                *)
(*                                                                         *)
(* Two parties act: the owner and an attacker. The attacker holds its own  *)
(* devices, may compromise some of the owner's devices (it can then use    *)
(* their keys; the owner still can too), and may hold 0..2 of the owner's   *)
(* recovery custodians. The majority rule is floor(N/2)+1 over the current *)
(* controller set; quarantine removes a vote but never shrinks N.          *)
(*                                                                         *)
(* Environment assumption (documented, not hidden): a live, non-lost owner *)
(* device reacts to a recovery notice before the activation delay elapses. *)
(* It is encoded as the guard on Tick (OwnerMustReact). Configurations that *)
(* let every owner device go offline show the residual risk T-041.         *)
(*                                                                         *)
(* 2026-09-26 (review F-1/F-2): recovery method count never changes        *)
(* authority (any live controller vetoes; MethodCountMutation restores the *)
(* old rule so TLC can show it is caught); lifting a quarantine needs a    *)
(* device-only majority; a quarantine that leaves fewer voting controllers *)
(* than the majority enters SAFETY_FREEZE (§4 rule 4).                     *)
(*                                                                         *)
(* ADR-0137: a quarantine-induced safety freeze may be resolved by a       *)
(* device-only parent majority using the narrow challenge capability. The  *)
(* resolution may selectively restore a non-empty subset of quarantined    *)
(* controllers; each restored controller must challenge-sign, and the      *)
(* resulting state MUST regain a voting majority. A                        *)
(* fork freeze is a separate device-only bridge action and never uses the  *)
(* quarantine challenge exception.                                        *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS
  HonestInit,        \* the owner's initial controller devices
  HonestSpare,       \* owner devices that may later be linked or recovered onto
  AttackerDevs,      \* attacker-owned devices
  CanCompromise,     \* owner devices the attacker may compromise
  AttackerMethods,   \* recovery custodians the attacker initially holds (0..2)
  OwnerMethods,      \* recovery custodians the owner holds (0 = recovery off)
  Delay,             \* recovery activation delay, in ticks
  LegacyWait,        \* owner inactivity, in ticks, before a legacy claim can activate
  MaxGen,            \* bound on the number of transitions (state constraint)
  OwnerStructural,   \* TRUE: owner may link/revoke/quarantine/retire/arm legacy
  OwnerMayGoOffline, \* TRUE: owner devices may go offline or be lost
  AttackerMayRecover, \* TRUE: attacker may file recovery requests
  OwnerKeepsOneLive, \* TRUE: offline/loss never takes the owner's last usable device
  MethodCountMutation \* TRUE: mutation restoring the rejected method-count veto rule (F-1)

VARIABLES
  controllers,   \* current controller devices
  quarantined,   \* controllers whose vote is suspended
  revoked,       \* permanently revoked devices
  compromised,   \* owner devices whose keys the attacker can use
  offline,       \* owner devices temporarily unable to react
  lost,          \* owner devices permanently lost by the owner
  atkMethods,    \* recovery custodians the attacker currently holds
  gen,           \* AccountControlState generation (transition counter)
  epoch,         \* recovery epoch; rotation invalidates pending requests
  pending,       \* the pending recovery request, or NoPending
  retired,       \* terminal AccountRetirement reached
  legacy,        \* [armed, claim, elapsed]
  idle,          \* ticks the owner has had no usable device
  frozen,        \* fork detected: constitutional changes blocked
  linkLog,       \* one record per enrollment: [dev, notified]
  last           \* signers of the last step: [kind, signers, rv, challenge]

vars == <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
          gen, epoch, pending, retired, legacy, idle, frozen, linkLog, last>>

Parties == {"owner", "attacker"}
Other(p) == IF p = "owner" THEN "attacker" ELSE "owner"
NoPending == [active |-> FALSE, party |-> "owner", dev |-> "none", methods |-> 0,
              elapsed |-> 0, epoch |-> 0]
Majority(S) == (Cardinality(S) \div 2) + 1
Max(a, b) == IF a > b THEN a ELSE b

\* Devices a party can sign with right now.
Usable(p) ==
  IF p = "owner"
    THEN (controllers \ quarantined) \ (lost \cup offline \cup AttackerDevs)
    ELSE (controllers \ quarantined) \cap (compromised \cup AttackerDevs)

\* Devices a party can sign with, including quarantined ones: a quarantined
\* controller keeps a control-only challenge capability (§5) that counts only
\* toward resolving a safety freeze.
Signable(p) ==
  IF p = "owner"
    THEN controllers \ (lost \cup offline \cup AttackerDevs)
    ELSE controllers \cap (compromised \cup AttackerDevs)

Methods(p) == IF p = "owner" THEN OwnerMethods ELSE atkMethods
Act(k, V, rv) == [kind |-> k, signers |-> V, rv |-> rv, challenge |-> {}]
NoAct == Act("none", {}, 0)
CandidateDevs(p) == IF p = "owner" THEN HonestSpare ELSE AttackerDevs

\* R-185: a recovery method is exactly one vote and can vote immediately only
\* together with at least one current controller.
Authorized(p, V, rv) ==
  /\ V \subseteq Usable(p)
  /\ rv \in 0..1
  /\ (rv = 1) => (Methods(p) >= 1 /\ V # {})
  /\ Cardinality(V) + rv >= Majority(controllers)

CanTransition == ~frozen /\ ~retired /\ gen < MaxGen

AttackerOwns == Cardinality(Usable("attacker")) >= Majority(controllers)
OwnerOwns == Cardinality(Usable("owner")) >= Majority(controllers)
                /\ Usable("attacker") = {}

\* ADR-0111/0127: any live controller vetoes a recovery-alone request, whatever
\* the custodian/method composition. The mutation restores the old rule, in which a
\* two-method request could be stopped only by a majority act.
CanVeto(p) ==
  IF MethodCountMutation /\ pending.methods = 2
    THEN FALSE
    ELSE Usable(p) # {}

\* Environment assumption: a live owner device answers a recovery notice.
OwnerMustReact ==
  /\ pending.active
  /\ pending.party = "attacker"
  /\ pending.epoch = epoch
  /\ \/ CanVeto("owner")
     \/ MethodCountMutation /\ pending.methods = 2
        /\ \E V \in SUBSET Usable("owner"), rv \in 0..1 : Authorized("owner", V, rv)

Init ==
  /\ controllers = HonestInit
  /\ quarantined = {}
  /\ revoked = {}
  /\ compromised = {}
  /\ offline = {}
  /\ lost = {}
  /\ atkMethods = AttackerMethods
  /\ gen = 0
  /\ epoch = 0
  /\ pending = NoPending
  /\ retired = FALSE
  /\ legacy = [armed |-> FALSE, claim |-> FALSE, elapsed |-> 0]
  /\ idle = 0
  /\ frozen = FALSE
  /\ linkLog = {}
  /\ last = NoAct

-----------------------------------------------------------------------------
(* Constitutional transitions (each is one AccountControlState step).      *)

Link(p, V, rv, d) ==
  /\ CanTransition
  /\ p = "attacker" \/ OwnerStructural
  /\ Authorized(p, V, rv)
  /\ d \in CandidateDevs(p) \ (controllers \cup revoked)
  /\ controllers' = controllers \cup {d}
  /\ linkLog' = linkLog \cup {[dev |-> d, notified |-> controllers]}
  /\ gen' = gen + 1
  /\ last' = Act("promote", V, rv)
  /\ UNCHANGED <<quarantined, revoked, compromised, offline, lost, atkMethods, epoch,
                 pending, retired, legacy, idle, frozen>>

Revoke(p, V, rv, t) ==
  /\ CanTransition
  /\ p = "attacker" \/ OwnerStructural
  /\ Authorized(p, V, rv)
  /\ t \in controllers
  /\ (controllers \ quarantined) \ {t} # {}   \* found by TLC: never leave zero voting controllers
  /\ controllers' = controllers \ {t}
  /\ quarantined' = quarantined \ {t}
  /\ revoked' = revoked \cup {t}
  /\ gen' = gen + 1
  /\ last' = Act("revoke", V, rv)
  /\ UNCHANGED <<compromised, offline, lost, atkMethods, epoch, pending, retired, legacy,
                 idle, frozen, linkLog>>

\* Any single usable controller may quarantine another; it never shrinks N.
\* §4 rule 4: leaving fewer voting controllers than the majority enters SAFETY_FREEZE.
Quarantine(p, a, t) ==
  /\ CanTransition
  /\ p = "attacker" \/ OwnerStructural
  /\ a \in Usable(p)
  /\ t \in controllers \ (quarantined \cup {a})
  /\ quarantined' = quarantined \cup {t}
  /\ frozen' = (Cardinality(controllers \ quarantined') < Majority(controllers))
  /\ gen' = gen + 1
  /\ last' = Act("quarantine", {a}, 0)
  /\ UNCHANGED <<controllers, revoked, compromised, offline, lost, atkMethods, epoch,
                 pending, retired, legacy, idle, linkLog>>

\* §4.1 lift row: a device-only majority of the parent's controllers (quarantined
\* controllers stay in N but do not sign); a recovery vote never counts.
Unquarantine(p, V, t) ==
  /\ CanTransition
  /\ V \subseteq Usable(p)
  /\ Cardinality(V) >= Majority(controllers)
  /\ t \in quarantined
  /\ quarantined' = quarantined \ {t}
  /\ gen' = gen + 1
  /\ last' = Act("lift", V, 0)
  /\ UNCHANGED <<controllers, revoked, compromised, offline, lost, atkMethods, epoch,
                 pending, retired, legacy, idle, frozen, linkLog>>

\* Rotating the RecoveryPolicy invalidates every pending request (it is bound to
\* the old epoch). When the owner rotates, the new custodian is unknown to the attacker.
RotateRecovery(p, V, rv) ==
  /\ CanTransition
  /\ Authorized(p, V, rv)
  /\ epoch' = epoch + 1
  /\ pending' = NoPending
  /\ atkMethods' = IF p = "owner" THEN 0 ELSE atkMethods
  /\ gen' = gen + 1
  /\ last' = Act("recovery_policy", V, rv)
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, retired,
                 legacy, idle, frozen, linkLog>>

Retire(p, V, rv) ==
  /\ CanTransition
  /\ p = "attacker" \/ OwnerStructural
  /\ Authorized(p, V, rv)
  /\ retired' = TRUE
  /\ pending' = NoPending
  /\ gen' = gen + 1
  /\ last' = Act("retire", V, rv)
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 epoch, legacy, idle, frozen, linkLog>>

-----------------------------------------------------------------------------
(* ADR-0111 recovery. `methods` records how many custodians produced the    *)
(* single RecoveryPolicy vote; it never changes authority (F-1).            *)

RequestRecovery(p, d, m) ==
  /\ CanTransition
  /\ p = "owner" \/ AttackerMayRecover
  /\ ~pending.active
  /\ m \in 1..2
  /\ Methods(p) >= m
  /\ d \in CandidateDevs(p) \ (controllers \cup revoked)
  /\ pending' = [active |-> TRUE, party |-> p, dev |-> d, methods |-> m,
                 elapsed |-> 0, epoch |-> epoch]
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, retired, legacy, idle, frozen, linkLog, last>>

\* RecoveryVeto: any usable controller of the other party, acting alone.
Veto(p) ==
  /\ pending.active
  /\ pending.party # p
  /\ CanVeto(p)
  /\ pending' = NoPending
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, retired, legacy, idle, frozen, linkLog, last>>

Reconstitute(p, keep) ==
  /\ pending.dev \notin controllers \cup revoked   \* found by TLC: target must still be fresh
  /\ controllers' = keep \cup {pending.dev}
  /\ revoked' = revoked \cup (controllers \ keep)
  /\ quarantined' = {}
  /\ linkLog' = linkLog \cup {[dev |-> pending.dev, notified |-> controllers]}
  /\ epoch' = epoch + 1
  /\ gen' = gen + 1
  /\ pending' = NoPending

\* Immediate: recovery vote + >= 1 current controller reach the majority.
CosignComplete(p, V) ==
  /\ CanTransition
  /\ pending.active /\ pending.party = p /\ pending.epoch = epoch
  /\ V \subseteq Usable(p) /\ V # {}
  /\ Cardinality(V) + 1 >= Majority(controllers)
  /\ Reconstitute(p, V)
  /\ last' = Act("recovery_cosigned", V, 1)
  /\ UNCHANGED <<compromised, offline, lost, atkMethods, retired, legacy, idle, frozen>>

\* Delayed: the full delay elapsed and nobody stopped the request.
DelayedComplete(p) ==
  /\ CanTransition
  /\ pending.active /\ pending.party = p /\ pending.epoch = epoch
  /\ pending.elapsed >= Delay
  /\ Reconstitute(p, {})
  /\ last' = Act("recovery_delayed", {}, 1)
  /\ UNCHANGED <<compromised, offline, lost, atkMethods, retired, legacy, idle, frozen>>

-----------------------------------------------------------------------------
(* Fork detection: two parties independently authorize conflicting children *)
(* of the same parent (e.g. a shared compromised device or shared custody). *)
(* Clients that see both heads freeze instead of choosing by timestamp.     *)

Fork ==
  /\ CanTransition
  /\ \E V1 \in SUBSET Usable("owner"), r1 \in 0..1, V2 \in SUBSET Usable("attacker"), r2 \in 0..1 :
        Authorized("owner", V1, r1) /\ Authorized("attacker", V2, r2)
  /\ frozen' = TRUE
  /\ pending' = NoPending
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, retired, legacy, idle, linkLog, last>>

\* ADR-0137: quarantine-induced SAFETY_FREEZE resolution. Quarantined
\* controllers retain only a challenge-signing capability for this action.
\* The restored set R may be selective so a controller suspected of compromise
\* can remain quarantined, but the child MUST exit the freeze.
ResolveFreeze(p, V, R) ==
  /\ frozen /\ ~retired
  /\ quarantined # {}
  /\ Cardinality(controllers \ quarantined) < Majority(controllers)
  /\ R \subseteq quarantined /\ R # {}
  /\ R \subseteq V
  /\ V \subseteq Signable(p)
  /\ Cardinality(V) >= Majority(controllers)
  /\ Cardinality(controllers \ (quarantined \ R)) >= Majority(controllers)
  /\ frozen' = FALSE
  /\ quarantined' = quarantined \ R
  /\ gen' = gen + 1
  /\ last' = [kind |-> "resolve", signers |-> V, rv |-> 0, challenge |-> V \cap quarantined]
  /\ UNCHANGED <<controllers, revoked, compromised, offline, lost, atkMethods,
                 epoch, pending, retired, legacy, idle, linkLog>>

\* Fork resolution is deliberately separate. It uses only ordinary usable
\* controller authority of the common-ancestor state; quarantined challenge
\* signatures and recovery votes do not count. Branch-coverage/fork_ack is
\* checked by the protocol authorization model outside this bounded state model.
ResolveFork(p, V) ==
  /\ frozen /\ ~retired
  /\ Cardinality(controllers \ quarantined) >= Majority(controllers)
  /\ V \subseteq Usable(p)
  /\ Cardinality(V) >= Majority(controllers)
  /\ frozen' = FALSE
  /\ gen' = gen + 1
  /\ last' = Act("fork_bridge", V, 0)
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 epoch, pending, retired, legacy, idle, linkLog>>

-----------------------------------------------------------------------------
(* LegacyPolicy: minimal power. It can only retire/memorialize an account   *)
(* after the owner has been inactive for LegacyWait ticks, and any usable   *)
(* owner device can contest a claim.                                        *)

ArmLegacy(V, rv) ==
  /\ CanTransition /\ OwnerStructural
  /\ Authorized("owner", V, rv)
  /\ ~legacy.armed
  /\ legacy' = [legacy EXCEPT !.armed = TRUE]
  /\ gen' = gen + 1
  /\ last' = Act("legacy", V, rv)
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 epoch, pending, retired, idle, frozen, linkLog>>

ClaimLegacy ==
  /\ legacy.armed /\ ~legacy.claim /\ ~retired
  /\ legacy' = [legacy EXCEPT !.claim = TRUE, !.elapsed = 0]
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, pending, retired, idle, frozen, linkLog, last>>

ContestLegacy ==
  /\ legacy.claim
  /\ Usable("owner") # {}
  /\ legacy' = [legacy EXCEPT !.claim = FALSE, !.elapsed = 0]
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, pending, retired, idle, frozen, linkLog, last>>

ActivateLegacy ==
  /\ legacy.claim /\ ~retired
  /\ idle >= LegacyWait
  /\ legacy.elapsed >= LegacyWait
  /\ retired' = TRUE
  /\ pending' = NoPending
  /\ gen' = gen + 1
  /\ last' = Act("legacy_activate", {}, 0)
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 epoch, legacy, idle, frozen, linkLog>>

-----------------------------------------------------------------------------
(* Environment.                                                             *)

Tick ==
  /\ ~OwnerMustReact
  /\ pending.active \/ legacy.claim \/ Usable("owner") = {}
  /\ pending' = IF pending.active /\ pending.elapsed < Delay
                  THEN [pending EXCEPT !.elapsed = @ + 1] ELSE pending
  /\ legacy' = IF legacy.claim /\ legacy.elapsed < LegacyWait
                 THEN [legacy EXCEPT !.elapsed = @ + 1] ELSE legacy
  /\ idle' = IF Usable("owner") = {} THEN IF idle < LegacyWait THEN idle + 1 ELSE idle ELSE 0
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, retired, frozen, linkLog, last>>

Compromise(d) ==
  /\ d \in CanCompromise \ compromised
  /\ compromised' = compromised \cup {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, offline, lost, atkMethods, gen, epoch,
                 pending, retired, legacy, idle, frozen, linkLog, last>>

\* With OwnerKeepsOneLive, the owner never loses its last usable device.
KeepsLive(d) == ~OwnerKeepsOneLive \/ Usable("owner") \ {d} # {}

GoOffline(d) ==
  /\ OwnerMayGoOffline
  /\ d \in (HonestInit \cup HonestSpare) \ (offline \cup lost)
  /\ KeepsLive(d)
  /\ offline' = offline \cup {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, lost, atkMethods, gen,
                 epoch, pending, retired, legacy, idle, frozen, linkLog, last>>

GoOnline(d) ==
  /\ d \in offline
  /\ offline' = offline \ {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, lost, atkMethods, gen,
                 epoch, pending, retired, legacy, idle, frozen, linkLog, last>>

Lose(d) ==
  /\ OwnerMayGoOffline
  /\ d \in (HonestInit \cup HonestSpare) \ lost
  /\ KeepsLive(d)
  /\ lost' = lost \cup {d}
  /\ offline' = offline \ {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, atkMethods, gen, epoch,
                 pending, retired, legacy, idle, frozen, linkLog, last>>

Next ==
  \/ \E p \in Parties, V \in SUBSET (HonestInit \cup HonestSpare \cup AttackerDevs), rv \in 0..1 :
        \/ \E d \in HonestSpare \cup AttackerDevs : Link(p, V, rv, d)
        \/ \E t \in controllers : Revoke(p, V, rv, t)
        \/ RotateRecovery(p, V, rv)
        \/ Retire(p, V, rv)
  \/ \E p \in Parties, V \in SUBSET controllers, t \in controllers : Unquarantine(p, V, t)
  \/ \E p \in Parties, a \in controllers, t \in controllers : Quarantine(p, a, t)
  \/ \E p \in Parties, d \in HonestSpare \cup AttackerDevs, m \in 1..2 : RequestRecovery(p, d, m)
  \/ \E p \in Parties : Veto(p) \/ DelayedComplete(p)
  \/ \E p \in Parties, V \in SUBSET controllers : CosignComplete(p, V) \/ ResolveFork(p, V)
  \/ \E p \in Parties, V \in SUBSET controllers, R \in SUBSET controllers : ResolveFreeze(p, V, R)
  \/ Fork
  \/ \E V \in SUBSET controllers, rv \in 0..1 : ArmLegacy(V, rv)
  \/ ClaimLegacy \/ ContestLegacy \/ ActivateLegacy
  \/ Tick
  \/ \E d \in CanCompromise : Compromise(d)
  \/ \E d \in HonestInit \cup HonestSpare : GoOffline(d) \/ GoOnline(d) \/ Lose(d)

Spec == Init /\ [][Next]_vars

StateBound == gen <= MaxGen

-----------------------------------------------------------------------------
(* Invariants that must always hold.                                        *)

TypeOK ==
  /\ controllers \subseteq HonestInit \cup HonestSpare \cup AttackerDevs
  /\ quarantined \subseteq controllers
  /\ revoked \subseteq HonestInit \cup HonestSpare \cup AttackerDevs
  /\ compromised \subseteq CanCompromise
  /\ atkMethods \in 0..2
  /\ gen \in Nat
  /\ pending.elapsed \in 0..Delay
  /\ idle \in 0..LegacyWait
  /\ frozen \in BOOLEAN /\ retired \in BOOLEAN

RevokedNeverController == controllers \cap revoked = {}
ControllersNonEmpty == controllers # {}

\* At least one controller can always vote (quarantine never silences everyone).
SomeVotingController == controllers \ quarantined # {}

\* Device linking is never silent: every controller that was not an initial
\* controller entered through a logged enrollment that notified the prior set.
NoSilentLink ==
  \A d \in controllers \ HonestInit : \E r \in linkLog : r.dev = d /\ r.notified # {}

\* A pending request is always bound to an epoch that exists.
PendingEpochSane == pending.active => pending.epoch <= epoch

\* F-2 / ADR-0137: lifts, quarantine-freeze resolution and fork bridges are
\* device-only majorities of N. Only quarantine-freeze resolution may carry
\* challenge signatures from quarantined controllers.
LiftIsDeviceOnlyMajority ==
  last.kind \in {"lift", "resolve", "fork_bridge"} =>
    /\ last.rv = 0
    /\ Cardinality(last.signers) >= Majority(controllers)
    /\ (last.kind \in {"lift", "fork_bridge"}) => last.challenge = {}

\* F-2: a quarantine below the safety threshold is always frozen or resolved.
QuarantineBelowThresholdFreezes ==
  (last.kind = "quarantine" /\ Cardinality(controllers \ quarantined) < Majority(controllers)) => frozen

-----------------------------------------------------------------------------
(* Configuration-specific claims.                                           *)

\* ADR-0111: holding custodians alone never takes over from a live owner.
AttackerNeverOwns == ~AttackerOwns /\ ~(controllers \subseteq AttackerDevs)

\* R-185 one-vote rule, for a static owner device set (OwnerStructural = FALSE):
\* owning requires a device-only majority, or one device fewer when the attacker
\* holds a custodian (but never fewer than one device).
OneVoteBound ==
  AttackerOwns =>
    Cardinality(compromised \cap HonestInit) >=
      IF AttackerMethods >= 1 THEN Max(Majority(HonestInit) - 1, 1) ELSE Majority(HonestInit)

-----------------------------------------------------------------------------
(* Action properties.                                                       *)

\* Anti-rollback: the accepted generation and recovery epoch never go backwards.
NoRollback == [][gen' >= gen /\ epoch' >= epoch]_vars

RetiredIsTerminal ==
  [][retired => UNCHANGED <<controllers, revoked, epoch, retired>>]_vars

FrozenBlocksControllerChanges ==
  [][frozen => UNCHANGED <<controllers, revoked, epoch>>]_vars

LegacyNeverAddsControllers ==
  [][(legacy.claim /\ retired' /\ ~retired) => controllers' = controllers]_vars

\* F-2 / ADR-0137: SAFETY_FREEZE exits only through the matching device-only
\* resolution path: quarantine challenge resolution or a separate fork bridge.
FreezeExitOnlyByResolve ==
  [][(frozen /\ ~frozen') =>
       (last'.kind \in {"resolve", "fork_bridge"} /\ last'.rv = 0)]_vars

-----------------------------------------------------------------------------
(* Trap invariants: expected to be VIOLATED. Each counterexample is a      *)
(* golden vector showing that a documented behavior is reachable.           *)

TrapOwnerRecoversAfterTotalLoss ==
  ~(lost = HonestInit /\ controllers \subseteq HonestSpare /\ controllers # {})

TrapTwoDeviceReconstitution ==
  ~(lost # {} /\ lost \cap controllers = {} /\ controllers \cap HonestInit # {} /\ Cardinality(HonestInit) = 2)

TrapLegacyActivates == ~(retired /\ legacy.armed)

TrapForkFreezes == ~frozen

\* F-2 golden traces, replayed through the §4.1 table by acs_tla_crosscheck.py.
TrapLift == last.kind # "lift"
TrapQuarantineFreeze == ~(last.kind = "quarantine" /\ frozen)
TrapChallengeResolves == ~(last.kind = "resolve" /\ last.challenge # {})
TrapCosignedRecovery == last.kind # "recovery_cosigned"
TrapDelayedRecovery == last.kind # "recovery_delayed"

=============================================================================
