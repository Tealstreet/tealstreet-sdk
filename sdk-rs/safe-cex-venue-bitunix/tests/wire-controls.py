"""Slice-readiness controls, serial Cargo runs; each oracle runs one full corpus."""
import os
from pathlib import Path
import subprocess
import re

root = Path(__file__).resolve().parents[3]
crate = root / 'sdk-rs/safe-cex-venue-bitunix'
source = crate / 'src/wire.rs'
oracle = crate / 'tests/wire-oracle.cjs'
originals = {source: source.read_text(), oracle: oracle.read_text()}
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-bitunix', '--jobs', '1', '--test', 'wire', '--', '--nocapture']
logs = root / 'sdk-rs/target/bitunix-controls'
logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
def swap(text, a, b):
    assert a in text and b in text
    return text.replace(a, '__SWAP__').replace(b, a).replace('__SWAP__', b)
controls = [
    (source, swap(originals[source], 'rename = "ctime"', 'rename = "mtime"'), 'position decoded fields "all fields"'),
    (source, swap(originals[source], '"1min"', '"3min"'), 'interval decoded variant'),
    (source, originals[source].replace('T::deserialize(decoder).map(Some)', 'Option::<T>::deserialize(decoder)'), 'position shape "null maxQty"'),
    (source, originals[source].replace('.map_err(serde::de::Error::custom)', '.map(|mut row| { if row.ctime == 0.0 { row.ctime = 0.0; } row }).map_err(serde::de::Error::custom)'), 'position shape "bits ctime -0"'),
    (oracle, originals[oracle].replace('return checker.getPropertiesOfType(type);', "return checker.getPropertiesOfType(type).filter(p => p.name !== 'inherited');"), 'inherited required-field census'),
]
# Reintroduce each direct-Serde enum bug independently; the record's nested
# numeric guard must not conceal these public decoding routes.
for name, assertion in [
    ('PendingPositionSide', 'position-side direct string shape'),
    ('PendingMarginMode', 'margin-mode direct string shape'),
    ('PendingPositionMode', 'position-mode direct string shape'),
    ('KlineInterval', 'interval direct string shape'),
]:
    text = originals[source]
    pattern = r"impl<'de> Deserialize<'de> for " + name + r" \{.*?\n\}"
    text, count = re.subn(pattern, '', text, flags=re.S)
    assert count == 1, name
    old = '#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]\npub enum ' + name
    assert old in text, name
    text = text.replace(old, old.replace('Serialize)', 'Serialize, Deserialize)'))
    controls.append((source, text, assertion))
try:
    for index, (file, changed, assertion) in enumerate(controls):
        assert changed != originals[file], assertion
        file.write_text(changed)
        result = subprocess.run(command, capture_output=True, text=True)
        (logs / f'wire-{index}-fail.log').write_text(result.stdout + result.stderr)
        assert result.returncode != 0 and assertion in result.stdout + result.stderr, (assertion, result.stdout, result.stderr)
        print('FAIL observed: ' + assertion, flush=True)
        file.write_text(originals[file])
        result = subprocess.run(command, capture_output=True, text=True)
        (logs / f'wire-{index}-pass.log').write_text(result.stdout + result.stderr)
        assert result.returncode == 0, result.stdout + result.stderr
        print('PASS restored: ' + assertion, flush=True)
finally:
    for file, original in originals.items():
        file.write_text(original)
