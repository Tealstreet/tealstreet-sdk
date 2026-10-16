"""Regress each repaired boundary and require its named native assertion."""
from pathlib import Path
import argparse
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parents[3]
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
out = root / 'sdk-rs/target/apexomni-market-execution-controls' / sha
out.mkdir(parents=True, exist_ok=True)
position_test = 'continuation_position_short_circuit_matches_source'
market_test = 'market_execution_boundary_tests::market_filter_errors_precede_typed_output_admission'
custom = 'let custom = numeric(get(row, "customImr"))?;'
cases = [
    ('ignored-custom', 'exchange.rs', position_test, custom,
     'let _ = numeric(get(row, "customInitialMarginRate"))?;\n        ' + custom,
     'APEX-CONT mapPosition:ignored-custom native unexpected rejection'),
    ('ignored-default', 'exchange.rs', position_test, custom,
     'let _ = numeric(&market.info.as_ref().and_then(|v| v.get("defaultInitialMarginRate")).cloned().map(J::from).unwrap_or(J::Undefined))?;\n        ' + custom,
     'APEX-CONT mapPosition:ignored-default native unexpected rejection'),
    ('missing-market-unrealised', 'exchange.rs', position_test,
     'let unrealized_pnl = float(&or_zero(data::v(row, "unrealisedPnl")?))?;',
     'let unrealized_pnl = if matches!(get(row, "symbol"), J::String(s) if s == "MISSING") { 0.0 } else { float(&or_zero(data::v(row, "unrealisedPnl")?))? };',
     'APEX-CONT missing-market unrealisedPnl must fail before lookup'),
    ('filter-before-map', 'exchange.rs', market_test,
     'filtered.push(row);', 'filtered.push(row);\n                break;',
     'APEX-BOUNDARY markets/late-null native error kind'),
    ('numeric-before-admission', 'exchange.rs', market_test,
     'let amount = float(get(row, "stepSize"))?;',
     'if !matches!(get(row, "enableTrade"), J::Bool(_)) { return Err(failure("shared typed Market refuses non-boolean enableTrade".into())); }\n            let amount = float(get(row, "stepSize"))?;',
     'APEX-BOUNDARY markets/nonboolean-bad-step native error kind'),
    ('ws-asset-coercion', 'executions.rs',
     'market_execution_boundary_tests::websocket_assets_preserve_strings_and_refuse_raw_nonstrings',
     'Some(field_string(asset)?)',
     'Some({ let _ = field_string(&J::String(String::new()))?; string(asset).map_err(ExchangeError::new)? })',
     'APEX-BOUNDARY native commissionAsset refuses raw nonstring'),
    ('history-asset-coercion', 'historical_fills.rs',
     'market_execution_boundary_tests::historical_assets_refuse_rest_records_with_explicit_cached_fallback',
     'Some(field_string(&asset)?)',
     'Some({ let _ = field_string(&J::String(String::new()))?; text(&asset)? })',
     'native whole REST result refused'),
    ('whole-map-boolean-admission', 'exchange.rs', market_test,
     '            mapped.push((',
     '            if !matches!(get(row, "enableTrade"), J::Bool(_)) { return Err(failure("shared typed Market refuses non-boolean enableTrade".into())); }\n            mapped.push((',
     'APEX-BOUNDARY markets/nonboolean-before-later-bad-step native error kind'),
    ('whole-map-string-admission', 'exchange.rs', market_test,
     '            mapped.push((',
     '            let _ = field_string(get(row, "symbol"))?;\n            mapped.push((',
     'APEX-BOUNDARY markets/numeric-id-before-later-bad-step native error kind'),
    ('market-id-coercion', 'exchange.rs', market_test,
     'id: field_string(get(row, "symbol"))?', 'id: text(get(row, "symbol"))?',
     'APEX-BOUNDARY native Market refuses unsupported raw field'),
    ('history-early-admission', 'historical_fills.rs',
     'market_execution_boundary_tests::historical_assets_refuse_rest_records_with_explicit_cached_fallback',
     '            result.push((', '            let _ = field_string(&asset)?;\n            result.push((',
     'APEX-BOUNDARY fetchHistoricalFills/raw-before-null cache=true native operations'),
    ('history-before-time-filter', 'historical_fills.rs',
     'market_execution_boundary_tests::historical_assets_refuse_rest_records_with_explicit_cached_fallback',
     '            for (execution, asset) in executions {',
     '            for (execution, asset) in executions {\n                if from.is_some() { let _ = field_string(&asset)?; }',
     'APEX-BOUNDARY fetchHistoricalFills/raw-outside-window cache=true native operations'),
    ('history-before-deduplication', 'historical_fills.rs',
     'market_execution_boundary_tests::historical_assets_refuse_rest_records_with_explicit_cached_fallback',
     '                let side = if execution.side == OrderSide::Buy {',
     '                let _ = field_string(&asset)?;\n                let side = if execution.side == OrderSide::Buy {',
     'APEX-BOUNDARY fetchHistoricalFills/raw-overwritten cache=true native executions'),
    ('history-before-truncation', 'historical_fills.rs',
     'market_execution_boundary_tests::historical_assets_refuse_rest_records_with_explicit_cached_fallback',
     '            deduped.truncate(count);',
     '            for (_, asset) in &deduped { let _ = field_string(asset)?; }\n            deduped.truncate(count);',
     'APEX-BOUNDARY fetchHistoricalFills/raw-truncated cache=true native executions'),
    ('ws-side-coercion-kind', 'executions.rs',
     'market_execution_boundary_tests::websocket_coercion_failures_keep_source_type_error',
     'let text = text(value)?;', 'let text = string(value).map_err(ExchangeError::new)?;',
     'APEX-BOUNDARY WS coercion/side native error kind'),
    ('ws-price-coercion-kind', 'exchange.rs',
     'market_execution_boundary_tests::websocket_coercion_failures_keep_source_type_error',
     'executions::parse(value).map_err(|message| {\n        let mut error = failure(message);\n        error.name = "TypeError".into();\n        error\n    })',
     'executions::parse(value).map_err(failure)',
     'APEX-BOUNDARY WS coercion/price native error kind'),
    ('missing-config-admission', 'exchange.rs', market_test,
     '            old.remove("contractConfig");',
     '            return Err(failure("shared retained JSON refuses this prepared value".into()));',
     'APEX-BOUNDARY markets/missing-config native error kind'),
]
assert len(cases) == 17
receipt = {'sha': sha, 'attempts': [], 'survivors': [], 'invalid_controls': []}


