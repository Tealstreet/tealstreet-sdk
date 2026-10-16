"""Check audit metadata against the independent frozen git tree, not itself."""
from pathlib import Path
from collections import Counter
import hashlib
import json
import subprocess
import sys
import tomllib

directory = Path(__file__).resolve().parent
root = directory.parents[3]
selection = json.loads((directory / "selection.json").read_text())
classification = json.loads((directory / "classification.json").read_text())
base = "d74a1c01f77439f9df61fa330063be194aab7576"
assert selection["input_sha"] == classification["frozen_input_sha"] == base
if "--control-selection" in sys.argv:
    selection["venues"]["okx"]["sample"][0]["selection_sha256"] = "0" * 64
expected_divergent = {"okx": 26, "kraken": 31, "weex": 35}
expected_eligible = {"okx": 24, "kraken": 30, "weex": 35}
all_labels = Counter()
for venue in ["okx", "kraken", "weex"]:
    prefix = f"sdk-rs/parity/manifest/exchanges/{venue}"
    paths = subprocess.check_output(["git", "ls-tree", "-r", "--name-only", base, "--", prefix], cwd=root, text=True).splitlines()
    counts = Counter()
    eligible = []
    for path in paths:
        parsed = tomllib.loads(subprocess.check_output(["git", "show", f"{base}:{path}"], cwd=root, text=True))
        for row in parsed["symbol"]:
            counts[row["status"]] += 1
            if row["status"] == "divergent" and "CHOSEN" in row.get("note", ""):
                digest = hashlib.sha256(("divergence-audit:" + parsed["file"]["ts"] + "::" + row["ts"]).encode()).hexdigest()
                eligible.append({"manifest": path, "source": parsed["file"]["ts"], **row, "selection_sha256": digest})
    assert counts["divergent"] == expected_divergent[venue], "AUDIT-CENSUS independent venue expectation"
    assert len(eligible) == expected_eligible[venue], "AUDIT-ELIGIBLE fixed population"
    sample = sorted(eligible, key=lambda row: row["selection_sha256"])[:12]
    assert len(sample) == 12, "AUDIT-SAMPLE independent twelve-row floor"
    assert selection["venues"][venue]["sample"] == sample, "AUDIT-SELECTION frozen git row/hash/order"
    assert selection["venues"][venue]["counts"] == dict(counts), "AUDIT-CENSUS captured git population"
    assert selection["venues"][venue]["eligible"] == len(eligible)
    rows = classification["venues"][venue]["rows"]
    assert len(rows) == 12 and classification["venues"][venue]["last_iteration"] == 12
    for iteration, (original, audited) in enumerate(zip(sample, rows), 1):
        assert audited["iteration"] == iteration and audited["ts"] == original["ts"] and audited["source"] == original["source"], "AUDIT-CLASSIFICATION no substitutions"
    labels = Counter(row["classification"] for row in rows)
    assert dict(labels) == classification["venues"][venue]["counts"]
    all_labels.update(labels)
    assert not subprocess.check_output(["git", "diff", base, "--", prefix], cwd=root), "AUDIT-ZERO-STATUS manifest bytes changed"
    for tree in [f"packages/safe-cex/src/exchanges/{venue}", f"sdk-rs/safe-cex-venue-{venue}"]:
        assert not subprocess.check_output(["git", "diff", base, "--", tree], cwd=root), "AUDIT-ZERO-PRODUCTION venue source/test tree changed"
assert all_labels == {"supported": 16, "finding": 19, "unresolved": 1}, "AUDIT-CLASSIFICATION independently reviewed floor"
assert classification["status_rows_moved"] == 0
for anchor in ["shared-utf8-string-boundary", "closed-typed-output-boundary", "owned-snapshot-vs-live-alias", "numeric-ingress-defect", "divergence-taxonomy", "forced-surrogate-conflict"]:
    assert f'<a id="{anchor}"></a>' in (directory.parents[1] / "MANIFEST-SPEC.md").read_text()
print(json.dumps({"input_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(), "frozen_input_sha": base, "sample": 36, "population_divergent": 92, "labels": dict(all_labels), "status_rows_moved": 0, "stop": "iteration12 per venue"}))
