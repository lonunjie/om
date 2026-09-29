#!/usr/bin/env python3
from __future__ import annotations
import json, sys
from pathlib import Path

def bytesv(x):
    if isinstance(x,dict) and set(x)=={'$bytes'}: return bytes.fromhex(x['$bytes'])
    raise ValueError('expected bytes')

def norm(x):
    if isinstance(x,dict):
        if set(x)=={'$bytes'}: return bytesv(x)
        if set(x)=={'$set'}: return [norm(v) for v in x['$set']]
        if set(x)=={'$map'}: return {norm(k):norm(v) for k,v in x['$map']}
        return {int(k):norm(v) for k,v in x.items()}
    if isinstance(x,list): return [norm(v) for v in x]
    return x

def head(major,n):
    if n<24:return bytes([(major<<5)|n])
    if n<256:return bytes([(major<<5)|24,n])
    if n<65536:return bytes([(major<<5)|25])+n.to_bytes(2,'big')
    if n<2**32:return bytes([(major<<5)|26])+n.to_bytes(4,'big')
    if n<2**64:return bytes([(major<<5)|27])+n.to_bytes(8,'big')
    raise ValueError('integer too large')

def enc(x):
    if x is None:return b'\xf6'
    if x is False:return b'\xf4'
    if x is True:return b'\xf5'
    if isinstance(x,int) and x>=0:return head(0,x)
    if isinstance(x,str):
        b=x.encode(); return head(3,len(b))+b
    if isinstance(x,bytes): return head(2,len(x))+x
    if isinstance(x,list): return head(4,len(x))+b''.join(enc(v) for v in x)
    if isinstance(x,dict):
        pairs=[(enc(k),enc(v)) for k,v in x.items()]
        pairs.sort(key=lambda kv:(len(kv[0]),kv[0]))
        if any(pairs[i][0]>=pairs[i+1][0] for i in range(len(pairs)-1)): raise ValueError('duplicate/unsorted map')
        return head(5,len(pairs))+b''.join(k+v for k,v in pairs)
    raise ValueError(type(x).__name__)

def need(m,k):
    if k not in m: raise ValueError(f'missing:{k}')
    return m[k]

def bstr(m,k,nmin,nmax):
    v=need(m,k)
    if not isinstance(v,bytes) or not (nmin<=len(v)<=nmax): raise ValueError(f'field:{k}')
    return v

def tstr(m,k,nmin,nmax):
    v=need(m,k)
    if not isinstance(v,str) or not (nmin<=len(v.encode())<=nmax): raise ValueError(f'field:{k}')
    return v

def uint(m,k,nmin=0):
    v=need(m,k)
    if type(v) is not int or v<nmin: raise ValueError(f'field:{k}')
    return v

def principal(v):
    if not isinstance(v,dict) or set(v)!={0,1}: raise ValueError('principal')
    if v[0] not in (1,2,3) or not isinstance(v[1],bytes) or not 16<=len(v[1])<=32: raise ValueError('principal')

def digest(v):
    if not isinstance(v,dict) or set(v)!={0,1} or v[0]!=1 or not isinstance(v[1],bytes) or len(v[1])!=32: raise ValueError('digest')

def common_top(m,typ):
    if m.get(0)!=typ or m.get(1)!=1: raise ValueError('object_type_or_version')
    if any(k not in {0,1,2,3,4,5,6,7,8,9,10,11,12,23} for k in m): raise ValueError('unknown_field')

def validate_space(m):
    common_top(m,24); bstr(m,2,32,32); uint(m,3,1)
    if 4 in m:digest(m[4])
    p=m.get(5)
    if not isinstance(p,dict) or 0 not in p or not isinstance(p[0],str) or not 1<=len(p[0].encode())<=256: raise ValueError('profile')
    for k,n in ((1,2048),(2,4096),(3,2048)):
        if k in p and (not isinstance(p[k],str) or not 1<=len(p[k].encode())<=n): raise ValueError('profile')
    mem=m.get(6)
    if not isinstance(mem,list) or not 1<=len(mem)<=1024: raise ValueError('membership')
    for x in mem:
        if not isinstance(x,dict) or 0 not in x: raise ValueError('membership')
        principal(x[0])
        if 1 in x:
            if not isinstance(x[1],list) or not 1<=len(x[1])<=64 or any(not isinstance(r,bytes) or len(r)!=16 for r in x[1]): raise ValueError('roles')
    if 7 in m:
        if not isinstance(m[7],list) or not 1<=len(m[7])<=128: raise ValueError('roles')
        for r in m[7]:
            if not isinstance(r,dict) or set(r)-{0,1,2,3} or not isinstance(r.get(0),bytes) or len(r[0])!=16: raise ValueError('role')
            tstr(r,1,1,128)
            for k in (2,3):
                if k in r and (not isinstance(r[k],list) or not 1<=len(r[k])<=64 or any(not isinstance(c,str) or not 1<=len(c.encode())<=128 for c in r[k])): raise ValueError('role_scope')
    if 8 in m:
        if not isinstance(m[8],list) or not 1<=len(m[8])<=512: raise ValueError('links')
        for l in m[8]:
            if not isinstance(l,dict) or l.get(0) not in (1,2,3): raise ValueError('link')
            bstr(l,1,32,32); bstr(l,3,1,65536)
            if 2 in l and l[2] not in (1,2): raise ValueError('visibility')
    g=m.get(9)
    if not isinstance(g,dict) or not isinstance(g.get(0),list) or not 1<=len(g[0])<=64: raise ValueError('governance')
    for p in g[0]: principal(p)
    if type(g.get(1)) is not int or not 1<=g[1]<=64: raise ValueError('threshold')
    if 2 in g and (not isinstance(g[2],list) or not 1<=len(g[2])<=128): raise ValueError('delegation')

