#!/usr/bin/env python3
"""Replay every ADR-0264 W4 bridges canonical CBOR vector; standard library only."""
from __future__ import annotations
import collections
import json
from pathlib import Path
from python_codec import (CodecError, BY_NAME, DOMAINS, FIELDS, TYPES, NEW_NAMES,
                          decode, encode, jsonable, norm, object_digest,
                          origin_fingerprint, sign_wire, validate_wire)


def require(ok,why):
    if not ok:raise AssertionError(why)


def main():
    here=Path(__file__).resolve().parent
    data=json.loads((here/'vectors.json').read_text())
    require(data['encoding']=='OM-CBOR-v1','encoding mismatch')
    yes=data['positive'];no=data['negative']
    require(len(yes)>=12 and len(no)>=40,'not enough acceptance/rejection vectors')
    ids=[v['id'] for v in yes+no]
    require(len(ids)==len(set(ids)),'nonunique vector IDs')
    by_error=collections.Counter();by_form=collections.Counter()
    for is_valid,group in ((True,yes),(False,no)):
        for v in group:
            identity=v['id'];name=v['object'];body=norm(v['value'])
            wire=bytes.fromhex(v['wire_hex'])
            sigs=norm(v['signatures']) if 'signatures' in v else None
            canonical=sign_wire(name,body,signatures=sigs)
            require(canonical.hex()==v['canonical_hex'],identity+': invalid canonical_hex')
            if is_valid:
                require(wire==canonical,identity+': positive must have canonical wire')
                require(encode(decode(wire))==wire,identity+': not round-trip deterministic')
            elif v['error_class'] not in ('non_canonical','max_bytes'):
                require(wire==canonical,identity+': semantic negative altered wire')
            kwargs={'receiver_wave':v.get('receiver_wave','W4')}
            for context in ('prior','control','mapping'):
                if context+'_value' in v:kwargs[context]=norm(v[context+'_value'])
            if 'now' in v:kwargs['now']=v['now']
            try:
                decoded=validate_wire(name,wire,**kwargs)
            except CodecError as exc:
                if is_valid:raise AssertionError(identity+': valid vector rejected '+str(exc)) from exc
                require(exc.code==v['error_class'],identity+': expected '+v['error_class']+' got '+exc.code)
                by_error[exc.code]+=1
            else:
                require(is_valid,identity+': negative accepted')
                require(decoded==body,identity+': wrong decoded body')
                require(jsonable(decoded)==v['value'],identity+': diagnostic CBOR/JSON mismatch')
                if BY_NAME[name]['hash']:
                    hashed=object_digest(name,decoded)
                    require(hashed[0]==1 and len(hashed[1])==32,identity+': wrong body hash')
                by_form[name]+=1
    bridge_types=[o for o in TYPES['types'] if o['scope']=='bridges']
    require([x['type_id'] for x in bridge_types]==list(range(53,59)),'W4 bridge ids must be 53..58')
    require([x['type_id'] for x in TYPES['types'] if x['scope']=='services_bots']==list(range(47,53)),
            'W4 service assignments changed')
    require(len({x['type_id'] for x in TYPES['types']})==len(TYPES['types']),'object id collision')
    require(len({x['label'] for x in DOMAINS['labels']})==len(DOMAINS['labels']),'domain label collision')
    labels={x['label']:x for x in DOMAINS['labels']}
    for t in bridge_types:
        require(t['wave']=='W4' and t['name'] in FIELDS['structs'],t['name']+' missing W4 registry struct')
        if t['hash']:
            require(labels[t['hash']]['construction']=='DS-HASH',t['name']+' hash construction')
        if t['signed']:
            require(labels[t['signed']['label']]['construction']=='DS-SIG',t['name']+' signature construction')
            require(t['signed']['roles']==['controller'],t['name']+' signature role')
    require(labels['OM-INTEROP-FP-MAC-v1']['use']=='mac','missing keyed fingerprint label')
    fields=FIELDS['structs']['ConversationEvent']['fields']
    key13=next(x for x in fields if x['key']==13)
    key14=next(x for x in fields if x['key']==14)
    require(key13['name']=='representation' and key13['wave']=='W4','service field 13 changed')
    require(key14['name']=='foreign_provenance' and key14['wave']=='W4' and
            key14['type']=={'ref':'ForeignProvenanceBody'},'typed critical event field 14')
    require(all(x['key']!=14 for x in FIELDS['structs']['ConversationEvent'].get('reserved',[])),
            'field 14 still reserved')
    require(all(x not in FIELDS['enums']['event_kind'] for x in ('19','20')),
            'no new bridge kind should be allocated')
    secret=b'\xa0'*32
    fp=origin_fingerprint(secret,'matrix','ev-0001')
    require(len(fp)==32,'bad fingerprint size')
    require(fp!=origin_fingerprint(b'\xa1'*32,'matrix','ev-0001'),'cross-Room linkability')
    require(fp!=origin_fingerprint(secret,'matrix','ev-0002'),'distinct events collided')
    require(fp!=origin_fingerprint(secret,'telegram','ev-0001'),'distinct namespaces collided')
    print('PASS ADR-0264 OM-CBOR-v1: '+str(len(yes))+' positive + '+str(len(no))+' negative = '+str(len(ids))+' vectors')
    print('Accepted forms: '+', '.join(k+'='+str(v) for k,v in sorted(by_form.items())))
    print('Exact rejected classes: '+', '.join(k+'='+str(v) for k,v in sorted(by_error.items())))
    print('PASS: deterministic CBOR, top-level bounds, field/wave guards, W0 key14 rejection, signed roles, chain/author/trace constraints, Room-keyed fingerprints and registries.')
    print('NOT PROVEN: crypto signatures, MLS/Room governance, real identity proofs, adapter execution, a second independent codec or independent human review.')

if __name__=='__main__':main()
