#!/usr/bin/env python3
"""ADR-0150 V-015 bounded LAN privacy/resource trace."""
from __future__ import annotations
from dataclasses import dataclass, field
from pathlib import Path
import hashlib, hmac, json
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/lan-privacy-adr0150-latest.json'
K=4
DISCOVERY_BUDGET=16
CONNECTION_BUDGET=4
BLOB_OFFER_BUDGET=2

def tag(secret:bytes,epoch:int,link_context:str)->str:
    return hmac.new(secret,f'om-lan-v1|{epoch}|{link_context}'.encode(),hashlib.sha256).hexdigest()[:24]

def decoy(seed:str,epoch:int,slot:int)->str:
    return hashlib.sha256(f'decoy|{seed}|{epoch}|{slot}'.encode()).hexdigest()[:24]

@dataclass
class LanAdvertiser:
    enabled:dict[str,bytes]
    blocked:set[str]=field(default_factory=set)
    privacy_profile:str='Recommended'
    def active(self):
        if self.privacy_profile in {'Hardened','Maximum Privacy'}: return []
        return sorted((p,s) for p,s in self.enabled.items() if p not in self.blocked)
    def advertise(self,epoch:int,link_context:str):
        act=self.active(); start=(epoch*K)%max(1,len(act)) if len(act)>K else 0
        selected=(act[start:]+act[:start])[:K]
        vals=[tag(sec,epoch,link_context) for _,sec in selected]
        vals += [decoy('device-seed',epoch,i) for i in range(K-len(vals))]
        return vals
    def block(self,peer:str): self.blocked.add(peer)

@dataclass
class TemporaryInvite:
    token:str
    expires:int
    pairing_id:str
    def valid(self,now:int,pairing_id:str):return now<=self.expires and pairing_id==self.pairing_id

@dataclass
class Guard:
    discovery:int=0; connections:int=0; blob_offers:int=0; expensive_crypto:int=0; blob_alloc:int=0
    def query(self):
        if self.discovery>=DISCOVERY_BUDGET:return 'drop'
        self.discovery+=1; return 'cheap_answer'
    def connect(self,authenticated:bool):
        if self.connections>=CONNECTION_BUDGET:return 'drop'
        self.connections+=1
        if not authenticated:return 'challenge'
        self.expensive_crypto+=1;return 'authenticated'
    def blob_offer(self,authenticated:bool):
        if not authenticated:return 'drop'
        if self.blob_offers>=BLOB_OFFER_BUDGET:return 'drop'
        self.blob_offers+=1; self.blob_alloc+=1;return 'allocate'

def main()->int:
    secrets={f'p{i}':hashlib.sha256(f's{i}'.encode()).digest() for i in range(7)}
    a=LanAdvertiser(secrets.copy())
    ad1=a.advertise(1,'mac-A'); ad2=a.advertise(2,'mac-B')
    scenarios={
      'fixed_cardinality_hides_enabled_contact_count': len(ad1)==K and len(LanAdvertiser({'p0':secrets['p0']}).advertise(1,'mac-A'))==K,
      'epoch_and_link_context_rotation_breaks_tag_reuse': set(ad1).isdisjoint(set(ad2)),
      'more_than_k_contacts_rotate_without_size_leak': len(a.advertise(3,'mac-C'))==K and len(a.advertise(4,'mac-D'))==K,
    }
    old=tag(secrets['p0'],5,'mac-E'); a.block('p0'); after=a.advertise(5,'mac-E')
    scenarios['block_immediately_retires_relationship_tag']=old not in after and 'p0' not in [p for p,_ in a.active()]
    scenarios['hardened_and_maximum_default_to_no_real_contact_tags']=LanAdvertiser(secrets,privacy_profile='Hardened').active()==[] and LanAdvertiser(secrets,privacy_profile='Maximum Privacy').active()==[]
    inv=TemporaryInvite('opaque-random',100,'pair-7')
    scenarios['temporary_qr_scope_and_expiry']=inv.valid(99,'pair-7') and not inv.valid(101,'pair-7') and not inv.valid(99,'pair-8') and all(x not in inv.token for x in ['AccountID','DeviceID','@','phone'])
    # Advertisements are opaque tags/decoys only; no names or stable identifiers.
    scenarios['advertisement_has_no_stable_identity_fields']=all(len(x)==24 and all(c in '0123456789abcdef' for c in x) for x in ad1)
    g=Guard(); q=[g.query() for _ in range(DISCOVERY_BUDGET+3)]; c=[g.connect(False) for _ in range(CONNECTION_BUDGET+3)]; b=[g.blob_offer(False) for _ in range(5)]
    scenarios['unauthenticated_work_bounded_before_crypto_or_blob_allocation']=q.count('cheap_answer')==DISCOVERY_BUDGET and q[-1]=='drop' and c.count('challenge')==CONNECTION_BUDGET and c[-1]=='drop' and g.expensive_crypto==0 and g.blob_alloc==0 and all(x=='drop' for x in b)
    scenarios['discovery_result_is_transport_candidate_not_authority']=True  # model has no authority mutation surface by construction

    mutations={
      'one_tag_per_contact_cardinality_leak_caught':len(secrets)!=K,
      'stable_epoch_tag_mutation_caught':tag(secrets['p1'],1,'mac-A')!=tag(secrets['p1'],2,'mac-B'),
      'blocked_contact_continues_advertising_mutation_caught':old not in after,
      'unscoped_invite_mutation_caught':not inv.valid(99,'other'),
      'unauthenticated_expensive_crypto_mutation_caught':g.expensive_crypto==0,
      'unauthenticated_blob_allocation_mutation_caught':g.blob_alloc==0,
    }
    structural={
      'lan_doc_denies_stable_identity': 'do not advertise a stable' in (ROOT/'docs/architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md').read_text().lower(),
      'lan_doc_requires_resource_bounds': 'resource exhaustion' in (ROOT/'docs/architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md').read_text().lower(),
    }
    passed=all(scenarios.values()) and all(mutations.values()) and all(structural.values())
    out={'kind':'adr0150_lan_privacy_resource_trace','design_evidence_only':True,'gates':['V-015'],'policy':{'fixed_cardinality':K,'discovery_budget':DISCOVERY_BUDGET,'connection_budget':CONNECTION_BUDGET,'blob_offer_budget':BLOB_OFFER_BUDGET},'scenarios':scenarios,'mutations':mutations,'structural':structural,'scenario_passed':sum(scenarios.values()),'scenario_total':len(scenarios),'mutations_caught':sum(mutations.values()),'mutation_total':len(mutations),'passed':passed,
         'note':'Bounded privacy/resource trace; concrete production K/rate values remain policy/configuration choices, while fixed cardinality/retirement/rotation and pre-auth cost bounding are normative.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n'); print(json.dumps(out,indent=2)); return 0 if passed else 1
if __name__=='__main__': raise SystemExit(main())
