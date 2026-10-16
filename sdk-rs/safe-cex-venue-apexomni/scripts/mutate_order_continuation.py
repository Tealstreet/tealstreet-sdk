"""Require the order native assertions to reject five native and one source regression."""
from pathlib import Path
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parents[3]
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
out = root / 'sdk-rs/target/apexomni-order-controls' / sha
out.mkdir(parents=True, exist_ok=True)
path = root / 'sdk-rs/safe-cex-venue-apexomni/src/exchange.rs'
original = path.read_bytes()
source = original.decode()
cases = [
    ('premature-type-refusal', 'let price = float(raw_price)?;',
     'if !matches!(order_type, TableHit::Mapped(_)) { return Err(failure("shared typed Order refuses unknown venue order type".into())); } let price = float(raw_price)?;',
     'APEX-ORDER type-function native clock count'),
    ('sl-clock-before-tp-error', 'let tp_price = float(data::v(tp_part, "triggerPrice")?)?;',
     'let tp_price = match float(data::v(tp_part, "triggerPrice")?) { Ok(value) => value, Err(error) => { if sl_price > 0.0 { self.base.runtime.now_ms(); } return Err(error); } };',
     'APEX-ORDER tp-before-sl-clock native clock count'),
    ('raw-type-key', 'let type_key = text(data::v(row, "type")?)?;',
     'let type_key = match data::v(row, "type")? { J::String(value) => value.clone(), _ => "UNKNOWN".into() };',
     'APEX-ORDER missing-market-bad-type native expected source failure'),
    ('base-extra-clock', 'let last_update = self.base.runtime.now_ms();',
     'let last_update = { self.base.runtime.now_ms(); self.base.runtime.now_ms() };',
     'APEX-ORDER valid native clock count'),
    ('early-reduce-refusal', 'let raw_reduce_only = data::v(row, "reduceOnly")?;',
     'let raw_reduce_only = data::v(row, "reduceOnly")?; if !matches!(raw_reduce_only, J::Bool(_)) && truthy(raw_reduce_only) { return Err(failure("shared typed Order refuses non-boolean reduceOnly".into())); }',
     'APEX-ORDER numeric-reduce native clock count'),
]
assert len(cases) == 5
receipt = {'sha': sha, 'attempts': [], 'survivors': [], 'invalid_controls': []}


def run(name, expected, marker=None, env_extra=None):
    command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(root/'sdk-rs/Cargo.toml'),
               '--locked', '--jobs', '2', '-p', 'safe-cex-venue-apexomni', '--test', 'exchange',
               'order_continuation_tests::order_coercions_and_clocks_precede_typed_admission', '--', '--exact', '--nocapture']
    start = time.monotonic()
    result = subprocess.run(command, cwd=root, env={**os.environ, **(env_extra or {})}, capture_output=True, text=True)
    text = result.stdout + result.stderr
    (out/(name+'.log')).write_text(text)
    valid = result.returncode == expected and (('test result: FAILED.' in text and marker in text)
                                             if expected == 101 else '1 passed' in text)
    receipt['attempts'].append({'name': name, 'exit': result.returncode, 'expected_exit': expected,
                               'assertion': marker, 'valid': valid, 'seconds': round(time.monotonic()-start, 2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control': name, 'call_site': 'ApexOmniExchange::map_order'})
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
run('source-amount-mutation', 101, 'APEX-ORDER valid native orders',
    {'SAFE_CEX_APEX_BOUNDARY_SOURCE_CONTROL': 'order'})
assert run('source-amount-restored', 0)
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
assert len(receipt['attempts']) == 12, receipt
assert not receipt['survivors'] and not receipt['invalid_controls'], receipt
