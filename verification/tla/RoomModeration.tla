---------------------------- MODULE RoomModeration ----------------------------
(***************************************************************************)
(* V-016 (Room half) feasibility model: timeout / remove / ban / unban /   *)
(* admit races, moderator demoted while acting, Space-delegated moderation *)
(* scope, epoch-bound write access, late (past-epoch) messages, and report *)
(* evidence access and retention bounds.                                   *)
(*                                                                         *)
(* Derived from the public design prose, not from any existing OM model:   *)
(*   DECENTRALIZED_ABUSE_MODERATION.md  sections 6 (Room moderation        *)
(*     semantics: timeout, remove vs ban), 7 (Space moderation via         *)
(*     SpaceAuthorityBinding), 8 (ModerationDecision), 9 (appeals), 10     *)
(*     (malicious administrators), 11 (EvidencePackage), 24 (invariants)   *)
(*   ABUSE_SAFETY_AND_REPORTING.md, EVIDENCE_AND_EXTERNAL_REPORTING.md     *)
(*   GROUP_COMMIT_AND_STATE_CONVERGENCE.md section 4 (membership changes   *)
(*     bind to the MLS epoch); SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md *)
(*     (every accepted transition satisfies the prior state's policy).     *)
(* Written by a Claude agent (feasibility evidence, not product code).     *)
(*                                                                         *)
(* Mutation = "none" is the proposed-correct semantics; other values       *)
(* disable one rule so TLC must find a counterexample.                     *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
  P,             \* principals
  Rooms,         \* rooms; messages/epochs are modelled for MsgRoom only
  MsgRoom,
  Receivers,     \* honest receiving clients of MsgRoom
  InitGov, InitMods, InitMembers,   \* [Rooms -> SUBSET P]
  SpaceMods,     \* Space moderators (Space governance-delegated)
  BoundRooms,    \* Rooms whose SpaceAuthorityBinding delegates moderation
  Intents,       \* tuple of intent records [actor, room, action, subject]
  Window,        \* past-epoch acceptance window (epochs)
  MaxEpoch, MaxSteps, MaxMsgs, MaxEvidence, RetainMax, HoldMax,
  Mutation       \* "none" | "auth_at_creation" | "global_mod" | "mod_vs_gov"
                 \* | "admit_at_creation" | "no_rekey" | "stale_evidence_acl" | "no_expiry"

VARIABLES
  gov, mods, members, banned, timedOut,    \* [Rooms -> SUBSET P]
  epoch,          \* MLS epoch of MsgRoom
  writersAt,      \* [0..MaxEpoch -> SUBSET P] who may post at each epoch of MsgRoom
  membersAt,      \* [0..MaxEpoch -> SUBSET P] cryptographic roster per epoch
  keys,           \* [P -> SUBSET 0..MaxEpoch] epoch secrets each principal obtained
  created, pending, applied, okAtCreation,
  msgs,           \* set of <<sender, epoch>> sent by (possibly hostile) clients
  recvEpoch, seen, acc, accAt,  \* receiver state; accAt: <<m, receiver epoch at acceptance>>
  evidence,       \* set of evidence records
  demoted,        \* ghost: principals demoted so far
  steps, last

vars == <<gov, mods, members, banned, timedOut, epoch, writersAt, membersAt, keys,
          created, pending, applied, okAtCreation, msgs, recvEpoch, seen, acc, accAt,
          evidence, demoted, steps, last>>
roomVars == <<gov, mods, members, banned, timedOut, epoch, writersAt, membersAt, keys, demoted>>
recvVars == <<msgs, recvEpoch, seen, acc, accAt>>

ModActions == {"ban", "unban", "remove", "timeout", "untimeout", "admit"}

-----------------------------------------------------------------------------
\* Who holds moderation authority in room r (section 6/7): Room moderators and
\* governance, plus Space moderators only where the Room's binding delegates it.
Moderators(r) ==
  IF Mutation = "global_mod"
  THEN UNION {mods[x] \cup gov[x] : x \in Rooms} \cup SpaceMods
  ELSE mods[r] \cup gov[r] \cup (IF r \in BoundRooms THEN SpaceMods ELSE {})

Authorized(i) ==
  LET x == Intents[i] IN
  CASE x.action = "demote" -> x.actor \in gov[x.room]
    [] x.action = "admit"  -> x.actor \in Moderators(x.room)
                              /\ (Mutation = "admit_at_creation" \/ x.subject \notin banned[x.room])
    [] OTHER ->
         /\ x.actor \in Moderators(x.room)
         /\ x.actor # x.subject                             \* no self-lifting / self-action
         \* Proposed rule (section 10): acting against a governance principal is a
         \* constitutional change, not ordinary moderation.
         /\ (Mutation = "mod_vs_gov" \/ x.subject \notin gov[x.room] \/ x.actor \in gov[x.room])

\* The accepted-order authorisation check: against the state the intent is applied to.
ApplyOK(i) == IF Mutation \in {"auth_at_creation", "admit_at_creation"} THEN okAtCreation[i] ELSE Authorized(i)

RosterChange(i) ==
  LET x == Intents[i] IN
  /\ x.room = MsgRoom
  /\ x.action \in {"ban", "remove", "timeout", "untimeout", "admit"}
  /\ Mutation # "no_rekey"

-----------------------------------------------------------------------------
Init ==
  /\ gov = InitGov /\ mods = InitMods /\ members = InitMembers
  /\ banned = [r \in Rooms |-> {}] /\ timedOut = [r \in Rooms |-> {}]
  /\ epoch = 0
  /\ writersAt = [e \in 0..MaxEpoch |-> IF e = 0 THEN InitMembers[MsgRoom] ELSE {}]
  /\ membersAt = [e \in 0..MaxEpoch |-> IF e = 0 THEN InitMembers[MsgRoom] ELSE {}]
  /\ keys = [p \in P |-> IF p \in InitMembers[MsgRoom] THEN {0} ELSE {}]
  /\ created = {} /\ pending = {} /\ applied = {}
  /\ okAtCreation = [i \in DOMAIN Intents |-> FALSE]
  /\ msgs = {} /\ recvEpoch = [r \in Receivers |-> 0]
  /\ seen = [r \in Receivers |-> {}] /\ acc = [r \in Receivers |-> {}] /\ accAt = [r \in Receivers |-> {}]
  /\ evidence = {} /\ demoted = {}
  /\ steps = 0
  /\ last = [kind |-> "init", actor |-> "none", intent |-> 0, authorized |-> TRUE, room |-> "none"]

Env == steps < MaxSteps /\ steps' = steps + 1

\* A client (possibly modified/hostile) signs a moderation intent at any time.
Create(i) ==
  /\ Env /\ i \notin created
  /\ created' = created \cup {i} /\ pending' = pending \cup {i}
  /\ okAtCreation' = [okAtCreation EXCEPT ![i] = Authorized(i)]
  /\ last' = [kind |-> "create", actor |-> Intents[i].actor, intent |-> i, authorized |-> Authorized(i), room |-> Intents[i].room]
  /\ UNCHANGED <<roomVars, applied, recvVars, evidence>>

\* The Room's commit ordering applies pending intents one at a time, each checked
\* against the state immediately before it (rejected intents are dropped).
Apply(i) ==
  LET x == Intents[i]
      r == x.room
      bump == RosterChange(i) /\ ApplyOK(i)
  IN
  /\ i \in pending
  /\ bump => epoch < MaxEpoch
  /\ pending' = pending \ {i}
  /\ applied' = applied \cup {<<i, ApplyOK(i)>>}
  /\ last' = [kind |-> "apply", actor |-> x.actor, intent |-> i,
              authorized |-> (x.actor \in (IF x.action = "demote" THEN gov[r] ELSE mods[r] \cup gov[r] \cup (IF r \in BoundRooms THEN SpaceMods ELSE {}))),
              room |-> r]
  /\ IF ~ApplyOK(i)
     THEN UNCHANGED roomVars
     ELSE
       LET mem2 == CASE x.action \in {"ban", "remove"} -> members[r] \ {x.subject}
                     [] x.action = "admit" -> members[r] \cup {x.subject}
                     [] OTHER -> members[r]
           ban2 == CASE x.action = "ban" -> banned[r] \cup {x.subject}
                     [] x.action = "unban" -> banned[r] \ {x.subject}
                     [] OTHER -> banned[r]
           to2  == CASE x.action = "timeout" -> timedOut[r] \cup {x.subject}
                     [] x.action \in {"untimeout", "ban", "remove"} -> timedOut[r] \ {x.subject}
                     [] OTHER -> timedOut[r]
           mods2 == IF x.action = "demote" THEN mods[r] \ {x.subject} ELSE mods[r]
       IN
       /\ members' = [members EXCEPT ![r] = mem2]
       /\ banned' = [banned EXCEPT ![r] = ban2]
       /\ timedOut' = [timedOut EXCEPT ![r] = to2]
       /\ mods' = [mods EXCEPT ![r] = mods2]
       /\ demoted' = IF x.action = "demote" THEN demoted \cup {x.subject} ELSE demoted
       /\ UNCHANGED gov
       /\ IF bump
          THEN /\ epoch' = epoch + 1
               /\ writersAt' = [writersAt EXCEPT ![epoch + 1] = mem2 \ to2]
               /\ membersAt' = [membersAt EXCEPT ![epoch + 1] = mem2]
               /\ keys' = [p \in P |-> IF p \in mem2 THEN keys[p] \cup {epoch + 1} ELSE keys[p]]
          ELSE IF r = MsgRoom
               \* no_rekey mutation: the roster changes inside the current epoch
               THEN /\ writersAt' = [writersAt EXCEPT ![epoch] = mem2 \ to2]
                    /\ membersAt' = [membersAt EXCEPT ![epoch] = mem2]
                    /\ UNCHANGED <<epoch, keys>>
               ELSE UNCHANGED <<epoch, writersAt, membersAt, keys>>
  /\ UNCHANGED <<created, okAtCreation, recvVars, evidence, steps>>

\* A hostile client may send at any epoch whose secret it ever obtained.
Send(p, e) ==
  /\ Env /\ Cardinality(msgs) < MaxMsgs
  /\ e \in keys[p] /\ <<p, e>> \notin msgs
  /\ msgs' = msgs \cup {<<p, e>>}
  /\ last' = [kind |-> "send", actor |-> p, intent |-> e, authorized |-> TRUE, room |-> MsgRoom]
  /\ UNCHANGED <<roomVars, created, pending, applied, okAtCreation, recvEpoch, seen, acc, accAt, evidence>>

\* Receivers learn commits in order, possibly lagging.
Advance(r) ==
  /\ recvEpoch[r] < epoch
  /\ recvEpoch' = [recvEpoch EXCEPT ![r] = @ + 1]
  /\ last' = [kind |-> "advance", actor |-> r, intent |-> recvEpoch[r] + 1, authorized |-> TRUE, room |-> MsgRoom]
  /\ UNCHANGED <<roomVars, created, pending, applied, okAtCreation, msgs, seen, acc, accAt, evidence, steps>>

\* A receiver decrypts only epochs it has reached and still retains (Window),
\* and accepts only if the sender could post in the message's own epoch.
Deliver(r, m) ==
  /\ m \in msgs /\ m \notin seen[r] /\ m[2] <= recvEpoch[r]
  /\ seen' = [seen EXCEPT ![r] = @ \cup {m}]
  /\ LET ok == recvEpoch[r] - m[2] <= Window /\ m[1] \in writersAt[m[2]] IN
       /\ acc' = IF ok THEN [acc EXCEPT ![r] = @ \cup {m}] ELSE acc
       /\ accAt' = IF ok THEN [accAt EXCEPT ![r] = @ \cup {<<m, recvEpoch[r]>>}] ELSE accAt
  /\ last' = [kind |-> "deliver", actor |-> r, intent |-> m[2], authorized |-> TRUE, room |-> MsgRoom]
  /\ UNCHANGED <<roomVars, created, pending, applied, okAtCreation, msgs, recvEpoch, evidence, steps>>

\* ---- Report evidence (sections 11 and 9). A reporter may select messages it
\* accepted, or fabricate one; message content is reporter-held, never marked
\* independently verifiable. The package is encrypted to the moderators in
\* force at report time and retained for a bounded time (extended while appealed).
Readers == IF Mutation = "stale_evidence_acl" THEN InitMods[MsgRoom] \cup InitGov[MsgRoom]
           ELSE mods[MsgRoom] \cup gov[MsgRoom]

Report(r, m) ==
  /\ Env /\ Cardinality(evidence) < MaxEvidence /\ m \in msgs
  /\ evidence' = evidence \cup {[msg |-> m, reporter |-> r, forged |-> m \notin acc[r], verifiable |-> FALSE,
                                 readers |-> Readers, demotedAtBirth |-> demoted, age |-> 0, hold |-> FALSE]}
  /\ last' = [kind |-> "report", actor |-> r, intent |-> m[2], authorized |-> TRUE, room |-> MsgRoom]
  /\ UNCHANGED <<roomVars, created, pending, applied, okAtCreation, recvVars>>

