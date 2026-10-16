"""Check the twelve selected controls; retain old pricing evidence unchanged."""
from pathlib import Path
from datetime import datetime, timezone
import argparse, ast, hashlib, importlib.util, json, math, re, subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
DRIVER = ROOT / 'sdk-rs/safe-cex-venue-polymarket/tests/class_cost.py'
spec = importlib.util.spec_from_file_location('driver', DRIVER)
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
parser = argparse.ArgumentParser()
parser.add_argument('--sha', required=True)
args = parser.parse_args()
sha = subprocess.check_output(['git', 'rev-parse', args.sha], cwd=ROOT, text=True).strip()
digest = lambda raw: hashlib.sha256(raw).hexdigest()
blob = lambda path: subprocess.check_output(['git', 'show', sha + ':' + path], cwd=ROOT)
reference = {path: digest(blob(path)) for path in driver.input_hashes()}
assert driver.input_hashes() == reference, 'working source differs from measured source'
assert digest(DRIVER.read_bytes()) == reference[str(DRIVER.relative_to(ROOT))]

# The expected control identities are independent of the summary under test.
expected_names = {
    'name', 'tick-membership', 'precision-default', 'token-identity',
    'record-identity', 'http-endpoint', 'market-quote', 'ticker-batch',
    'placement-side', 'source-map-erasure', 'token-length17', 'token-length65',
}
tree = ast.parse(blob(str(DRIVER.relative_to(ROOT))))
function = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'controls')
assignment = next(n for n in function.body if isinstance(n, ast.Assign)
                  and any(isinstance(t, ast.Name) and t.id == 'mutations' for t in n.targets))
mutations = ast.literal_eval(assignment.value)
assert len(mutations) == 12 and {m[0] for m in mutations} == expected_names


def read(label, target, test, code, inputs, assertion=None):
    record = json.loads((HERE / (label + '.json')).read_text())
    raw = (HERE / (label + '.log')).read_bytes()
    text = raw.decode()
    assert record['label'] == label and record['sha'] == sha, label
    assert record['command'] == driver.cargo(target, test), label
    assert record['test'] == test and record['exit'] == code, label
    assert math.isfinite(record['wall_seconds']) and record['wall_seconds'] >= 0, label
    assert record['input_sha256'] == inputs, label
    assert record['log_sha256'] == digest(raw), label
    assert 'could not compile' not in text, label
    if assertion:
        assert record['expected_assertion'] == assertion, label
        assert re.search(r'panicked at[\s\S]*?' + re.escape(assertion), text), label
        assert f'test {test} ... FAILED' in text, label
        assert 'test result: FAILED. 0 passed; 1 failed;' in text, label
    else:
        assert f'test {test} ... ok' in text, label
        assert 'test result: ok. 1 passed; 0 failed;' in text, label
    return record, text


for cls, target, test in driver.SAMPLES:
    row, log = read('baseline-' + test, target, test, 0, reference)
    assert row['cases'] == 512 and row['class'] == cls
    assert row['non_evidence_dirty_inputs'] == []
    if test == 'class_cost_tokens_native':
        assert 'index296 length65; 9037 native pairs; max profile length1025' in log

outcomes = []
for label, file, before, after, target, test, assertion in mutations:
    source = str((driver.SRC / file).resolve().relative_to(ROOT))
    original = blob(source)
    assert original.decode().count(before) == 1, label
    mutated = original.decode().replace(before, after).encode()
    inputs = {**reference, source: digest(mutated)}
    mutant, log = read('mutant-' + label, target, test, 101, inputs, assertion)
    restored, _ = read('restore-' + label, target, test, 0, reference)
    assert mutant['mutation'] == label and mutant['source'] == source, label
    assert mutant['original_sha256'] == restored['restored_sha256'] == digest(original), label
    assert mutant['mutated_sha256'] == digest(mutated), label
    assert mutant['non_evidence_dirty_inputs'] == [source], label
    assert restored['non_evidence_dirty_inputs'] == [], label
    if label == 'token-length65':
        assert 'generated_tokens_native_length:296' in log, label
        assert 'left: 0' in log and 'right: 65' in log, label
    outcomes.append({'mutation': label, 'mutant_exit': 101, 'restore_exit': 0,
                     'assertion': assertion, 'source': source})

summary = json.loads((HERE / 'control-summary.json').read_text())
assert summary['sha'] == sha
assert (summary['selected_controls'], summary['detected'], summary['survivors'],
        summary['restored_passes'], summary['stopped_after_control_index']) == (12, 12, 0, 12, 11)
assert len(summary['outcomes']) == 12
assert {r['mutation'] for r in summary['outcomes']} == expected_names
for row in summary['outcomes']:
    assert row['detected'] and row['restored_pass'] and row['sha'] == sha
    assert row['mutant_exit'] == 101 and row['restore_exit'] == 0
assert json.loads((HERE / 'survivors.json').read_text()) == []
result = {'sha': sha, 'verified_utc': datetime.now(timezone.utc).isoformat(),
          'verifier_sha256': digest(Path(__file__).read_bytes()),
          'baseline_rows': 9, 'selected_rust_faults_caught': 11,
          'selected_source_encoder_faults_caught': 1, 'selected_controls_caught': 12,
          'restorations_passed': 12, 'survivors': 0, 'control_stop_index': 11,
          'case_stop_index': 511, 'length65_iteration': 296,
          'scope': 'Only these twelve selected faults; not all faults, array lengths or complete parity.',
          'outcomes': outcomes}
(HERE / 'verification.json').write_text(json.dumps(result, indent=2) + '\n')
print('VERIFIED: 11/11 Rust + 1/1 encoder controls caught; 12 restorations; 0 survivors', sha)
