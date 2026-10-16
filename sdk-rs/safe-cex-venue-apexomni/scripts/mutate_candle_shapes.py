"""Require the candle native assertions to reject four native and one source regression."""
from pathlib import Path
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parents[3]
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
out = root / 'sdk-rs/target/apexomni-candle-controls' / sha
out.mkdir(parents=True, exist_ok=True)
path = root / 'sdk-rs/safe-cex-venue-apexomni/src/lifecycle.rs'
original = path.read_bytes()
source = original.decode()
cases = [
    ('missing-map', 'matches!(map, J::Undefined | J::Null)', 'matches!(map, J::Null)',
     'APEX-CANDLE object-map-absent native unexpected rejection'),
    ('falsy-map', 'matches!(map, J::Undefined | J::Null)', '!data::truthy(map)',
     'APEX-CANDLE object-map-false native expected rejection'),
    ('null-row', 'numeric(data::property(r, "t")?)?', 'numeric(data::get(r, "t"))?',
     'APEX-CANDLE array-null native expected rejection'),
    ('nan-order', '''a.timestamp
                .is_nan()
                .cmp(&b.timestamp.is_nan())
                .then_with(|| {
                    a.timestamp
                        .partial_cmp(&b.timestamp)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })''', '''a.timestamp.partial_cmp(&b.timestamp).unwrap_or(std::cmp::Ordering::Equal)''',
     'APEX-CANDLE array-mixed-nan native candles'),
]
assert len(cases) == 4
receipt = {'sha': sha, 'attempts': [], 'survivors': [], 'invalid_controls': []}


def run(name, expected, marker=None, env_extra=None):
    command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(root/'sdk-rs/Cargo.toml'),
               '--locked', '--jobs', '2', '-p', 'safe-cex-venue-apexomni', '--test', 'exchange',
               'lifecycle_tests::candle_response_shapes_match_actual_source', '--', '--exact', '--nocapture']
    start = time.monotonic()
    result = subprocess.run(command, cwd=root, env={**os.environ, **(env_extra or {})}, capture_output=True, text=True)
    text = result.stdout + result.stderr
    (out/(name+'.log')).write_text(text)
    valid = result.returncode == expected and (('test result: FAILED.' in text and marker in text)
                                             if expected == 101 else '1 passed' in text)
    receipt['attempts'].append({'name': name, 'exit': result.returncode, 'expected_exit': expected,
                               'assertion': marker, 'valid': valid, 'seconds': round(time.monotonic()-start, 2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control': name, 'call_site': 'ApexOmniExchange::fetch_ohlcv'})
    elif not valid:
        receipt['invalid_controls'].append(name)
    (out/'controls.json').write_text(json.dumps(receipt, indent=2)+'\n')
    print(name, result.returncode, 'valid', valid, flush=True)
    return valid


for name, before, after, marker in cases:
    assert source.count(before) == 1, (name, before)
    try:
        path.write_text(source.replace(before, after))
        run(name+'-mutation', 101, marker)
    finally:
        path.write_bytes(original)
    assert run(name+'-restored', 0), name
run('source-timestamp-mutation', 101, 'APEX-CANDLE array-valid native candles',
    {'SAFE_CEX_APEX_BOUNDARY_SOURCE_CONTROL': 'candles'})
assert run('source-timestamp-restored', 0)
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
assert not receipt['survivors'] and not receipt['invalid_controls'], receipt
