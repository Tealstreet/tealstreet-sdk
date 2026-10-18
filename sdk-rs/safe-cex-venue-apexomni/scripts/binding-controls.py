#!/usr/bin/env python3
"""Prove each Exchange-method E2E rejects a compiled, wrong production body."""
import argparse
import pathlib
import re
import subprocess

crate = pathlib.Path(__file__).resolve().parents[1]
workspace = crate.parent
source = crate / 'src/binding/exchange.rs'
methods = ['place_order', 'start', 'get_account', 'validate_account', 'fetch_positions',
           'fetch_orders', 'fetch_markets', 'fetch_tickers', 'fetch_symbol_max_leverage',
           'set_leverage', 'change_position_mode', 'change_margin_type', 'update_order',
           'cancel_orders', 'schedule_cancel', 'fetch_public_trades', 'listen_ohlcv', 'listen_order_book']
parser = argparse.ArgumentParser()
parser.add_argument('--only', choices=methods)
args = parser.parse_args()
original = source.read_text()

def run(method=None):
    command = ['cargo', 'test', '-p', 'safe-cex-venue-apexomni', '--jobs', '1', '--test', 'binding']
    if method:
        command += ['bound_' + method, '--', '--exact']
    else:
        command += ['bound_']
    return subprocess.run(command, cwd=workspace, text=True, capture_output=True)

try:
    baseline = run()
    assert baseline.returncode == 0 and f'{len(methods)} passed' in baseline.stdout, baseline.stdout + baseline.stderr
    for method in ([args.only] if args.only else methods):
        start = original.index('impl Exchange for ApexOmniExchange')
        declaration = re.search(r'    fn ' + re.escape(method) + r'\(', original[start:])
        assert declaration, method
        declaration_start = start + declaration.start()
        body_start = original.index('{', declaration_start)
        depth, end = 1, body_start + 1
        # Bound method bodies contain balanced braces in their literal diagnostics.
        while depth:
            depth += (original[end] == '{') - (original[end] == '}')
            end += 1
        mutant = (original[:declaration_start] + '    #[allow(unused_variables)]\n'
                  + original[declaration_start:body_start]
                  + ('{ Err(ExchangeError::new("binding mutation")) }' if method.startswith('listen_') else '{ Box::pin(async { Err(ExchangeError::new("binding mutation")) }) }')
                  + original[end:])
        source.write_text(mutant)
        result = run(method)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and ('test bound_' + method + ' ... FAILED') in output, output
        assert 'binding mutation' in output, output
        print('CONTROL PASS: ' + method + ' compiled and failed its named E2E', flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
restored = run()
assert restored.returncode == 0 and f'{len(methods)} passed' in restored.stdout, restored.stdout + restored.stderr
print(f'RESTORED PASS: {len(methods)} Exchange methods (16 required plus 2 listeners)', flush=True)
