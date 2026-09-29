#!/usr/bin/env python3
from pathlib import Path
import json, hashlib, sys, io
from PIL import Image
import cairosvg

ROOT=Path(__file__).resolve().parents[4]
CONTRACT=ROOT/'docs/design/ui-ux/contracts/om-identity-assets-r11c.json'
req_ids={'open-loop','dialogue-fold','peer-link','om-fold','one-thread'}
required_suffixes={
    'base/mark.svg','base/app-default.svg','base/app-dark.svg',
    'pwa/icon-192.png','pwa/icon-512.png','pwa/icon-maskable-512.png','pwa/icon-monochrome.svg',
    'android/foreground.svg','android/background.svg','android/monochrome.svg','android/adaptive-preview-512.png',
    'apple/background.svg','apple/foreground.svg','apple/default-1024.png','apple/dark-1024.png','apple/tinted-preview-1024.png',
    'web/favicon-16.png','web/favicon-32.png','web/favicon-48.png','web/favicon.svg',
    'notification/mark.svg','notification/mark-96.png'
}

def fail(msg):
    print('FAIL:',msg); sys.exit(1)

d=json.load(open(CONTRACT))
ids={c['id'] for c in d['candidates']}
if ids!=req_ids: fail(f'candidate IDs mismatch: {ids}')
for c in d['candidates']:
    fmap={Path(f['path']).name: f for f in c['files']}
    rels={str(Path(f['path']).relative_to(f'docs/design/ui-ux/assets/identity/r11c/candidates/{c["id"]}')) for f in c['files']}
    missing=required_suffixes-rels
    if missing: fail(f'{c["id"]} missing {sorted(missing)}')
    for f in c['files']:
        p=ROOT/f['path']
        if not p.exists(): fail(f'missing file {f["path"]}')
        h=hashlib.sha256(p.read_bytes()).hexdigest()
        if h!=f['sha256']: fail(f'hash mismatch {f["path"]}')
    # actual tiny raster must have visible content and not become an all-filled block
    svg=(ROOT/f'docs/design/ui-ux/assets/identity/r11c/candidates/{c["id"]}/base/mark.svg').read_bytes()
    png=cairosvg.svg2png(bytestring=svg,output_width=16,output_height=16)
    im=Image.open(io.BytesIO(png)).convert('RGBA')
    alpha=list(im.getchannel('A').get_flattened_data())
    cov=sum(v>=128 for v in alpha)/len(alpha)
    if not (0.08 <= cov <= 0.50): fail(f'{c["id"]} 16px coverage out of range: {cov:.3f}')
print(f'PASS: {len(d["candidates"])} identity candidates; required platform assets/hashes/tiny raster checks valid')
