"""Require compiled native callback failures and byte-exact restoration."""
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-venue-hyperliquid/src/api.rs'
original = source.read_bytes()
logs = Path(tempfile.mkdtemp(prefix='hl-api-signing-controls-'))
scenes = [
    ('ignore-emitter-failure', b'                emit(&format!(\n                    "Refusing to sign unexpected Hyperliquid action \\"{action_type}\\""\n                ))?;',
     b'                let _ = emit(&format!(\n                    "Refusing to sign unexpected Hyperliquid action \\"{action_type}\\""\n                ));'),
    ('replace-original-cause', b'Some(error) => Err(error),',
     b'Some(error) => Err(ExchangeError::new(error.message.clone())),')
]


def run(log):
    result = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-p',
                             'safe-cex-venue-hyperliquid', '--lib', 'binding::api_signing_audit::'],
                            cwd=sdk, env={**os.environ, 'CARGO_INCREMENTAL': '0'},
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    log.write_text(result.stdout)
    return result


for name, before, after in scenes:
    assert source.read_bytes() == original
    assert original.count(before) == 1, name
    try:
        source.write_bytes(original.replace(before, after))
        red = run(logs / f'{name}-red.log')
        assert red.returncode == 101, (name, logs)
        assert 'could not compile' not in red.stdout, (name, logs)
        assert 'HL_API_SIGNING' in red.stdout and 'panicked at' in red.stdout, (name, logs)
        assert 'api_signing_audit_listener_failure_rejects_before_transport ... FAILED' in red.stdout, (name, logs)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    green = run(logs / f'{name}-restored.log')
    assert green.returncode == 0 and '1 passed; 0 failed' in green.stdout, (name, logs)
    print(f'{name}: compiled named RED / exact-restored GREEN; {logs}', flush=True)
