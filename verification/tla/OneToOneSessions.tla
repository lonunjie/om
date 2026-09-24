---------------------------- MODULE OneToOneSessions ----------------------------
(***************************************************************************)
(* Feasibility model for V-003 (one_to_one_session_model). Not product      *)
(* code. Written by a Claude agent from ONE_TO_ONE_SESSION_MANAGEMENT.md,   *)
(* MULTI_DEVICE_SYNC.md, ASYNC_KEY_MATERIAL.md, ACCOUNT_CONTINUITY_AND_     *)
(* CONTROL.md, CRYPTOGRAPHY_AND_AGILITY.md "Downgrade floor" and Q-050 in   *)
(* ARCHITECTURE_RESOLUTIONS_20260923.md.                                    *)
(*                                                                          *)
(* Topology (fixed): account A = {a1}; account B has control device b1 and  *)
(* messaging device b3, which b1 links (gen 1) and then revokes (gen 2).    *)
(* bx is a rogue device a provider may advertise for B; it is never in any  *)
(* AccountControlState. Senders is a constant; b1 self-syncs to b3.        *)
(*                                                                          *)
(* Per (local, remote) device pair the manager keeps a set of sessions      *)
(* (Q-050: Absent / Initiating / Active / Stale; Retired == dropped).       *)
(* Preferred outbound session = the smallest non-stale SessionID (Q-050     *)
(* "deterministic ordering"). Stale sessions stay decryptable (in-flight).  *)
(* SessionIDs come from a global counter standing in for fresh randomness.  *)
(*                                                                          *)
(* The boolean constants are session-manager RULES that the docs state     *)
(* only as outcomes (or not at all). The passing configuration enables all  *)
(* of them; each *_Gap configuration disables one and TLC must find a       *)
(* counterexample (captured as a regression vector).                        *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS
  Senders,                \* devices that originate logical messages (subset of {"a1","b1","b3"})
  MaxMsgs, MaxSid, MaxCrash, MaxRestore, MaxDup, MaxWeak, MaxInject,
  RestoreDevs,            \* devices that may take a backup and restore it (a state-space bound)
  PersistBeforeSend,      \* K1 write-ahead: ratchet/outbox state durable before an envelope leaves
  DedupeDurable,          \* K2 receive is transactional: dedupe set + session state durable before ack
  FloorFromACS,           \* K3 downgrade floor also derives from AccountControlState minimum profile
  RetryOnReset,           \* K4 sender re-encrypts a logical message named by an authenticated reset
  RecheckOnEncrypt,       \* K5 eligibility re-checked at encrypt time (fanout and retries)
  ValidateAgainstACS,     \* K6 recipient set from AccountControlState, never a provider list
  ReconcileControlHead,   \* K7 control head durable and reconciled after crash/restore
  RestoreDropsSessions,   \* K8 backup restore never restores ratchet state (re-establish instead)
  RejectFromRevoked,      \* K9 receiver rejects traffic from a device it knows is revoked
  PrekeyUntilConfirmed,   \* K10 every message on an unconfirmed (Initiating) session carries the prekey header
  RestoreRotatesPrekeys   \* K11 backups exclude handshake prekey secrets; a restored device publishes fresh
                          \*     prekeys, so a prekey message for a pre-restore prekey is undecryptable

Accounts == {"A", "B"}
Real     == {"a1", "b1", "b3"}
Acct(d)  == IF d = "a1" THEN "A" ELSE "B"
MaxGen   == [a \in Accounts |-> IF a = "A" THEN 0 ELSE 2]
Members(a, g) ==
  IF a = "A" THEN {"a1"} ELSE CASE g = 0 -> {"b1"} [] g = 1 -> {"b1", "b3"} [] OTHER -> {"b1"}
ACSMinProfile == 2        \* AccountControlState.minimum_crypto_profile (both accounts)
Profiles == {1, 2}
Max(x, y) == IF x >= y THEN x ELSE y
Msgs == 1..MaxMsgs

VARIABLES
  gen,        \* authoritative control generation per account (the chain head)
  known,      \* known[d][a]: control generation device d has validated for account a
  sess,       \* sess[d]: set of session records
  floor,      \* floor[d][p]: local downgrade floor towards peer p
  seen,       \* seen[d]: EventIDs accepted (dedupe)
  fan,        \* fan[d]: pending <<m, r>> encryptions (fanout / retry outbox)
  P,          \* P[d]: durable copy of [sess, floor, seen, fan, known]
  Bk,         \* Bk[d]: [has: BOOLEAN, snap: snapshot] (at most one backup)
  epoch,      \* epoch[d]: nextSid at d's last restore; sessions below it used pre-restore prekeys
  net,        \* in-flight envelopes (a set; duplication is bounded redelivery)
  nextSid, msgCount, crashes, restores, dups, weak, injected,
  \* ---- ghost (history) variables used only by invariants ----
  everKnown,  \* highest generation device d ever validated (never rolled back)
  acc,        \* acc[d][m]: acceptances of m at d since d's history last reset
  keyLog,     \* <<sid, sending device, ctr, m>> for every encryption (one chain per direction)
  sendLog,    \* [s, r, ok] for every envelope; ok = r eligible under s's best-ever knowledge
  badAccept,  \* acceptances from a sender the receiver had known to be revoked
  downgrade,  \* sessions created below the established floor
  gfloor,     \* gfloor[{d,p}]: strongest profile ever established on the pair
  intended,   \* <<m, r, s>>: sender s meant to deliver m to r
  refused,    \* <<m, r>> the sender deliberately gave up on (ineligible / below floor)
  disturbed,  \* devices that crashed or restored
  restored,   \* <<m, r, s>> intended before some restore: history lost by design, exempt from C1
  lastAct

vars == <<gen, known, sess, floor, seen, fan, P, Bk, epoch, net, nextSid, msgCount, crashes, restores,
          dups, weak, injected, everKnown, acc, keyLog, sendLog, badAccept, downgrade, gfloor,
          intended, refused, disturbed, restored, lastAct>>

Snap(d) == [sess |-> sess[d], floor |-> floor[d], seen |-> seen[d], fan |-> fan[d], known |-> known[d]]

Init ==
  /\ gen = [a \in Accounts |-> 0]
  /\ known = [d \in Real |-> [a \in Accounts |-> 0]]
  /\ sess = [d \in Real |-> {}]
  /\ floor = [d \in Real |-> [p \in Real |-> 0]]
  /\ seen = [d \in Real |-> {}]
  /\ fan = [d \in Real |-> {}]
  /\ P = [d \in Real |-> Snap(d)]
  /\ Bk = [d \in Real |-> [has |-> FALSE, snap |-> Snap(d)]]
  /\ epoch = [d \in Real |-> 0]
  /\ net = {}
  /\ nextSid = 1 /\ msgCount = 0 /\ crashes = 0 /\ restores = 0 /\ dups = 0
  /\ weak = {} /\ injected = FALSE
  /\ everKnown = [d \in Real |-> [a \in Accounts |-> 0]]
  /\ acc = [d \in Real |-> [m \in Msgs |-> 0]]
  /\ keyLog = {} /\ sendLog = {} /\ badAccept = {} /\ downgrade = {}
  /\ gfloor = [q \in {{x, y} : x, y \in Real} |-> 0]
  /\ intended = {} /\ refused = {} /\ disturbed = {} /\ restored = {}
  /\ lastAct = <<"Init">>

\* ---------------------------------------------------------------- helpers
SelfRevoked(d) == d \notin Members(Acct(d), known[d][Acct(d)])
Eligible(s) ==   \* devices s currently believes it must encrypt to
  LET acsSet == (Members("A", known[s]["A"]) \cup Members("B", known[s]["B"])) \ {s}
  IN IF ValidateAgainstACS THEN acsSet
     ELSE acsSet \cup (IF injected THEN {"bx"} ELSE {})
EverEligible(s, r) == r \in Real /\ r \in Members(Acct(r), everKnown[s][Acct(r)])
EffFloor(d, p) == IF FloorFromACS THEN Max(floor[d][p], ACSMinProfile) ELSE floor[d][p]
Live(d, p) == {x \in sess[d] : x.peer = p /\ ~x.stale}
SidsOf(S) == {x.sid : x \in S}
Pref(d, p) == CHOOSE i \in SidsOf(Live(d, p)) : \A j \in SidsOf(Live(d, p)) : i <= j
BundleProf(r) == IF r \in weak THEN 1 ELSE 2
PairKey(x, y) == {x, y}

\* Learn a control generation (fetch/validate the AccountControlState chain up to g).
SetKnown(d, a, g) ==
  /\ known' = [known EXCEPT ![d][a] = Max(@, g)]
  /\ everKnown' = [everKnown EXCEPT ![d][a] = Max(@, g)]

\* Durable-state update used by transactional actions: all fields of d become durable.
Commit(d, s2, f2, n2, fn2, k2) == [P EXCEPT ![d] = [sess |-> s2, floor |-> f2, seen |-> n2, fan |-> fn2, known |-> k2]]

\* ---------------------------------------------------------------- control plane
\* b1 (control device) links b3 (gen 1) and later revokes it (gen 2). The authorizing device
\* validates the new state immediately; R-166's notice is derived from exactly this state.
ControlStep ==
  /\ gen["B"] < MaxGen["B"]
  /\ gen' = [gen EXCEPT !["B"] = @ + 1]
  \* the linking ceremony (gen 0 -> 1) also hands the new device b3 the state that authorizes it
  /\ LET linked == IF gen["B"] = 0 THEN {"b1", "b3"} ELSE {"b1"}
     IN /\ known' = [d \in Real |-> IF d \in linked THEN [known[d] EXCEPT !["B"] = gen["B"] + 1] ELSE known[d]]
        /\ everKnown' = [d \in Real |-> IF d \in linked THEN [everKnown[d] EXCEPT !["B"] = Max(@, gen["B"] + 1)] ELSE everKnown[d]]
        /\ P' = IF ReconcileControlHead
                THEN [d \in Real |-> IF d \in linked THEN [P[d] EXCEPT !.known = known'[d]] ELSE P[d]] ELSE P
  /\ lastAct' = <<"ControlStep", gen["B"] + 1>>
  /\ UNCHANGED <<epoch, sess, floor, seen, fan, Bk, net, nextSid, msgCount, crashes, restores, dups, weak,
                 injected, acc, keyLog, sendLog, badAccept, downgrade, gfloor, intended, refused, disturbed, restored>>

Learn(d, a) ==
  /\ known[d][a] < gen[a]
  /\ SetKnown(d, a, gen[a])
  /\ P' = IF ReconcileControlHead THEN [P EXCEPT ![d].known = known'[d]] ELSE P
  /\ lastAct' = <<"Learn", d, a>>
  /\ UNCHANGED <<epoch, gen, sess, floor, seen, fan, Bk, net, nextSid, msgCount, crashes, restores, dups,
                 weak, injected, acc, keyLog, sendLog, badAccept, downgrade, gfloor, intended, refused, disturbed, restored>>

\* ---------------------------------------------------------------- adversary / provider
ServeWeakBundle(r) ==   \* provider (or depletion) offers only weaker handshake material for r
  /\ Cardinality(weak) < MaxWeak /\ r \notin weak
  /\ weak' = weak \cup {r}
  /\ lastAct' = <<"ServeWeakBundle", r>>
  /\ UNCHANGED <<epoch, gen, known, sess, floor, seen, fan, P, Bk, net, nextSid, msgCount, crashes, restores,
                 dups, injected, everKnown, acc, keyLog, sendLog, badAccept, downgrade, gfloor,
                 intended, refused, disturbed, restored>>

InjectRogueDevice ==     \* provider advertises bx for account B
  /\ ~injected /\ MaxInject > 0
  /\ injected' = TRUE
  /\ lastAct' = <<"InjectRogueDevice">>
  /\ UNCHANGED <<epoch, gen, known, sess, floor, seen, fan, P, Bk, net, nextSid, msgCount, crashes, restores,
                 dups, weak, everKnown, acc, keyLog, sendLog, badAccept, downgrade, gfloor, intended,
                 refused, disturbed, restored>>

\* ---------------------------------------------------------------- sending
NewMessage(s) ==
  /\ s \in Senders /\ msgCount < MaxMsgs /\ ~SelfRevoked(s)
  /\ LET m == msgCount + 1
         f2 == fan[s] \cup {<<m, r>> : r \in Eligible(s)}
     IN /\ msgCount' = m
        /\ fan' = [fan EXCEPT ![s] = f2]
        /\ intended' = intended \cup {<<m, r, s>> : r \in Eligible(s) \cap Real}
        /\ P' = IF PersistBeforeSend THEN [P EXCEPT ![s].fan = f2] ELSE P
        /\ lastAct' = <<"NewMessage", s, m>>
  /\ UNCHANGED <<epoch, gen, known, sess, floor, seen, Bk, net, nextSid, crashes, restores, dups, weak,
                 injected, everKnown, acc, keyLog, sendLog, badAccept, downgrade, gfloor, refused, disturbed, restored>>

\* Encrypt one pending <<m, r>>: existing preferred session, or initiate a new one.
Encrypt(s, m, r) ==
  /\ <<m, r>> \in fan[s]
  /\ LET f2 == fan[s] \ {<<m, r>>}
         stillOk == r \in Eligible(s)
         okGhost == EverEligible(s, r)
         hdr == known[s][Acct(s)]
     IN
     IF RecheckOnEncrypt /\ ~stillOk THEN            \* send raced a revocation: fail, don't encrypt
        /\ fan' = [fan EXCEPT ![s] = f2]
        /\ refused' = refused \cup {<<m, r>>}
        /\ P' = IF PersistBeforeSend THEN [P EXCEPT ![s].fan = f2] ELSE P
        /\ lastAct' = <<"EncryptRefusedIneligible", s, m, r>>
        /\ UNCHANGED <<epoch, sess, floor, net, nextSid, keyLog, sendLog, downgrade, gfloor>>
     ELSE IF r \in Real /\ Live(s, r) # {} THEN
        LET x == CHOOSE y \in Live(s, r) : y.sid = Pref(s, r)
            x2 == [x EXCEPT !.ctr = @ + 1]
            s2 == (sess[s] \ {x}) \cup {x2}
        IN /\ sess' = [sess EXCEPT ![s] = s2]
           /\ fan' = [fan EXCEPT ![s] = f2]
           /\ net' = net \cup {[t |-> "msg", from |-> s, to |-> r, sid |-> x.sid, prof |-> x.prof,
                                ctr |-> x2.ctr, m |-> m, pk |-> PrekeyUntilConfirmed /\ ~x.conf, hdr |-> hdr]}
           /\ keyLog' = keyLog \cup {<<x.sid, s, x2.ctr, m>>}
           /\ sendLog' = sendLog \cup {[s |-> s, r |-> r, ok |-> okGhost]}
           /\ P' = IF PersistBeforeSend THEN [P EXCEPT ![s].sess = s2, ![s].fan = f2] ELSE P
           /\ lastAct' = <<"Encrypt", s, m, r, x.sid>>
           /\ UNCHANGED <<epoch, floor, nextSid, refused, downgrade, gfloor>>
     ELSE IF r \in Real /\ BundleProf(r) < EffFloor(s, r) THEN   \* refuse to initiate below floor
        /\ fan' = [fan EXCEPT ![s] = f2]
        /\ refused' = refused \cup {<<m, r>>}
        /\ P' = IF PersistBeforeSend THEN [P EXCEPT ![s].fan = f2] ELSE P
        /\ lastAct' = <<"EncryptRefusedFloor", s, m, r>>
        /\ UNCHANGED <<epoch, sess, floor, net, nextSid, keyLog, sendLog, downgrade, gfloor>>
     ELSE
        /\ nextSid <= MaxSid
        /\ LET prof == IF r \in Real THEN BundleProf(r) ELSE 2
               x == [peer |-> r, sid |-> nextSid, prof |-> prof, stale |-> FALSE, ctr |-> 1, conf |-> FALSE]
               s2 == sess[s] \cup {x}
               fl2 == IF r \in Real THEN [floor[s] EXCEPT ![r] = Max(@, prof)] ELSE floor[s]
           IN /\ sess' = [sess EXCEPT ![s] = s2]
              /\ floor' = [floor EXCEPT ![s] = fl2]
              /\ fan' = [fan EXCEPT ![s] = f2]
              /\ nextSid' = nextSid + 1
              /\ net' = IF r \in Real
                        THEN net \cup {[t |-> "msg", from |-> s, to |-> r, sid |-> nextSid, prof |-> prof,
                                        ctr |-> 1, m |-> m, pk |-> TRUE, hdr |-> hdr]}
                        ELSE net   \* envelope to a rogue device leaves our model (attacker reads it)
              /\ keyLog' = keyLog \cup {<<nextSid, s, 1, m>>}
              /\ sendLog' = sendLog \cup {[s |-> s, r |-> r, ok |-> okGhost]}
              /\ downgrade' = IF r \in Real /\ prof < gfloor[PairKey(s, r)]
                              THEN downgrade \cup {<<s, r, nextSid>>} ELSE downgrade
              /\ P' = IF PersistBeforeSend THEN [P EXCEPT ![s].sess = s2, ![s].floor = fl2, ![s].fan = f2] ELSE P
              /\ lastAct' = <<"Initiate", s, m, r, nextSid, prof>>
              /\ UNCHANGED <<epoch, refused, gfloor>>
  /\ UNCHANGED <<epoch, gen, known, seen, Bk, msgCount, crashes, restores, dups, weak, injected, everKnown,
                 acc, badAccept, intended, disturbed, restored>>

