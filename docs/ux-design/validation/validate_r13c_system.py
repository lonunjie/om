#!/usr/bin/env python3
from pathlib import Path
import json, sys
root=Path(__file__).resolve().parents[1]
p=root/'contracts/om-system-completeness-contract-v0.json'
d=json.loads(p.read_text())
errors=[]
required_groups={"appearance","interaction","notifications","privacy-security","devices","data-storage","accessibility-language","advanced"}
if set(d.get('settings_groups',[]))!=required_groups: errors.append('settings groups mismatch')
if set(d.get('layout_capabilities',{}))!={"compact","split","expanded","multi-pane"}: errors.append('layout capability set mismatch')
if len(d.get('localization_invariants',[]))<7: errors.append('too few localization invariants')
if len(d.get('sensory_events',[]))<6: errors.append('too few sensory events')
b=d.get('performance_budgets_ms',{})
for k in ['text_echo_p95_app_work','common_local_feedback_p95','cached_chat_first_content_p95','omnibar_hot_p95','web_inp_health_p75_ceiling']:
    if not isinstance(b.get(k),(int,float)) or b[k]<=0: errors.append('invalid budget '+k)
if b.get('omnibar_hot_p95',999)>b.get('web_inp_health_p75_ceiling',0): errors.append('hot omnibar slower than web health ceiling')
if d.get('token_layers')!=['primitive','semantic','component-alias','state-override']: errors.append('token layers mismatch')
for scope in ['required','optional','experimental','v2plus']:
    if not d.get('v1_scope',{}).get(scope): errors.append('empty scope '+scope)
if d.get('identity_human_gate')!='paused': errors.append('identity human gate unexpectedly changed')
if errors:
    print('FAIL: '+ '; '.join(errors)); sys.exit(1)
print('PASS: R13C system contract — %d settings groups, %d layouts, %d localization invariants, %d sensory events, %d performance budgets' % (len(d['settings_groups']),len(d['layout_capabilities']),len(d['localization_invariants']),len(d['sensory_events']),len(d['performance_budgets_ms'])))
