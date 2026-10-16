"""One subprocess per full corpus, sequential mutation and restoration."""
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/utilities.rs'
original = source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-phemex', '--test', 'utilities', '--locked', '--jobs', '1']
controls = [
    ('calculatePriceDiff', 'side == PositionSide::Long', 'side != PositionSide::Long', 'price difference native bits'),
    ('formatSymbolIn', '.to_uppercase()', '.to_lowercase()', 'native symbol uppercase'),
    ('formatSymbolOut numeric type', 'found.expect("truthy value exists").clone()', 'if found.and_then(safe_cex_num::numeric_ingress::js_value_as_f64).is_some_and(f64::is_infinite) { serde_json::Value::String("Infinity".into()) } else { found.expect("truthy value exists").clone() }', 'native symbol output numeric type'),
    ('formatSymbolOut', '.find(|market|', '.rfind(|market|', 'native symbol output first match'),
]
for name, before, after, assertion in controls:
    try:
        assert original.count(before) == 1
        source.write_text(original.replace(before, after))
        result = subprocess.run(command, capture_output=True, text=True)
        assert result.returncode not in (0, 126) and assertion in result.stdout + result.stderr, result.stdout + result.stderr
        print(f'FAIL confirmed: {name}: {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command, capture_output=True, text=True)
    assert restored.returncode == 0, restored.stdout + restored.stderr
    print(f'PASS restored: {name}', flush=True)
