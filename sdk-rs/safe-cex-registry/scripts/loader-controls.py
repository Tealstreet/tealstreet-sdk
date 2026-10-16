"""Two compiled controls; restore exact bytes even when a control survives."""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parents[3]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
command = ['cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-registry', '--test', 'loaders', '--jobs', '2']
controls = [
    ('native-export', 'loaders.rs', 'pub fn native_apexomni(host: &dyn NativeModuleHost) -> NativeConstructorResult {\n    module_host::native_load(host, "./apexomni/apexomni.exchange", "ApexOmniExchange")',
     'pub fn native_apexomni(host: &dyn NativeModuleHost) -> NativeConstructorResult {\n    module_host::native_load(host, "./apexomni/apexomni.exchange", "WrongExport")', 'REGISTRY native require and export before future return'),
    ('deferred-import', 'module_host.rs', 'let imported = host.import_module(specifier);\n    Box::pin(async move { imported.await?.constructor_export(export) })',
     'Box::pin(async move { host.import_module(specifier).await?.constructor_export(export) })', 'REGISTRY web import before poll'),
]
for name, filename, old, new, assertion in controls:
    source = root / 'sdk-rs/safe-cex-registry/src' / filename
    original = source.read_bytes()
    text = original.decode()
    if text.count(old) != 1:
        raise SystemExit(name + ': mutation site drift')
    try:
        source.write_text(text.replace(old, new))
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
        output = result.stdout + result.stderr
        if result.returncode != 101 or assertion not in output or 'error[E' in output:
            raise SystemExit(name + ': missing compiled named failure, survivor/finding or compile error\n' + output)
        print(f'{name}: exit 101 at {assertion}', flush=True)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
    if restored.returncode:
        raise SystemExit(restored.stdout + restored.stderr)
    print(f'{name}: restored exit 0 (2 tests)', flush=True)
