#!/usr/bin/env python3
"""Sequential Inverse consumer faults: require runtime failures and restore bytes."""
from pathlib import Path
import os
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-bybit/src/binding/exchange.rs'
ORIGINAL = SOURCE.read_bytes()
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-bybit', '--test', 'inverse_binding', '--test', 'inverse_batch_binding']
LOGS = Path(os.environ.get('INVERSE_CONTROL_LOG_DIR') or tempfile.mkdtemp(prefix='bybit-inverse-controls-'))
LOGS.mkdir(parents=True, exist_ok=True)
print(f"Control logs: {LOGS}", flush=True)
METHODS = {
    'start': None, 'get_account': None, 'validate_account': None,
    'fetch_positions': None, 'fetch_orders': None, 'fetch_markets': None,
    'fetch_tickers': None, 'place_order': None, 'place_orders': None,
    'update_order': None, 'cancel_orders': None, 'set_leverage': None,
    'change_position_mode': None, 'change_margin_type': None,
    'fetch_symbol_max_leverage': '1.', 'schedule_cancel': '()',
    'fetch_public_trades': 'vec![]', 'dispose': '()',
}
SELECTED = sys.argv[1:] or [*METHODS, 'constructor']
assert all(name in METHODS or name == 'constructor' for name in SELECTED)

def run(label, negative=False):
    result = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    if negative:
        assert result.returncode == 101 and 'Finished `test`' in output and 'error[E' not in output, f'{label}: must compile then fail; {LOGS}'
        assert 'panicked at' in output and ' ... FAILED' in output, f'{label}: named runtime failure required; {LOGS}'
    else:
        assert result.returncode == 0 and '9 passed; 0 failed' in output and '1 passed; 0 failed' in output, f'{label}: all nine E2Es and 36-source-case test must pass; {LOGS}'
    print(f'{label}: exit {result.returncode}', flush=True)

try:
    run('baseline')
    for name in SELECTED:
        text = ORIGINAL.decode()
        if name == 'constructor':
            needle = '                | ExchangeName::Bybitinverse\n'
            assert text.count(needle) == 1
            changed = text.replace(needle, '')
        else:
            begin = text.index('impl Exchange for BybitExchange')
            match = re.search(r'    fn ' + name + r"(?:<'[^>]+>)?\(", text[begin:])
            assert match, name
            fn_start = begin + match.start()
            body_start = text.index('{', fn_start)
            if name == 'dispose':
                replacement = 'Ok(())'
            elif METHODS[name] is None:
                replacement = 'Box::pin(async { Err(ExchangeError::new("disabled Inverse ' + name + '")) })'
            else:
                replacement = 'Box::pin(async { Ok(' + METHODS[name] + ') })'
            # Keep original references so the fault cannot fail for dead-code imports.
            changed = text[:fn_start] + '    #[allow(unreachable_code, unused_variables)]\n' + text[fn_start:body_start+1] + '\n        return ' + replacement + ';' + text[body_start+1:]
        SOURCE.write_text(changed)
        try:
            run(name, True)
        finally:
            SOURCE.write_bytes(ORIGINAL)
    run('restored')
finally:
    SOURCE.write_bytes(ORIGINAL)
assert SOURCE.read_bytes() == ORIGINAL
print(f'{len(SELECTED)} compiled consumer faults caught; exact restoration passes; logs {LOGS}')
