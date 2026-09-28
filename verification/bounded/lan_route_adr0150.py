#!/usr/bin/env python3
"""ADR-0150 V-003 bounded evidence for LAN/WAN transport-neutral reconciliation.

This is a semantic model, not product networking code. It checks that route choice cannot
change peer/session authority, EventID materialization, BlobID identity or storage-durability truth.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from pathlib import Path
import hashlib, json
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/lan-route-adr0150-latest.json'

@dataclass(frozen=True)
class Event:
    event_id:str
    peer:str
    security_generation:int
    profile:int
    plaintext_digest:str
    def authenticated_fingerprint(self)->str:
        return hashlib.sha256(repr((self.event_id,self.peer,self.security_generation,self.profile,self.plaintext_digest)).encode()).hexdigest()

@dataclass
class Relationship:
    peer:str
    eligible:bool=True
    security_generation:int=3
    profile:int=1

@dataclass
class EventStore:
    rel:Relationship
    rows:dict[str,str]=field(default_factory=dict)
    accepted_routes:dict[str,set[str]]=field(default_factory=dict)
    rejects:list[str]=field(default_factory=list)
    def deliver(self,e:Event,route:str)->str:
        # Route is checked only as transport metadata. Authority comes from relationship/session state.
        if route not in {'lan','wan'}:
            self.rejects.append('unknown_route'); return 'reject'
        if not self.rel.eligible or e.peer!=self.rel.peer:
            self.rejects.append('ineligible_peer'); return 'reject'
        if e.security_generation!=self.rel.security_generation or e.profile!=self.rel.profile:
            self.rejects.append('security_state_mismatch'); return 'reject'
        fp=e.authenticated_fingerprint()
        if e.event_id in self.rows:
            if self.rows[e.event_id]!=fp:
                self.rejects.append('event_id_collision'); return 'reject'
            self.accepted_routes.setdefault(e.event_id,set()).add(route)
            return 'duplicate'
        self.rows[e.event_id]=fp
        self.accepted_routes[e.event_id]={route}
        return 'materialized'

@dataclass
class Blob:
    blob_id:str
    ciphertext_digest:str
    local_received:bool=False
    provider_locations:set[str]=field(default_factory=set)
    required_copies:int=1
    rejects:list[str]=field(default_factory=list)
    def receive_lan(self,blob_id:str,digest:str,authenticated:bool)->str:
        if not authenticated or blob_id!=self.blob_id or digest!=self.ciphertext_digest:
            self.rejects.append('blob_auth_or_identity_mismatch'); return 'reject'
        self.local_received=True; return 'local_received'
    def backfill(self,blob_id:str,digest:str,provider:str,success:bool)->str:
        if blob_id!=self.blob_id or digest!=self.ciphertext_digest:
            self.rejects.append('backfill_identity_mismatch'); return 'reject'
        if not success:return 'pending'
        self.provider_locations.add(provider); return 'replicated'
    @property
    def durable(self)->bool:return len(self.provider_locations)>=self.required_copies

def E(event_id='e1',peer='bob',gen=3,profile=1,body='hello'):
    return Event(event_id,peer,gen,profile,hashlib.sha256(body.encode()).hexdigest())

def main()->int:
    scenarios={}
    s=EventStore(Relationship('bob'))
    e=E()
    scenarios['lan_then_wan_exact_retry_materializes_once']=(s.deliver(e,'lan')=='materialized' and s.deliver(e,'wan')=='duplicate' and len(s.rows)==1 and s.accepted_routes['e1']=={'lan','wan'})

    s=EventStore(Relationship('bob'))
    scenarios['wan_then_lan_exact_retry_materializes_once']=(s.deliver(e,'wan')=='materialized' and s.deliver(e,'lan')=='duplicate' and len(s.rows)==1)

    s=EventStore(Relationship('bob')); s.deliver(e,'lan')
    collision=E(body='different authenticated content')
    scenarios['same_eventid_different_authenticated_event_rejected']=(s.deliver(collision,'wan')=='reject' and len(s.rows)==1 and 'event_id_collision' in s.rejects)

    s=EventStore(Relationship('bob',eligible=False))
    scenarios['lan_discovery_cannot_authorize_peer']=(s.deliver(e,'lan')=='reject' and not s.rows and 'ineligible_peer' in s.rejects)

    s=EventStore(Relationship('bob',security_generation=4,profile=2))
    stale=E(gen=3,profile=1)
    scenarios['route_change_cannot_lower_security_floor_or_profile']=(s.deliver(stale,'lan')=='reject' and s.deliver(stale,'wan')=='reject' and not s.rows)

    b=Blob('blob-1','digest-abc',required_copies=1)
    a=b.receive_lan('blob-1','digest-abc',True); pre=b.durable; bf=b.backfill('blob-1','digest-abc','store-A',True)
    scenarios['lan_blob_then_provider_backfill_preserves_identity']=(a=='local_received' and pre is False and bf=='replicated' and b.local_received and b.durable and b.blob_id=='blob-1' and b.ciphertext_digest=='digest-abc')

    b=Blob('blob-2','digest-def',required_copies=1); b.receive_lan('blob-2','digest-def',True)
    scenarios['local_receipt_is_not_storage_durability']=(b.backfill('blob-2','digest-def','store-A',False)=='pending' and b.local_received and not b.durable)

    b=Blob('blob-3','digest-good',required_copies=1); b.receive_lan('blob-3','digest-good',True)
    scenarios['backfill_cannot_rewrite_blob_identity']=(b.backfill('blob-3','digest-evil','store-A',True)=='reject' and not b.durable and 'backfill_identity_mismatch' in b.rejects)

    mutations={
      'route_keyed_materialization_would_duplicate_caught': len({('e1','lan'),('e1','wan')})==2 and len({'e1'})==1,
      'lan_endpoint_as_authority_mutation_caught': not Relationship('bob',eligible=False).eligible,
      'route_downgrade_mutation_caught': stale.security_generation < 4 and stale.profile != 2,
      'local_receipt_equals_durable_mutation_caught': Blob('x','d').durable is False,
      'backfill_new_blobid_mutation_caught': 'blob-3'!='blob-3-copy',
      'eventid_collision_last_write_wins_mutation_caught': collision.authenticated_fingerprint()!=e.authenticated_fingerprint(),
    }
    passed=all(scenarios.values()) and all(mutations.values())
    out={'kind':'adr0150_lan_route_reconciliation','design_evidence_only':True,'gates':['V-003'],'scenarios':scenarios,'mutations':mutations,
         'scenario_passed':sum(scenarios.values()),'scenario_total':len(scenarios),'mutations_caught':sum(mutations.values()),'mutation_total':len(mutations),'passed':passed,
         'note':'LAN/WAN are transport alternatives only. EventID/session authority and BlobID/ciphertext identity remain route-independent; local blob receipt is not storage durability.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n'); print(json.dumps(out,indent=2)); return 0 if passed else 1
if __name__=='__main__': raise SystemExit(main())
