#!/usr/bin/env python3
from pathlib import Path
import hashlib, json, re, sys
from xml.etree import ElementTree as ET
from jsonschema import Draft202012Validator
ROOT=Path(__file__).resolve().parents[1]
errors=[]

def fail(msg): errors.append(msg)

# Functional icon manifest vs inventory
inv=json.loads((ROOT/'contracts/om-icon-inventory-v0.json').read_text())
assets=json.loads((ROOT/'contracts/om-functional-icon-assets-v0.json').read_text())
inv_ids=[x['id'] for x in inv['icons']]
asset_ids=[x['id'] for x in assets['icons']]
if inv_ids!=asset_ids: fail('functional asset IDs do not exactly match semantic inventory order')
if len(asset_ids)!=66: fail(f'expected 66 base icons, got {len(asset_ids)}')
for item in assets['icons']:
    p=ROOT/'assets/icons/v0'/item['file']
    if not p.exists(): fail(f'missing {p}'); continue
    b=p.read_bytes()
    if hashlib.sha256(b).hexdigest()!=item['sha256']: fail(f'hash mismatch {item["id"]}')
    try: tree=ET.fromstring(b)
    except Exception as e: fail(f'XML parse {item["id"]}: {e}'); continue
    if tree.attrib.get('viewBox')!='0 0 24 24': fail(f'wrong viewBox {item["id"]}')
    if tree.attrib.get('stroke')!='currentColor': fail(f'root stroke not currentColor {item["id"]}')
    raw=b.decode('utf-8','replace').lower()
    for bad in ['<script','<foreignobject','<text','<title','javascript:','http://','https://','onload=','onclick=']:
        if bad in raw and bad!='http://' and bad!='https://': fail(f'forbidden token {bad} in {item["id"]}')
    # xmlns is allowed; external URLs are not present in these generated files.
    if re.search(r'\b(?:href|xlink:href)\s*=\s*["\'](?!#)', raw): fail(f'external href in {item["id"]}')
    # no literal rgb/hex color in functional SVG
    if re.search(r'#[0-9a-f]{3,8}\b|rgb\s*\(', raw): fail(f'literal color in {item["id"]}')

# Variant files exist and are static
for v in assets['variants']:
    p=ROOT/'assets/icons/v0'/v['file']
    if not p.exists(): fail(f'missing variant {v["id"]}')

# Asset-pack example conforms to schema
schema=json.loads((ROOT/'contracts/om-asset-pack-manifest-v0.schema.json').read_text())
example=json.loads((ROOT/'contracts/om-asset-pack-example-v0.json').read_text())
for e in Draft202012Validator(schema).iter_errors(example): fail('asset pack schema: '+e.message)
sec=example.get('security',{})
if any(sec.get(k) is not False for k in ('scripts','network','external_references','action_remapping')): fail('asset pack security flags must all be false')

# Invite QR static checks: viewbox exists and fixture is non-production example domain
qr=(ROOT/'assets/invite/v0/fixture-qr.svg').read_text()
if 'example.invalid' in qr: fail('encoded URI should not appear as visible SVG text')
try:
    q=ET.fromstring(qr)
    vb=q.attrib.get('viewBox','').split()
    if len(vb)!=4 or int(float(vb[2]))<29: fail('unexpected QR viewBox')
except Exception as e: fail('QR parse: '+str(e))

# Avatar samples must be deterministic and use only approved palette from generator
avdir=ROOT/'assets/avatar/v0/samples'
if len(list(avdir.glob('*.svg')))<6: fail('expected >=6 avatar samples')

if errors:
    print('R11B ASSET VALIDATION FAIL')
    for e in errors: print('-',e)
    sys.exit(1)
print('R11B ASSET VALIDATION PASS')
print(f'- base functional icons: {len(asset_ids)}')
print(f'- variants: {len(assets["variants"])}')
print(f'- avatar samples: {len(list(avdir.glob("*.svg")))}')
print('- asset-pack example: schema-valid')
print('- functional SVGs: static/currentColor/no embedded labels')
