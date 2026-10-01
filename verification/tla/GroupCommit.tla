---------------------------- MODULE GroupCommit -----------------------------
(***************************************************************************)
(* V-004 formal model: GroupCommitOrdering under crash, partition and      *)
(* coordinator equivocation.                                               *)
(*                                                                         *)
(* Derived from the public design prose, not from existing OM models:      *)
(*   GROUP_COMMIT_AND_STATE_CONVERGENCE.md  sections 2-6 (coordinator,     *)
(*     succession, reductions, journal, account-control events)            *)
(*   ADR-0074, ADR-0128 rules 1-2, ADR-0163 section 4                      *)
(* plus the ordering rule proposed by ADR-0173 (see its text):             *)
(*   among valid commits sharing one parent, the canonical successor is    *)
(*   the minimum of (non-reducing?, direct?, author index, target index).  *)
(* Written by a Claude agent (feasibility evidence, not product code).     *)
(*                                                                         *)
(* Abstraction. A "commit" is one authority-affecting MLS Commit with its  *)
(* GroupControlIntent/GroupCommitBinding. A member's state is the chain of *)
(* commits it has merged. MLS cannot roll back a merged epoch (probe       *)
(* s2_partition_fork_and_readd), so a member that merged a losing commit   *)
(* has forked and can only recover by a fork-resolution re-add from a      *)
(* member on the winning chain, which is roster-neutral and therefore not  *)
(* an authority-chain element here.                                        *)
(*                                                                         *)
(* Mutation = "none" is the proposed-correct semantics; other values      *)
(* disable one rule so TLC must find a counterexample.                     *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
  Members,        \* the group's devices
  Succ,           \* coordinator succession order: a sequence listing every member once
  Faulty,         \* members that may go offline, censor, or equivocate
  Intents,        \* set of [by, rm]: pending reducing intents (the by-device wants rm removed)
  MaxPub,         \* bound on published commits that are not one of the modeled intents
  AllowNop,       \* TRUE: the coordinator may also publish non-reducing commits
  AllowReplace,   \* TRUE: any non-coordinator may publish a coordinator replacement
  Mutation        \* "none" | "hash_only" | "no_detect" | "readd_removed" | "journal_loss"
                  \* | "coord_self"

NONE == "none"
N == Len(Succ)
Idx == [m \in Members |-> CHOOSE i \in 1..N : Succ[i] = m]
Range(s) == { s[i] : i \in 1..Len(s) }

VARIABLES
  pub,            \* set of published commits
  seen,           \* [Members -> SUBSET pub]
  chain,          \* [Members -> Seq(id)] merged commit ids
  status,         \* [Members -> {"active","forked","out"}]
  alive,          \* [Members -> BOOLEAN]
  journal         \* set of <<author, parent>> pairs already published by honest authors

vars == <<pub, seen, chain, status, alive, journal>>

id(c) == <<c.a, c.k, c.o, c.r>>

-----------------------------------------------------------------------------
\* Roster, coordinator and validity at a chain.
Roster(ch) == Members \ { ch[i][4] : i \in 1..Len(ch) }

Dist(k, j) == (j - k + N) % N

NextLive(pc, R) ==
  IF R \ {pc} = {} THEN pc
  ELSE CHOOSE m \in R \ {pc} :
         \A m2 \in R \ {pc} : Dist(Idx[pc], Idx[m]) <= Dist(Idx[pc], Idx[m2])

RECURSIVE CoordOf(_)
CoordOf(ch) ==
  IF ch = <<>> THEN Succ[1]
  ELSE LET prev == SubSeq(ch, 1, Len(ch) - 1)
           pc   == CoordOf(prev)
           last == ch[Len(ch)]
       IN IF last[3] = "replace" \/ last[4] = pc
          THEN NextLive(pc, Roster(ch))
          ELSE pc

\* A commit is valid at its parent chain c.p (a Seq of ids).
Valid(c) ==
  LET R  == Roster(c.p)
      co == CoordOf(c.p)
  IN /\ c.a \in R
     /\ \/ /\ c.o = "remove" /\ c.r \in R
           \* the coordinator never sequences its own removal; a direct self-leave is allowed
           /\ (c.k = "coord" => (c.a = co /\ (Mutation = "coord_self" \/ c.r # c.a)))
        \/ /\ c.o = "replace" /\ c.r = NONE /\ c.k = "direct" /\ c.a # co
        \/ /\ c.o = "nop" /\ c.r = NONE /\ c.k = "coord" /\ c.a = co

\* Total order on commits sharing a parent. Smaller wins. KeyId works on a commit id <<a,k,o,r>>.
RIdx(r) == IF r = NONE THEN 0 ELSE Idx[r]
KeyId(x) ==
  IF Mutation = "hash_only"
  THEN Idx[x[1]] * 10 + RIdx(x[4])
  ELSE ((IF x[3] = "remove" THEN 0 ELSE 1) * 2 + (IF x[2] = "coord" THEN 0 ELSE 1)) * 100
       + Idx[x[1]] * 10 + RIdx(x[4])
Key(c) == KeyId(<<c.a, c.k, c.o, c.r>>)
Better(c1, c2) == Key(c1) < Key(c2)

\* Canonical chain over everything published: follow the best valid commit at each parent.
RECURSIVE CanonFrom(_, _)
CanonFrom(ch, depth) ==
  LET cands == { c \in pub : c.p = ch /\ Valid(c) }
  IN IF cands = {} \/ depth = 0 THEN ch
     ELSE LET best == CHOOSE c \in cands : \A d \in cands : Key(c) <= Key(d)
          IN CanonFrom(Append(ch, id(best)), depth - 1)
Canon == CanonFrom(<<>>, N + MaxPub + 1)

-----------------------------------------------------------------------------
Commit(a, k, o, r, p) == [a |-> a, k |-> k, o |-> o, r |-> r, p |-> p]

Init ==
  /\ pub = {}
  /\ seen = [m \in Members |-> {}]
  /\ chain = [m \in Members |-> <<>>]
  /\ status = [m \in Members |-> "active"]
  /\ alive \in [Members -> BOOLEAN]
  /\ \A m \in Members : m \notin Faulty => alive[m]
  /\ journal = {}

Honest(m) == m \notin Faulty

BestSeenAt(m, ch) == { c \in seen[m] : c.p = ch /\ Valid(c) }

\* An honest member never publishes a commit worse than one it has already seen at that parent.
NoBetterSeen(m, c) == \A d \in BestSeenAt(m, c.p) : ~Better(d, c)

IsIntentCommit(c) == \E i \in Intents : c.o = "remove" /\ c.a = i.by /\ c.r = i.rm
Publish(m, c) ==
  /\ alive[m] /\ status[m] = "active" /\ c.a = m /\ c.p = chain[m]
  /\ Valid(c) /\ c \notin pub
  \* only commits that are not one of the modeled intents are bounded
  /\ (~IsIntentCommit(c) => Cardinality({ d \in pub : ~IsIntentCommit(d) }) < MaxPub)
  /\ (Honest(m) => NoBetterSeen(m, c))
  /\ (Honest(m) /\ Mutation # "journal_loss" => <<m, c.p>> \notin journal)
  /\ pub'     = pub \cup {c}
  /\ seen'    = [seen EXCEPT ![m] = @ \cup {c}]
  /\ chain'   = [chain EXCEPT ![m] = Append(@, id(c))]
  /\ status'  = [status EXCEPT ![m] = IF c.r = m THEN "out" ELSE @]
  /\ journal' = journal \cup {<<m, c.p>>}
  /\ UNCHANGED alive

\* A member publishes the commit for one of its own intents (coordinator-sequenced if it is the
\* coordinator, direct otherwise: the timeout is adversarially always elapsed).
PublishIntent(m) ==
  \E i \in Intents : i.by = m /\
    \E k \in (IF m = CoordOf(chain[m]) THEN {"coord"} ELSE {"direct"}) \cup (IF i.rm = m THEN {"direct"} ELSE {}) :
      Publish(m, Commit(m, k, "remove", i.rm, chain[m]))

\* Other behaviour: the coordinator's non-reducing commit, a replacement, any member's direct removal.
PublishOther(m) ==
  \E c \in { Commit(m, k, o, r, chain[m]) :
               k \in {"coord", "direct"}, o \in {"remove", "nop", "replace"}, r \in Members \cup {NONE} } :
    /\ (c.o = "nop" => AllowNop)
    /\ (c.o = "replace" => AllowReplace)
    /\ (c.o = "remove" => m \in Faulty)
    /\ Publish(m, c)

Deliver(m, c) ==
  /\ alive[m] /\ c \in pub /\ c \notin seen[m]
  /\ seen' = [seen EXCEPT ![m] = @ \cup {c}]
  /\ UNCHANGED <<pub, chain, status, alive, journal>>

Accept(m, c) ==
  /\ alive[m] /\ status[m] = "active"
  /\ c \in seen[m] /\ c.p = chain[m] /\ Valid(c)
  /\ \A d \in BestSeenAt(m, c.p) : ~Better(d, c)
  /\ chain'  = [chain EXCEPT ![m] = Append(@, id(c))]
  /\ status' = [status EXCEPT ![m] = IF c.r = m THEN "out" ELSE @]
  /\ UNCHANGED <<pub, seen, alive, journal>>

\* Epochs at which m's merged branch lost: m has seen a better valid commit at that parent.
LostAt(m) ==
  { i \in 1..Len(chain[m]) :
      \E d \in seen[m] :
        /\ d.p = SubSeq(chain[m], 1, i - 1) /\ Valid(d)
        /\ id(d) # chain[m][i]
        /\ Key(d) < KeyId(chain[m][i]) }
Lost(m) == LostAt(m) # {}

DetectFork(m) ==
  /\ Mutation # "no_detect"
  \* "out" counts too: a removal merged on a losing branch is not a canonical removal (finding F-24).
  /\ alive[m] /\ status[m] \in {"active", "out"} /\ Lost(m)
  /\ status' = [status EXCEPT ![m] = "forked"]
  /\ UNCHANGED <<pub, seen, chain, alive, journal>>

\* A member w on a chain that does not contain the commit m lost with re-adds the forked member m
\* (roster-neutral fork resolution). A forked member no longer in the winning roster is out.
Rejoin(m, w) ==
  /\ alive[m] /\ alive[w] /\ status[m] = "forked" /\ status[w] = "active"
  /\ w \in Roster(chain[w]) /\ ~Lost(w)
  /\ LET i == CHOOSE j \in LostAt(m) : \A k \in LostAt(m) : j <= k
     IN /\ Len(chain[w]) >= i
        /\ SubSeq(chain[w], 1, i - 1) = SubSeq(chain[m], 1, i - 1)
        /\ chain[w][i] # chain[m][i]
        \* w's chain contains the best commit m knows of at the point where m lost
        /\ \A d \in seen[m] :
             (d.p = SubSeq(chain[m], 1, i - 1) /\ Valid(d)) => KeyId(chain[w][i]) <= Key(d)
  \* only a device in w's roster can be re-added (the OM roster check; probe s3_external_commit_policy)
  /\ (m \in Roster(chain[w]) \/ Mutation = "readd_removed")
  /\ chain'  = [chain EXCEPT ![m] = chain[w]]
  /\ status' = [status EXCEPT ![m] = "active"]
  /\ seen' = [seen EXCEPT ![m] = @ \cup seen[w]]
  /\ UNCHANGED <<pub, alive, journal>>

\* Crash (faulty members only): the device goes offline for good. Its journal survives (ADR-0163 rule 2).
Crash(m) ==
  /\ m \in Faulty /\ alive[m]
  /\ alive' = [alive EXCEPT ![m] = FALSE]
  /\ UNCHANGED <<pub, seen, chain, status, journal>>

\* Mutation journal_loss: a restart forgets the member's last merged commit and its journal entry
\* (ADR-0163 rules 2 and 9 say the prepared state must survive; this is what happens without them).
JournalLoss(m) ==
  /\ Mutation = "journal_loss" /\ Honest(m) /\ alive[m] /\ status[m] = "active" /\ Len(chain[m]) > 0
  /\ chain'   = [chain EXCEPT ![m] = SubSeq(@, 1, Len(@) - 1)]
  /\ journal' = journal \ {<<m, SubSeq(chain[m], 1, Len(chain[m]) - 1)>>}
  /\ UNCHANGED <<pub, seen, status, alive>>

Protocol ==
  \/ \E m \in Members : PublishIntent(m) \/ PublishOther(m) \/ DetectFork(m)
  \/ \E m \in Members, c \in pub : Deliver(m, c) \/ Accept(m, c)
  \/ \E m, w \in Members : Rejoin(m, w)

Next == Protocol \/ \E m \in Members : Crash(m) \/ JournalLoss(m)

Spec == Init /\ [][Next]_vars /\ WF_vars(Protocol)

-----------------------------------------------------------------------------
\* ---- Safety ---------------------------------------------------------------
TypeOK ==
  /\ pub \subseteq [a : Members, k : {"coord", "direct"}, o : {"remove", "nop", "replace"},
                    r : Members \cup {NONE}, p : Seq(Members \X {"coord", "direct"} \X
                                                   {"remove", "nop", "replace"} \X (Members \cup {NONE}))]
  /\ status \in [Members -> {"active", "forked", "out"}]

\* Every merged commit was valid at its parent (authorization is evaluated at the parent state).
OnlyValidMerged ==
  \A m \in Members : \A i \in 1..Len(chain[m]) :
    \E c \in pub : id(c) = chain[m][i] /\ c.p = SubSeq(chain[m], 1, i - 1) /\ Valid(c)

\* An honest author never publishes two commits at one parent (ADR-0163 rules 3 and 9).
NoHonestSelfEquivocation ==
  \A c1, c2 \in pub : (c1.a = c2.a /\ c1.a \notin Faulty /\ c1.p = c2.p) => c1 = c2

\* An active member is in its own chain's roster.
ActiveInRoster == \A m \in Members : status[m] = "active" => m \in Roster(chain[m])

\* A reducing commit always outranks a non-reducing one at the same parent, so a coordinator cannot use
\* a same-parent race to keep a reduction from landing.
ReductionOutranksNonReduction ==
  \A c1, c2 \in pub :
    (c1.p = c2.p /\ c1.o = "remove" /\ c2.o # "remove" /\ Valid(c1) /\ Valid(c2)) => Key(c1) < Key(c2)

\* The coordinator never sequences its own removal or replacement.
CoordinatorNeverSequencesSelf ==
  \A c \in pub : Valid(c) /\ c.k = "coord" => (c.a = CoordOf(c.p) /\ c.r # c.a /\ c.o # "replace")

\* ---- Convergence (terminal states, under WF(Protocol)) --------------------
AliveActiveExists == \E w \in Members : alive[w] /\ status[w] = "active"

Converged ==
  /\ \A m \in Members : (alive[m] /\ status[m] = "active") => chain[m] = Canon
  /\ AliveActiveExists =>
       /\ \A m \in Members : (alive[m] /\ m \in Roster(Canon)) => status[m] = "active"
       /\ \A m \in Members : (alive[m] /\ m \notin Roster(Canon)) => status[m] \in {"out", "forked"}

\* Reduction liveness: every intent is applied, or moot (target gone, author gone).
IntentsSettled ==
  \A i \in Intents :
    \/ i.rm \notin Roster(Canon)
    \/ i.by \notin Roster(Canon)
    \/ ~alive[i.by]
    \/ ~AliveActiveExists

EventuallyConverged == <>[](Converged /\ IntentsSettled)
-----------------------------------------------------------------------------
\* ---- Scenarios (config replacements) -------------------------------------
SuccS == <<"a", "b", "c">>
\* b removes c; c removes the coordinator a.
IntentsRace == { [by |-> "b", rm |-> "c"], [by |-> "c", rm |-> "a"] }
\* b removes the (offline) coordinator a.
IntentsCoordGone == { [by |-> "b", rm |-> "a"] }
=============================================================================
