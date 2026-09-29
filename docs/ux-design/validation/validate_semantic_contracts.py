#!/usr/bin/env python3
import json
from pathlib import Path

UIUX = Path(__file__).resolve().parents[1]
components = json.loads((UIUX/'contracts/om-ui-components-v0.json').read_text())
a11y = json.loads((UIUX/'contracts/om-accessibility-mapping-v0.json').read_text())
web = json.loads((UIUX/'contracts/om-ui-web-conformance-v0.json').read_text())
fail=[]
required={'chat-row','message','composer','omnibar','contextual-surface','top-level-tabs','invite-empty-state'}
found=set(components.get('components',{}))
if required-found:
    fail.append(f'missing components: {sorted(required-found)}')

# Semantic event names must describe intent/state, not input devices/gestures.
banned=('click','tap','swipe','mousedown','mouseup','pointer','touch','keypress','keydown')
for cname,c in components.get('components',{}).items():
    for ev in c.get('events',[]):
        if any(x in ev.lower() for x in banned):
            fail.append(f'{cname}: input-specific semantic event {ev!r}')

omni=components['components'].get('omnibar',{})
for inv in ('bare_entity_semantics_stable','explicit_verbs_outrank_learning','prediction_never_executes'):
    if inv not in omni.get('invariants',[]):
        fail.append(f'omnibar missing invariant {inv}')

msg=components['components'].get('message',{})
if not msg.get('a11y',{}).get('gesture_only_forbidden'):
    fail.append('message must forbid gesture-only access')

for platform in ('web','ios','android'):
    if platform not in a11y.get('platforms',{}):
        fail.append(f'accessibility mapping missing {platform}')

wr=web.get('requirements',{})
omniw=wr.get('omnibar',{})
if omniw.get('input_role')!='combobox' or omniw.get('popup_role')!='listbox' or omniw.get('candidate_role')!='option':
    fail.append('web Omnibar conformance roles incomplete')
if not omniw.get('dom_focus_remains_on_input') or not omniw.get('uses_aria_activedescendant'):
    fail.append('web Omnibar must retain input focus and expose active descendant')
if omniw.get('candidate_has_nested_focusable_controls') is not False:
    fail.append('web Omnibar options must not contain nested focusable controls')
if wr.get('target_floor_css_px',0) < 44:
    fail.append('web target floor below OM 44 CSS-pixel minimum')
if not wr.get('gesture_only_actions_forbidden'):
    fail.append('web contract must forbid gesture-only actions')

print('OM semantic contract checks')
print(f'components: {len(found)}')
print(f'platform mappings: {len(a11y.get("platforms",{}))}')
print(f'web conformance requirements: {len(web.get("requirements",{}))}')
if fail:
    print('FAIL')
    for x in fail:
        print('-',x)
    raise SystemExit(1)
print('Semantic contract checks: PASS')
