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
output = directory / ("full-measured-" + sha[:10])
output.mkdir(exist_ok=True)
profiles = [('okx-currency-key', 'FULL-OKX-CURRENCY-KEY'), ('okx-open-interest-key', 'FULL-OKX-OI-KEY'), ('okx-subscribe-unit', 'FULL-OKX-SUBSCRIBE-UNIT'), ('okx-message-unit', 'FULL-OKX-MESSAGE-UNIT'), ('kraken-validation-prefix', 'FULL-KRAKEN-VALIDATION-PREFIX'), ('kraken-tickers-prefix', 'FULL-KRAKEN-TICKERS-PREFIX'), ('kraken-orders-prefix', 'FULL-KRAKEN-ORDERS-PREFIX'), ('kraken-spot-balance-prefix', 'FULL-KRAKEN-SPOT-BALANCE-PREFIX'), ('kraken-spot-candles-prefix', 'FULL-KRAKEN-SPOT-CANDLES-PREFIX'), ('kraken-fill-batch-unit', 'FULL-KRAKEN-FILL-BATCH-UNIT second id'), ('kraken-log-row-unit', 'FULL-KRAKEN-LOG-ROW-UNIT'), ('kraken-position-leg-comparison', 'FULL-KRAKEN-POSITION-LEG distinct retained units'), ('kraken-token-unit', 'FULL-KRAKEN-TOKEN-UNIT'), ('kraken-spot-balance-unit', 'FULL-KRAKEN-SPOT-BALANCE-UNIT'), ('weex-balance-alias-unit', 'FULL-WEEX-BALANCE-ALIAS'), ('weex-execution-unit', 'FULL-WEEX-EXECUTION-UNIT'), ('weex-historical-unit', 'FULL-WEEX-HISTORICAL-UNIT'), ('weex-income-unit', 'FULL-WEEX-INCOME-UNIT'), ('weex-public-trade-unit', 'FULL-WEEX-PUBLIC-TRADE-UNIT'), ('weex-maker-error-order', 'FULL-WEEX-MAKER-ERROR-ORDER later commission wins'), ('weex-order-result-unit', 'FULL-WEEX-ORDER-RESULT-UNIT')]
assert len(profiles) == 21
if sys.argv[1:]:
    assert sys.argv[1:] == ["weex-maker-error-order"], "only exact proof-repair selector supported"
    profiles = [p for p in profiles if p[0] == "weex-maker-error-order"]
receipt = {"input_sha": sha, "scope": "actual-source in-memory controls, no Rust/native production mutation or SDK assembly proof", "stop": profiles[-1][0], "controls": []}

def run(name, args):
    command = ["node", str(directory.relative_to(root) / "full-source-observations.mjs"), *args]
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
