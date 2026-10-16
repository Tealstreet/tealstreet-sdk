"""Compiling production and live-source controls; restores every edited byte."""
from pathlib import Path
import os
import subprocess
import tempfile
import sys

selected = set(sys.argv[1:])

root = Path(__file__).resolve().parents[3]
production = root / 'sdk-rs/safe-cex-venue-bitget/src/positioning.rs'
test = root / 'sdk-rs/safe-cex-venue-bitget/tests/positioning.rs'
oracle = root / 'sdk-rs/safe-cex-venue-bitget/scripts/positioning-capture.cjs'
controls = [
    ('mode-swap', production, 'return Ok(mode);', 'return Ok(match mode { PositionMode::Hedged => PositionMode::OneWay, PositionMode::OneWay => PositionMode::Hedged });', 'POSITIONING native enum'),
    ('margin-swap', production, 'return Ok(margin);', 'return Ok(match margin { MarginType::Isolated => MarginType::Cross, MarginType::Cross => MarginType::Isolated });', 'POSITIONING native enum'),
    ('reporter-discard', production, 'report(&message)?;', 'if report(&message).is_err() {}', 'POSITIONING native error name'),
    ('early-uta', production, 'if let Some(mode) = resolve(symbol).await?', 'let early_uta = uses_uta();\n    let uses_uta = || early_uta;\n    if let Some(mode) = resolve(symbol).await?', 'POSITIONING success effects'),
    ('empty-source', oracle, 'const boundaries = [];', 'cases.splice(0);\n  const boundaries = [];', 'POSITIONING source total'),
    ('extra-source', oracle, 'const boundaries = [];', 'cases.push(cases[0]);\n  const boundaries = [];', 'POSITIONING source total'),
    ('subset-source', oracle, 'const boundaries = [];', 'cases.splice(1);\n  const boundaries = [];', 'POSITIONING source total'),
    ('same-length-source', oracle, 'const boundaries = [];', 'cases.fill(cases[0]);\n  const boundaries = [];', 'POSITIONING input content'),
    ('reversed-source', oracle, 'const boundaries = [];', 'cases.reverse();\n  const boundaries = [];', 'POSITIONING input content'),
    ('missing-enum', oracle, "const source = enums.join('\\n')", "const source = ''", 'POSITIONING source diagnostics'),
]
env = dict(os.environ)
env['CARGO_BUILD_JOBS'] = '1'
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-venue-bitget', '--test', 'positioning', '--jobs', '1', '--', '--include-ignored']
for name, file, before, after, assertion in controls:
    if selected and name not in selected:
        continue
    original = file.read_bytes()
    assert before in original.decode(), name
    try:
        file.write_text(original.decode().replace(before, after, 1))
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
        output = result.stdout + result.stderr
        log = Path(tempfile.gettempdir()) / f'bitget-positioning-{name}.log'
        log.write_text(output)
        assert result.returncode == 101 and assertion in output, f'{name}: code {result.returncode}; inspect {log}'
        assert 'could not compile' not in output, f'{name}: compile failure is not behavior evidence'
        print(name, result.returncode, assertion, flush=True)
    finally:
        file.write_bytes(original)
for name, mutation, assertion in [
    ('empty-bypass', 'cases.splice(0);', 'POSITIONING required native input'),
    ('subset-bypass', 'cases.splice(1);', 'POSITIONING required native input'),
    ('extra-bypass', 'cases.push(cases[0]);', 'POSITIONING source consumed count'),
]:
    if selected and name not in selected:
        continue
    original_source, original_test = oracle.read_bytes(), test.read_bytes()
    try:
        oracle.write_text(original_source.decode().replace('const boundaries = [];', mutation + '\n  const boundaries = [];', 1))
        guard = 'assert_eq!(cases.len(), 128, "POSITIONING source total");'
        assert guard in original_test.decode()
        test.write_text(original_test.decode().replace(guard, '', 1))
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
        output = result.stdout + result.stderr
        (Path(tempfile.gettempdir()) / f'bitget-positioning-{name}.log').write_text(output)
        assert result.returncode == 101 and assertion in output, name
        assert 'could not compile' not in output, name
        print(name, result.returncode, assertion, flush=True)
    finally:
        oracle.write_bytes(original_source)
        test.write_bytes(original_test)
restored = subprocess.run(command, cwd=root, env=env)
assert restored.returncode == 0, 'POSITIONING restored suite'
