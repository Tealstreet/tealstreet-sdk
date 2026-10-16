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
profiles = [
    ("okx-ticker-unit", "SOURCE-OKX-TICKER-UNIT"),
    ("okx-mark-unit", "SOURCE-OKX-MARK-UNIT"),
    ("okx-index-unit", "SOURCE-OKX-INDEX-UNIT"),
    ("okx-funding-unit", "SOURCE-OKX-FUNDING-UNIT"),
    ("okx-pretty-unit", "SOURCE-OKX-PRETTY-UNIT"),
    ("okx-finite-callable", "SOURCE-OKX-FINITE-CALLABLE"),
    ("okx-map-series-prefix", "SOURCE-OKX-MAPSERIES-PREFIX"),
    ("kraken-fill-unit", "SOURCE-KRAKEN-FILL-UNIT"),
    ("kraken-account-log-unit", "SOURCE-KRAKEN-LOG-UNIT"),
    ("kraken-error-unit", "SOURCE-KRAKEN-ERROR-UNIT"),
    ("kraken-leverage-key", "SOURCE-KRAKEN-LEVERAGE-KEY"),
    ("kraken-candle-prefix", "SOURCE-KRAKEN-CANDLE-PREFIX"),
    ("kraken-spot-position-unit", "SOURCE-KRAKEN-SPOT-UNIT"),
    ("kraken-account-page-unit", "SOURCE-KRAKEN-PAGE-UNIT"),
    ("weex-market-alias", "SOURCE-WEEX-MARKET-ALIAS"),
    ("weex-ticker-alias-unit", "SOURCE-WEEX-TICKER-ALIAS"),
    ("weex-ticker-unit", "SOURCE-WEEX-TICKER-UNIT"),
    ("weex-fake-unit", "SOURCE-WEEX-FAKE-UNIT"),
    ("weex-integer-unit", "SOURCE-WEEX-INTEGER-UNIT"),
    ("weex-response-unit", "SOURCE-WEEX-RESPONSE-UNIT"),
]
assert len(profiles) == 20
receipt = {"input_sha": sha, "scope": "actual-source in-memory controls, no Rust/native production mutation or SDK assembly proof", "stop": "weex-response-unit", "controls": []}

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
