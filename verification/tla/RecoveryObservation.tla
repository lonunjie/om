------------------------- MODULE RecoveryObservation -------------------------
(***************************************************************************)
(* V-002 / R-278 / ADR-0131 rule 6: a recovery-alone successor is final     *)
(* for a verifier only after THAT verifier has observed the RecoveryRequest *)
(* for the full policy delay, with no valid RecoveryVeto seen. There is no  *)
(* trusted global recovery clock: the Home or custodian that delivers the   *)
(* request (the adversary here) may withhold it, deliver it late to chosen  *)
(* verifiers, attach any "filed N ticks ago" claim, and withhold vetoes.    *)
(*                                                                         *)
(* `real[v]` is a ghost: true time elapsed since v first saw the request.  *)
(* `local[v]` is what v's own monotonic clock tells it. The rule under     *)
(* test is that acceptance is driven by `local`, which starts at 0 on first *)
(* observation. TrustClaimedAge = TRUE is the mutation that starts it at    *)
(* the claimed age instead; TLC must find the acceleration attack then.    *)
(*                                                                         *)
(* Environment assumption (as in AccountControl.tla OwnerMustReact): with  *)
(* ControllersReact, a live controller that has seen the request vetoes    *)
(* before time advances.                                                   *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS
  Verifiers,        \* every party that validates the account's control state
  LiveControllers,  \* current, non-quarantined controller devices (a subset of Verifiers)
  Delay,            \* RecoveryPolicy activation delay, in ticks
  TrustClaimedAge,  \* MUTATION switch: TRUE = verifier trusts the deliverer's age claim
  ControllersReact, \* environment: a live controller that saw the request vetoes promptly
  MaxReal           \* bound on real time

VARIABLES
  now,        \* real time (ghost)
  seen,       \* v has observed the RecoveryRequest
  local,      \* v's own elapsed observation time, capped at Delay
  real,       \* ghost: real time since v first observed it, capped at MaxReal
  vetoIssued, \* some live controller signed a RecoveryVeto
  vetoSeen,   \* v has observed a valid RecoveryVeto
  accepted    \* v treats the recovery-alone successor as final

vars == <<now, seen, local, real, vetoIssued, vetoSeen, accepted>>

Min(a, b) == IF a < b THEN a ELSE b

Init ==
  /\ now = 0
  /\ seen = [v \in Verifiers |-> FALSE]
  /\ local = [v \in Verifiers |-> 0]
  /\ real = [v \in Verifiers |-> 0]
  /\ vetoIssued = FALSE
  /\ vetoSeen = [v \in Verifiers |-> FALSE]
  /\ accepted = [v \in Verifiers |-> FALSE]

\* The adversarial deliverer reveals the request to v at a time of its choosing, with any age claim.
Deliver(v, claim) ==
  /\ ~seen[v]
  /\ seen' = [seen EXCEPT ![v] = TRUE]
  /\ local' = [local EXCEPT ![v] = IF TrustClaimedAge THEN claim ELSE 0]
  /\ real' = [real EXCEPT ![v] = 0]
  /\ UNCHANGED <<now, vetoIssued, vetoSeen, accepted>>

ReactionPending == \E c \in LiveControllers : seen[c] /\ ~vetoIssued

Tick ==
  /\ now < MaxReal
  /\ ControllersReact => ~ReactionPending
  /\ now' = now + 1
  /\ local' = [v \in Verifiers |-> IF seen[v] THEN Min(local[v] + 1, Delay) ELSE local[v]]
  /\ real' = [v \in Verifiers |-> IF seen[v] THEN Min(real[v] + 1, MaxReal) ELSE real[v]]
  /\ UNCHANGED <<seen, vetoIssued, vetoSeen, accepted>>

IssueVeto(c) ==
  /\ c \in LiveControllers /\ seen[c] /\ ~vetoIssued /\ ~accepted[c]
  /\ vetoIssued' = TRUE
  /\ vetoSeen' = [vetoSeen EXCEPT ![c] = TRUE]
  /\ UNCHANGED <<now, seen, local, real, accepted>>

\* Veto propagation is also adversarial: it may reach v late or never.
DeliverVeto(v) ==
  /\ vetoIssued /\ ~vetoSeen[v]
  /\ vetoSeen' = [vetoSeen EXCEPT ![v] = TRUE]
  /\ UNCHANGED <<now, seen, local, real, vetoIssued, accepted>>

Accept(v) ==
  /\ seen[v] /\ ~accepted[v] /\ ~vetoSeen[v]
  /\ local[v] >= Delay
  /\ accepted' = [accepted EXCEPT ![v] = TRUE]
  /\ UNCHANGED <<now, seen, local, real, vetoIssued, vetoSeen>>

Next ==
  \/ \E v \in Verifiers, claim \in 0..Delay : Deliver(v, claim)
  \/ Tick
  \/ \E c \in LiveControllers : IssueVeto(c)
  \/ \E v \in Verifiers : DeliverVeto(v) \/ Accept(v)

Spec == Init /\ [][Next]_vars

TypeOK ==
  /\ now \in 0..MaxReal
  /\ \A v \in Verifiers : local[v] \in 0..Delay /\ real[v] \in 0..MaxReal
  /\ vetoIssued \in BOOLEAN

-----------------------------------------------------------------------------
(* R-278: acceptance never precedes the verifier's own full interval, however late *)
(* it saw the request and whatever age the deliverer claimed.                      *)
AcceptOnlyAfterOwnInterval == \A v \in Verifiers : accepted[v] => real[v] >= Delay

\* With the reaction assumption, a live controller is never overridden by recovery alone.
LiveControllerNeverAccepts == ControllersReact => \A c \in LiveControllers : ~accepted[c]

\* A verifier that has seen a valid veto never accepts afterwards.
VetoBlocksAcceptance ==
  [][\A v \in Verifiers : (~accepted[v] /\ accepted'[v]) => ~vetoSeen[v]]_vars

\* Once accepted, stays accepted (no flapping), and observation never restarts.
AcceptanceStable == [][\A v \in Verifiers : accepted[v] => accepted'[v] /\ seen'[v]]_vars

-----------------------------------------------------------------------------
(* Golden trap: one verifier has accepted while a verifier that saw the request   *)
(* later is still inside its own interval (the rule's intended behavior).         *)
TrapLateVerifierStillPending ==
  ~(\E v1, v2 \in Verifiers :
      accepted[v1] /\ seen[v2] /\ ~accepted[v2] /\ ~vetoSeen[v2] /\ real[v2] < Delay)

(* Residual risk: if the deliverer withholds the request from EVERY live         *)
(* controller, a partitioned non-controller verifier can accept after its own   *)
(* full interval. Mitigations outside this rule: Homes fan requests out; the   *)
(* controllers' own successor states create a fork (§8) on first contact.       *)
NoPeerAcceptsWhileControllersUninformed ==
  ~(\E v \in Verifiers \ LiveControllers : accepted[v] /\ \A c \in LiveControllers : ~seen[c])
=============================================================================
