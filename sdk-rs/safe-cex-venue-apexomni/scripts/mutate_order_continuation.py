"""Require the order native assertions to reject five native and one source regression."""
from pathlib import Path
import json
import os
import subprocess
import sys
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
     'if matches!(order_type, TableHit::Inherited(_)) { return Err(failure("shared typed Order refuses unknown venue order type".into())); } let price = float(raw_price)?;',
     'APEX-ORDER type-before-price native error kind'),
    ('sl-clock-before-tp-error', 'let tp_price = float(data::v(tp_part, "triggerPrice")?)?;',
     'let tp_price = match float(data::v(tp_part, "triggerPrice")?) { Ok(value) => value, Err(error) => { if sl_price > 0.0 { self.base.runtime.now_ms(); } return Err(error); } };',
     'APEX-ORDER tp-before-sl-clock virtual clock must not read the raw runtime'),
    ('raw-type-key', 'let type_key = text(data::v(row, "type")?)?;',
     'let type_key = match data::v(row, "type")? { J::String(value) => value.clone(), _ => "UNKNOWN".into() };',
     'APEX-ORDER missing-market-bad-type native expected source failure'),
    ('raw-base-clock', 'let last_update = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();',
     'let last_update = self.base.runtime.now_ms();',
     'native synced clock readers diverge'),
    ('early-reduce-refusal', 'let price = float(raw_price)?;',
     'let early_reduce_only = data::v(row, "reduceOnly")?; if !matches!(early_reduce_only, J::Bool(_)) && truthy(early_reduce_only) { return Err(failure("shared typed Order refuses non-boolean reduceOnly".into())); } let price = float(raw_price)?;',
     'APEX-ORDER reduce-before-price native error kind'),
]
assert len(cases) == 5
requested = set(sys.argv[1:])
assert requested <= {case[0] for case in cases} | {'source-amount'}, requested
selected = [case for case in cases if not requested or case[0] in requested]
run_source = not requested or 'source-amount' in requested
receipt = {'sha': sha, 'attempts': [], 'survivors': [], 'invalid_controls': []}


def run(name, expected, marker=None, env_extra=None):
    native_clock = name.startswith('raw-base-clock')
    target = 'native_clock' if native_clock else 'exchange'
    test = ('native_synced_clock_reaches_http_sockets_models_and_expiration' if native_clock
            else 'order_continuation_tests::order_coercions_and_clocks_precede_typed_admission')
    command = [os.environ.get('CARGO', 'cargo'), 'test', '--locked', '--jobs', '1',
               '-p', 'safe-cex-venue-apexomni', '--test', target, test, '--', '--exact', '--nocapture']
    start = time.monotonic()
    result = subprocess.run(command, cwd=root/'sdk-rs', env={**os.environ, **(env_extra or {})}, capture_output=True, text=True)
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


for name, before, after, marker in selected:
    assert source.count(before) == 1, (name, before)
    try:
        path.write_text(source.replace(before, after))
        run(name+'-mutation', 101, marker)
    finally:
        path.write_bytes(original)
    assert run(name+'-restored', 0), name
if run_source:
    run('source-amount-mutation', 101, 'APEX-ORDER valid native orders',
        {'SAFE_CEX_APEX_BOUNDARY_SOURCE_CONTROL': 'order'})
    assert run('source-amount-restored', 0)
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
assert len(receipt['attempts']) == 2 * (len(selected) + int(run_source)), receipt
assert not receipt['survivors'] and not receipt['invalid_controls'], receipt
