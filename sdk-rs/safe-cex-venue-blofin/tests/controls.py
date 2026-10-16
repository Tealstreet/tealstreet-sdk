"""Sequential fail/restore/pass controls; run with python3.14 from the worktree."""
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-blofin'
CARGO = '/opt/homebrew/opt/rustup/bin/cargo'
COMMAND = [CARGO, 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-blofin', '--jobs', '1', '--test', 'types']
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH required'

def run():
    return subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

def control(file, change, assertion):
    original = file.read_text()
    mutated = change(original)
    assert original != mutated, f'{assertion}: mutation did not apply'
    try:
        file.write_text(mutated)
        result = run()
        assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
        print(f'FAIL proved: {assertion}; exit={result.returncode}', flush=True)
    finally:
        file.write_text(original)
    result = run()
    assert result.returncode == 0, result.stdout
    print(f'PASS restored: {assertion}', flush=True)

rust = CRATE / 'src/types.rs'
node = CRATE / 'tests/types_oracle.cjs'
control(rust, lambda s: s.replace('20000.0', '20001.0', 1), 'constant_RECV_WINDOW')
# Swap both Serialize and Deserialize names: a roundtrip still agrees.
def swap(s):
    start = s.index('pub struct FetchPositionResponseData')
    end = s.index('impl JsNumberOutput for FetchPositionResponseData', start)
    return s[:start] + s[start:end].replace('"positionId"', '"__swap__"').replace('"instId"', '"positionId"').replace('"__swap__"', '"instId"') + s[end:]
control(rust, swap, 'decoded_FetchPositionResponseData_position_id')
control(node, lambda s: s.replace("assert(!node.heritageClauses?.length, 'inherited_fields_refused');", '// deliberately broken heritage guard'), 'heritage_control')
# The strict map-only wrapper must refuse positional records at every level.
control(rust, lambda s: s.replace('safe_cex_types::strict_decode::object(d)', 'd'), 'record_boundary')
control(rust, lambda s: s.replace('T::deserialize(d).map(Some)', 'Option::<T>::deserialize(d)'), 'record_boundary')
