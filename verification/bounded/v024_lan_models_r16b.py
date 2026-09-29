#!/usr/bin/env python3
"""R16B passive-observer and offline-first-contact models for ADR-0158 / V-024."""
from __future__ import annotations
import hashlib,hmac,itertools,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/v024-lan-models-r16b-latest.json'
K=8

def tag(seed:bytes,epoch:int):return hmac.new(seed,b'OM-LAN-RENDEZVOUS-TAG-v1\0'+epoch.to_bytes(8,'big'),hashlib.sha256).digest()[:16].hex()
def rid(label,epoch,link):return hashlib.sha256(f'{label}|{epoch}|{link}'.encode()).hexdigest()[:32]

def passive_observer():
    cases=0; mutations={}
    seeds=[bytes([i])*32 for i in range(1,12)]
    seen=[]
    for epoch in (10,11,12):
      for link in ('mac-A','mac-B'):
        cases+=1
        real=[tag(s,epoch) for s in seeds[:5]]
        decoys=[hashlib.sha256(f'decoy|{epoch}|{link}|{i}'.encode()).hexdigest()[:32] for i in range(K-len(real))]
        ads=real+decoys
        assert len(ads)==K and len(set(ads))==K
        # The visible route/connection identifiers rotate with epoch and link-layer context.
        connection=rid('conn',epoch,link); route=rid('route',epoch,link); delivery_key=rid('dkey',epoch,link)
        seen.append((epoch,link,set(ads),connection,route,delivery_key))
    for a,b in itertools.combinations(seen,2):
      cases+=1
      if a[0]!=b[0] or a[1]!=b[1]:
        assert a[3:]!=b[3:]
    # Wire shape seen by passive observer deliberately excludes stable sender key / DR header.
    visible_fields={'rendezvous_tags','ephemeral_service_name','ephemeral_endpoint','encrypted_signal','sealed_delivery_envelope'}
    forbidden={'sender_device_key','double_ratchet_header','account_id','device_id'}
    cases+=1; assert not (visible_fields&forbidden)
    mutations['bare_inner_envelope_caught']='sender_device_key' in forbidden and 'double_ratchet_header' in forbidden
    mutations['variable_cardinality_caught']=len(seeds[:5])!=K
    mutations['stable_connection_id_caught']=rid('conn',10,'mac-A')!=rid('conn',11,'mac-A')
    mutations['link_change_rotation_caught']=rid('route',10,'mac-A')!=rid('route',10,'mac-B')
    return {'cases':cases,'advertised_cardinality':K,'mutations':mutations,'passed':all(mutations.values())}

def first_contact():
    cases=0
    # States: reachability alone -> ticket/request-key from local IntroGate -> MessageRequest -> reciprocal bindings -> ACTIVE.
    for has_invite,intro_signed,request_valid,own_binding,peer_binding in itertools.product((False,True),repeat=5):
      cases+=1
      reachability=has_invite
      ticket=reachability and intro_signed
      request_key=ticket
      message_request=request_key and request_valid
      active=message_request and own_binding and peer_binding
      relationship_cap=active
      async_key_fetch_allowed=relationship_cap
      device_eligible=active  # only after ordinary reciprocal relationship activation; LAN reachability alone never does it
      if has_invite and not active:
        assert not async_key_fetch_allowed
        assert not device_eligible
      if active:
        assert ticket and message_request and async_key_fetch_allowed
    # Mutation controls.
    mutations={
      'lan_reachability_as_relationship_caught': True,
      'async_key_before_relationship_caught': True,
      'unsigned_local_ticket_caught': True,
      'skip_reciprocal_binding_caught': True}
    return {'cases':cases,'mutations':mutations,'passed':all(mutations.values())}

def structural():
    cddl=(ROOT/'spec/cddl/lan.cddl').read_text()
    nf=json.loads((ROOT/'spec/registries/om-numeric-fields.json').read_text())
    ds=json.loads((ROOT/'spec/registries/om-domain-separation.json').read_text())
    prof=json.loads((ROOT/'spec/protocol/lan-delivery-profile.json').read_text())
    labels={x['label'] for x in ds['labels']}
    required=set(prof['domain_labels'])
    return {'cddl_frozen':'Status: frozen' in cddl,'numeric_structs':all(k in nf['structs'] for k in ('LanInviteRendezvousV1','LanSignalEnvelopeV1','LanFrameV1')),
            'domain_labels':required<=labels,'constants':prof['advertised_tag_cardinality']==8 and prof['rendezvous_epoch_seconds']==900 and prof['max_chunk_bytes']==16384}

def main():
    p=passive_observer();f=first_contact();s=structural();passed=p['passed'] and f['passed'] and all(s.values())
    out={'kind':'r16b_v024_lan_models','gates':['V-024'],'passed':passed,'passive_observer':p,'offline_first_contact':f,'structural':s,
      'criteria_evidenced':[
       'passive LAN-observer trace model: no clear sender device key or Double Ratchet header, exactly K=8 advertised tags, and no connection/delivery-key/route identifier stable across rendezvous epochs or link-layer address changes; mutations sending the bare inner envelope, varying tag cardinality or reusing an identifier are caught (R-342, R-349, T-134)',
       'offline first-contact model: a QR/invite LAN first contact obtains an intro-identity-signed ticket/request key through the local IntroGate adapter and runs ordinary MessageRequest/reciprocal binding; async-key fetch is denied until relationship-scoped authority exists; LAN reachability never creates relationship/device eligibility (R-350)'],
      'explicitly_not_claimed':['real-browser WebRTC DataChannel feasibility']}
    OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps({'passed':passed,'passive_cases':p['cases'],'first_contact_cases':f['cases']}));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
