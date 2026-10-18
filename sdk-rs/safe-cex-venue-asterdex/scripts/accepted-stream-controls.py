#!/usr/bin/env python3
"""Compile grouped production corruptions; restore exact bytes, then replay GREEN."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
result = workspace / 'target/asterdex-stream-contract-results.json'
logs = workspace / 'target/asterdex-stream-controls'
logs.mkdir(parents=True, exist_ok=True)
runner = ['node', str(crate / 'scripts/accepted-stream-contracts.mjs')]
changes = {
 'src/ws_helpers.rs': [
  ('.any(|suffix| stream.ends_with(suffix))', '.any(|suffix| { let _ = stream.ends_with(suffix); false })', 1),
  ('data: data.clone(),', 'data: serde_json::Value::Null,', 1),
  ('high: parse("h")?', 'high: parse("o")?', 1),
  ('let price = parse_float(&trade.p);', 'let price = parse_float(&trade.q);', 1),
  ('percentage: parse_truthy("P")?', 'percentage: parse_truthy("c")?', 1),
  ('match data.get("r")', 'match data.get("p")', 1),
  ('key: handler,', 'key: { let _ = handler; "wrong-handler" },', 1),
  ('reset_retry_count(0);', 'reset_retry_count(1);', 1),
  ('clear_refresh();\n    clear_watchdog();', 'clear_watchdog();\n    clear_refresh();', 1),
  ('mark_ping_sent();', 'let _ = &mark_ping_sent;', 2),
  ('subscription_id + 1', 'subscription_id + 2', 2),
 ],
 'src/private_stream.rs': [
  ('js_string(value).map_err(coercion)', 'js_string(value).map_err(coercion).map(|_| String::new())', 1),
  ('JsJsonValue::from(value.clone())', 'JsJsonValue::from({ let _ = value; Value::Null })', 1),
 ],
 'src/public_stream.rs': [('js_string(v).map_err(coerce)', 'js_string(v).map_err(coerce).map(|_| String::new())', 1)],
}
originals = {crate / file: (crate / file).read_bytes() for file in changes}
receipt = {}
try:
 for name, replacements in changes.items():
  file = crate / name
  source = originals[file].decode()
  for before, after, count in replacements:
   assert source.count(before) == count, f'control anchor changed: {name}: {before}'
   source = source.replace(before, after)
  file.write_text(source)
 if result.exists(): result.unlink()
 with (logs / 'native-disconnect-red.log').open('w') as out:
  red = subprocess.run(runner, cwd=workspace, env=os.environ, stdout=out, stderr=subprocess.STDOUT)
 assert red.returncode != 0, 'production corruptions unexpectedly passed'
 assert result.exists(), 'control did not reach runtime receipts; inspect build log'
 rows = json.loads(result.read_text())
 assert len(rows) == 19 and all(row['failures'] for row in rows), rows
 assert all(not row.get('classificationOnly') for row in rows), 'runtime witnesses required'
 receipt['namedFailures'] = {row['name']: row['failures'] for row in rows}
 print('Production corruptions: all 19 compiled native owners fail by name.', flush=True)
finally:
 for file, original in originals.items():
  file.write_bytes(original)
  assert file.read_bytes() == original, f'exact restoration failed: {file}'
 receipt['restoredSha256'] = {str(file.relative_to(crate)): hashlib.sha256(original).hexdigest() for file, original in originals.items()}
 (logs / 'control-results.json').write_text(json.dumps(receipt, indent=2) + '\n')
with (logs / 'exact-restoration-green.log').open('w') as out:
 green = subprocess.run(runner, cwd=workspace, env=os.environ, stdout=out, stderr=subprocess.STDOUT)
assert green.returncode == 0, 'restored production source failed'
print('Exact production restoration: all 19 native owners PASS.', flush=True)