def validate_intent(m):
    common_top(m,25); bstr(m,2,32,32); digest(m[3]); uint(m,4,1); bstr(m,5,32,32); tstr(m,6,1,128)
    if type(m.get(7)) is not bool or m.get(8) not in (1,2,3): raise ValueError('intent')
    bstr(m,9,1,65536)
    if 10 in m: principal(m[10])
    if 11 in m: bstr(m,11,16,32)
    if 12 in m and (not isinstance(m[12],list) or not 1<=len(m[12])<=128): raise ValueError('capabilities')

def validate_event(m):
    if m.get(0)!=10 or m.get(1)!=1: raise ValueError('event_type')
    required={0,1,2,3,4,5,6,7,8,9,11};
    if not required<=set(m): raise ValueError('event_missing')
    if any(k not in {0,1,2,3,4,5,6,7,8,9,10,11,12,23} for k in m): raise ValueError('event_unknown')
    if m[2]<1 or not isinstance(m[2],int): raise ValueError('event_protocol')
    bstr(m,3,16,16); bstr(m,4,32,32); principal(m[5]); bstr(m,6,16,32)
    if not isinstance(m[7],dict): raise ValueError('hlc')
    if m[8] not in range(6,15): raise ValueError('kind')
    if not isinstance(m[9],int) or m[9]<1: raise ValueError('kind_version')
    body=m[11]
    if not isinstance(body,dict): raise ValueError('body')
    if m[8]==7:
        bstr(body,0,16,16)
        if 1 in body:tstr(body,1,1,256)
    elif m[8]==11:
        bstr(body,0,16,16); tstr(body,1,1,256)
    elif m[8]==12:
        bstr(body,0,16,16); tstr(body,1,1,256)
    elif m[8]==13:
        bstr(body,0,16,16)
        if body.get(1) not in (1,2,3): raise ValueError('topic_status')
    elif m[8]==14:
        bstr(body,0,16,16)
        if 1 in body:bstr(body,1,16,16)

def validate_pub_control(m):
    common_top(m,26); bstr(m,2,32,32); uint(m,3,1)
    if 4 in m:digest(m[4])
    p=m[5]; tstr(p,0,1,256)
    for k,n in ((1,2048),(2,4096)):
        if k in p and (not isinstance(p[k],str) or not 1<=len(p[k].encode())<=n): raise ValueError('profile')
    if not isinstance(m[6],list) or not 1<=len(m[6])<=64: raise ValueError('publisher_authority')
    for x in m[6]: principal(x)
    if not isinstance(m[7],list) or not 1<=len(m[7])<=64: raise ValueError('keys')
    for x in m[7]:
        bstr(x,0,16,64)
        if x.get(1) not in (1,2): raise ValueError('key_status')
        if x.get(1)==2 and (2 not in x or type(x[2]) is not int): raise ValueError('last_valid_seq')
    if 8 in m:bstr(m,8,32,32)
    g=m[9]
    if not isinstance(g.get(0),list) or not 1<=len(g[0])<=64: raise ValueError('gov')
    for x in g[0]:principal(x)
    if type(g.get(1)) is not int or not 1<=g[1]<=64: raise ValueError('gov')

def validate_pub_event(m):
    common_top(m,27); bstr(m,2,32,32); bstr(m,3,16,32)
    if not isinstance(m.get(4),dict): raise ValueError('clock')
    if m.get(5) not in (1,2,3): raise ValueError('kind')
    if not isinstance(m.get(6),dict): raise ValueError('body')
    bstr(m,7,16,64); uint(m,8,0); digest(m[10])
    if m[8]==0 and 9 in m: raise ValueError('genesis_prev_hash')
    if m[8]>0 and 9 not in m: raise ValueError('missing_prev_hash')
    if m[5] in (1,2):
        if 0 not in m[6]: raise ValueError('pub_body_content')
        if not isinstance(m[6][0],dict): raise ValueError('rich_text')
    else:
        if 0 in m[6]: tstr(m[6],0,1,256)

def validate(name,m):
    return {'SpaceControlState':validate_space,'SpaceDelegatedIntent':validate_intent,'ConversationEvent':validate_event,'PublicationControlState':validate_pub_control,'PublicationEvent':validate_pub_event}[name](m)

def signature(role):
    return {0:b'\x10'*16,1:1,2:role,3:b'\x20'*64}

def envelope(body,role): return {1:enc(body),2:[signature(role)]}

def main(path):
    vec=json.load(open(path)); out={}
    roles={'SpaceControlState':3,'SpaceDelegatedIntent':3,'PublicationControlState':3,'PublicationEvent':7}
    for group,want in [('positive',True),('negative',False)]:
        for v in vec[group]:
            m=norm(v['value']); ok=True; err=None
            try: validate(v['object'],m)
            except Exception as e: ok=False; err=str(e)
            if ok!=want: raise SystemExit(f"{v['id']}: verdict mismatch {ok} != {want} ({err})")
            if ok:
                body=enc(m)
                wire=enc(envelope(m,roles[v['object']])) if v.get('envelope') else body
                out[v['id']]={'verdict':'accept','body_hex':body.hex(),'wire_hex':wire.hex()}
            else: out[v['id']]={'verdict':'reject','error':err}
    print(json.dumps(out,sort_keys=True))
    return out
if __name__=='__main__': main(sys.argv[1])
