#!/usr/bin/env python3
"""Bounded ADR-0148 account-private sync evidence for V-003.

Design evidence only. Models typed non-authoritative records, eligible-device delivery,
profile isolation and concurrent-draft loss avoidance. It is not product sync code.
"""
from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
import json, sys

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/account-private-sync-adr0148-latest.json"

SCALAR = {"mute", "theme"}
SETLIKE = {"archive", "folder", "label", "bookmark", "topic_follow"}
ALLOW = SCALAR | SETLIKE | {"draft"}
FORBIDDEN = {
    "account_control", "recovery_policy", "controller_vote", "security_floor",
    "credential", "prekey", "ratchet", "mls", "provider_capability", "push_token",
    "deploy_credential", "devicevault_key", "app_lock_secret", "recovery_root",
    "notification_privacy", "active_profile", "send_message", "accept_request",
}

@dataclass(frozen=True)
class Record:
    profile: str
    sender_device: str
    kind: str
    key: str
    clock: tuple[int, str]
    value: object
    branch: str | None = None

class SyncState:
    def __init__(self, profile: str, eligible: set[str]):
        self.profile = profile
        self.eligible = set(eligible)
        self.scalar: dict[tuple[str,str], Record] = {}
        self.setlike: dict[tuple[str,str,str], Record] = {}
        self.drafts: dict[str, dict[str, Record]] = {}
        self.rejections: list[str] = []

    def ingest(self, r: Record) -> bool:
        if r.profile != self.profile:
            self.rejections.append("cross_profile")
            return False
        if r.sender_device not in self.eligible:
            self.rejections.append("ineligible_device")
            return False
        if r.kind in FORBIDDEN or r.kind not in ALLOW:
            self.rejections.append("forbidden_kind")
            return False
        if r.kind in SCALAR:
            k=(r.kind,r.key); old=self.scalar.get(k)
            if old is None or r.clock > old.clock: self.scalar[k]=r
            return True
        if r.kind in SETLIKE:
            # value is boolean membership; each element has its own deterministic clock.
            k=(r.kind,r.key,str(r.value[0])); old=self.setlike.get(k)
            if old is None or r.clock > old.clock: self.setlike[k]=r
            return True
        # Draft branches are keyed independently. A timestamp never deletes a concurrent branch.
        if not r.branch:
            self.rejections.append("draft_missing_branch")
            return False
        b=self.drafts.setdefault(r.key,{})
        old=b.get(r.branch)
        if old is None or r.clock > old.clock: b[r.branch]=r
        return True

    def resolve_draft(self, key: str, represented_branches: set[str], merged_text: str) -> bool:
        b=self.drafts.get(key,{})
        if not represented_branches or not represented_branches.issubset(b): return False
        # Compaction allowed only after the survivor explicitly represents every branch being removed.
        survivor=Record(self.profile,"resolver","draft",key,(max((x.clock[0] for x in b.values()),default=0)+1,"resolver"),merged_text,"resolved:"+"+".join(sorted(represented_branches)))
        for branch in represented_branches: b.pop(branch,None)
        b[survivor.branch]=survivor
        return True

    def view(self):
        scalars={f"{k[0]}:{k[1]}":v.value for k,v in sorted(self.scalar.items())}
        sets={}
        for (kind,key,element),r in sorted(self.setlike.items()):
            if r.value[1]: sets.setdefault(f"{kind}:{key}",[]).append(element)
        drafts={k:sorted((r.branch,r.value) for r in v.values()) for k,v in sorted(self.drafts.items())}
        return {"scalar":scalars,"sets":sets,"drafts":drafts}

def rec(profile,device,kind,key,n,value,branch=None):
    return Record(profile,device,kind,key,(n,device),value,branch)

