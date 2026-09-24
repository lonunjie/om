"""Feasibility prototype for V-003. Not product code. Not for production.

Reference state machine for the per-device 1:1 session manager, written in Python independently of
the TLA+ model (verification/tla/OneToOneSessions.tla) from the same docs:
ONE_TO_ONE_SESSION_MANAGEMENT.md, MULTI_DEVICE_SYNC.md, ASYNC_KEY_MATERIAL.md,
ACCOUNT_CONTINUITY_AND_CONTROL.md, CRYPTOGRAPHY_AND_AGILITY.md ("Downgrade floor"), Q-050.
No cryptography is performed: sessions, counters and profiles are abstract.

Rules K1..K10 are switchable (see README "Session-manager rules"). Step labels match the TLA+
model's `lastAct` labels so TLC counterexamples can be replayed step by step.
"""
from __future__ import annotations

import copy
from dataclasses import dataclass, field

KNOBS = ["PersistBeforeSend", "DedupeDurable", "FloorFromACS", "RetryOnReset", "RecheckOnEncrypt",
         "ValidateAgainstACS", "ReconcileControlHead", "RestoreDropsSessions", "RejectFromRevoked",
         "PrekeyUntilConfirmed"]
ACS_MIN_PROFILE = 2


@dataclass
class Topology:
    """accounts: account -> list of member sets, one per control generation (index = generation).
    The first device of generation 0 is the account's control device that authorizes changes."""
    accounts: dict[str, list[frozenset[str]]]
    senders: frozenset[str]

    def acct(self, d: str) -> str:
        for a, gens in self.accounts.items():
            if any(d in g for g in gens):
                return a
        return "B"  # rogue devices claim account B

    def members(self, a: str, g: int) -> frozenset[str]:
        gens = self.accounts[a]
        return gens[min(g, len(gens) - 1)]

    @property
    def devices(self) -> list[str]:
        return sorted({d for gens in self.accounts.values() for g in gens for d in g})

    def controller(self, a: str) -> str:
        return sorted(self.accounts[a][0])[0]


TLC_TOPOLOGY = Topology({"A": [frozenset({"a1"})],
                         "B": [frozenset({"b1"}), frozenset({"b1", "b3"}), frozenset({"b1"})]},
                        frozenset({"a1", "b1"}))
FUZZ_TOPOLOGY = Topology({"A": [frozenset({"a1"}), frozenset({"a1", "a2"})],
                          "B": [frozenset({"b1", "b2"}), frozenset({"b1", "b2", "b3"}), frozenset({"b1", "b2"})]},
                         frozenset({"a1", "a2", "b1", "b2", "b3"}))


@dataclass
class Bounds:
    max_msgs: int = 2
    max_sid: int = 4
    max_crash: int = 1
    max_restore: int = 0
    max_dup: int = 1
    max_weak: int = 1
    max_inject: int = 1


class Infeasible(Exception):
    """A replayed step is not enabled or produced a different outcome."""


@dataclass
class Dev:
    known: dict
    sess: list = field(default_factory=list)   # dicts: peer sid prof stale ctr conf
    floor: dict = field(default_factory=dict)
    seen: set = field(default_factory=set)
    fan: set = field(default_factory=set)       # (m, r)

    def snap(self):
        return copy.deepcopy({"sess": self.sess, "floor": self.floor, "seen": self.seen, "fan": self.fan, "known": self.known})


