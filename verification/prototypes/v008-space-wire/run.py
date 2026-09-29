#!/usr/bin/env python3
from __future__ import annotations
import json, subprocess, sys
from pathlib import Path
HERE=Path(__file__).resolve().parent
V=HERE/'vectors.json'
py=json.loads(subprocess.check_output([sys.executable,str(HERE/'python_codec.py'),str(V)],text=True))
node=json.loads(subprocess.check_output(['node',str(HERE/'node_codec.mjs'),str(V)],text=True))
if set(py)!=set(node): raise SystemExit('vector id mismatch')
for k in py:
    if py[k]!=node[k]: raise SystemExit(f'{k}: python/node mismatch\nPY={py[k]}\nNODE={node[k]}')
summary={'kind':'v008_space_publication_wire_r16c','passed':True,'vector_count':len(py),'positive':sum(x['verdict']=='accept' for x in py.values()),'negative':sum(x['verdict']=='reject' for x in py.values()),'codecs':['python_codec.py','node_codec.mjs'],'agreement':True}
(HERE.parent.parent/'results/v008-space-publication-wire-r16c-latest.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
