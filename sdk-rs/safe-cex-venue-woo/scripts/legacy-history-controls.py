"""Run compiling legacy fetch mutations against native request/row assertions."""
from pathlib import Path
import os
import subprocess

source = Path('sdk-rs/safe-cex-venue-woo/src/legacy_fills.rs')
original = source.read_text()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--offline', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'legacy_history']
env = dict(os.environ, CARGO_BUILD_JOBS='1', SAFE_CEX_TYPESCRIPT_PATH=str(Path.cwd() / 'node_modules/typescript'))
controls = [
    ('fallback', '    candidates.push(None);', '    candidates.reverse();', 'LEGACY native request sequence'),
    ('swallow', 'Some(error) => Err(error),', 'Some(_) => Ok(Vec::new()),', 'LEGACY expected retained error'),
    ('empty', 'return Ok(rows.to_vec());', 'return Ok(Vec::new());', 'LEGACY native row cardinality'),
    ('subset', 'return Ok(rows.to_vec());', 'return Ok(rows[..rows.len() - 1].to_vec());', 'LEGACY native row cardinality'),
    ('extra', 'return Ok(rows.to_vec());', 'return Ok(rows.iter().cloned().chain(std::iter::once(serde_json::Value::Null)).collect());', 'LEGACY native row cardinality'),
]
try:
    for name, before, after, assertion in controls:
        assert original.count(before) == 1, name
        source.write_text(original.replace(before, after))
        run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-legacy-control-' + name + '.log').write_text(run.stdout)
        assert run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout, (name, run.stdout)
        print(name, '101', assertion, flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
assert run.returncode == 0, run.stdout
print('restored target passed', flush=True)
