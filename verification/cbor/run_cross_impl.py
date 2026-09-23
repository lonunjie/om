#!/usr/bin/env python3
from __future__ import annotations
import json, subprocess, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]; HERE=Path(__file__).resolve().parent
fix=HERE/'fixtures.json'
p=json.loads(subprocess.check_output([sys.executable,str(HERE/'python_canonical.py'),str(fix)],text=True))
n=json.loads(subprocess.check_output(['node',str(HERE/'node_canonical.mjs'),str(fix)],text=True))
assert p==n, {'python':p,'node':n}
# Negative profile checks are intentionally local to each implementation source:
# floats are refused and only definite lengths are ever emitted.
result={
 'gate':'V-005',
 'gate_status_effect':'foundation_evidence_only_gate_remains_open',
 'independent_encoders':['Python stdlib implementation','Node.js stdlib implementation'],
 'vectors_compared':len(p),
 'all_vectors_identical':True,
 'golden_vectors':p,
 'remaining':[
   'freeze CDDL/numeric field registry/domain-separation string for every signed/hashed v1 object',
   'generate object-specific positive and negative canonicality vectors',
   'consume those vectors from production-language decoder/encoder paths',
   'reject non-canonical alternate byte encodings for all signed/hashed object types'
 ]
}
out=ROOT/'verification/results/cbor-cross-impl-latest.json'; out.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
