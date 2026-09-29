from pathlib import Path
import json, sys, re
root=Path(__file__).resolve().parents[4]
ux=root/'docs/design/ui-ux'
errors=[]
short=['one-thread','open-loop','peer-link']
for c in short:
    base=ux/'assets/identity/r12'/c
    for f in ['ios-light-grid.png','ios-dark-grid.png','ios-tinted-grid.png','android-standard-grid.png','android-themed-grid.png','pwa-browser-context.png']:
        p=base/f
        if not p.exists() or p.stat().st_size<1000: errors.append(f'missing/empty {p.relative_to(root)}')
for p in [ux/'assets/identity/r12/previews/platform-context-shortlist-r12.png',ux/'prototypes/identity-validation-v1/index.html',ux/'prototypes/icon-comprehension-v1/index.html',ux/'contracts/om-icon-comprehension-r12.json',ux/'research/PRELIMINARY_IDENTITY_NAME_COLLISION_R12.md']:
    if not p.exists(): errors.append(f'missing {p.relative_to(root)}')
for hp in [ux/'prototypes/identity-validation-v1/index.html',ux/'prototypes/icon-comprehension-v1/index.html']:
    s=hp.read_text()
    if re.search(r'\bfetch\s*\(|XMLHttpRequest|WebSocket\s*\(',s): errors.append(f'network-capable code in {hp.relative_to(root)}')
    if 'localStorage' not in s: errors.append(f'no localStorage in {hp.relative_to(root)}')
# test set size and security-sensitive presence
j=json.loads((ux/'contracts/om-icon-comprehension-r12.json').read_text())
ids={x['id'] for x in j['items']}
if len(ids)!=20: errors.append(f'icon test set count {len(ids)} != 20')
for req in ['omni','spaces','verify','recovery','retry','read']:
    if req not in ids: errors.append(f'missing priority icon {req}')
if errors:
    print('R12 identity validation: FAIL')
    for e in errors: print('-',e)
    sys.exit(1)
print('R12 identity validation: PASS')
print('shortlist:', ', '.join(short))
print('platform context assets: 18 + combined sheet')
print('priority icon comprehension items:', len(ids))
print('local-only fixtures: PASS')
