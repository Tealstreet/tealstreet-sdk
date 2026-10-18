#!/usr/bin/env python3
"""Compile live reader mutations, require named runtime RED, restore exact bytes/GREEN."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / 'sdk-rs'
SOURCE = SDK / 'safe-cex-venue-bybit/src/rest_fills.rs'
EVIDENCE = SDK / 'safe-cex-venue-bybit/evidence/delegated-history-controls.json'
original = SOURCE.read_bytes()
text = original.decode()
command = ['cargo', 'test', '--locked', '--jobs', '2', '-p', 'safe-cex-venue-bybit',
           '--all-features', '--test', 'delegated_fix_audit']
env = dict(os.environ, CARGO_INCREMENTAL='0')
controls = [
    ('bypass final admission', 'admit_execution_rows(&rt, rows?)', '{ let _ = admit_execution_rows; rows }', 'BYBIT-HISTORY-ROW'),
    ('lose good neighbors', 'accepted.push(row);', 'accepted.push(row); accepted.clear();', 'BYBIT-HISTORY-ROW'),
    ('admit bad symbol', 'if !value.is_string() && (required || !value.is_undefined()) {',
     'if field != "symbol" && !value.is_string() && (required || !value.is_undefined()) {', 'BYBIT-HISTORY-ROW'),
    ('admit bad commission asset', 'if !value.is_string() && (required || !value.is_undefined()) {',
     'if field != "commissionAsset" && !value.is_string() && (required || !value.is_undefined()) {', 'BYBIT-HISTORY-ROW'),
    ('suppress visible refusal', 'Refused Bybit execution row {index}: {}',
     'Hidden execution row {index}: {}', 'BYBIT-HISTORY-ROW one visible refusal'),
    ('validate before later source error', 'executions.call_method("push", mapped.iterable_values()?)?;',
     'let mapped = admit_execution_rows(rt, mapped)?;\n        executions.call_method("push", mapped.iterable_values()?)?;',
     'BYBIT-HISTORY-SOURCE-ERROR no premature refusal'),
]
for name, before, after, marker in controls:
    assert text.count(before) == 1, (name, text.count(before))

results = []
def run(label):
    output = subprocess.run(command, cwd=SDK, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    path = Path('/tmp/bybit-history-control-' + label + '.log')
    path.write_text(output.stdout)
    return output, str(path), hashlib.sha256(output.stdout.encode()).hexdigest()

def green(output):
    return output.returncode == 0 and '10 passed; 0 failed; 0 ignored' in output.stdout

baseline, _, _ = run('baseline')
assert green(baseline), baseline.stdout
try:
    for i, (name, before, after, marker) in enumerate(controls):
        SOURCE.write_text(text.replace(before, after, 1))
        broken, red_log, red_hash = run(str(i) + '-red')
        detected = (broken.returncode == 101 and 'panicked at' in broken.stdout
                    and marker in broken.stdout and 'error: could not compile' not in broken.stdout)
        if i < 5:
            for family in ['primary', 'demo', 'inverse']:
                detected = detected and ('delegated_' + family + '_history_strict_output_keeps_good_neighbors ... FAILED' in broken.stdout)
        SOURCE.write_bytes(original)
        restored, green_log, green_hash = run(str(i) + '-green')
        result = dict(control=name, named_assertion=marker, compiled_runtime_red=detected,
                      mutation_exit=broken.returncode, restored_exit=restored.returncode,
                      exact_restore=SOURCE.read_bytes() == original, green=green(restored),
                      red_log=red_log, red_log_sha256=red_hash,
                      green_log=green_log, green_log_sha256=green_hash)
        results.append(result)
        print(json.dumps(result), flush=True)
        assert detected and green(restored), broken.stdout + '\nRESTORED:\n' + restored.stdout
finally:
    SOURCE.write_bytes(original)
    EVIDENCE.parent.mkdir(exist_ok=True)
    EVIDENCE.write_text(json.dumps(dict(
        command=command, cwd='sdk-rs', CARGO_INCREMENTAL='0',
        measured_head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        source_sha256=hashlib.sha256(original).hexdigest(),
        source_exactly_restored=SOURCE.read_bytes() == original,
        baseline_exit=baseline.returncode, controls=results,
        planned_floor=6, executed=len(results)), indent=2) + '\n')
assert len(results) == 6
print('Six independent compiled RED -> exact restore -> GREEN controls.', flush=True)
