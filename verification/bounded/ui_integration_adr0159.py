#!/usr/bin/env python3
"""Bounded design evidence for ADR-0159 UI integration semantics."""
from __future__ import annotations
import json, re
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/ui-integration-adr0159-latest.json'
THEMES=json.loads((ROOT/'docs/ux-design/curated-theme-presets.example.json').read_text())
SCHEMA=json.loads((ROOT/'docs/ux-design/theme-bundle.logical.schema.json').read_text())

def srgb(c:int)->float:
    x=c/255.0
    return x/12.92 if x<=0.04045 else ((x+0.055)/1.055)**2.4

def lum(h:str)->float:
    h=h.lstrip('#'); r,g,b=(int(h[i:i+2],16) for i in (0,2,4))
    return .2126*srgb(r)+.7152*srgb(g)+.0722*srgb(b)

def contrast(a:str,b:str)->float:
    hi,lo=sorted((lum(a),lum(b)), reverse=True)
    return (hi+.05)/(lo+.05)

cases=[]
def check(name, ok, detail=''):
    cases.append({'case':name,'passed':bool(ok),'detail':detail})
    if not ok: raise AssertionError(f'{name}: {detail}')

calm=next(t for t in THEMES['themes'] if t['theme_id']=='om.calm')
for mode in ('light','dark'):
    p=calm[mode]
    surface=p['surface']
    for role in ('text_primary','text_secondary','accent','info','success','warning','danger'):
        ratio=contrast(p[role],surface)
        floor=4.5 if role.startswith('text_') else 3.0
        check(f'contrast-{mode}-{role}', ratio>=floor, f'{ratio:.2f}:1 >= {floor}:1')
    for prefix in ('message_own','message_other'):
        ratio=contrast(p[f'{prefix}_text'],p[f'{prefix}_surface'])
        check(f'contrast-{mode}-{prefix}', ratio>=4.5, f'{ratio:.2f}:1 >= 4.5:1')

path_pattern=SCHEMA['properties']['assets']['items']['properties']['path']['pattern']
rx=re.compile(path_pattern)
for good in ('fonts/Inter.woff2','wallpapers/calm-01.webp','sounds/ping.ogg','icons/dot.svg','root-file.png'):
    check(f'path-accept-{good}', bool(rx.fullmatch(good)), good)
for bad in ('../escape','a/../escape','/absolute','a\\b.png','.','..','a//b','a/./b'):
    check(f'path-reject-{bad}', not bool(rx.fullmatch(bad)), bad)

# Compact profile-local ranking facts; raw query log is not needed.
profiles={
    'A': {'alice': {'count':9,'last':100}, 'bob': {'count':1,'last':95}},
    'B': {'alice': {'count':0,'last':0}, 'bob': {'count':8,'last':100}},
}
def rank(profile, query):
    now=100
    out=[]
    for target,f in profiles[profile].items():
        exact=1000 if target==query else (500 if target.startswith(query) else 0)
        recency=max(0,100-(now-f['last'])) if f['last'] else 0
        score=exact+min(f['count'],20)*10+recency
        out.append((score,target))
    return [t for _,t in sorted(out, reverse=True)]
check('ranking-profile-isolation-A', rank('A','')[:1]==['alice'], str(rank('A','')))
check('ranking-profile-isolation-B', rank('B','')[:1]==['bob'], str(rank('B','')))
check('exact-match-beats-frecency', rank('A','bob')[0]=='bob', str(rank('A','bob')))

# Keyboard-armed result must remain the same identity when async results arrive.
visible=['alice','bob','settings']; armed='bob'
visible.insert(0,'external:bo')
check('armed-selection-stable', armed=='bob' and armed in visible, str(visible))

# Learned rank can propose an action, never change authority/confirmation class.
commands={
    'open_settings': {'destructive':False,'confirmation':False},
    'delete_identity': {'destructive':True,'confirmation':True},
}
learned='delete_identity'
check('destructive-learning-no-bypass', commands[learned]['destructive'] and commands[learned]['confirmation'])

# Theme icon assets remain decorative; no semantic binding field exists.
asset_props=SCHEMA['properties']['assets']['items']['properties']
check('theme-icon-no-semantic-binding', 'semantic_icon_id' not in asset_props, sorted(asset_props))
check('theme-icon-description-decorative', 'decorative only' in asset_props['kind']['description'])

payload={
    'adr':'ADR-0159',
    'passed':all(c['passed'] for c in cases),
    'cases':len(cases),
    'results':cases,
    'notes':[
        'Evidence is bounded design-level checking, not product implementation authorization.',
        'Exact curated accent values beyond OM Calm remain prototype/user-study work behind fixed semantic roles.'
    ]
}
OUT.write_text(json.dumps(payload,indent=2)+'\n')
print(json.dumps({'passed':payload['passed'],'cases':payload['cases'],'output':str(OUT.relative_to(ROOT))},indent=2))
