#!/usr/bin/env python3
from pathlib import Path
import json, sys, re
ROOT=Path(__file__).resolve().parents[3]
errors=[]
def need(path, *terms):
    p=ROOT/path
    if not p.exists(): errors.append(f'missing {path}'); return
    t=p.read_text(encoding='utf-8')
    for term in terms:
        if term.lower() not in t.lower(): errors.append(f'{path} missing {term!r}')
need('docs/architecture/ux/UI_UX_SYSTEM.md','Chats','Omnibar','Protection','Focus','V-018','V-022')
need('docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md','T0 sponsored','Publication','Safety Freeze','call timeline item','typed Core intent')
need('docs/architecture/ux/ACTION_TO_CORE_INTENT_MAP.md','SuspendDevice','ScheduleComposeIntent','BuildEvidencePackage','Publication')
need('docs/ux-design/SCREEN_MAP_AND_JOURNEYS.md','Invite someone','Sponsored','Topic','Publication','Suspend this device','Safety Freeze')
need('docs/ux-design/specs/ARCHITECTURE_MANDATED_V1_SURFACES.md','Polls','MessageRequest','Protection','EvidencePackage','link previews')
# stale fixed-tab seed must be gone as a normative declaration
screen=(ROOT/'docs/ux-design/SCREEN_MAP_AND_JOURNEYS.md').read_text()
if re.search(r'AppShell\s*\n├── Chats\s*\n.*├── Calls\s*\n.*├── People',screen,re.S): errors.append('fixed three-tab shell survived')
# machine ledgers
reqs=[json.loads(x) for x in (ROOT/'machine/requirements.jsonl').read_text().splitlines() if x.strip()]
ids={x['id'] for x in reqs}
for n in range(351,361):
    if f'R-{n}' not in ids: errors.append(f'missing R-{n}')
decs=[json.loads(x) for x in (ROOT/'machine/decisions.jsonl').read_text().splitlines() if x.strip()]
if not any(x.get('id')=='D-181' and x.get('adr')=='ADR-0159' for x in decs): errors.append('missing D-181/ADR-0159')
tr=json.loads((ROOT/'machine/requirement_traceability.json').read_text())
traced={x.get('requirement') for x in tr.get('traces',[])}
for n in range(351,361):
    if f'R-{n}' not in traced: errors.append(f'R-{n} untraced')
# action map must be valid and security-critical entries use core path
am=json.loads((ROOT/'docs/ux-design/contracts/om-action-core-intent-map-v1.json').read_text())
for a in am['actions']:
    if a.get('risk')=='security-critical' and not str(a.get('path','')).startswith('core'):
        errors.append(f"security-critical action not core-routed: {a['id']}")
if errors:
    print('R15 UI/UX integration: FAIL')
    for e in errors: print(' -',e)
    sys.exit(1)
print(f"R15 UI/UX integration: PASS — {len(reqs)} requirements, {len(decs)} decisions, {len(am['actions'])} mapped actions")