Expired(it) == it.age >= RetainMax + (IF it.hold THEN HoldMax ELSE 0)

Tick ==
  /\ Env /\ evidence # {}
  /\ Mutation = "no_expiry" \/ \A it \in evidence : ~Expired(it)
  /\ evidence' = {[it EXCEPT !.age = @ + 1] : it \in evidence}
  /\ last' = [kind |-> "tick", actor |-> "clock", intent |-> 0, authorized |-> TRUE, room |-> MsgRoom]
  /\ UNCHANGED <<roomVars, created, pending, applied, okAtCreation, recvVars>>

Appeal(it) ==
  /\ Env /\ it \in evidence /\ ~it.hold /\ ~Expired(it)
  /\ evidence' = (evidence \ {it}) \cup {[it EXCEPT !.hold = TRUE]}
  /\ last' = [kind |-> "appeal", actor |-> it.reporter, intent |-> 0, authorized |-> TRUE, room |-> MsgRoom]
  /\ UNCHANGED <<roomVars, created, pending, applied, okAtCreation, recvVars>>

Purge(it) ==
  /\ Mutation # "no_expiry" /\ it \in evidence /\ Expired(it)
  /\ evidence' = evidence \ {it}
  /\ last' = [kind |-> "purge", actor |-> "store", intent |-> 0, authorized |-> TRUE, room |-> MsgRoom]
  /\ UNCHANGED <<roomVars, created, pending, applied, okAtCreation, recvVars, steps>>

