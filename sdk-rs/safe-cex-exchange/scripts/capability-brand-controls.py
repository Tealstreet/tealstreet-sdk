from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-exchange/src/capabilities.rs'
output = Path('/tmp/native-capability-brand-controls')
output.mkdir(exist_ok=True)
saved = source.read_bytes()
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-exchange',
           '--test', 'core_contract', 'native_capability_brand_', '--jobs', '1']
mutations = [
    ('cross-method',
     b'matches!(self.origin, Origin::Base(actual) if actual == method)',
     b'matches!(self.origin, Origin::Base(actual) if actual == method || actual == BaseCapabilityMethod::FetchIncomeHistory)',
     'CAPABILITY_BRAND exact method identity'),
    ('branded-override',
     b'matches!(self.origin, Origin::Base(actual) if actual == method)',
     b'matches!(self.origin, Origin::Base(actual) if actual == method) || matches!(self.origin, Origin::Implemented) && method == BaseCapabilityMethod::FetchIncomeHistory',
     'CAPABILITY_BRAND replacement is unbranded'),
]
for name, original, mutated, assertion in mutations:
    assert saved.count(original) == 1, name
    try:
        source.write_bytes(saved.replace(original, mutated))
        with (output / f'{name}.log').open('wb') as log:
            result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
        receipt = (output / f'{name}.log').read_text()
        assert result.returncode == 101 and 'Finished `test` profile' in receipt, receipt
        assert assertion in receipt, receipt
        print(f'{name}: compiled named RED', flush=True)
    finally:
        source.write_bytes(saved)
        assert source.read_bytes() == saved
    with (output / f'{name}-restored.log').open('wb') as log:
        result = subprocess.run(command, cwd=workspace, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, (output / f'{name}-restored.log').read_text()
    print(f'{name}: exact restoration GREEN', flush=True)
