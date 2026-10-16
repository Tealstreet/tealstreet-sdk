"""Run source oracles without repository node_modules; preserve named failures."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
compiler = os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert compiler, 'external isolated TypeScript path required'
with tempfile.TemporaryDirectory(prefix='cryptocom-source-') as directory:
    mirror = Path(directory)
    assert not any((p / 'node_modules').exists() for p in [mirror, *mirror.parents])
    for relative in ['packages/safe-cex/src/exchanges/cryptocom', 'sdk-rs/safe-cex-venue-cryptocom/tests']:
        shutil.copytree(root / relative, mirror / relative, ignore=shutil.ignore_patterns('__pycache__'))
    for relative in ['packages/safe-cex/src/types.ts', 'packages/logging/src/LogSeverity.ts',
                     'packages/safe-cex/src/utils/createPositionId.ts', 'packages/safe-cex/src/utils/orderbook.ts']:
        destination = mirror / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root / relative, destination)
    tests = mirror / 'sdk-rs/safe-cex-venue-cryptocom/tests'
    records = []
    def run(name, script, expected, assertion=None, env=None):
        result = subprocess.run(['node', str(tests / script)], env=env or os.environ.copy(),
                                capture_output=True, text=True)
        assert result.returncode == expected, (name, result.returncode, result.stderr)
        if assertion:
            assert assertion in result.stderr, (name, result.stderr)
        if expected == 0:
            rows = json.loads(result.stdout)['rows']
            floor = {'runtime_oracle.cjs': 70, 'order_writes_oracle.cjs': 91, 'bridge_oracle.cjs': 65}[script]
            assert len(rows) == floor, name
        records.append({'name': name, 'exit': result.returncode, 'named_assertion': assertion})
    run('runtime-isolated', 'runtime_oracle.cjs', 0)
    run('bridge-isolated', 'bridge_oracle.cjs', 0)
    bridge_mutated = os.environ.copy()
    bridge_mutated['SAFE_CEX_CRYPTOCOM_BRIDGE_SOURCE_MUTATION'] = 'getAccount'
    run('bridge-source-noop-mutated', 'bridge_oracle.cjs', 1, 'source original failure exchange:getAccount', bridge_mutated)
    run('bridge-source-noop-restored', 'bridge_oracle.cjs', 0)
    run('order-writes-isolated', 'order_writes_oracle.cjs', 0)
    mutated = os.environ.copy()
    mutated['SAFE_CEX_CRYPTOCOM_RUNTIME_MUTATION'] = 'timeout'
    run('source-timeout-mutated', 'runtime_oracle.cjs', 1, 'CRYPTOCOM-RUNTIME-SOURCE-API-TIMEOUT', mutated)
    run('source-timeout-restored', 'runtime_oracle.cjs', 0)
    package_script = tests / 'order_writes_oracle.cjs'
    original = package_script.read_bytes()
    try:
        package_script.write_text(original.decode().replace("'p-iteration'", "'cryptocom-missing-package-control'"))
        run('missing-package-mutated', 'order_writes_oracle.cjs', 1, 'MODULE_NOT_FOUND')
    finally:
        package_script.write_bytes(original)
    run('missing-package-restored', 'order_writes_oracle.cjs', 0)
    missing = os.environ.copy()
    missing.pop('SAFE_CEX_TYPESCRIPT_PATH', None)
    run('missing-compiler', 'runtime_oracle.cjs', 1, 'compiler path required', missing)
    print(json.dumps({'sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
                      'dependency_resolution': 'external isolated compiler install; no node_modules ancestor',
                      'floor': 10, 'stop': 'index9 missing-compiler', 'controls': records}, indent=2))
