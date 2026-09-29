#!/usr/bin/env python3
from pathlib import Path
import shutil, sys
from playwright.sync_api import sync_playwright
root=Path(__file__).resolve().parents[1]
html=(root/'prototypes/system-completeness-v0/index.html').read_text()
checks=[]
def ck(name,cond,detail=''):
    checks.append((name,bool(cond),detail))
with sync_playwright() as p:
    exe=shutil.which('chromium') or shutil.which('google-chrome') or shutil.which('google-chrome-stable')
    if not exe:
        print('NOT RUN: Chromium unavailable'); sys.exit(2)
    b=p.chromium.launch(headless=True,executable_path=exe,args=['--no-sandbox','--disable-gpu'])
    page=b.new_page(viewport={'width':390,'height':844})
    page.set_content(html,wait_until='load')
    ck('settings_default_active', page.locator('#settings').evaluate("e=>e.classList.contains('active')"))
    inp=page.locator('input[aria-label="Search settings"]')
    box=inp.bounding_box(); ck('settings_input_target_44', box and box['height']>=44, str(box))
    page.locator('[data-tab="language"]').click(); ck('language_tab_switch',page.locator('#language').evaluate("e=>e.classList.contains('active')"))
    ck('bdi_present',page.locator('#language bdi').count()==1)
    page.set_viewport_size({'width':320,'height':800}); page.wait_for_timeout(50)
    overflow=page.evaluate('document.documentElement.scrollWidth > document.documentElement.clientWidth')
    ck('reflow_320_no_page_horizontal_scroll',not overflow,f"scroll={page.evaluate('document.documentElement.scrollWidth')} client={page.evaluate('document.documentElement.clientWidth')}")
    page.locator('[data-tab="copy"]').focus(); outline=page.locator('[data-tab="copy"]').evaluate("e=>getComputedStyle(e).outlineStyle")
    # browser default focus is acceptable for this fixture; just require focus actually lands there.
    ck('keyboard_focusable_tabs',page.evaluate("document.activeElement.dataset.tab")=='copy',outline)
    ck('no_external_resources','http://' not in html and 'https://' not in html)
    b.close()
failed=[x for x in checks if not x[1]]
for name,ok,detail in checks: print(('PASS' if ok else 'FAIL')+': '+name+((' — '+detail) if detail else ''))
if failed: sys.exit(1)
print(f'PASS: R13C fixture {len(checks)}/{len(checks)} checks')
