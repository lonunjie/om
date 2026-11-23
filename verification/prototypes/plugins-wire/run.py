#!/usr/bin/env python3
"""Replay W4 plugin exact-byte fixtures with independent stdlib codec."""
from __future__ import annotations
import collections
import json
from pathlib import Path
from python_codec import (CodecError, FIELDS, DOMAINS, OBJECTS, PLUGIN_NAMES, TYPES,
                          decode, diagnostic, from_diagnostic, wrap, validate_wire, encode, digest)

def check(ok,why):
    if not ok:raise AssertionError(why)

def replay():
    path=Path(__file__).resolve().parent/'vectors.json'
    test=json.loads(path.read_text())
    check(test['encoding']=='OM-CBOR-v1','unexpected encoding')
    good=test['positive'];bad=test['negative']
    check(len(good)>=14 and len(bad)>=50,'missing required vector count')
    check(len({x['id'] for x in good+bad})==len(good)+len(bad),'non-unique fixture IDs')
    classes=collections.Counter();forms=collections.Counter()
    for expected,group in [(True,good),(False,bad)]:
        for v in group:
            name=v['object'];body=from_diagnostic(v['value']);code=bytes.fromhex(v['wire_hex'])
            sigs=[from_diagnostic(x) for x in v['signatures']] if 'signatures' in v else None
            canonical=wrap(name,body,sigs)
            check(canonical.hex()==v['canonical_hex'],v['id']+' canonical oracle mismatch')
            if expected:
                check(code==canonical,v['id']+' positive noncanonical')
                check(encode(decode(code))==code,v['id']+' not round-trip deterministic')
            elif v['error_class'] not in ('non_canonical','max_bytes'):
                check(code==canonical,v['id']+' semantic negative uses corrupted wire')
            ctx={}
            for key in ('prior','prior_release','prior_release_record','control','manifest','pending_call'):
                if key+'_value' in v:ctx[key]=from_diagnostic(v[key+'_value'])
            if 'grants_values' in v:ctx['grants']=[from_diagnostic(x) for x in v['grants_values']]
            if 'now' in v:ctx['now']=v['now']
            if 'tainted' in v:ctx['tainted']=v['tainted']
            if 'policy' in v:ctx['policy']=v['policy']
            if 'channel_hex' in v:ctx['channel']=bytes.fromhex(v['channel_hex'])
            if 'origin_bound' in v:ctx['origin_bound']=v['origin_bound']
            ctx['receiver_wave']=v.get('receiver_wave','W4')
            try:result=validate_wire(name,code,**ctx)
            except CodecError as e:
                if expected:raise AssertionError(f'{v["id"]} positive failed: {e}') from e
                check(e.code==v['error_class'],f'{v["id"]}: expected {v["error_class"]}, got {e.code} ({e})')
                classes[e.code]+=1
            else:
                check(expected,v['id']+' invalid vector was accepted')
                check(result==body,v['id']+' wrong decoded structure')
                check(diagnostic(result)==v['value'],v['id']+' diagnostic mismatch')
                if TYPES[name]['hash']:
                    h=digest(name,result)
                    check(h[0]==1 and len(h[1])==32,v['id']+' digest wrong')
                forms[name]+=1
    plugin=[r for r in OBJECTS['types'] if r['scope']=='plugins']
    check([x['type_id'] for x in plugin]==list(range(59,68)),'not allocated additively after bridge 58')
    check([x['type_id'] for x in OBJECTS['types'] if x['scope']=='bridges']==list(range(53,59)),
          'bridges 53..58 were changed')
    check(len({x['type_id'] for x in OBJECTS['types']})==len(OBJECTS['types']),'type collision')
    check(len({x['label'] for x in DOMAINS['labels']})==len(DOMAINS['labels']),'domain collision')
    names={x['label']:x for x in DOMAINS['labels']}
    for t in plugin:
        check(t['wave']=='W4' and t['name'] in FIELDS['structs'],t['name']+' schema missing')
        if t['signed']:
            lbl=t['signed']['label'];check(names[lbl]['construction']=='DS-SIG',lbl+' wrong label')
            check(t['signed']['roles']==(['device'] if t['name']=='PluginCapabilityGrant' else ['controller']),
                  t['name']+' wrong role')
        if t['hash']:check(names[t['hash']]['construction']=='DS-HASH',t['name']+' wrong digest')
        for field in FIELDS['structs'][t['name']]['fields']:
            check(field.get('wave')=='W4','missing W4 numeric-field wave tag')
    check(len(FIELDS['enums']['plugin_capability_id'])==17,'architecture capability registry incomplete')
    check(len(PLUGIN_NAMES)==9,'unexpected plugin top-level object count')
    print('PASS ADR-0266: '+str(len(good))+' positive + '+str(len(bad))+' negative = '+str(len(good)+len(bad))+' exact-CBOR vectors')
    print('Accepted forms: '+', '.join(f'{name}={count}' for name,count in sorted(forms.items())))
    print('Exact rejected classes: '+', '.join(f'{name}={count}' for name,count in sorted(classes.items())))
    print('PASS W4 registry continuity, strict typed maps, signed roles, bound requests, revoked chains, permission ceiling, taint/sending constraints and W0 rejection.')
    print('NOT PROVEN: real signatures, external user consent, runtime site isolation, independent codec/review, release authorization.')

if __name__=='__main__':replay()