Next ==
  \/ \E i \in DOMAIN Intents : Create(i) \/ Apply(i)
  \/ \E p \in P, e \in 0..MaxEpoch : Send(p, e)
  \/ \E r \in Receivers : Advance(r)
  \/ \E r \in Receivers, m \in msgs : Deliver(r, m) \/ Report(r, m)
  \/ Tick
  \/ \E it \in evidence : Appeal(it) \/ Purge(it)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Safety properties *)

\* Moderation authority is always scoped: every applied action was taken by a
\* principal holding moderation authority in *that* room in the state it was
\* applied to (catches demotion races, cross-room and unbound-Space authority).
ScopedAuthority ==
  (last.kind = "apply" /\ <<last.intent, TRUE>> \in applied) => last.authorized

\* No removed / timed-out / banned member ever has a message accepted in an
\* epoch at or after the one that excluded it: write access ends at the bound epoch.
WriteBoundedByEpoch ==
  \A r \in Receivers : \A m \in acc[r] : m[1] \in writersAt[m[2]]

\* Cryptographic side: a principal only ever holds secrets of epochs whose
\* roster included it (removal is a real rekey, not a UI flag).
NoKeyAfterRemoval == \A p \in P : \A e \in keys[p] : p \in membersAt[e]

\* Old-epoch messages are accepted only inside the retention window.
StaleWriteWindow == \A r \in Receivers : \A x \in accAt[r] : x[2] - x[1][2] <= Window

