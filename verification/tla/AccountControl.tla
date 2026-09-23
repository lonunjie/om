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
  AttackerMayRecover \* TRUE: attacker may file recovery requests

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
  linkLog        \* one record per enrollment: [dev, notified]

vars == <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
          gen, epoch, pending, retired, legacy, idle, frozen, linkLog>>

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

Methods(p) == IF p = "owner" THEN OwnerMethods ELSE atkMethods
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

\* Environment assumption: a live owner device answers a recovery notice.
OwnerMustReact ==
  /\ pending.active
  /\ pending.party = "attacker"
  /\ pending.epoch = epoch
  /\ \/ pending.methods = 1 /\ Usable("owner") # {}
     \/ pending.methods = 2 /\ \E V \in SUBSET Usable("owner"), rv \in 0..1 : Authorized("owner", V, rv)

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
  /\ UNCHANGED <<compromised, offline, lost, atkMethods, epoch, pending, retired, legacy,
                 idle, frozen, linkLog>>

\* Any single usable controller may quarantine another; it never shrinks N.
Quarantine(p, a, t) ==
  /\ CanTransition
  /\ p = "attacker" \/ OwnerStructural
  /\ a \in Usable(p)
  /\ t \in controllers \ (quarantined \cup {a})
  /\ quarantined' = quarantined \cup {t}
  /\ gen' = gen + 1
  /\ UNCHANGED <<controllers, revoked, compromised, offline, lost, atkMethods, epoch,
                 pending, retired, legacy, idle, frozen, linkLog>>

Unquarantine(p, V, rv, t) ==
  /\ CanTransition
  /\ Authorized(p, V, rv)
  /\ t \in quarantined
  /\ quarantined' = quarantined \ {t}
  /\ gen' = gen + 1
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
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, retired,
                 legacy, idle, frozen, linkLog>>

Retire(p, V, rv) ==
  /\ CanTransition
  /\ p = "attacker" \/ OwnerStructural
  /\ Authorized(p, V, rv)
  /\ retired' = TRUE
  /\ pending' = NoPending
  /\ gen' = gen + 1
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 epoch, legacy, idle, frozen, linkLog>>

-----------------------------------------------------------------------------
(* ADR-0111 recovery.                                                       *)

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
                 gen, epoch, retired, legacy, idle, frozen, linkLog>>

\* A single-method request can be vetoed by any usable controller of the other party.
\* (A two-method request can be stopped only by a majority act: RotateRecovery.)
Veto(p) ==
  /\ pending.active
  /\ pending.party # p
  /\ pending.methods = 1
  /\ Usable(p) # {}
  /\ pending' = NoPending
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, retired, legacy, idle, frozen, linkLog>>

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
  /\ UNCHANGED <<compromised, offline, lost, atkMethods, retired, legacy, idle, frozen>>

\* Delayed: the full delay elapsed and nobody stopped the request.
DelayedComplete(p) ==
  /\ CanTransition
  /\ pending.active /\ pending.party = p /\ pending.epoch = epoch
  /\ pending.elapsed >= Delay
  /\ Reconstitute(p, {})
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
                 gen, epoch, retired, legacy, idle, linkLog>>

\* Only a device-only majority (no recovery vote) resolves a freeze.
ResolveFreeze(p, V) ==
  /\ frozen /\ ~retired
  /\ V \subseteq Usable(p)
  /\ Cardinality(V) >= Majority(controllers)
  /\ frozen' = FALSE
  /\ gen' = gen + 1
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
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 epoch, pending, retired, idle, frozen, linkLog>>

ClaimLegacy ==
  /\ legacy.armed /\ ~legacy.claim /\ ~retired
  /\ legacy' = [legacy EXCEPT !.claim = TRUE, !.elapsed = 0]
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, pending, retired, idle, frozen, linkLog>>

ContestLegacy ==
  /\ legacy.claim
  /\ Usable("owner") # {}
  /\ legacy' = [legacy EXCEPT !.claim = FALSE, !.elapsed = 0]
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, offline, lost, atkMethods,
                 gen, epoch, pending, retired, idle, frozen, linkLog>>

ActivateLegacy ==
  /\ legacy.claim /\ ~retired
  /\ idle >= LegacyWait
  /\ legacy.elapsed >= LegacyWait
  /\ retired' = TRUE
  /\ pending' = NoPending
  /\ gen' = gen + 1
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
                 gen, epoch, retired, frozen, linkLog>>

Compromise(d) ==
  /\ d \in CanCompromise \ compromised
  /\ compromised' = compromised \cup {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, offline, lost, atkMethods, gen, epoch,
                 pending, retired, legacy, idle, frozen, linkLog>>

GoOffline(d) ==
  /\ OwnerMayGoOffline
  /\ d \in (HonestInit \cup HonestSpare) \ (offline \cup lost)
  /\ offline' = offline \cup {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, lost, atkMethods, gen,
                 epoch, pending, retired, legacy, idle, frozen, linkLog>>

GoOnline(d) ==
  /\ d \in offline
  /\ offline' = offline \ {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, lost, atkMethods, gen,
                 epoch, pending, retired, legacy, idle, frozen, linkLog>>

Lose(d) ==
  /\ OwnerMayGoOffline
  /\ d \in (HonestInit \cup HonestSpare) \ lost
  /\ lost' = lost \cup {d}
  /\ offline' = offline \ {d}
  /\ UNCHANGED <<controllers, quarantined, revoked, compromised, atkMethods, gen, epoch,
                 pending, retired, legacy, idle, frozen, linkLog>>

Next ==
  \/ \E p \in Parties, V \in SUBSET (HonestInit \cup HonestSpare \cup AttackerDevs), rv \in 0..1 :
        \/ \E d \in HonestSpare \cup AttackerDevs : Link(p, V, rv, d)
        \/ \E t \in controllers : Revoke(p, V, rv, t) \/ Unquarantine(p, V, rv, t)
        \/ RotateRecovery(p, V, rv)
        \/ Retire(p, V, rv)
  \/ \E p \in Parties, a \in controllers, t \in controllers : Quarantine(p, a, t)
  \/ \E p \in Parties, d \in HonestSpare \cup AttackerDevs, m \in 1..2 : RequestRecovery(p, d, m)
  \/ \E p \in Parties : Veto(p) \/ DelayedComplete(p)
  \/ \E p \in Parties, V \in SUBSET controllers : CosignComplete(p, V) \/ ResolveFreeze(p, V)
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

-----------------------------------------------------------------------------
(* Trap invariants: expected to be VIOLATED. Each counterexample is a      *)
(* golden vector showing that a documented behavior is reachable.           *)

TrapOwnerRecoversAfterTotalLoss ==
  ~(lost = HonestInit /\ controllers \subseteq HonestSpare /\ controllers # {})

TrapTwoDeviceReconstitution ==
  ~(lost # {} /\ lost \cap controllers = {} /\ controllers \cap HonestInit # {} /\ Cardinality(HonestInit) = 2)

TrapLegacyActivates == ~(retired /\ legacy.armed)

TrapForkFreezes == ~frozen
=============================================================================
