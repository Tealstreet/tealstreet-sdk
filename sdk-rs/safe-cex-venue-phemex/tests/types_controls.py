"""Each control runs the entire single-process Node corpus, sequentially."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/types.rs'
oracle = crate / 'tests/types_oracle.cjs'
original = source.read_text()
oracle_original = oracle.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-phemex', '--test', 'types', '--jobs', '1']
def swapped(a, b):
    left, right = f'#[serde(rename = "{a}")]', f'#[serde(rename = "{b}")]'
    assert original.count(left) == original.count(right) == 2
    return original.replace(left, '__SWAP__').replace(right, left).replace('__SWAP__', right)
controls = [
    ('POSITIONS_RESPONSE account symmetric rename', source, swapped('accountBalanceRv', 'totalUsedBalanceRv'), 'native account.accountBalanceRv'),
    ('POSITIONS_RESPONSE position symmetric rename', source, swapped('avgEntryPriceRp', 'avgEntryPrice'), 'native positions[0].avgEntryPriceRp'),
    ('PhemexApiResponse metadata omission', source, original.replace('    pub code: f64,', '    #[serde(skip_deserializing)]\n    pub code: f64,'), 'envelope text ingress'),
    ('inherited required field census', oracle, oracle_original.replace('const properties = checker.getPropertiesOfType(type);', "const properties = checker.getPropertiesOfType(type).filter(p => p.name !== 'inheritedRequired');"), 'inherited required-field census'),
]
for name, target, mutation, assertion in controls:
    try:
        assert mutation != target.read_text(), name
        target.write_text(mutation)
        result = subprocess.run(command, capture_output=True, text=True)
        assert result.returncode != 0 and assertion in result.stdout + result.stderr, (name, result.stdout + result.stderr)
        print(f'FAIL confirmed: {name}: {assertion}', flush=True)
    finally:
        source.write_text(original)
        oracle.write_text(oracle_original)
    restored = subprocess.run(command, capture_output=True, text=True)
    assert restored.returncode == 0, restored.stdout + restored.stderr
    print(f'PASS restored: {name}', flush=True)
