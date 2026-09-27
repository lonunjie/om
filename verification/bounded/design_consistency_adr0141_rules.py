#!/usr/bin/env python3
from pathlib import Path
import json, sys
ROOT=Path(__file__).resolve().parents[2]
errors=[]
def bad(s): errors.append(s)
def j(rel): return json.loads((ROOT/rel).read_text(encoding="utf-8"))
def jl(rel): return [json.loads(x) for x in (ROOT/rel).read_text(encoding="utf-8").splitlines() if x.strip()]
num=j("spec/registries/om-numeric-fields.json")
legacy=next((x for x in num.get("extensions",[]) if x.get("name")=="om.acs.legacy_policy"),None)
if not legacy or legacy.get("critical") is not True: bad("LegacyPolicy extension is not critical")
acs=next((x for x in num.get("structs",{}).get("AccountControlState",{}).get("fields",[]) if x.get("key")==13),None)
if not acs or "proposed" in acs.get("note","").lower(): bad("AccountControlState device_credentials registry row still proposed")
cddl=(ROOT/"spec/cddl/account.cddl").read_text(encoding="utf-8")
if "device_credentials (set; proposed addition)" in cddl: bad("AccountControlState device_credentials still marked proposed")
if "non-critical key-23 extension" in cddl or "preserve/ignore it" in cddl: bad("LegacyPolicy CDDL still non-critical/preserve-ignore")
ds=j("spec/registries/om-domain-separation.json")
proposed=[x.get("label") for x in ds.get("labels",[]) if x.get("wave")=="W0" and x.get("status")=="proposed"]
if proposed: bad("W0 domain labels still proposed: "+",".join(proposed))
for lab in ("OM-RECOVERY-PROVIDER-METADATA-KEY-v1","OM-RECOVERY-RELATIONSHIP-SNAPSHOT-KEY-v1","OM-RECOVERY-RECORD-AEAD-AD-v1","OM-RECOVERY-RECORD-KEY-v1"):
 x=next((q for q in ds.get("labels",[]) if q.get("label")==lab),None)
 if not x or x.get("status")!="frozen" or "reserved" in x.get("construction","").lower(): bad("recovery label not frozen/filled: "+lab)
time=j("spec/registries/om-time-fields.json"); rules="\n".join(time.get("rules",[]))
if "normatively frozen at 300 s" not in rules or "Proposed; per-object constants remain policy work" in rules: bad("300-second default not frozen")
home=(ROOT/"spec/protocol/home-api.md").read_text(encoding="utf-8").lower()
for tok in ("url path","query string","cookie","referrer-policy: no-referrer","cross-origin redirect","cache-control: no-store","analytics","crash reports"):
 if tok not in home: bad("Home API capability hygiene missing "+tok)
rr=j("spec/protocol/recovery-record.logical.schema.json")
for f in ("record_generation","recovery_epoch","last_known_acs_head"):
 if f not in rr.get("required",[]): bad("RecoveryRecord required field missing "+f)
rdoc=(ROOT/"docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md").read_text(encoding="utf-8").lower()
for tok in ("advisory and potentially stale","high-water mark","relationshiprecoverykey","revalidate"):
 if tok not in rdoc: bad("recovery policy missing "+tok)
call=(ROOT/"spec/protocol/call-session.logical.schema.json").read_text(encoding="utf-8")
if "is open (A-035)" in call or "MLS exporter over (CallID" in call: bad("call schema stale A-035")
if "joined participant DeviceIDs" not in call or "Room MLS epoch possession alone" not in call: bad("resolved call isolation missing")
callsdoc=(ROOT/"docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md").read_text(encoding="utf-8")
if "Call keys come from an MLS exporter" in callsdoc or "Group call keys derive through the MLS exporter" in callsdoc: bad("calls architecture retains superseded MLS-exporter media-key source")
req={x.get("id"):x for x in jl("machine/requirements.jsonl")}; th={x.get("id"):x for x in jl("machine/threat_register.jsonl")}
if "not claimed until A-035" in req.get("R-233",{}).get("requirement",""): bad("R-233 stale")
if "open until A-035" in th.get("T-063",{}).get("residual_risk",""): bad("T-063 stale")
for rid in ("R-302","R-303","R-304","R-305","R-306","R-307"):
 if rid not in req: bad("missing "+rid)
for tid in ("T-107","T-108","T-109","T-110"):
 if tid not in th: bad("missing "+tid)
queue={x.get("id"):x for x in jl("machine/architecture_design_queue.jsonl")}
if "non-critical om.acs.legacy_policy" in queue.get("A-030",{}).get("resolution",""):
 bad("A-030 resolution still records superseded non-critical LegacyPolicy semantics")
v005_path = "verification/results/v005-node-adr0142-latest.json" if (ROOT/"verification/results/v005-node-adr0142-latest.json").exists() else "verification/results/v005-node-adr0141-latest.json"
v005=j(v005_path)
if not v005.get("passed_for_node_slice") or v005.get("vector_count",0) < 681 or v005.get("mismatch_count")!=0:
 bad("current V-005 Node evidence no longer preserves ADR-0141 coverage")
result={"checker":"design_consistency_adr0141_rules","passed":not errors,"errors":errors,"checks":{"legacy_critical":legacy is not None and legacy.get("critical") is True,"w0_proposed_labels":proposed,"recovery_freshness_fields":rr.get("required",[]),"call_isolation":"joined participant DeviceIDs" in call}}
(ROOT/"verification/results/design-consistency-adr0141-latest.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
if errors:
 [print("ERROR:",e,file=sys.stderr) for e in errors]; raise SystemExit(1)
print("ADR-0141 semantic consistency OK")
