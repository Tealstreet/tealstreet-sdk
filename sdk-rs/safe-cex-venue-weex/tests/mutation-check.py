"""Mutate each production constant; require its named oracle failure, then restore."""
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parent.parent
sdk = crate.parent
source = crate / 'src/types.rs'
original = source.read_text()
cargo = os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo')
command = [cargo, 'test', '--manifest-path', str(sdk / 'Cargo.toml'), '-p',
           'safe-cex-venue-weex', '--jobs', '1', '--test', 'types_oracle',
           'constants_match_actual_typescript', '--', '--exact']
blocks = list(re.finditer(r'pub const (\w+):[\s\S]*?;\n', original))
assert len(blocks) == 26
try:
    for block in blocks:
        name = block[1]
        text = block[0]
        if name == 'RECV_WINDOW':
            changed = text.replace('20_000.0', '20_001.0')
        else:
            # Change a real key/member value, not the oracle or a serialized fixture.
            changed = re.sub(r'"([^"]*)"', lambda m: '"' + m[1] + '_MUTATED"', text, count=1)
        assert changed != text
        source.write_text(original[:block.start()] + changed + original[block.end():])
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode != 0 and f'constant::{name}' in output, output
        print(f'FAIL observed: constant::{name}', flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
subprocess.run(command, check=True)
print('Restored PASS: all 26 constants', flush=True)
