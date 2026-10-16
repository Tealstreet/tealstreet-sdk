"""Sequential source-only audit controls; published receipts retain actual input SHA."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

directory = Path(__file__).resolve().parent
root = directory.parents[3]
sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
assert not subprocess.check_output(["git", "diff", "HEAD", "--name-only"], cwd=root), "commit inputs before measuring"
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH"), "compiler environment required"
output = directory / ("measured-" + sha[:10])
output.mkdir(exist_ok=True)
profiles = [('mexc-fetchMarkets', 'MEXC-PREFIX-fetchMarkets immediate callback'), ('mexc-fetchTickers', 'MEXC-PREFIX-fetchTickers immediate callback'), ('mexc-fetchAndSetOrders', 'MEXC-PREFIX-fetchAndSetOrders immediate callback'), ('mexc-fetchAndSetRegularOrders', 'MEXC-PREFIX-fetchAndSetRegularOrders immediate callback'), ('mexc-normalizeMarket', 'MEXC-UNIT-normalizeMarket returned symbol'), ('mexc-normalizeTicker', 'MEXC-UNIT-normalizeTicker returned symbol'), ('mexc-normalizeOrder', 'MEXC-UNIT-normalizeOrder returned symbol'), ('mexc-normalizePlanOrder', 'MEXC-UNIT-normalizePlanOrder returned symbol'), ('mexc-normalizePosition', 'MEXC-UNIT-normalizePosition returned symbol'), ('mexc-normalizeStopOrder', 'MEXC-UNIT-normalizeStopOrder returned symbol'), ('mexc-normalizeExecution', 'MEXC-UNIT-normalizeExecution returned symbol'), ('mexc-normalize-request-consumer', 'MEXC-REQUEST actual returned config data unit'), ('mexc-factory-response', 'MEXC-RESPONSE actual returned field unit'), ('mexc-private-query-erasure', 'MEXC-QUERY private returned units'), ('mexc-query-encoding', 'MEXC-QS two encoded fields')]
assert len(profiles) == 15
receipt = {"input_sha": sha, "scope": "actual-source in-memory controls, no Rust/native production mutation or SDK assembly proof", "stop": "mexc-query-encoding", "controls": []}

def run(name, args):
    command = ["node", str(directory.relative_to(root) / "source-observations.mjs"), *args]
    result = subprocess.run(command, cwd=root, capture_output=True, text=True)
    diagnostic = result.stdout + result.stderr
    sanitized = diagnostic.replace(str(root), "<worktree>").replace(str(Path.home()), "<home>")
    compiler = os.environ["SAFE_CEX_TYPESCRIPT_PATH"]
    sanitized = sanitized.replace(compiler, "<typescript>")
    log = output / (name + ".log")
    log.write_text(sanitized)
    return {"command": command, "exit": result.returncode, "log": log.name, "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()}, diagnostic

whole, _ = run("all-source-observations", [])
receipt["all_source_observations"] = whole
for profile, marker in profiles:
    baseline, _ = run(profile + "-baseline", [profile])
    mutant, diagnostic = run(profile + "-mutant", [profile, "--control"])
    restored, _ = run(profile + "-restored", [profile])
    caught = mutant["exit"] == 1 and "AssertionError" in diagnostic and marker in diagnostic
    receipt["controls"].append({"profile": profile, "marker": marker, "baseline": baseline, "mutant": mutant, "restored": restored, "caught": caught})
    (output / "controls.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(profile, baseline["exit"], mutant["exit"], restored["exit"], "caught", caught, flush=True)
receipt["tracked_inputs_unchanged"] = not subprocess.check_output(["git", "diff", "HEAD", "--name-only"], cwd=root)
receipt["accepted"] = whole["exit"] == 0 and receipt["tracked_inputs_unchanged"] and all(c["caught"] and c["baseline"]["exit"] == c["restored"]["exit"] == 0 for c in receipt["controls"])
(output / "controls.json").write_text(json.dumps(receipt, indent=2) + "\n")
sys.exit(0 if receipt["accepted"] else 1)
