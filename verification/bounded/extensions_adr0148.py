#!/usr/bin/env python3
"""ADR-0148 presentation-extension vectors, amended by ADR-0149.

Independent Python/Node agreement is preserved while current canonical semantics use
integer-key payloads, registry-bound object/criticality, and deterministic geo fallback.
"""
from __future__ import annotations
from pathlib import Path
import copy, json, re, subprocess, sys
ROOT=Path(__file__).resolve().parents[2]
VEC=ROOT/'verification/vectors/adr0148-v1-extension-vectors.json'
OUT=ROOT/'verification/results/extensions-adr0148-latest.json'
sys.path.insert(0,str(ROOT/'verification/cbor'))
from python_canonical import enc

REG={
 'om.voice-note.v1':('ConversationEvent',False),
 'om.location-card.v1':('ConversationEvent',False),
}

def expand(raw):
    v=copy.deepcopy(raw)
    if v.get('repeat_waveform_bytes'):
        pairs=v['payload']['$map']
        for pair in pairs:
            if pair[0]==1: pair[1]={'$bytes':'00'*v['repeat_waveform_bytes']}
    return v

def mapdict(p):
    if not isinstance(p,dict) or set(p)!={'$map'} or not isinstance(p['$map'],list): return None
    out={}
    for pair in p['$map']:
        if not isinstance(pair,list) or len(pair)!=2 or not isinstance(pair[0],int) or isinstance(pair[0],bool): return None
        if pair[0] in out:return None
        out[pair[0]]=pair[1]
    return out

def blen(x):
    try:return len(bytes.fromhex(x['$bytes'])) if isinstance(x,dict) and set(x)=={'$bytes'} else None
    except Exception:return None

def e7(x:int)->str:
    sign='-' if x<0 else ''
    q,r=divmod(abs(x),10_000_000)
    return f'{sign}{q}.{r:07d}'

def geo_token(lat:int,lon:int)->str:return f'geo:{e7(lat)},{e7(lon)}'

def contains_token(text:str,token:str)->bool:
    # ASCII token boundary: do not accept the required point merely as a prefix of a different coordinate.
    return re.search(r'(?<![A-Za-z0-9_.+-])'+re.escape(token)+r'(?![0-9.])',text) is not None

def validate(raw):
    v=expand(raw); eid=v['extension_id']; critical=v['critical']; owner=v.get('owning_object'); c=v.get('context',{})
    if eid not in REG:
        return ('reject_object','unknown_critical_extension') if critical else ('accept_ignored',None)
    expected_owner,expected_critical=REG[eid]
    if owner!=expected_owner:return 'reject_object','wrong_extension_object'
    if critical!=expected_critical:return 'reject_object','criticality_mismatch'
    p=mapdict(v.get('payload'))
    if p is None:return 'reject_object','wrong_type'
    if eid=='om.voice-note.v1':
        if set(p)-{0,1}:return 'reject_object','unknown_field'
        if 0 not in p:return 'reject_object','missing_field'
        i=p[0]
        if not isinstance(i,int) or isinstance(i,bool) or not 0<=i<=31:return 'reject_object','attachment_index_schema'
        if 1 in p:
            n=blen(p[1])
            if n is None or not 16<=n<=256:return 'reject_object','waveform_bounds'
        aa=c.get('attachments',[])
        if i>=len(aa):return 'ignore_extension','attachment_index'
        a=aa[i]
        if not isinstance(a.get('media_type'),str) or not a['media_type'].startswith('audio/'):return 'ignore_extension','audio_attachment'
        if not isinstance(a.get('duration_ms'),int) or isinstance(a.get('duration_ms'),bool) or a['duration_ms']<0:return 'ignore_extension','duration_ms'
        return 'accept_extension',None
    if set(p)-{0,1,2,3}:return 'reject_object','unknown_field'
    if 0 not in p or 1 not in p:return 'reject_object','missing_field'
    lat_off,lon_off=p[0],p[1]
    if not isinstance(lat_off,int) or isinstance(lat_off,bool) or not 0<=lat_off<=1_800_000_000:return 'reject_object','lat_bounds'
    if not isinstance(lon_off,int) or isinstance(lon_off,bool) or not 0<=lon_off<=3_600_000_000:return 'reject_object','lon_bounds'
    if 2 in p and (not isinstance(p[2],int) or isinstance(p[2],bool) or not 1<=p[2]<=1_000_000):return 'reject_object','accuracy_bounds'
    if 3 in p and (not isinstance(p[3],str) or not 1<=len(p[3].encode())<=256):return 'reject_object','label_bounds'
    lat,lon=lat_off-900_000_000,lon_off-1_800_000_000
    fb=c.get('fallback')
    if not isinstance(fb,str) or not contains_token(fb,geo_token(lat,lon)):return 'ignore_extension','fallback_mismatch'
    return 'accept_extension',None

def main():
    vectors=json.loads(VEC.read_text()); py={}; mismatches=[]
    for raw in vectors:
        v=expand(raw); entry={'$map':[[v['extension_id'],{'$map':[[0,v['critical']],[1,v['payload']]]}]]}
        verdict,error=validate(raw); py[raw['id']]={'verdict':verdict,'error':error,'cbor_hex':enc(entry).hex()}
        if verdict!=raw['expected_verdict'] or error!=raw.get('expected_error'):mismatches.append(raw['id']+':expected')
    cp=subprocess.run(['node',str(ROOT/'verification/cbor/node_adr0148_extensions.mjs'),str(VEC)],capture_output=True,text=True)
    if cp.returncode:
        print(cp.stderr,file=sys.stderr);return cp.returncode
    node=json.loads(cp.stdout)
    for k in py:
        if py[k]!=node.get(k):mismatches.append(k+':python_node')
    semantics={
      'integer_key_payloads': all(set(x['payload'])=={'$map'} for x in vectors),
      'mismatched_nonempty_location_fallback_ignored': py['location-mismatched-nonempty-fallback']['verdict']=='ignore_extension',
      'wrong_object_rejected': py['voice-wrong-object']['verdict']=='reject_object' and py['location-wrong-object']['verdict']=='reject_object',
      'wrong_criticality_rejected': py['voice-critical-forbidden']['verdict']=='reject_object' and py['location-critical-forbidden']['verdict']=='reject_object',
      'unknown_noncritical_ignored': py['unknown-noncritical']['verdict']=='accept_ignored',
    }
    passed=not mismatches and all(semantics.values())
    out={'kind':'adr0148_v1_extension_vectors_amended_adr0149','vector_count':len(vectors),
         'python_node_agreement':len(vectors)-sum(x.endswith(':python_node') for x in mismatches),
         'semantics':semantics,'mismatches':mismatches,'passed':passed,
         'note':'ADR-0148 vectors amended by ADR-0149: registered payloads are integer-keyed; semantic cross-reference/fallback failures ignore the non-critical extension while schema/object/criticality violations reject the object.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
