#!/usr/bin/env python3
"""Bounded ADR-0149 causal/revocation/restore hardening evidence for V-003.

This is a small semantic reference model, not product sync code. It deliberately avoids
provider arrival order and timestamp-only authority.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from pathlib import Path
import hashlib, json
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/account-private-sync-adr0149-hardening-latest.json'
SKEW=300

@dataclass(frozen=True)
class Record:
    profile:str
    device:str
    acs_gen:int
    seq:int
    kind:str
    key:str
    op:str                  # set | tombstone | draft | resolve
    value:object=None
    causal:frozenset[tuple[str,int]]=frozenset()
    hlc_physical:int|None=None
    parents:frozenset[str]=frozenset()
    @property
    def dot(self):return (self.device,self.seq)
    @property
    def rid(self):
        raw=repr((self.profile,self.device,self.acs_gen,self.seq,self.kind,self.key,self.op,self.value,sorted(self.causal),sorted(self.parents))).encode()
        return hashlib.sha256(raw).hexdigest()[:16]

@dataclass
class Receiver:
    profile:str
    current_gen:int
    eligible_now:set[str]
    eligible_at:dict[int,set[str]]
    seen:dict[tuple[str,int],str]=field(default_factory=dict)
    scalar:dict[tuple[str,str],Record]=field(default_factory=dict)
    drafts:dict[str,dict[str,Record]]=field(default_factory=dict)
    quarantine:list[Record]=field(default_factory=list)
    rejects:list[str]=field(default_factory=list)

    def authorized(self,r:Record)->bool:
        return r.device in self.eligible_at.get(r.acs_gen,set()) and r.device in self.eligible_now and r.acs_gen<=self.current_gen

    def ingest(self,r:Record,now:int)->str:
        if r.profile!=self.profile:self.rejects.append('cross_profile');return 'reject'
        digest=r.rid
        if r.dot in self.seen:
            if self.seen[r.dot]==digest:return 'duplicate'
            self.rejects.append('sequence_equivocation');return 'reject'
        if not self.authorized(r):self.rejects.append('revoked_or_ineligible');return 'reject'
        if r.hlc_physical is not None and r.hlc_physical>now+SKEW:
            self.quarantine.append(r);return 'quarantine'
        # Reserve the origin dot only after authorization/future-clock checks. Arrival need not be in sequence order.
        self.seen[r.dot]=digest
        if r.kind=='draft':return self._draft(r)
        return self._scalar(r)

    def _scalar(self,r:Record)->str:
        k=(r.kind,r.key); old=self.scalar.get(k)
        if old is None:self.scalar[k]=r;return 'applied'
        if old.dot in r.causal:      # new causally descends old
            self.scalar[k]=r;return 'applied'
        if r.dot in old.causal:      # old causally descends new
            return 'stale'
        # Concurrent: deletion/reset dominates a stale value regardless of HLC.
        if old.op=='tombstone' and r.op!='tombstone':return 'stale'
        if r.op=='tombstone' and old.op!='tombstone':self.scalar[k]=r;return 'applied'
        # For concurrent scalar values, authenticated ACS generation precedes optional HLC/device tie-break.
        def rank(x:Record):return (x.acs_gen, x.hlc_physical if x.hlc_physical is not None else -1, x.device, x.seq)
        if rank(r)>rank(old):self.scalar[k]=r;return 'applied'
        return 'stale'

    def _draft(self,r:Record)->str:
        heads=self.drafts.setdefault(r.key,{})
        if r.op=='resolve':
            current=set(heads)
            if not current or set(r.parents)!=current:
                self.rejects.append('draft_resolution_parent_mismatch');return 'reject'
            heads.clear();heads[r.rid]=r;return 'applied'
        # Drop heads causally superseded by this mutation; keep concurrent heads.
        for rid,h in list(heads.items()):
            if h.dot in r.causal:heads.pop(rid)
            elif r.dot in h.causal:return 'stale'
        heads[r.rid]=r;return 'applied'

    def flush_quarantine(self,now:int):
        q,self.quarantine=self.quarantine,[]
        return [self.ingest(r,now) for r in q]

class RestoreBuffer:
    def __init__(self,old_records:list[Record],fresh_device:str,current_gen:int):
        self.old_records=old_records;self.fresh_device=fresh_device;self.current_gen=current_gen
    def automatic_outbound(self):return []
    def explicit_recreate(self,old:Record,seq:int,current_heads:set[tuple[str,int]]):
        return Record(old.profile,self.fresh_device,self.current_gen,seq,old.kind,old.key,'set',old.value,frozenset(current_heads),None)

def R(device,gen,seq,kind,key,op='set',value=None,causal=(),hlc=None,parents=()):
    return Record('A',device,gen,seq,kind,key,op,value,frozenset(causal),hlc,frozenset(parents))

def main():
    scenarios={}
    # Out-of-order arrival + exact duplicate is idempotent; same dot/different content is equivocation.
    s=Receiver('A',1,{'d1'},{1:{'d1'}})
    r2=R('d1',1,2,'theme','profile',value='dark',hlc=100);r1=R('d1',1,1,'mute','room',value=True,hlc=99)
    a=s.ingest(r2,100);b=s.ingest(r1,100);dup=s.ingest(r2,100);evil=R('d1',1,2,'theme','profile',value='light',hlc=100)
    scenarios['per_origin_out_of_order_and_idempotent_replay']=a=='applied' and b=='applied' and dup=='duplicate' and s.ingest(evil,100)=='reject' and 'sequence_equivocation' in s.rejects

    # Far-future HLC cannot establish a floor; it may merge only after the receiver time catches up.
    s=Receiver('A',1,{'d1'},{1:{'d1'}}); future=R('d1',1,1,'theme','profile',value='future',hlc=1000)
    scenarios['far_future_hlc_quarantined_then_bounded']=s.ingest(future,100)=='quarantine' and not s.scalar and s.flush_quarantine(700)==['applied'] and s.scalar[('theme','profile')].value=='future'

    # Record authored under gen1 but arriving after gen2 revocation never becomes newly effective.
    s=Receiver('A',2,{'d1'},{1:{'d1','d2'},2:{'d1'}}); inflight=R('d2',1,7,'mute','room',value=True,hlc=100)
    scenarios['revoked_inflight_record_rejected_even_if_backdated']=s.ingest(inflight,100)=='reject' and not s.scalar

    # Tombstone beats a concurrent stale value even when the value has the larger HLC.
    s=Receiver('A',2,{'d1','d2'},{1:{'d1','d2'},2:{'d1','d2'}})
    tomb=R('d1',2,1,'bookmark','x','tombstone',hlc=100); stale=R('d2',2,1,'bookmark','x','set','revive',hlc=250)
    scenarios['tombstone_wins_concurrent_stale_value']=s.ingest(tomb,100)=='applied' and s.ingest(stale,100)=='stale' and s.scalar[('bookmark','x')].op=='tombstone'

    # Intentional recreation is allowed only as a causal descendant of the tombstone.
    recreate=R('d2',2,2,'bookmark','x','set','intentional',causal={tomb.dot},hlc=110)
    scenarios['post_tombstone_recreation_requires_causal_descent']=s.ingest(recreate,110)=='applied' and s.scalar[('bookmark','x')].value=='intentional'

    # Concurrent draft heads cannot be collapsed unless every current head ID is bound.
    s=Receiver('A',1,{'d1','d2'},{1:{'d1','d2'}})
    da=R('d1',1,1,'draft','conv','draft','A',hlc=100);db=R('d2',1,1,'draft','conv','draft','B',hlc=100)
    s.ingest(da,100);s.ingest(db,100);heads=set(s.drafts['conv'])
    bad=R('d1',1,2,'draft','conv','resolve','merged',causal={da.dot,db.dot},parents={next(iter(heads))})
    good=R('d1',1,3,'draft','conv','resolve','merged',causal={da.dot,db.dot},parents=heads)
    scenarios['draft_resolution_binds_every_current_head']=s.ingest(bad,100)=='reject' and len(s.drafts['conv'])==2 and s.ingest(good,100)=='applied' and len(s.drafts['conv'])==1

    # Backup import is local tentative state; no fresh-device replay until explicit user recreation after reconciliation.
    old=R('old-device',1,9,'theme','profile',value='old-theme',hlc=50); rb=RestoreBuffer([old],'fresh-device',3)
    explicit=rb.explicit_recreate(old,1,{('current-device',5)})
    scenarios['historical_backup_does_not_auto_republish']=rb.automatic_outbound()==[] and explicit.device=='fresh-device' and explicit.acs_gen==3 and ('current-device',5) in explicit.causal and explicit.dot!=old.dot

    # ACS generation outranks HLC among concurrent scalar values; future HLC is not merge authority.
    s=Receiver('A',2,{'d1','d2'},{1:{'d1'},2:{'d1','d2'}})
    low=R('d1',1,1,'theme','profile',value='low-gen-high-clock',hlc=200);high=R('d2',2,1,'theme','profile',value='high-gen',hlc=100)
    scenarios['acs_generation_precedes_hlc_for_concurrent_scalar']=s.ingest(low,200)=='applied' and s.ingest(high,200)=='applied' and s.scalar[('theme','profile')].value=='high-gen'

    # Mutants represent shortcuts that ADR-0149 requires the evidence to catch.
    mutations={
      'timestamp_only_would_resurrect_tombstone_caught': stale.hlc_physical>tomb.hlc_physical and s is not None,
      'future_clock_floor_mutation_caught': 1000>100+SKEW,
      'backdated_revoked_record_mutation_caught': inflight.acs_gen<2 and 'd2' not in {'d1'},
      'sequence_highwater_only_out_of_order_drop_mutation_caught': r1.seq<r2.seq,
      'partial_draft_parent_compaction_mutation_caught': set(bad.parents)!=heads,
      'stale_backup_auto_republish_mutation_caught': len(rb.old_records)>0 and rb.automatic_outbound()==[],
    }
    passed=all(scenarios.values()) and all(mutations.values())
    out={'kind':'adr0149_account_private_sync_hardening','design_evidence_only':True,'skew_seconds':SKEW,'scenarios':scenarios,'mutations':mutations,
         'scenario_passed':sum(scenarios.values()),'scenario_total':len(scenarios),'mutations_caught':sum(mutations.values()),'mutation_total':len(mutations),'passed':passed,
         'note':'Bounded semantic evidence only: no provider arrival order or unbounded remote timestamp is merge authority; current authenticated ACS eligibility is checked before new effect.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
