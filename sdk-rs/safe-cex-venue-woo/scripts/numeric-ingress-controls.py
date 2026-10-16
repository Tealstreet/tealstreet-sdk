"""Replay compiling numeric-boundary bypasses; named bit failures are required."""
from pathlib import Path
import os
import sys
import subprocess

root = Path('sdk-rs/safe-cex-venue-woo')
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
cases = [
    ('types', 'AccountResponse', 'source bits'),
    ('v3_contracts', 'WooV3Instrument', 'source bits'),
    ('v3_public', 'WooV3FuturesRow', 'source bits'),
    ('v3_public', 'WooV3MappedOrderBookSnapshot', 'source bits'),
    ('legacy_fills', 'WooLegacyFillsQuery', 'source bits'),
    ('v3_contracts', 'WooV3InstrumentBaseMin', 'WooV3InstrumentBaseMin raw -0 numeric union acceptance'),
]
requested = set(sys.argv[1:])
assert requested <= {name for _, name, _ in cases}, 'unknown numeric control name'
for module, name, assertion in cases:
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    path = root / f'src/{module}.rs'
    original = path.read_text()
    start = original.index(f"impl<'de> Deserialize<'de> for {name} {{")
    end = original.index('\n}\n', start) + 2
    old = original[start:end]
    wire = 'QueryWire' if module == 'legacy_fills' else name + ('NumericWire' if name.endswith('BaseMin') else 'Wire')
    body = f'{wire}::deserialize(deserializer)' if name.endswith('BaseMin') else f"let map = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?; {wire}::deserialize(serde_json::Value::Object(map)).map_err(serde::de::Error::custom)"
    replacement = f'''impl<'de> Deserialize<'de> for {name} {{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{
        {body}
    }}
}}'''
    assert old != replacement and 'JsNumberInput::deserialize' in old
    try:
        path.write_text(original[:start] + replacement + original[end:])
        run = subprocess.run([
            'cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
            '--locked', '--offline', '--jobs', '2', '-p', 'safe-cex-venue-woo',
            '--test', 'numeric_ingress',
        ], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path(f'/tmp/woo-ingress-control-{name}.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)
