#!/usr/bin/env python3
from __future__ import annotations
import json,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
V=HERE/'vectors.json';OUT=ROOT/'verification/results/v024-lan-wire-r16b-latest.json'
def run(cmd):
 p=subprocess.run(cmd,capture_output=True,text=True,cwd=ROOT)
 if p.returncode: print(p.stdout);print(p.stderr,file=sys.stderr);raise SystemExit(p.returncode)
 return json.loads(p.stdout.strip().splitlines()[-1])
def main():
 py=run([sys.executable,str(HERE/'python_lan_codec.py'),str(V),'--write-expected'])
 node=run(['node',str(HERE/'node_lan_codec.mjs'),str(V)])
 d=json.loads(V.read_text())
 passed=not py['mismatches'] and not node['mismatches'] and all(x.get('expected') for x in d['positives'])
 out={'kind':'r16b_v024_lan_two_codec_vectors','gates':['V-024'],'passed':passed,'python':py,'node':node,
      'positive_vectors':len(d['positives']),'negative_vectors':len(d['negatives']),
      'criterion_evidenced':'ADR-0158 LanDeliveryProfile frozen as spec/cddl/lan.cddl with registered numeric fields/domain-separation labels and profile constants, plus positive/negative vectors on which two independently written codecs agree (R-342, R-349)',
      'note':'The codecs are independently written schema-specific validators/encoders. Browser WebRTC feasibility is separate.'}
 OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
