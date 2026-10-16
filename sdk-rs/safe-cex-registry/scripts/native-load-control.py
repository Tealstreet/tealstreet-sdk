"""Require the native loader's laziness assertion to reject eager evaluation."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-registry/src/registry.rs'
command = ['cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-registry', '--test', 'native_load', '--jobs', '2']
original = source.read_bytes()
text = original.decode()
needle = 'Box::new(move || {'
if text.count(needle) != 1:
    raise SystemExit('REGISTRY native eager mutation site drift')
try:
    source.write_text(text.replace(needle, 'let _ = loader();\n    Box::new(move || {'))
    result = subprocess.run(command, cwd=root, capture_output=True, text=True)
    output = result.stdout + result.stderr
    if result.returncode != 101 or 'REGISTRY native lazy construction' not in output or 'error[E' in output:
        raise SystemExit('REGISTRY native missing compiled named failure\n' + output)
    print('native-eager-load: exit 101 at REGISTRY native lazy construction', flush=True)
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=root, capture_output=True, text=True)
if restored.returncode:
    raise SystemExit(restored.stdout + restored.stderr)
print('native-eager-load: restored exit 0', flush=True)
