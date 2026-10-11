#!/usr/bin/env python3
import json
from pathlib import Path

UIUX = Path(__file__).resolve().parents[1]
CLEAR = UIUX / "tokens/om-theme-clear-v1.json"
PRESETS = UIUX / "tokens/om-theme-presets-v1.json"

def rgb(h):
    h=h.lstrip('#')
    return tuple(int(h[i:i+2],16)/255 for i in (0,2,4))

def lin(c):
    return c/12.92 if c <= 0.04045 else ((c+0.055)/1.055)**2.4

def lum(h):
    r,g,b=rgb(h)
    return 0.2126*lin(r)+0.7152*lin(g)+0.0722*lin(b)

def contrast(a,b):
    hi,lo=sorted((lum(a),lum(b)), reverse=True)
    return (hi+0.05)/(lo+0.05)

def check_ratio(label, fg, bg, minimum, failures):
    ratio=contrast(fg,bg)
    ok=ratio >= minimum
    print(f"{label:52} {ratio:5.2f}:1  {'PASS' if ok else 'FAIL'} >= {minimum}:1")
    if not ok:
        failures.append(f"{label}: {ratio:.2f}:1 < {minimum}:1")

failures=[]
clear=json.loads(CLEAR.read_text())
required={
  "surface.base","surface.raised","text.primary","text.secondary","text.tertiary",
  "border.strong","accent.primary","accent.onPrimary","state.focus","state.selected",
  "status.success","status.warning","status.danger","message.incoming.surface",
  "message.outgoing.surface","message.failed.surface"
}
print("OM Calm semantic token checks")
for mode in ("light","dark"):
    t=clear[mode]
    missing=sorted(required-set(t))
    if missing: failures.append(f"{mode}: missing tokens {missing}")
    for fg,bg,minimum in [
      ("text.primary","surface.base",4.5),("text.secondary","surface.base",4.5),
      ("text.tertiary","surface.base",4.5),("accent.primary","surface.base",3.0),
      ("state.focus","surface.base",3.0),("status.success","surface.base",3.0),
      ("status.warning","surface.base",3.0),("status.danger","surface.base",3.0),
      ("accent.onPrimary","accent.primary",4.5)]:
        if fg in t and bg in t:
            check_ratio(f"{mode} {fg} on {bg}",t[fg],t[bg],minimum,failures)

print("\nPreset-pair core checks")
presets=json.loads(PRESETS.read_text())["presets"]
for p in presets:
    for mode in ("light","dark"):
        t=p[mode]
        check_ratio(f"{p['name']} {mode} text/base",t["text"],t["base"],4.5,failures)
        check_ratio(f"{p['name']} {mode} secondary/base",t["secondary"],t["base"],4.5,failures)
        check_ratio(f"{p['name']} {mode} accent/base",t["accent"],t["base"],3.0,failures)
        check_ratio(f"{p['name']} {mode} onAccent/accent",t["onAccent"],t["accent"],4.5,failures)
        check_ratio(f"{p['name']} {mode} focus/base",t["focus"],t["base"],3.0,failures)

if failures:
    print("\nFAILURES")
    for x in failures: print("-",x)
    raise SystemExit(1)
print("\nStatic token checks: PASS")
