#!/usr/bin/env python3
import json, sys
from pathlib import Path

path = Path(__file__).resolve().parents[1] / "contracts" / "om-icon-inventory-v0.json"
data = json.loads(path.read_text())
icons = data["icons"]
errors = []
ids = [i["id"] for i in icons]
if len(ids) != len(set(ids)):
    errors.append("duplicate icon ids")
for i in icons:
    for k in ["id","name","category","semantic_kind","familiarity_tier","label_policy","risk","rtl","default_color_role"]:
        if not i.get(k):
            errors.append(f'{i.get("id","<?>")}: missing {k}')
    if i["risk"] in {"destructive","security-critical"} and i["label_policy"] == "icon-ok":
        errors.append(f'{i["id"]}: risky action cannot be icon-ok')
    if i["familiarity_tier"] in {"N","S"} and i["label_policy"] == "icon-ok":
        errors.append(f'{i["id"]}: novel/security icon cannot default to icon-only')
    if i["rtl"] not in {"no","mirror","contextual"}:
        errors.append(f'{i["id"]}: invalid rtl metadata')

if errors:
    print("FAIL")
    for e in errors: print("-", e)
    sys.exit(1)
print(f"PASS: {len(icons)} icon semantic entries validated")
