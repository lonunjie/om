#!/usr/bin/env python3
import json, pathlib, sys
p=pathlib.Path(__file__).resolve().parents[1]/"reconciliation"/"om-r14-reconciliation-v0.json"
d=json.loads(p.read_text())
errors=[]
if d.get("merge_performed") is not False: errors.append("R14 must be audit-only")
if len(d.get("items",[])) < 30: errors.append("reconciliation matrix unexpectedly small")
for x in d.get("items",[]):
    for k in ("id","status","severity","architecture_scope","finding","recommended_resolution"):
        if not x.get(k): errors.append(f"{x.get('id','?')}: missing {k}")
ids=[x["id"] for x in d.get("items",[])]
if len(ids)!=len(set(ids)): errors.append("duplicate reconciliation ids")
required={"shell-defaults","draft-sync","spaces","home-onboarding-tiers","recovery-default","device-suspend-vs-revoke","action-registry-core-intents"}
if not required.issubset(ids): errors.append("missing required high-risk reconciliation rows")
if errors:
    print("FAIL")
    print("\n".join(errors))
    sys.exit(1)
print(f"PASS: {len(ids)} reconciliation items; audit-only")