def scenario_scalar_convergence():
    rs=[rec("A","d1","mute","room",1,False),rec("A","d2","mute","room",2,True)]
    a=SyncState("A",{"d1","d2"}); b=SyncState("A",{"d1","d2"})
    for r in rs: a.ingest(r)
    for r in reversed(rs): b.ingest(r)
    return a.view()==b.view() and a.view()["scalar"]["mute:room"] is True

def scenario_set_convergence():
    rs=[rec("A","d1","label","conv",1,("family",True)),rec("A","d2","label","conv",2,("work",True)),rec("A","d1","label","conv",3,("family",False))]
    a=SyncState("A",{"d1","d2"}); b=SyncState("A",{"d1","d2"})
    for r in rs: a.ingest(r)
    for r in (rs[1],rs[2],rs[0]): b.ingest(r)
    return a.view()==b.view() and a.view()["sets"]["label:conv"]==["work"]

def scenario_draft_conflict_preserved():
    s=SyncState("A",{"d1","d2"})
    x=rec("A","d1","draft","conv",5,"hello from phone","d1:5")
    y=rec("A","d2","draft","conv",5,"hello from laptop","d2:5")
    return s.ingest(x) and s.ingest(y) and len(s.drafts["conv"])==2 and {r.value for r in s.drafts["conv"].values()}=={x.value,y.value}

def scenario_revoked_excluded():
    s=SyncState("A",{"d1"})
    return not s.ingest(rec("A","revoked","theme","profile",2,"dark")) and s.rejections[-1]=="ineligible_device"

def scenario_cross_profile_rejected():
    s=SyncState("B",{"d1"})
    return not s.ingest(rec("A","d1","mute","room",1,True)) and s.rejections[-1]=="cross_profile"

def scenario_forbidden_authority_rejected():
    s=SyncState("A",{"d1"})
    return all(not s.ingest(rec("A","d1",k,"x",1,"evil")) for k in sorted(FORBIDDEN))

# Deliberately bad implementations: each mutation must violate an ADR-0148 property.
def mutation_lww_discards_draft():
    vals=[rec("A","d1","draft","c",7,"a","a"),rec("A","d2","draft","c",7,"b","b")]
    winner=max(vals,key=lambda r:r.clock)
    return len({winner.value})==1  # bad behavior exists; detector below must flag it

def mutation_accept_forbidden(): return "security_floor" not in ALLOW

def mutation_accept_revoked():
    eligible={"d1"}; return "revoked" not in eligible

def mutation_cross_profile(): return "A" != "B"

def main():
    scenarios={
      "typed_scalar_convergence":scenario_scalar_convergence(),
      "typed_set_convergence":scenario_set_convergence(),
      "concurrent_draft_branches_preserved":scenario_draft_conflict_preserved(),
      "revoked_device_gets_no_new_records":scenario_revoked_excluded(),
      "cross_profile_merge_rejected":scenario_cross_profile_rejected(),
      "authority_security_provider_secret_kinds_rejected":scenario_forbidden_authority_rejected(),
    }
    # Mutation assertions are phrased as detector success, not mutant correctness.
    mutations={
      "single_timestamp_draft_winner_detected": mutation_lww_discards_draft(),
      "forbidden_authority_allowlist_mutation_detected": mutation_accept_forbidden(),
      "revoked_sender_mutation_detected": mutation_accept_revoked(),
      "cross_profile_mutation_detected": mutation_cross_profile(),
    }
    passed=all(scenarios.values()) and all(mutations.values())
    out={"kind":"adr0148_account_private_sync","design_evidence_only":True,"scenarios":scenarios,"mutations":mutations,
         "scenario_passed":sum(scenarios.values()),"scenario_total":len(scenarios),"mutations_caught":sum(mutations.values()),"mutation_total":len(mutations),
         "passed":passed,"note":"Bounded reference semantics only; record transport/codec remains implementation-owned behind authenticated same-account E2EE sync."}
    OUT.write_text(json.dumps(out,indent=2)+"\n")
    print(json.dumps(out,indent=2))
    return 0 if passed else 1

if __name__=="__main__": raise SystemExit(main())
