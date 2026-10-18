#!/usr/bin/env python3
"""Reintroduce the lossy topic seam; require named native wire failures, then restore."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

os.environ['CARGO_INCREMENTAL'] = '0'
sdk = Path(__file__).resolve().parents[2]
os.environ['SAFE_CEX_TYPESCRIPT_PATH'] = str(sdk.parent / 'node_modules/typescript')
production = sdk / 'safe-cex-venue-kraken/src/binding/sockets.rs'
original = production.read_bytes()
needle = 'RetainedJsonInput::Text(t.to_vec())'
assert original.decode().count(needle) == 1
receipt = Path(tempfile.mkdtemp(prefix='kraken-topic-alias-control-'))
tests = ['topic_code_units_reach_futures_subscription_wire_like_live_source', 'topic_code_units_reach_spot_subscription_wire_like_live_source']
def run(stage, negative):
    result = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-kraken', '--lib', 'accepted_topic_alias', '--', '--nocapture'], cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (receipt / f'{stage}.log').write_text(result.stdout)
    if negative:
        assert result.returncode != 0 and 'could not compile' not in result.stdout and 'panicked at' in result.stdout, result.stdout
        for test in tests:
            assert f'test binding::exchange::accepted_topic_alias::{test} ... FAILED' in result.stdout, result.stdout
    else:
        assert result.returncode == 0 and '3 passed; 0 failed' in result.stdout, result.stdout
    return result.returncode
try:
    production.write_text(original.decode().replace(needle, 'RetainedJsonInput::Text(lossy(t).encode_utf16().collect())'))
    red = run('lossy-topic', True)
finally:
    production.write_bytes(original)
green = run('exact-restored', False)
(receipt / 'receipts.json').write_text(json.dumps({'namedFuturesAndSpotFailures': tests, 'compiledRed': red, 'exactRestoredGreen': green}, indent=2)+'\n')
print(receipt, flush=True)