def run(name, test, expected, marker=None, extra_env=None):
    command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path',
               str(root / 'sdk-rs/Cargo.toml'), '--locked', '--jobs', '2',
               '-p', 'safe-cex-venue-apexomni', '--test', 'exchange', test,
               '--', '--exact', '--nocapture']
    start = time.monotonic()
    result = subprocess.run(command, cwd=root, env={**os.environ, **(extra_env or {})}, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (out / (name + '.log')).write_text(output)
    valid = result.returncode == expected and (
        ('test result: FAILED.' in output and marker in output)
        if expected == 101 else '1 passed' in output)
    receipt['attempts'].append({'name': name, 'exit': result.returncode,
                               'expected_exit': expected, 'named_assertion': marker,
                               'valid': valid, 'seconds': round(time.monotonic()-start, 2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control': name, 'call_site': test})
    elif not valid:
        receipt['invalid_controls'].append(name)
    (out / 'controls.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(name, 'exit', result.returncode, 'valid', valid, flush=True)
    return valid


parser = argparse.ArgumentParser()
parser.add_argument('--only', choices=['all'] + [case[0] for case in cases] +
                    ['source-position', 'source-markets', 'source-ws', 'source-history'], default='all')
selection = parser.parse_args().only
receipt['selection'] = selection

for name, file, test, before, after, marker in cases:
    if selection not in ('all', name):
        continue
    path = root / 'sdk-rs/safe-cex-venue-apexomni/src' / file
    original = path.read_bytes()
    source = original.decode()
    assert source.count(before) == 1, (name, before)
    try:
        path.write_text(source.replace(before, after))
        run(name + '-mutation', test, 101, marker)
    finally:
        path.write_bytes(original)
    assert run(name + '-restored', test, 0), name
source_controls = [
    ('position', position_test, 'APEX-CONT mapPosition:ignored-custom native position'),
    ('markets', market_test, 'APEX-BOUNDARY markets/valid native markets'),
    ('ws', 'market_execution_boundary_tests::websocket_assets_preserve_strings_and_refuse_raw_nonstrings',
     'APEX-BOUNDARY recordWsExecution/string cache=false native executions'),
    ('history', 'market_execution_boundary_tests::historical_assets_refuse_rest_records_with_explicit_cached_fallback',
     'APEX-BOUNDARY fetchHistoricalFills/string cache=false native executions'),
]
assert len(source_controls) == 4
for name, test, marker in source_controls:
    if selection not in ('all', 'source-' + name):
        continue
    run('source-' + name + '-mutation', test, 101, marker,
        {'SAFE_CEX_APEX_BOUNDARY_SOURCE_CONTROL': name})
    assert run('source-' + name + '-restored', test, 0), name
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
assert len(receipt['attempts']) == (42 if selection == 'all' else 2), receipt
assert not receipt['survivors'] and not receipt['invalid_controls'], receipt
print('CONTROLS COMPLETE', sha, out.relative_to(root), flush=True)
