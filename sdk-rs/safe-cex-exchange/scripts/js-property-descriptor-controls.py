from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/js_runtime/value.rs'
output = Path('/tmp/js-property-descriptor-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
original = b'Prop::Accessor { configurable, .. } => *configurable = false,'
mutated = b'Prop::Accessor { configurable, .. } => *configurable = true,'
assert saved.count(original) == 1
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
           '--test', 'js_property_descriptors', '--jobs', '1']
environment = dict(os.environ, CARGO_INCREMENTAL='0')
try:
    source.write_bytes(saved.replace(original, mutated))
    with (output / 'accessor-configurable.log').open('wb') as log:
        result = subprocess.run(command, cwd=workspace, env=environment,
                                stdout=log, stderr=subprocess.STDOUT)
    receipt = (output / 'accessor-configurable.log').read_text()
    assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
    assert 'source_accessor_flags_identity_and_frozen_setter_match_node ... FAILED' in receipt, receipt
    print('Compiled accessor-configurability mutation: named RED', flush=True)
finally:
    source.write_bytes(saved)
    assert source.read_bytes() == saved
with (output / 'restored.log').open('wb') as log:
    result = subprocess.run(command, cwd=workspace, env=environment,
                            stdout=log, stderr=subprocess.STDOUT)
assert result.returncode == 0, (output / 'restored.log').read_text()
print('Exact restoration: native/source property witnesses GREEN', flush=True)
