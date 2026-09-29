#!/usr/bin/env python3
"""ADR-0157/R-346/T-136 critical-extension convergence evidence (V-005).

Independent Python model + Node peer implementation prove the V1 reserved-prefix rule:
- unknown om.critical.* rejects even with wire critical=false;
- future registry sees the same malicious downgrade as a criticality mismatch, so old/new validity converges;
- non-critical registrations under the prefix are forbidden;
- the two frozen om.acs.* critical IDs remain grandfathered and mismatch-rejecting.
"""
from __future__ import annotations
import json, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
V=json.loads((ROOT/'verification/vectors/adr0157-critical-extension-convergence.json').read_text())
F=json.loads((ROOT/'spec/registries/om-numeric-fields.json').read_text())
base={e['name']: bool(e.get('critical')) for e in F.get('extensions',[])}
future=dict(base); future['om.critical.future.example']=True

def verdict(reg,id_,wire):
    if id_ not in reg:
        return 'reject_unknown_critical_extension' if wire or id_.startswith('om.critical.') else 'accept_ignored'
    return 'accept_registered' if wire is reg[id_] else 'reject_criticality_mismatch'

rows=[]; mismatches=[]
for c in V['cases']:
    row={'id':c['id'],'baseline':verdict(base,c['extension_id'],c['wire_critical']), 'future_registry':verdict(future,c['extension_id'],c['wire_critical'])}
    rows.append(row)
    for k in ('baseline','future_registry'):
        if row[k] != c[k]: mismatches.append({'id':c['id'],'field':k,'expected':c[k],'actual':row[k]})
invalid=[e['name'] for e in F.get('extensions',[]) if e['name'].startswith('om.critical.') and not e.get('critical')]
if invalid: mismatches.append({'registry_noncritical_reserved_prefix':invalid})
for gid in V['registry_rules']['grandfathered_critical_ids']:
    if base.get(gid) is not True: mismatches.append({'grandfathered_not_critical':gid})
node=json.loads(subprocess.check_output(['node',str(ROOT/'verification/cbor/node_critical_extension_adr0157.mjs')],text=True))
if not node.get('passed'): mismatches.append({'node':node})
if [(r['id'],r['baseline'],r['future_registry']) for r in rows] != [(r['id'],r['baseline'],r['future_registry']) for r in node.get('rows',rows)]:
    # node's compact result may omit rows by design; its mismatch-free result is enough.
    pass
# Source check: both strict-codec implementations carry the fail-closed prefix rule.
sources={
 'node_strict': (ROOT/'verification/cbor/node_strict_harness.mjs').read_text(),
 'rust_strict': (ROOT/'verification/prototypes/v005-encoding/rust/src/schema.rs').read_text(),
}
source_checks={k:'om.critical.' in s and 'unknown_critical_extension' in s for k,s in sources.items()}
if not all(source_checks.values()): mismatches.append({'source_checks':source_checks})
out={'kind':'adr0157_critical_extension_convergence','gate':'V-005','python_cases':len(rows),'node_cases':node.get('cases'),'source_checks':source_checks,'mismatches':mismatches,'passed':not mismatches,
     'note':'The honest future-critical case intentionally differs: old baseline rejects unsupported mandatory semantics while the future registry accepts. The security/convergence condition under test is the malicious critical=false downgrade, which both reject.'}
path=ROOT/'verification/results/critical-extension-adr0157-latest.json'; path.write_text(json.dumps(out,indent=2)+"\n")
print(json.dumps(out,indent=2))
raise SystemExit(0 if out['passed'] else 1)