\* A ban is an admission prohibition: banned principals are never members.
BanExcludes == \A r \in Rooms : banned[r] \cap members[r] = {}

\* Ordinary moderation never removes, bans or silences governance principals.
GovernanceProtected ==
  \A r \in Rooms : gov[r] \subseteq members[r] /\ gov[r] \cap (banned[r] \cup timedOut[r]) = {}

\* Evidence is readable only by moderators in force when it was reported,
\* never by a moderator already demoted; message content is never "verifiable".
EvidenceScoped == \A it \in evidence : it.readers \cap it.demotedAtBirth = {} /\ ~it.verifiable

\* Retention is bounded (appeal hold extends it by at most HoldMax).
RetentionBound == \A it \in evidence : it.age <= RetainMax + (IF it.hold THEN HoldMax ELSE 0)

\* Receivers that saw the same message agree on it (checked as a residual risk).
ReceiversAgree ==
  \A r1, r2 \in Receivers : \A m \in seen[r1] \cap seen[r2] : (m \in acc[r1]) = (m \in acc[r2])

(* Golden-witness traps *)
Ok(k) == <<k, TRUE>> \in applied
Rejected(k) == <<k, FALSE>> \in applied
TrapDemotedModeratorRejected == ~(\E k \in DOMAIN Intents : Rejected(k) /\ Intents[k].actor \in demoted)
TrapUnbanAfterBan == ~(\E k1, k2 \in DOMAIN Intents :
     Ok(k1) /\ Ok(k2) /\ Intents[k1].action = "ban" /\ Intents[k2].action = "unban"
     /\ Intents[k2].subject \notin banned[Intents[k2].room])
