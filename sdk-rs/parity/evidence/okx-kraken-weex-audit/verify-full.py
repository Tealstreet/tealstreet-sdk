"""Independent full-block inventory/status and published-log integrity gate."""
from pathlib import Path
from collections import Counter
import hashlib
import json
import subprocess
import sys
import tomllib

directory=Path(__file__).resolve().parent
root=directory.parents[3]
base="d74a1c01f77439f9df61fa330063be194aab7576"
integration="333fd419fdbf70f081039610f22da9ee653bc96f"
selection=json.loads((directory/"full-selection.json").read_text())
verdicts=json.loads((directory/"full-classification.json").read_text())
expectations={"okx":(26,13,13,0),"kraken":(31,15,16,0),"weex":(35,19,12,4)}
assert selection["input_sha"]==verdicts["frozen_input_sha"]==base
annotated=0
for venue,(population,supported,finding,unresolved) in expectations.items():
    prefix=f"sdk-rs/parity/manifest/exchanges/{venue}"
    paths=subprocess.check_output(["git","ls-tree","-r","--name-only",base,"--",prefix],cwd=root,text=True).splitlines()
    frozen=[]
    for file in paths:
        before=tomllib.loads(subprocess.check_output(["git","show",f"{base}:{file}"],cwd=root,text=True))
        parent=tomllib.loads(subprocess.check_output(["git","show",f"{integration}:{file}"],cwd=root,text=True))
        current=tomllib.loads((root/file).read_text())
        if "--control-status" in sys.argv and venue=="okx":
            current["symbol"][0]["status"]="pending"
        assert before["file"]==parent["file"]==current["file"],"FULL-AUDIT file targets changed"
        assert len(parent["symbol"])==len(current["symbol"]),"FULL-AUDIT rows added/lost"
        for a,b in zip(parent["symbol"],current["symbol"]):
            assert {k:v for k,v in a.items() if k!="note"}=={k:v for k,v in b.items() if k!="note"},"FULL-AUDIT-ZERO-STATUS targets/status/identity changed"
            if a.get("note")!=b.get("note"):
                assert b["note"].startswith("FINDING full-block audit (owner: rust-venue-polymarket):"),"FULL-AUDIT finding annotation"
                assert b["note"].endswith(a.get("note","")),"FULL-AUDIT historical annotation lost"
                annotated+=1
        for row in before["symbol"]:
            if row["status"]=="divergent":
                digest=hashlib.sha256(("divergence-audit:"+before["file"]["ts"]+"::"+row["ts"]).encode()).hexdigest()
                frozen.append({"manifest":file,"source":before["file"]["ts"],**row,"selection_sha256":digest})
    frozen.sort(key=lambda r:r["selection_sha256"])
    actual=selection["venues"][venue]["rows"]
    assert len(frozen)==len(actual)==population,"FULL-AUDIT independent population floor"
    assert frozen==[{k:v for k,v in row.items() if k!="in_original_sample"} for row in actual],"FULL-AUDIT immutable full selection"
    rows=verdicts["venues"][venue]["rows"]
    assert len(rows)==population
    for i,(a,b) in enumerate(zip(actual,rows),1):
        assert (a["source"],a["ts"],a["in_original_sample"],i)==(b["source"],b["ts"],b["in_original_sample"],b["iteration"]),"FULL-AUDIT no verdict substitution"
    counts=Counter(r["classification"] for r in rows)
    assert (counts["supported"],counts["finding"],counts["unresolved"])==(supported,finding,unresolved)
    for tree in [f"packages/safe-cex/src/exchanges/{venue}",f"sdk-rs/safe-cex-venue-{venue}"]:
        assert not subprocess.check_output(["git","diff",integration,"--",tree],cwd=root),"FULL-AUDIT production/test change unique to extension"
assert annotated==37,"FULL-AUDIT independent annotation expectation"
classes=Counter(r["cause_class"] for group in verdicts["venues"].values() for r in group["rows"] if r["classification"]=="finding")
assert classes=={"explanation":28,"existing":4,"scheduling":6,"documentation":1,"behavior":2}
assert verdicts["status_rows_moved"]==0
for relative,expected in [("measured-3f2d53594f",20),("full-measured-bc1af045fb",21)]:
    receipt=json.loads((directory/relative/"controls.json").read_text())
    assert receipt["accepted"] and receipt["tracked_inputs_unchanged"] and len(receipt["controls"])==expected
    for item in [receipt["all_source_observations"]]+[c[stage] for c in receipt["controls"] for stage in ["baseline","mutant","restored"]]:
        assert hashlib.sha256((directory/relative/item["log"]).read_bytes()).hexdigest()==item["log_sha256"],"FULL-AUDIT published log hash"
    for c in receipt["controls"]:
        assert c["caught"] and [c[s]["exit"] for s in ["baseline","mutant","restored"]]==[0,1,0]
fixed=json.loads((directory/"same-class-control.json").read_text())
assert fixed["prior_control_rejected_for_same_class_precedence"]
receipt=json.loads((directory/fixed["receipt"]).read_text())
assert receipt["input_sha"]==fixed["input_sha"] and receipt["accepted"]
assert len(receipt["controls"])==1
c=receipt["controls"][0]
assert c["profile"]=="weex-maker-error-order"
assert c["marker"]=="FULL-WEEX-MAKER-ERROR-ORDER later commission wins"
assert [c[s]["exit"] for s in ["baseline","mutant","restored"]]==[0,1,0]
assert c["caught"]
for item in [receipt["all_source_observations"]]+[c[s] for s in ["baseline","mutant","restored"]]:
    assert hashlib.sha256((directory/Path(fixed["receipt"]).parent/item["log"]).read_bytes()).hexdigest()==item["log_sha256"]
print(json.dumps({"input_sha":subprocess.check_output(["git","rev-parse","HEAD"],cwd=root,text=True).strip(),"frozen_sha":base,"integration_parent":integration,"population":92,"supported":47,"finding":41,"unresolved":4,"new_finding_annotations":37,"status_rows_moved":0,"source_profile_controls":41,"finding_classes":dict(classes)}))
