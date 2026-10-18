#!/usr/bin/env python3
"""Check that the constructed binding reaches the real SDK initialization owner."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-apexomni/src/zklink_host.rs'
original = source.read_bytes()
needle = b'            host.init().await?;'
if original.count(needle) != 1:
    raise RuntimeError('review the SDK initialization await before mutation')
mutant = original.replace(needle, b'            std::future::pending::<()>().await;\n' + needle)
test = 'native_sdk_initialization_has_no_deferred_module_work'
command = ['cargo', '+1.99.0', 'test', '--color', 'never', '--locked', '--jobs', '1',
           '-p', 'safe-cex-venue-apexomni', '--test', 'binding', test, '--', '--exact']
logs = workspace / 'target/initialization-controls'
logs.mkdir(parents=True, exist_ok=True)

def run(label):
    result = subprocess.run(command, cwd=workspace, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    (logs / f'{label}.log').write_text(result.stdout)
    return result

try:
    source.write_bytes(mutant)
    red = run('pending-red')
    if (red.returncode != 101 or f'test {test} ... FAILED' not in red.stdout
            or 'SDK initialization needs deferred module work' not in red.stdout or 'error[E' in red.stdout):
        raise RuntimeError(red.stdout)
    print('RED: a compiled pending SDK initializer fails the actual binding witness', flush=True)
finally:
    source.write_bytes(original)
restored = run('restored')
if restored.returncode != 0 or '1 passed' not in restored.stdout:
    raise RuntimeError(restored.stdout)
print('GREEN: exact source restoration passes the actual binding witness', flush=True)