\* ---------------------------------------------------------------- receiving
\* Deliver envelope e; keep it in the network when this is a bounded redelivery (duplicate).
Consume(e, dup) == net' = IF dup THEN net ELSE net \ {e}

RecvMsg(e, dup) ==
  LET r == e.to
      f == e.from
      k1 == [known[r] EXCEPT ![Acct(f)] = Max(@, e.hdr)]          \* head gossip piggyback
      senderRevoked == f \notin Members(Acct(f), k1[Acct(f)])
      everRevoked == f \notin Members(Acct(f), Max(everKnown[r][Acct(f)], e.hdr))
      have == {x \in sess[r] : x.sid = e.sid /\ x.peer = f}
      rest == IF dup THEN net ELSE net \ {e}
  IN
  /\ e.t = "msg" /\ e \in net
  /\ dup => dups < MaxDup
  /\ dups' = IF dup THEN dups + 1 ELSE dups
  /\ known' = [known EXCEPT ![r] = k1]
  /\ everKnown' = [everKnown EXCEPT ![r][Acct(f)] = Max(@, e.hdr)]
  /\ IF r \notin Members(Acct(r), k1[Acct(r)]) \/ (RejectFromRevoked /\ senderRevoked) THEN
        \* compliant revoked device stops; traffic from a known-revoked sender is rejected
        /\ net' = rest
        /\ lastAct' = <<"RecvRejectedRevoked", r, f, e.m>>
        /\ P' = IF ReconcileControlHead THEN [P EXCEPT ![r].known = k1] ELSE P
        /\ UNCHANGED <<epoch, sess, floor, seen, acc, badAccept, gfloor, downgrade>>
     ELSE IF have = {} /\ (~e.pk \/ (RestoreRotatesPrekeys /\ e.sid < epoch[r])) THEN
        \* undecryptable: unknown/lost session, or a prekey message for a prekey this device no longer
        \* holds (K11) -> authenticated reset request naming sid and m
        /\ net' = rest \cup {[t |-> "reset", from |-> r, to |-> f, sid |-> e.sid, m |-> e.m]}
        /\ lastAct' = <<"RecvUndecryptable", r, f, e.sid, e.m>>
        /\ P' = IF ReconcileControlHead THEN [P EXCEPT ![r].known = k1] ELSE P
        /\ UNCHANGED <<epoch, sess, floor, seen, acc, badAccept, gfloor, downgrade>>
     ELSE IF have = {} /\ e.prof < EffFloor(r, f) THEN
        /\ net' = rest
        /\ lastAct' = <<"RecvRejectedBelowFloor", r, f, e.sid>>
        /\ P' = IF ReconcileControlHead THEN [P EXCEPT ![r].known = k1] ELSE P
        /\ UNCHANGED <<epoch, sess, floor, seen, acc, badAccept, gfloor, downgrade>>
     ELSE
        LET created == have = {}
            x == IF created
                 THEN [peer |-> f, sid |-> e.sid, prof |-> e.prof, stale |-> FALSE, ctr |-> 0, conf |-> TRUE]
                 ELSE CHOOSE y \in have : TRUE
            x2 == [x EXCEPT !.conf = TRUE]
            base == (sess[r] \ have) \cup {x2}
            \* Retire (mark stale) other live sessions once the peer demonstrably uses our preferred one.
            s2 == IF ~created /\ ~x.stale /\ x.sid = Pref(r, f)
                  THEN {IF y.peer = f /\ y.sid # x.sid THEN [y EXCEPT !.stale = TRUE] ELSE y : y \in base}
                  ELSE base
            fl2 == [floor[r] EXCEPT ![f] = Max(@, e.prof)]
            n2 == seen[r] \cup {e.m}
        IN /\ net' = rest
           /\ sess' = [sess EXCEPT ![r] = s2]
           /\ floor' = [floor EXCEPT ![r] = fl2]
           /\ seen' = [seen EXCEPT ![r] = n2]
           /\ acc' = IF e.m \in seen[r] THEN acc ELSE [acc EXCEPT ![r][e.m] = @ + 1]
           /\ badAccept' = IF everRevoked /\ e.m \notin seen[r] THEN badAccept \cup {<<r, f, e.m>>} ELSE badAccept
           /\ gfloor' = IF created THEN [gfloor EXCEPT ![PairKey(r, f)] = Max(@, e.prof)] ELSE gfloor
           /\ downgrade' = IF created /\ e.prof < gfloor[PairKey(r, f)]
                           THEN downgrade \cup {<<r, f, e.sid>>} ELSE downgrade
           /\ P' = IF DedupeDurable THEN Commit(r, s2, fl2, n2, fan[r], k1)
                   ELSE IF ReconcileControlHead THEN [P EXCEPT ![r].known = k1] ELSE P
           /\ lastAct' = <<"Recv", r, f, e.sid, e.m, IF created THEN "created" ELSE "existing">>
  /\ UNCHANGED <<epoch, gen, fan, Bk, nextSid, msgCount, crashes, restores, weak, injected, keyLog, sendLog,
                 intended, refused, disturbed, restored>>

\* Reset request (authenticated by the requesting device; replays are bounded redelivery).
RecvReset(e, dup) ==
  LET s == e.to
      stale2 == {IF x.sid = e.sid /\ x.peer = e.from THEN [x EXCEPT !.stale = TRUE] ELSE x : x \in sess[s]}
      f2 == IF RetryOnReset THEN fan[s] \cup {<<e.m, e.from>>} ELSE fan[s]
  IN
  /\ e.t = "reset" /\ e \in net
  /\ dup => dups < MaxDup
  /\ dups' = IF dup THEN dups + 1 ELSE dups
  /\ Consume(e, dup)
  /\ sess' = [sess EXCEPT ![s] = stale2]
  /\ fan' = [fan EXCEPT ![s] = f2]
  /\ P' = IF PersistBeforeSend THEN [P EXCEPT ![s].sess = stale2, ![s].fan = f2] ELSE P
  /\ lastAct' = <<"RecvReset", s, e.from, e.sid, e.m>>
  /\ UNCHANGED <<epoch, gen, known, floor, seen, Bk, nextSid, msgCount, crashes, restores, weak, injected,
                 everKnown, acc, keyLog, sendLog, badAccept, downgrade, gfloor, intended, refused, disturbed, restored>>

\* ---------------------------------------------------------------- persistence
Persist(d) ==
  /\ P[d] # Snap(d)
  /\ P' = [P EXCEPT ![d] = Snap(d)]
  /\ lastAct' = <<"Persist", d>>
  /\ UNCHANGED <<epoch, gen, known, sess, floor, seen, fan, Bk, net, nextSid, msgCount, crashes, restores, dups,
                 weak, injected, everKnown, acc, keyLog, sendLog, badAccept, downgrade, gfloor, intended,
                 refused, disturbed, restored>>

\* Crash: volatile state lost, durable state P[d] survives.
Crash(d) ==
  /\ crashes < MaxCrash /\ P[d] # Snap(d)
  /\ crashes' = crashes + 1
  /\ sess' = [sess EXCEPT ![d] = P[d].sess]
  /\ floor' = [floor EXCEPT ![d] = P[d].floor]
  /\ seen' = [seen EXCEPT ![d] = P[d].seen]
  /\ fan' = [fan EXCEPT ![d] = P[d].fan]
  /\ known' = [known EXCEPT ![d] = P[d].known]
  /\ disturbed' = disturbed \cup {d}
  /\ lastAct' = <<"Crash", d>>
  /\ UNCHANGED <<epoch, gen, P, Bk, net, nextSid, msgCount, restores, dups, weak, injected, everKnown, acc,
                 keyLog, sendLog, badAccept, downgrade, gfloor, intended, refused, restored>>

TakeBackup(d) ==
  /\ ~Bk[d].has /\ MaxRestore > 0 /\ d \in RestoreDevs
  /\ Bk' = [Bk EXCEPT ![d] = [has |-> TRUE, snap |-> Snap(d)]]
  /\ lastAct' = <<"TakeBackup", d>>
  /\ UNCHANGED <<epoch, gen, known, sess, floor, seen, fan, P, net, nextSid, msgCount, crashes, restores, dups,
                 weak, injected, everKnown, acc, keyLog, sendLog, badAccept, downgrade, gfloor, intended,
                 refused, disturbed, restored>>

\* Restore from an older backup (history after the backup is lost with the device).
Restore(d) ==
  /\ restores < MaxRestore /\ Bk[d].has
  /\ LET b == Bk[d].snap
         s2 == IF RestoreDropsSessions THEN {} ELSE b.sess
         k2 == IF ReconcileControlHead THEN [a \in Accounts |-> Max(b.known[a], gen[a])] ELSE b.known
     IN /\ sess' = [sess EXCEPT ![d] = s2]
        /\ floor' = [floor EXCEPT ![d] = b.floor]
        /\ seen' = [seen EXCEPT ![d] = b.seen]
        /\ fan' = [fan EXCEPT ![d] = b.fan]
        /\ known' = [known EXCEPT ![d] = k2]
        /\ everKnown' = [everKnown EXCEPT ![d] = [a \in Accounts |-> Max(@[a], k2[a])]]
        /\ P' = [P EXCEPT ![d] = [sess |-> s2, floor |-> b.floor, seen |-> b.seen, fan |-> b.fan, known |-> k2]]
        \* restored history = backup history, so acceptance counting restarts from it
        /\ acc' = [acc EXCEPT ![d] = [m \in Msgs |-> IF m \in b.seen THEN 1 ELSE 0]]
  /\ restores' = restores + 1
  /\ epoch' = [epoch EXCEPT ![d] = nextSid]
  /\ disturbed' = disturbed \cup {d}
  /\ restored' = restored \cup intended
  /\ lastAct' = <<"Restore", d>>
  /\ UNCHANGED <<gen, Bk, net, nextSid, msgCount, crashes, dups, weak, injected, keyLog, sendLog,
                 badAccept, downgrade, gfloor, intended, refused>>

Next ==
  \/ ControlStep
  \/ \E d \in Real, a \in Accounts : Learn(d, a)
  \/ \E r \in Real : ServeWeakBundle(r)
  \/ InjectRogueDevice
  \/ \E s \in Senders : NewMessage(s)
  \/ \E s \in Real : \E pr \in fan[s] : Encrypt(s, pr[1], pr[2])
  \/ \E e \in net : \E dup \in BOOLEAN : RecvMsg(e, dup) \/ RecvReset(e, dup)
  \/ \E d \in Real : Persist(d) \/ Crash(d) \/ TakeBackup(d) \/ Restore(d)

Spec == Init /\ [][Next]_vars

\* ---------------------------------------------------------------- invariants
TypeOK == nextSid \in 1..(MaxSid + 1) /\ msgCount \in 0..MaxMsgs

\* S1 no logical message is accepted twice while it is still in the device's history (R-089)
NoDoubleAccept == \A d \in Real, m \in Msgs : acc[d][m] <= 1

\* S2 no (session, counter) key is used for two different plaintexts (ratchet rollback)
NoKeyReuse == \A x, y \in keyLog : (x[1] = y[1] /\ x[2] = y[2] /\ x[3] = y[3]) => x[4] = y[4]

\* S3 no envelope is created for a device outside the AccountControlState the sender has
\*    validated at its best-ever knowledge: covers "revocation known" and provider-injected devices
NoSendToIneligible == \A x \in sendLog : x.ok

\* S4 no message from a device is accepted after the receiver has known it is revoked
NoAcceptFromKnownRevoked == badAccept = {}

\* S5 no silent downgrade below the strongest profile ever established on a device pair
NoSilentDowngrade == downgrade = {}

Quiescent == net = {} /\ \A d \in Real : fan[d] = {}

\* C1 at quiescence every intended delivery reached its device, unless the sender deliberately
\*    refused it (ineligible/below floor), or the recipient or the sender is no longer an authorized
\*    device (a revoked device's unsent outbox is rejected by K9, by design)
Authorized(d) == d \in Members(Acct(d), gen[Acct(d)])
DeliveredAtQuiescence ==
  Quiescent =>
    \A t \in intended :
      t \in restored \/ ~Authorized(t[2]) \/ ~Authorized(t[3]) \/ <<t[1], t[2]>> \in refused
        \/ t[1] \in seen[t[2]]

\* C2 at quiescence, undisturbed authorized device pairs that both hold live sessions prefer the same one
SamePreferredAtQuiescence ==
  Quiescent =>
    \A d, p \in Real :
      (d # p /\ Authorized(d) /\ Authorized(p) /\ d \notin disturbed /\ p \notin disturbed
         /\ Live(d, p) # {} /\ Live(p, d) # {})
        => Pref(d, p) = Pref(p, d)

\* C3 (disturbed pairs) no black hole: a preferred session the peer cannot decrypt at quiescence
\*    is allowed only lazily; checked as a design-gap probe, not in the passing config.
NoBlackHoleAtQuiescence ==
  Quiescent =>
    \A d, p \in Real :
      (d # p /\ Live(d, p) # {}) => Pref(d, p) \in SidsOf({x \in sess[p] : x.peer = d})

StateBound == nextSid <= MaxSid + 1
=============================================================================