TrapBannedAdmitRejected == ~(\E k \in DOMAIN Intents : Rejected(k) /\ Intents[k].action = "admit")
TrapLateOldEpochWrite == ~(\E r \in Receivers : \E x \in accAt[r] :
     x[1][1] \notin writersAt[x[2]] /\ x[1][1] \in writersAt[x[1][2]])
TrapEvidenceHeldOnAppeal == ~(\E it \in evidence : it.hold /\ it.age = RetainMax + HoldMax)

-----------------------------------------------------------------------------
(* Scenario values bound from the .cfg files with `<-`. *)
I(a, r, act, s) == [actor |-> a, room |-> r, action |-> act, subject |-> s]
RR == {"R1", "R2"}
GovScen == [r \in RR |-> {"g"}]
ModsScen == [r \in RR |-> IF r = "R1" THEN {"m", "n"} ELSE {"n"}]
MembersScen == [r \in RR |-> IF r = "R1" THEN {"g", "m", "n", "u"} ELSE {"g", "n", "u"}]
\* A: concurrent ban/unban, moderator demoted while acting, admit racing a ban.
IntentsRaces == << I("m", "R1", "ban", "u"), I("n", "R1", "unban", "u"), I("g", "R1", "demote", "m"),
                   I("n", "R1", "admit", "u"), I("m", "R1", "timeout", "u") >>
\* B: scope attacks: other room, unbound Space, moderator vs governance, self-lift.
IntentsScope == << I("m", "R2", "ban", "u"), I("s", "R2", "ban", "u"), I("s", "R1", "timeout", "u"),
                   I("m", "R1", "remove", "g"), I("u", "R1", "untimeout", "u") >>
\* C: epoch-bound write access with lagging receivers.
IntentsWrite == << I("m", "R1", "timeout", "u"), I("n", "R1", "untimeout", "u"), I("m", "R1", "remove", "u") >>
\* D: evidence while a moderator is demoted.
IntentsEvidence == << I("g", "R1", "demote", "m") >>
=============================================================================
