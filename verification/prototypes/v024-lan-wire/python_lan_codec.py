#!/usr/bin/env python3
from __future__ import annotations
import json,sys
from pathlib import Path

def head(m,n):
    if n<24:return bytes([(m<<5)|n])
    if n<256:return bytes([(m<<5)|24,n])
    if n<65536:return bytes([(m<<5)|25])+n.to_bytes(2,'big')
    if n<2**32:return bytes([(m<<5)|26])+n.to_bytes(4,'big')
    return bytes([(m<<5)|27])+n.to_bytes(8,'big')
def val(x):
    if isinstance(x,int):return head(0,x)
    if isinstance(x,dict) and set(x)=={'hex'}:
        b=bytes.fromhex(x['hex']);return head(2,len(b))+b
    raise TypeError(x)
def enc(pairs):
    encoded=[(val(k),val(v)) for k,v in pairs]
    encoded.sort(key=lambda kv:(len(kv[0]),kv[0]))
    return head(5,len(encoded))+b''.join(k+v for k,v in encoded)
def asmap(pairs):
    if len({k for k,_ in pairs})!=len(pairs): raise ValueError('fields')
    return dict(pairs)
def b(v,nmin,nmax=None):
    if not isinstance(v,dict) or set(v)!={'hex'}: raise ValueError('type')
    n=len(bytes.fromhex(v['hex'])); hi=nmin if nmax is None else nmax
    if not nmin<=n<=hi: raise ValueError('size')
def u(v,lo=0,hi=2**64-1):
    if not isinstance(v,int) or not lo<=v<=hi: raise ValueError('range')
def validate(kind,pairs):
    m=asmap(pairs)
    if kind=='invite':
        if set(m)!={0}: raise ValueError('fields')
        if m[0]!=1: raise ValueError('version')
        return
    if kind=='signal':
        if set(m)!={0,1,2,3,4,5}: raise ValueError('fields')
        if m[0]!=1: raise ValueError('version')
        if m[1] not in (1,2): raise ValueError('kind')
        b(m[2],16);u(m[3]);b(m[4],12);b(m[5],17,12304);return
    if kind!='frame': raise ValueError('type')
    if m.get(0)!=1: raise ValueError('version')
    k=m.get(1)
    if k not in range(1,8): raise ValueError('kind')
    b(m.get(2),16)
    req={1:{0,1,2,3,4,5},2:{0,1,2,6,7},3:{0,1,2},4:{0,1,2,14},5:{0,1,2,8,9,10},6:{0,1,2,10,11,12},7:{0,1,2,13}}[k]
    if set(m)!=req: raise ValueError('fields')
    if k==1:u(m[3],1,8);u(m[4]);b(m[5],32)
    elif k==2:u(m[6]);b(m[7],1,16384)
    elif k==4:u(m[14])
    elif k==5:b(m[8],16);u(m[9],1,65535);b(m[10],16,64)
    elif k==6:b(m[10],16,64);b(m[11],32);b(m[12],32)
    elif k==7:u(m[13],1,65535)
def run(path,write=False):
    p=Path(path);d=json.loads(p.read_text()); bad=[]
    for x in d['positives']:
        try:validate(x['type'],x['value']);h=enc(x['value']).hex()
        except Exception as e:bad.append([x['id'],'positive',str(e)]);continue
        if x.get('expected') is None and write:x['expected']=h
        elif x.get('expected')!=h:bad.append([x['id'],'bytes',h])
    for x in d['negatives']:
        try:validate(x['type'],x['value']);bad.append([x['id'],'accepted',''])
        except ValueError as e:
            if str(e)!=x['error']:bad.append([x['id'],'error',str(e)])
    if write:p.write_text(json.dumps(d,indent=2)+'\n')
    print(json.dumps({'implementation':'python','positive':len(d['positives']),'negative':len(d['negatives']),'mismatches':bad}))
    return 1 if bad else 0
if __name__=='__main__':raise SystemExit(run(sys.argv[1],'--write-expected' in sys.argv[2:]))
