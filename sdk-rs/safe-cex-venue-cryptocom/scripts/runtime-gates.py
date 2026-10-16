"""Record sequential scoped gates against an immutable source revision."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import tempfile
import shutil

root = Path(__file__).resolve().parents[3]
os.chdir(root)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
evidence = Path('sdk-rs/safe-cex-venue-cryptocom/evidence/runtime-' + sha[:10])
evidence.mkdir(parents=True, exist_ok=True)
cargo = ['cargo']
common = ['--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-cryptocom', '--jobs', '1', '--locked']
yarn = ['node', '-e', "require('node:child_process').execFileSync(process.execPath,[process.env.SAFE_CEX_YARN_JS,...process.argv.slice(1)],{stdio:'inherit'})"]
gates = {
    'check': cargo + ['check', *common, '--all-targets'],
    'tests': cargo + ['test', *common, '--all-targets', '--', '--include-ignored', '--test-threads=1'],
    'host-clippy': cargo + ['clippy', *common, '--all-targets'],
    'wasm-clippy': cargo + ['clippy', *common, '--all-targets', '--target', 'wasm32-unknown-unknown'],
    'wasm-build': cargo + ['build', *common, '--target', 'wasm32-unknown-unknown'],
    'fmt': cargo + ['fmt', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-cryptocom', '--check'],
    'extract-check': yarn + ['parity:extract:check'],
    'parity-check': yarn + ['parity:check'],
    'parity-report': yarn + ['parity:report'],
    'parity-progress': yarn + ['parity:progress'],
    'ci-wiring-focused': ['node', '--test', '--test-name-pattern=Crypto.com', 'scripts/rust-ci-wiring.test.mjs'],
    'ci-wiring-full': ['node', '--test', 'scripts/rust-ci-wiring.test.mjs'],
    'isolated-oracles': [sys.executable, 'sdk-rs/safe-cex-venue-cryptocom/scripts/runtime-isolation.py'],
}
selected = sys.argv[1:] or list(gates)
records = json.loads((evidence / 'gates.json').read_text()) if (evidence / 'gates.json').exists() else []
for name in selected:
    argv = gates[name]
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == sha
    started = time.time()
    print('RUN', name, sha, flush=True)
    result = subprocess.run(argv, capture_output=True, text=True)
    content = result.stdout + result.stderr
    for value, replacement in [(str(root), '<worktree>'), (str(Path.home()), '<home>'),
                               (os.environ.get('SAFE_CEX_TYPESCRIPT_PATH', ''), '<external-typescript>'),
                               (os.environ.get('SAFE_CEX_YARN_JS', ''), '<external-yarn>')]:
        if value:
            content = content.replace(value, replacement)
    content = content.replace(tempfile.gettempdir(), '<temporary-path>')
    tools = [sys.executable, shutil.which('cargo') or sys.executable, shutil.which('node') or sys.executable]
    for tool in tools:
        content = content.replace(str(Path(tool).resolve()), '<tool-executable>')
    prefix = os.path.commonpath([str(Path(tool).parent) for tool in tools])
    if prefix != os.path.sep:
        content = content.replace(prefix, '<toolchain-prefix>')
    log_name = f'{len(records):03d}-{name}.log'
    (evidence / log_name).write_text(content)
    portable_argv = ['$PYTHON' if item == sys.executable else item for item in argv]
    records.append({'sha': sha, 'name': name, 'command': portable_argv, 'exit': result.returncode,
                    'seconds': round(time.time() - started, 2), 'log': log_name,
                    'environment': ['CARGO_BUILD_JOBS=1', 'SAFE_CEX_TYPESCRIPT_PATH and SAFE_CEX_YARN_JS supplied externally']})
    (evidence / 'gates.json').write_text(json.dumps(records, indent=2) + '\n')
    print(name, 'EXIT', result.returncode, flush=True)
failures = [r['name'] for r in records if r['exit'] != 0]
print('COMPLETE', sha, 'FAILED', failures, flush=True)
sys.exit(1 if failures else 0)
