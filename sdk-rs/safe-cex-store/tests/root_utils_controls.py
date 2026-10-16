"""One compiling negative control per newly ported behavior group, serially."""
from pathlib import Path
import os
import subprocess

command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
           '-p', 'safe-cex-store', '--test', 'root_utils', '--jobs', '2']
print('baseline_sha', subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), flush=True)
controls = [
    ('config', 'sdk-rs/safe-cex-store/src/config.rs',
     'pub const WS_BACKOFF_BASE_DELAY: f64 = 2000.0;',
     'pub const WS_BACKOFF_BASE_DELAY: f64 = 3000.0;',
     'ROOT-UTILS config native constant identity'),
    ('broker', 'sdk-rs/safe-cex-store/src/account_environment.rs',
     'get_public_env(&format!("{prefix}_{suffix}"), lookup)',
     'get_public_env(&format!("{prefix}_NAME"), lookup)',
     'ROOT-UTILS broker fallback identity'),
    ('telemetry', 'sdk-rs/safe-cex-store/src/telemetry.rs',
     'sink.event(name, data)', 'sink.event(name, data).or(Ok(()))',
     'ROOT-UTILS telemetry native failure propagation'),
]
for label, filename, old, new, assertion in controls:
    path = Path(filename)
    original = path.read_text()
    assert original.count(old) == 1, (label, 'unique mutation target')
    try:
        path.write_text(original.replace(old, new))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        Path('/tmp/p4-root-utils-' + label + '.log').write_text(output)
        assert result.returncode == 101 and assertion in output and 'could not compile' not in output, output
        print(label, 'compiled exit=101:', assertion, flush=True)
    finally:
        path.write_text(original)
result = subprocess.run(command, capture_output=True, text=True)
Path('/tmp/p4-root-utils-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('RUST RESTORED exit=0 4/4', flush=True)

# The source-side observable-unit witness is its own small compiling control.
# It proves the callback assertion discriminates without claiming Rust can
# construct a lone unit inside the required &str name parameter.
source = Path('packages/safe-cex/src/telemetry.ts')
original = source.read_text()
old = '  sink.event?.(name, data);'
new = r"  sink.event?.(name.replace(/\uD800/g, '\uFFFD'), data);"
assert original.count(old) == 1
node_command = ['node', 'sdk-rs/safe-cex-store/tests/root_utils_oracle.cjs']
try:
    source.write_text(original.replace(old, new))
    result = subprocess.run(node_command, capture_output=True, text=True)
    output = result.stdout + result.stderr
    Path('/tmp/p4-root-utils-source-unit.log').write_text(output)
    assertion = 'ROOT-UTILS source event name observable lone unit'
    assert result.returncode == 1 and assertion in output, output
    print('source-unit executed exit=1:', assertion, flush=True)
finally:
    source.write_text(original)
result = subprocess.run(node_command, capture_output=True, text=True)
Path('/tmp/p4-root-utils-source-restored.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print('SOURCE RESTORED exit=0; stopped after control 4', flush=True)
