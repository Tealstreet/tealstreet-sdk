from pathlib import Path
import os
import subprocess
import json

root = Path(__file__).resolve().parents[3]
venue = root / 'sdk-rs/safe-cex-venue-bitunix'
out = root / 'sdk-rs/target/bitunix-order-operation-evidence'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
 ('regular-remaining', 'order_models.rs', 'fetch_regular_orders', 'subtract(qty.as_ref(),traded.as_ref())?', 'subtract(traded.as_ref(),qty.as_ref())?', 'order_models', 'actual_typescript_regular_reads', 'MODEL native result'),
 ('tpsl-clock', 'order_models.rs', 'fetch_tpsl_orders', '("lastUpdate", n(clock))', '("lastUpdate", n(clock + 1.0))', 'order_models', 'actual_typescript_tpsl_reads', 'MODEL native result'),
 ('create-lots', 'order_models.rs', 'format_create_order', '(amount / max).ceil()', '(amount / max).floor()', 'order_models', 'actual_typescript_create_order', 'MODEL native effect'),
 ('tpsl-position-flag', 'order_models.rs', 'format_tpsl_order', '"isPositionOrder", JsJsonValue::Bool(true)', '"isPositionOrder", JsJsonValue::Bool(false)', 'order_models', 'actual_typescript_tpsl_format', 'MODEL native result'),
 ('standalone-endpoint', 'order_models.rs', 'update_standalone_tpsl_order', '"MODIFY_TP_SL_ORDER"', '"TP_SL_ORDER"', 'order_models', 'actual_typescript_standalone_amend', 'MODEL native effect'),
 ('amend-endpoint', 'order_models.rs', 'update_order', 'endpoint("MODIFY_ORDER")', 'endpoint("ORDER")', 'order_models', 'actual_typescript_regular_amend', 'MODEL native effect'),
 ('placement-retry', 'order_placement.rs', 'place_order', 'host.includes(&message, "Missing positionId")?', 'false', 'order_placement', 'actual_typescript_place_order', 'PLACE native result'),
 ('batch-size', 'order_placement.rs', 'place_order_batch', 'rows.chunks(5)', 'rows.chunks(4)', 'order_placement', 'actual_typescript_batch_order', 'BATCH native effect'),
]
results = []
for name, file, function, before, after, target, test, assertion in controls:
 source = venue / 'src' / file
 original = source.read_text()
 markers = ['pub fn ' + function, 'pub async fn ' + function]
 start = next(original.index(m) for m in markers if m in original)
 end = original.find('\npub ', start + 1)
 if end == -1:
  end = len(original)
 block = original[start:end]
 assert block.count(before) == 1, (name, before)
 command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bitunix', '--jobs', '2', '--test', target, test, '--', '--exact', '--nocapture']
 try:
  source.write_text(original[:start] + block.replace(before, after) + original[end:])
  failed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
  (out / (name + '-mutated.log')).write_text(failed.stdout)
  assert failed.returncode == 101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout, (name, failed.stdout[-3000:])
 finally:
  source.write_text(original)
 passed = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
 (out / (name + '-restored.log')).write_text(passed.stdout)
 assert passed.returncode == 0, (name, passed.stdout[-3000:])
 results.append({'family': name, 'compiled_exit': failed.returncode, 'assertion': assertion, 'restored_exit': passed.returncode})
 (out / 'controls.json').write_text(json.dumps(results, indent=2) + '\n')
 print(name + ': compiled native failure 101; restored pass 0', flush=True)
assert len(results) == 8
