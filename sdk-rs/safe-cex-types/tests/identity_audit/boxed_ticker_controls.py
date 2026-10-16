"""Run serial compiling negative controls against the boxed gateway assertion."""
from pathlib import Path
import os
import subprocess

target = Path('sdk-rs/safe-cex-types/src/gateway.rs')
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
           '-p', 'safe-cex-types', '--test', 'boxed_ticker', '--jobs', '2']
original = target.read_text()
needle = '#[serde(untagged)]\npub enum GatewayTickerData'
assert original.count(needle) == 1, 'BOXED-TICKER unique untagged production declaration'
print('baseline_sha', subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), flush=True)
try:
    target.write_text(original.replace(needle, 'pub enum GatewayTickerData'))
    result = subprocess.run(command, capture_output=True, text=True)
    output = result.stdout + result.stderr
    Path('/tmp/p4-boxed-ticker-tagged-control.log').write_text(output)
    assertion = 'BOXED-TICKER singular native decode'
    assert result.returncode == 101 and assertion in output and 'could not compile' not in output, output
    print('tagged-control compiled exit=101:', assertion, flush=True)
finally:
    target.write_text(original)
result = subprocess.run(command, capture_output=True, text=True)
Path('/tmp/p4-boxed-ticker-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('RESTORED exit=0 1/1', flush=True)