class World:
    def __init__(self, topo: Topology, bounds: Bounds, rules: dict[str, bool]):
        self.t, self.b, self.k = topo, bounds, {k: rules.get(k, True) for k in KNOBS}
        self.gen = {a: 0 for a in topo.accounts}
        self.dev = {d: Dev(known={a: 0 for a in topo.accounts}) for d in topo.devices}
        self.P = {d: self.dev[d].snap() for d in topo.devices}
        self.Bk: dict[str, dict | None] = {d: None for d in topo.devices}
        self.net: set[tuple] = set()
        self.next_sid, self.msg_count = 1, 0
        self.crashes = self.restores = self.dups = 0
        self.weak: set[str] = set()
        self.injected = False
        # ghosts
        self.ever = {d: {a: 0 for a in topo.accounts} for d in topo.devices}
        self.acc = {d: {} for d in topo.devices}
        self.key_log: dict[tuple, int] = {}
        self.violations: list[tuple] = []  # (invariant, detail) from action-time checks
        self.gfloor: dict[frozenset, int] = {}
        self.intended: set[tuple] = set()
        self.refused: set[tuple] = set()
        self.author: dict[int, str] = {}
        self.disturbed: set[str] = set()
        self.exempt: set[tuple] = set()
        self.log: list[list] = []

    # ------------------------------------------------------------------ helpers
    def acct(self, d):
        return self.t.acct(d)

    def self_revoked(self, d):
        return d not in self.t.members(self.acct(d), self.dev[d].known[self.acct(d)])

    def eligible(self, s):
        k = self.dev[s].known
        out = set().union(*(self.t.members(a, k[a]) for a in self.t.accounts)) - {s}
        if not self.k["ValidateAgainstACS"] and self.injected:
            out |= {"bx"}
        return out

    def ever_eligible(self, s, r):
        return r in self.dev and r in self.t.members(self.acct(r), self.ever[s][self.acct(r)])

    def eff_floor(self, d, p):
        f = self.dev[d].floor.get(p, 0)
        return max(f, ACS_MIN_PROFILE) if self.k["FloorFromACS"] else f

    def live(self, d, p):
        return [x for x in self.dev[d].sess if x["peer"] == p and not x["stale"]]

    def pref(self, d, p):
        return min(x["sid"] for x in self.live(d, p))

    def persist_fields(self, d, **fields):
        for f, v in fields.items():
            self.P[d][f] = copy.deepcopy(v)

    def learned(self, d, a, g):
        self.dev[d].known[a] = max(self.dev[d].known[a], g)
        self.ever[d][a] = max(self.ever[d][a], g)
        if self.k["ReconcileControlHead"]:
            self.persist_fields(d, known=self.dev[d].known)

    def quiescent(self):
        return not self.net and all(not v.fan for v in self.dev.values())

    # ------------------------------------------------------------------ actions
    def control_step(self, a):
        if self.gen[a] >= len(self.t.accounts[a]) - 1:
            raise Infeasible("chain end")
        g = self.gen[a] + 1
        self.gen[a] = g
        added = self.t.members(a, g) - self.t.members(a, g - 1)
        for d in {self.t.controller(a)} | added:   # linking hands the new device its authorizing state
            self.learned(d, a, g)
        return ["ControlStep", g]

    def learn(self, d, a):
        if self.dev[d].known[a] >= self.gen[a]:
            raise Infeasible("nothing to learn")
        self.learned(d, a, self.gen[a])
        return ["Learn", d, a]

    def serve_weak(self, r):
        if len(self.weak) >= self.b.max_weak or r in self.weak:
            raise Infeasible("weak budget")
        self.weak.add(r)
        return ["ServeWeakBundle", r]

    def inject(self):
        if self.injected or self.b.max_inject == 0:
            raise Infeasible("inject budget")
        self.injected = True
        return ["InjectRogueDevice"]

    def new_message(self, s):
        if s not in self.t.senders or self.msg_count >= self.b.max_msgs or self.self_revoked(s):
            raise Infeasible("cannot send")
        self.msg_count += 1
        m = self.msg_count
        el = self.eligible(s)
        self.dev[s].fan |= {(m, r) for r in el}
        self.intended |= {(m, r) for r in el if r in self.dev}
        self.author[m] = s
        if self.k["PersistBeforeSend"]:
            self.persist_fields(s, fan=self.dev[s].fan)
        return ["NewMessage", s, m]

    def encrypt(self, s, m, r):
        D = self.dev[s]
        if (m, r) not in D.fan:
            raise Infeasible("not pending")
        hdr = D.known[self.acct(s)]
        ok_ghost = self.ever_eligible(s, r)

        def done_fan():
            D.fan.discard((m, r))
            if self.k["PersistBeforeSend"]:
                self.persist_fields(s, fan=D.fan)

        if self.k["RecheckOnEncrypt"] and r not in self.eligible(s):
            done_fan()
            self.refused.add((m, r))
            return ["EncryptRefusedIneligible", s, m, r]
        real = r in self.dev
        if real and self.live(s, r):
            x = next(y for y in self.live(s, r) if y["sid"] == self.pref(s, r))
            x["ctr"] += 1
            pk = self.k["PrekeyUntilConfirmed"] and not x["conf"]
            self.net.add(("msg", s, r, x["sid"], x["prof"], x["ctr"], m, pk, hdr))
            self._key(x["sid"], s, x["ctr"], m)
            self._sendcheck(s, r, ok_ghost)
            D.fan.discard((m, r))
            if self.k["PersistBeforeSend"]:
                self.persist_fields(s, sess=D.sess, fan=D.fan)
            return ["Encrypt", s, m, r, x["sid"]]
        prof = (1 if r in self.weak else 2) if real else 2
        if real and prof < self.eff_floor(s, r):
            done_fan()
            self.refused.add((m, r))
            return ["EncryptRefusedFloor", s, m, r]
        if self.next_sid > self.b.max_sid:
            raise Infeasible("sid budget")
        sid = self.next_sid
        self.next_sid += 1
        D.sess.append({"peer": r, "sid": sid, "prof": prof, "stale": False, "ctr": 1, "conf": False})
        if real:
            D.floor[r] = max(D.floor.get(r, 0), prof)
            self.net.add(("msg", s, r, sid, prof, 1, m, True, hdr))
            if prof < self.gfloor.get(frozenset({s, r}), 0):
                self.violations.append(("NoSilentDowngrade", (s, r, sid)))
        self._key(sid, s, 1, m)
        self._sendcheck(s, r, ok_ghost)
        D.fan.discard((m, r))
        if self.k["PersistBeforeSend"]:
            self.persist_fields(s, sess=D.sess, floor=D.floor, fan=D.fan)
        return ["Initiate", s, m, r, sid, prof]

    def _key(self, sid, sender, ctr, m):
        k = (sid, sender, ctr)
        if k in self.key_log and self.key_log[k] != m:
            self.violations.append(("NoKeyReuse", k))
        self.key_log.setdefault(k, m)

    def _sendcheck(self, s, r, ok):
        if not ok:
            self.violations.append(("NoSendToIneligible", (s, r)))

    def _take(self, e, dup):
        if dup:
            if self.dups >= self.b.max_dup:
                raise Infeasible("dup budget")
            self.dups += 1
        else:
            self.net.discard(e)

    def recv(self, e, dup=False):
        if e not in self.net:
            raise Infeasible("not in flight")
        if e[0] == "reset":
            return self._recv_reset(e, dup)
        _, f, r, sid, prof, ctr, m, pk, hdr = e
        self._take(e, dup)
        D = self.dev[r]
        af = self.acct(f)
        D.known[af] = max(D.known[af], hdr)             # head gossip piggyback
        ever_hdr = max(self.ever[r][af], hdr)
        self.ever[r][af] = ever_hdr
        if self.k["ReconcileControlHead"]:
            self.persist_fields(r, known=D.known)
        sender_revoked = f not in self.t.members(af, D.known[af])
        if self.self_revoked(r) or (self.k["RejectFromRevoked"] and sender_revoked):
            return ["RecvRejectedRevoked", r, f, m]
        have = [x for x in D.sess if x["sid"] == sid and x["peer"] == f]
        if not have and not pk:
            self.net.add(("reset", r, f, sid, m))
            return ["RecvUndecryptable", r, f, sid, m]
        if not have and prof < self.eff_floor(r, f):
            return ["RecvRejectedBelowFloor", r, f, sid]
        created = not have
        if created:
            x = {"peer": f, "sid": sid, "prof": prof, "stale": False, "ctr": 0, "conf": True}
            pair = frozenset({r, f})
            if prof < self.gfloor.get(pair, 0):
                self.violations.append(("NoSilentDowngrade", (r, f, sid)))
            self.gfloor[pair] = max(self.gfloor.get(pair, 0), prof)
            D.sess.append(x)
        else:
            x = have[0]
            retire = (not x["stale"]) and x["sid"] == self.pref(r, f)
            x["conf"] = True
            if retire:   # peer demonstrably uses our preferred session: retire the others
                for y in D.sess:
                    if y["peer"] == f and y["sid"] != x["sid"]:
                        y["stale"] = True
        D.floor[f] = max(D.floor.get(f, 0), prof)
        if m not in D.seen:
            self.acc[r][m] = self.acc[r].get(m, 0) + 1
            if f not in self.t.members(af, ever_hdr):
                self.violations.append(("NoAcceptFromKnownRevoked", (r, f, m)))
        D.seen.add(m)
        if self.k["DedupeDurable"]:
            self.P[r] = D.snap()
        return ["Recv", r, f, sid, m, "created" if created else "existing"]

    def _recv_reset(self, e, dup):
        _, frm, s, sid, m = e
        self._take(e, dup)
        D = self.dev[s]
        for x in D.sess:
            if x["sid"] == sid and x["peer"] == frm:
                x["stale"] = True
        if self.k["RetryOnReset"]:
            D.fan.add((m, frm))
        if self.k["PersistBeforeSend"]:
            self.persist_fields(s, sess=D.sess, fan=D.fan)
        return ["RecvReset", s, frm, sid, m]

    def persist(self, d):
        if self.P[d] == self.dev[d].snap():
            raise Infeasible("already durable")
        self.P[d] = self.dev[d].snap()
        return ["Persist", d]

    def crash(self, d):
        if self.crashes >= self.b.max_crash or self.P[d] == self.dev[d].snap():
            raise Infeasible("crash budget/no-op")
        self.crashes += 1
        p = copy.deepcopy(self.P[d])
        D = self.dev[d]
        D.sess, D.floor, D.seen, D.fan, D.known = p["sess"], p["floor"], p["seen"], p["fan"], p["known"]
        self.disturbed.add(d)
        return ["Crash", d]

    def take_backup(self, d):
        if self.Bk[d] is not None or self.b.max_restore == 0:
            raise Infeasible("backup exists")
        self.Bk[d] = self.dev[d].snap()
        return ["TakeBackup", d]

    def restore(self, d):
        if self.restores >= self.b.max_restore or self.Bk[d] is None:
            raise Infeasible("restore budget")
        b = copy.deepcopy(self.Bk[d])
        D = self.dev[d]
        D.sess = [] if self.k["RestoreDropsSessions"] else b["sess"]
        D.floor, D.seen, D.fan = b["floor"], b["seen"], b["fan"]
        D.known = {a: max(b["known"][a], self.gen[a]) for a in self.t.accounts} if self.k["ReconcileControlHead"] else b["known"]
        for a in self.t.accounts:
            self.ever[d][a] = max(self.ever[d][a], D.known[a])
        self.P[d] = D.snap()
        self.acc[d] = {m: 1 for m in b["seen"]}
        self.restores += 1
        self.disturbed.add(d)
        self.exempt |= self.intended
        return ["Restore", d]

    # ------------------------------------------------------------------ invariants
    def check(self) -> list[str]:
        bad = sorted({v[0] for v in self.violations})
        if any(c > 1 for accs in self.acc.values() for c in accs.values()):
            bad.append("NoDoubleAccept")
        if self.quiescent():
            for m, r in self.intended:
                a, s = self.acct(r), self.author[m]
                sa = self.acct(s)
                if not ((m, r) in self.exempt or r not in self.t.members(a, self.gen[a])
                        or s not in self.t.members(sa, self.gen[sa])
                        or (m, r) in self.refused or m in self.dev[r].seen):
                    bad.append("DeliveredAtQuiescence")
                    break
            auth = {d for d in self.dev if d in self.t.members(self.acct(d), self.gen[self.acct(d)])}
            for d in auth:
                for p in auth:
                    if d != p and d not in self.disturbed and p not in self.disturbed and self.live(d, p) and self.live(p, d):
                        if self.pref(d, p) != self.pref(p, d):
                            bad.append("SamePreferredAtQuiescence")
        return sorted(set(bad))

    # ------------------------------------------------------------------ scheduling
    def enabled(self) -> list[tuple]:
        acts: list[tuple] = []
        for a in sorted(self.t.accounts):
            if self.gen[a] < len(self.t.accounts[a]) - 1:
                acts.append(("control_step", a))
        for d in self.t.devices:
            for a in sorted(self.t.accounts):
                if self.dev[d].known[a] < self.gen[a]:
                    acts.append(("learn", d, a))
        if len(self.weak) < self.b.max_weak:
            acts += [("serve_weak", r) for r in self.t.devices if r not in self.weak]
        if not self.injected and self.b.max_inject:
            acts.append(("inject",))
        if self.msg_count < self.b.max_msgs:
            acts += [("new_message", s) for s in sorted(self.t.senders) if not self.self_revoked(s)]
        for d in self.t.devices:
            acts += [("encrypt", d, m, r) for (m, r) in sorted(self.dev[d].fan)]
        for e in sorted(self.net, key=repr):
            acts.append(("recv", e, False))
            if self.dups < self.b.max_dup:
                acts.append(("recv", e, True))
        for d in self.t.devices:
            if self.P[d] != self.dev[d].snap():
                acts.append(("persist", d))
                if self.crashes < self.b.max_crash:
                    acts.append(("crash", d))
            if self.b.max_restore and self.Bk[d] is None:
                acts.append(("take_backup", d))
            if self.restores < self.b.max_restore and self.Bk[d] is not None:
                acts.append(("restore", d))
        return acts

    def apply(self, act: tuple) -> list:
        label = getattr(self, act[0])(*act[1:])
        self.log.append(label)
        return label

    def drain(self, cap=500):
        """Deliver everything without new faults until quiescent (or cap)."""
        n = 0
        while not self.quiescent() and n < cap:
            acts = [a for a in self.enabled() if a[0] in ("encrypt", "recv") and not (a[0] == "recv" and a[2])]
            if not acts:
                break
            try:
                self.apply(acts[0])
            except Infeasible:
                break
            n += 1
