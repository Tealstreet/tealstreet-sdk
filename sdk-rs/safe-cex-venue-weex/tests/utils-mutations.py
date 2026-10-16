"""One production lookup mutation per row; serial scoped tests, whole-corpus oracle."""
from pathlib import Path
import json
import os
import subprocess
crate = Path(__file__).resolve().parent.parent
source = crate / 'src/utils.rs'
original = source.read_text()
command = [os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '1', '--test', 'utils_oracle', '--', '--nocapture']
try:
    for ts, rust, *_ in json.loads((crate / 'tests/utility-enums.json').read_text()):
        start = original.index('pub fn '+rust+'(')
        end = original.index('\n}', start)+2
        block = original[start:end]
        mutated = block.replace('*key == normalized', '*key == normalized && normalized == "__mutation__"')
        assert mutated != block
        source.write_text(original[:start]+mutated+original[end:])
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout+result.stderr
        assertion = f'utility::{ts}::'
        assert result.returncode not in (0,126) and assertion in output and 'assertion `left == right` failed' in output, output
        print('FAIL observed: '+next(line for line in output.splitlines() if 'assertion `left == right` failed' in line), flush=True)
        source.write_text(original)
    mutated = original.replace('deserialize_with = "js_deserialize_passthrough"', 'deserialize_with = "mutated_input"') + "\nfn mutated_input<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Value, D::Error> {\n    let value = js_deserialize_passthrough(d)?;\n    Ok(if value.is_number() { serde_json::json!(0) } else { value })\n}\n"
    source.write_text(mutated)
    result = subprocess.run(command, capture_output=True, text=True)
    output = result.stdout+result.stderr
    assert result.returncode not in (0,126) and 'utility_numeric::-0::route_0' in output and 'assertion `left == right` failed' in output, output
    print('FAIL observed: utility_numeric::-0::route_0', flush=True)
finally:
    source.write_text(original)
subprocess.run(command, check=True)
print('Restored PASS after 7 production lookup mutations and 1 numeric ingress mutation', flush=True)
