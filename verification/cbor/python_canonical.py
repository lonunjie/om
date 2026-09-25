#!/usr/bin/env python3
from __future__ import annotations
import json, sys

def head(major,n):
    if n < 24: return bytes([(major<<5)|n])
    if n < 256: return bytes([(major<<5)|24,n])
    if n < 65536: return bytes([(major<<5)|25])+n.to_bytes(2,'big')
    if n < 2**32: return bytes([(major<<5)|26])+n.to_bytes(4,'big')
    if n < 2**64: return bytes([(major<<5)|27])+n.to_bytes(8,'big')
    raise ValueError('integer too large')

def enc(x):
    if x is None: return b'\xf6'
    if x is False: return b'\xf4'
    if x is True: return b'\xf5'
    if isinstance(x,float): raise TypeError('floats forbidden')
    if isinstance(x,int): return head(0,x) if x>=0 else head(1,-1-x)
    if isinstance(x,str):
        b=x.encode('utf-8'); return head(3,len(b))+b
    if isinstance(x,list): return head(4,len(x))+b''.join(enc(v) for v in x)
    if isinstance(x,dict):
        if set(x)=={'$bytes'}:
            b=bytes.fromhex(x['$bytes']); return head(2,len(b))+b
        if set(x)=={'$map'}:
            pairs=[]
            for k,v in x['$map']:
                ek=enc(k); pairs.append((ek,enc(v)))
            pairs.sort(key=lambda kv:(len(kv[0]),kv[0]))
            return head(5,len(pairs))+b''.join(k+v for k,v in pairs)
        pairs=[]
        for k,v in x.items():
            ek=enc(k); pairs.append((ek,enc(v)))
        pairs.sort(key=lambda kv:(len(kv[0]),kv[0]))
        return head(5,len(pairs))+b''.join(k+v for k,v in pairs)
    raise TypeError(type(x).__name__)

def main(path):
    out={}
    for f in json.load(open(path,encoding='utf-8')):
        h=enc(f['value']).hex(); out[f['id']]=h
        if f.get('expected') is not None and h != f['expected']:
            raise SystemExit(f"{f['id']}: expected {f['expected']} got {h}")
    print(json.dumps(out,sort_keys=True))
if __name__=='__main__': main(sys.argv[1])
