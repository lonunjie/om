#!/usr/bin/env python3
from pathlib import Path
p=Path(__file__).resolve().parents[1]/'prototypes/live-groups-security-v0/index.html'
s=p.read_text()
required=['camera not active','Answer video','Answer audio','Reconnecting','Relay route · E2EE unchanged','no channel hierarchy required','Verified by you','Verification changed','Review before sending','Revoking ends future authorization','Identity authority','History/data','Provider/discovery','AccountID: unchanged']
missing=[x for x in required if x not in s]
forbidden=['http://','https://','<script src=','<link rel="stylesheet" href=']
external=[x for x in forbidden if x in s]
if missing or external:
    print('R13B FIXTURE: FAIL')
    if missing: print('missing:',missing)
    if external: print('external deps:',external)
    raise SystemExit(1)
print(f'R13B FIXTURE: PASS — {len(required)} required semantic markers, standalone local-only')
