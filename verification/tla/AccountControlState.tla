------------------------- MODULE AccountControlState -------------------------
EXTENDS Naturals, FiniteSets

CONSTANTS InitialControllers, CandidateControllers, ProposalKinds, RecoveryEnabled

VARIABLES controllers, quarantined, revoked, frozen, generation, proposals

vars == <<controllers, quarantined, revoked, frozen, generation, proposals>>

Majority(S) == (Cardinality(S) \div 2) + 1
Authorized(A) == A \subseteq controllers /\ Cardinality(A) >= Majority(controllers)

Init ==
  /\ controllers = InitialControllers
  /\ quarantined = {}
  /\ revoked = {}
  /\ frozen = FALSE
  /\ generation = 0
  /\ proposals = {}

Quarantine(actor, target) ==
  /\ ~frozen
  /\ actor \in controllers
  /\ target \in controllers
  /\ actor # target
  /\ quarantined' = quarantined \cup {target}
  /\ generation' = generation + 1
  /\ UNCHANGED <<controllers, revoked, frozen, proposals>>

Promote(approvals, candidate) ==
  /\ ~frozen
  /\ candidate \in CandidateControllers \ controllers \ revoked
  /\ Authorized(approvals)
  /\ controllers' = controllers \cup {candidate}
  /\ generation' = generation + 1
  /\ UNCHANGED <<quarantined, revoked, frozen, proposals>>

PermanentRevoke(approvals, target) ==
  /\ ~frozen
  /\ target \in controllers
  /\ Authorized(approvals)
  /\ controllers' = controllers \ {target}
  /\ quarantined' = quarantined \ {target}
  /\ revoked' = revoked \cup {target}
  /\ generation' = generation + 1
  /\ UNCHANGED <<frozen, proposals>>

RecordConstitutionalProposal(kind) ==
  /\ ~frozen
  /\ kind \in ProposalKinds
  /\ proposals' = proposals \cup {kind}
  /\ IF Cardinality(proposals') >= 2
        THEN frozen' = TRUE
        ELSE frozen' = frozen
  /\ UNCHANGED <<controllers, quarantined, revoked, generation>>

ResolveFreeze(approvals) ==
  /\ frozen
  /\ Authorized(approvals) \/ RecoveryEnabled
  /\ frozen' = FALSE
  /\ proposals' = {}
  /\ generation' = generation + 1
  /\ UNCHANGED <<controllers, quarantined, revoked>>

Next ==
  \/ \E a, t \in controllers : Quarantine(a, t)
  \/ \E approvals \in SUBSET controllers, c \in CandidateControllers : Promote(approvals, c)
  \/ \E approvals \in SUBSET controllers, t \in controllers : PermanentRevoke(approvals, t)
  \/ \E k \in ProposalKinds : RecordConstitutionalProposal(k)
  \/ \E approvals \in SUBSET controllers : ResolveFreeze(approvals)

TypeOK ==
  /\ controllers \subseteq InitialControllers \cup CandidateControllers
  /\ quarantined \subseteq controllers
  /\ revoked \subseteq InitialControllers \cup CandidateControllers
  /\ controllers \cap revoked = {}
  /\ frozen \in BOOLEAN
  /\ generation \in Nat
  /\ proposals \subseteq ProposalKinds

QuarantineDoesNotShrinkDenominator ==
  Majority(controllers) = (Cardinality(controllers) \div 2) + 1

RevokedNeverActive == controllers \cap revoked = {}
FrozenHasConflictEvidence == frozen => Cardinality(proposals) >= 2

Spec == Init /\ [][Next]_vars

=============================================================================
