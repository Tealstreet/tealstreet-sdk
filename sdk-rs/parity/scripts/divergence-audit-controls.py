"""Prove native audit assertions discriminate, sequentially."""
from pathlib import Path
import argparse
import os
import subprocess

root = Path(__file__).resolve().parents[3]
base = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
cases = [
    ('bingx', 'data_reads', 'audit_validation_returns_coerced_hints', 'src/data_reads.rs',
     'if js_truthy(msg) {\n        text(msg)',
     'if js_truthy(msg) {\n        Ok(msg.map_or_else(String::new, Value::to_string))',
     'BINGX-AUDIT native coerced hint'),
    ('blofin', 'orders', 'audit_order_refusal_preempts_source_evaluation', 'src/orders.rs',
     'let order_side = lookup(&crate::types::ORDER_SIDE, &js_string(row.get("side"))?);',
     'now();\n        let order_side = lookup(&crate::types::ORDER_SIDE, &js_string(row.get("side"))?);',
     'BLOFIN-AUDIT native skipped clock'),
    ('blofin', 'rest_lifecycle', 'audit_validation_interleaves_sum_coercion', 'src/rest_lifecycle.rs',
     'let v = parse(field(row, "frozen")?)?;',
     'let v = parse(field(row, "available")?)?;',
     'BLOFIN-AUDIT validation native precedence'),
    ('apexomni', 'exchange', 'continuation_position_short_circuit_matches_source', 'src/exchange.rs', 'let custom = numeric(get(row, "customImr"))?;', 'let _ = numeric(get(row, "customInitialMarginRate"))?;\n        let custom = numeric(get(row, "customImr"))?;', 'APEX-CONT mapPosition:ignored-custom native unexpected rejection'),
    ('apexomni', 'exchange', 'continuation_position_short_circuit_matches_source', 'src/exchange.rs', 'let custom = numeric(get(row, "customImr"))?;', 'let _ = numeric(&market.info.as_ref().and_then(|v| v.get("defaultInitialMarginRate")).cloned().map(J::from).unwrap_or(J::Undefined))?;\n        let custom = numeric(get(row, "customImr"))?;', 'APEX-CONT mapPosition:ignored-default native unexpected rejection'),
]
parser = argparse.ArgumentParser()
parser.add_argument('--only', choices=['all', 'bingx', 'blofin', 'apexomni'], default='all')
selection = parser.parse_args().only
if selection != 'all':
    cases = [case for case in cases if case[0] == selection]
assert len(cases) == {'all': 5, 'bingx': 1, 'blofin': 2, 'apexomni': 2}[selection]
environment = {**os.environ, 'CARGO_BUILD_JOBS': '1'}
assert environment.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler required'
for venue, target, test, relative, before, after, marker in cases:
    path = root / f'sdk-rs/safe-cex-venue-{venue}' / relative
    original = path.read_bytes()
    source = original.decode()
    assert source.count(before) == 1, f'control selector: {test}'
    command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
               f'safe-cex-venue-{venue}', '--test', target, '--locked', '--jobs', '1',
               test, '--', '--exact', '--nocapture']
    try:
        path.write_text(source.replace(before, after))
        failed = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True)
        diagnostic = failed.stdout + failed.stderr
        assert failed.returncode == 101 and 'test result: FAILED' in diagnostic and marker in diagnostic, diagnostic
        print(f'{base} {test}: mutation=101 named={marker} selector={before}', flush=True)
    finally:
        path.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True)
    assert restored.returncode == 0 and '1 passed' in restored.stdout, restored.stdout + restored.stderr
    print(f'{base} {test}: restored=0', flush=True)
