from pathlib import Path
import os
import subprocess
import sys

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/library.rs'
if len(sys.argv) > 1 and sys.argv[1] == 'thenable-resolution':
    source = workspace / 'safe-cex-exchange/src/js_runtime/promise.rs'
output = Path('/tmp/js-series-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
cases = [
    ('input-await', b'await_js(Js::Promise(Promise::resolve(&runtime, array.get(&key)?)))', b'std::future::ready(Ok::<_, Thrown>(array.get(&key)?))',
     'installed_series_helpers_match_package_live_slots_awaits_and_receivers', 'SERIES_SOURCE'),
    ('module-root', b'NATIVE_MODULE_ROOTS: [&str; 1] = ["p-iteration"]',
     b'NATIVE_MODULE_ROOTS: [&str; 1] = ["not-loaded"]',
     'series_module_roots_survive_export_removal_and_release_with_scope', 'SERIES_MODULE'),
]
if len(sys.argv) > 1 and sys.argv[1] == 'thenable-resolution':
    cases = [('thenable-resolution', b'resolve_promise(promise.clone(), value);',
              b'promise.settle(Ok(value));',
              'installed_series_helpers_match_package_live_slots_awaits_and_receivers', 'SERIES_SOURCE')]
for label, original, mutated, test, assertion in cases:
    assert saved.count(original) == 1, label
    command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
               '--features', 'native', '--test', 'js_series_iteration', test, '--jobs', '1']
    try:
        source.write_bytes(saved.replace(original, mutated))
        with (output / (label + '.log')).open('wb') as log:
            result = subprocess.run(command, cwd=workspace, env=dict(os.environ),
                                    stdout=log, stderr=subprocess.STDOUT)
        receipt = (output / (label + '.log')).read_text()
        assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
        assert assertion in receipt, receipt
        print(label + ': compiled named RED', flush=True)
    finally:
        source.write_bytes(saved)
        assert source.read_bytes() == saved
    with (output / (label + '-restored.log')).open('wb') as log:
        result = subprocess.run(command, cwd=workspace, env=dict(os.environ),
                                stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, (output / (label + '-restored.log')).read_text()
    print(label + ': exact restoration GREEN', flush=True)
