#!/usr/bin/env python3
"""Replay every ADR-0262 canonical CBOR service vector (stdlib only)."""
from __future__ import annotations
import collections
import json
from pathlib import Path
from python_codec import (
    CodecError, BY_NAME, DOMAINS, FIELDS, TYPES,
    decode, encode, jsonable, norm, object_digest,
    sign_wire, validate_wire, validate_body,
)

def check(condition, message):
    if not condition: raise AssertionError(message)

def replay():
    root=Path(__file__).resolve().parent
    vectors=json.loads((root/'vectors.json').read_text())
    check(vectors['encoding']=='OM-CBOR-v1','unknown encoding')
    positive=vectors['positive'];negative=vectors['negative']
    check(len(positive)>=12 and len(negative)>=40,'minimum vectors not met')
    ids=[v['id'] for v in positive+negative]
    check(len(ids)==len(set(ids)),'duplicate vector IDs')
    outcomes=collections.Counter(); categories=collections.Counter()
    for accepted,items in ((True,positive),(False,negative)):
        for v in items:
            name=v['object'];body=norm(v['value']);wire=bytes.fromhex(v['wire_hex'])
            sigs=norm(v['signatures']) if 'signatures' in v else None
            canonical=sign_wire(name,body,signatures=sigs)
            check(canonical.hex()==v['canonical_hex'],v['id']+': canonical_hex mismatch')
            if accepted:
                check(wire==canonical,v['id']+': positive wire not canonical')
                decoded=decode(wire)
                check(encode(decoded)==wire,v['id']+': wrong re-encoded wire')
            else:
                if v['error_class']!='non_canonical':
                    check(wire==canonical,v['id']+': noncanonical byte mismatch on semantic case')
            kwargs={'receiver_wave':v.get('receiver_wave','W4')}
            if 'prior_value' in v:kwargs['prior']=norm(v['prior_value'])
            if 'grant_value' in v:kwargs['grant']=norm(v['grant_value'])
            try:
                actual=validate_wire(name,wire,**kwargs)
            except CodecError as ex:
                if accepted:raise AssertionError(v['id']+': accepted case failed '+str(ex)) from ex
                check(ex.code==v['error_class'],v['id']+': expected '+v['error_class']+', got '+ex.code)
                categories[ex.code]+=1;outcomes['negative']+=1
            else:
                check(accepted,v['id']+': unexpectedly accepted')
                check(actual==body,v['id']+': content mismatch')
                check(jsonable(actual)==v['value'],v['id']+': diagnostic representation mismatch')
                check(encode(actual)==(decode(wire)[1] if BY_NAME[name]['signed'] else wire),
                      v['id']+': body not re-encoded identically')
                if BY_NAME[name]['hash']:
                    d=object_digest(name,actual)
                    check(len(d[1])==32 and d[0]==1,v['id']+': digest mismatch')
                outcomes['positive']+=1
    # New registries must be complete; no existing type assignment/label collision.
    service_types=[t for t in TYPES['types'] if t['scope']=='services_bots']
    check([t['type_id'] for t in service_types]==list(range(47,53)),'type id sequence')
    labels={e['label'] for e in DOMAINS['labels']}
    check(len(labels)==len(DOMAINS['labels']),'domain label collision')
    for t in service_types:
        if t['hash']:check(t['hash'] in labels,t['name']+' missing hash label')
        if t['signed']:check(t['signed']['label'] in labels,t['name']+' missing sig label')
        check(t['name'] in FIELDS['structs'],t['name']+' missing numeric schema')
    event=FIELDS['structs']['ConversationEvent']
    check(any(f['key']==13 and f['name']=='representation' for f in event['fields']),
          'ConversationEvent key 13 not registered')
    check(any(k['key']==14 for k in event['reserved']), 'event key14 must remain reserved')
    print('PASS ADR-0262 OM-CBOR-v1: '+str(outcomes['positive'])+' positive, '+str(outcomes['negative'])+' negative, '+str(len(ids))+' total')
    print('Exact rejected error classes: '+', '.join(f'{name}={n}' for name,n in sorted(categories.items())))
    print('Re-encoded canonical bytes, signed-envelope role guards, W0 key13 rejection, chain and registry invariants: PASS')
    print('Not tested: cryptographic signatures, independent second implementation, authorization proofs, runtime MLS, consent/egress, independent review.')

if __name__=='__main__': replay()
