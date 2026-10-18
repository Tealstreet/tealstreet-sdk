#!/usr/bin/env python3
"""Require compiled failures in the real class capabilities and exact restores."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / 'safe-cex-venue-orderly'
test = 'native_exchange::structural_alias_tests::native_class_callable_aliases_preserve_values_futures_and_errors'
controls = [
    ('bound-arguments', 'host.rs', 'match body(args.into_iter().map(v).collect())?', 'match body({ drop(args); Vec::new() })?'),
    ('body-result', 'host.rs', 'Ok((*future.await?).clone())', 'future.await?; Ok(Js::Undefined)'),
    ('native-member', 'members.rs', '"mapMarginType" => {\n            let value = arg(0);', '"mapMarginType" => {\n            let value = v(Js::Undefined);'),
    ('http-factory', 'primitives.rs', '(rt.shared.extra.deps.http)(&typed)', '(rt.shared.extra.deps.http)(&safe_cex_types::ExchangeOptions { key: "replaced-key".into(), ..typed })'),
]
logs = workspace / 'target/orderly-class-alias-controls'
logs.mkdir(parents=True, exist_ok=True)

def run(label):
    result = subprocess.run(['cargo', '+1.99.0', 'test', '--color', 'never', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-orderly', '--lib', test, '--', '--exact', '--test-threads=1'], cwd=workspace, capture_output=True, text=True, timeout=900)
    output = result.stdout + result.stderr
    (logs / (label + '.log')).write_text(output)
    return result.returncode, output

for name, filename, before, after in controls:
    source = crate / 'src/binding' / filename
    original = source.read_bytes()
    assert original.count(before.encode()) == 1, name
    try:
        source.write_bytes(original.replace(before.encode(), after.encode()))
        code, output = run(name + '-fault')
        assert code == 101 and f'test {test} ... FAILED' in output and 'could not compile' not in output, output[-8000:]
        print(f'COMPILED RED {name}', flush=True)
    finally:
        source.write_bytes(original)
    code, output = run(name + '-restored')
    assert code == 0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output, output[-8000:]
    print(f'EXACT RESTORE GREEN {name}', flush=True)
