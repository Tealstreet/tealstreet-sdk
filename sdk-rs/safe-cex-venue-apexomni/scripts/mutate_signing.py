"""Prove the retained Contract argument through its oracle and real placement binding."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
workspace = root / 'sdk-rs'
source = workspace / 'safe-cex-venue-apexomni/src/transaction_signing.rs'
original = source.read_bytes()
before = b'            J::Number(payload.pair_id),'
after = b'            J::Number(payload.slot_id),'
assert original.count(before) == 1, 'one reviewed Contract pair argument'
logs = workspace / 'target/apexomni-controls'
logs.mkdir(parents=True, exist_ok=True)


def run(target, name, log):
    command = ['cargo', 'test', '-p', 'safe-cex-venue-apexomni', '--test', target,
               '--jobs', '1', name, '--', '--exact', '--nocapture']
    result = subprocess.run(command, cwd=workspace, capture_output=True,
                            text=True, timeout=600)
    output = result.stdout + result.stderr
    (logs / log).write_text(output)
    return result.returncode, output


cases = [('signing', 'contract_native', 'signContract operations'),
         ('binding', 'bound_place_order', 'complete source/native signed placement wire body')]
try:
    source.write_bytes(original.replace(before, after))
    for target, name, assertion in cases:
        code, output = run(target, name, f'{name}-mutation.log')
        assert code == 101 and 'test result: FAILED.' in output, output
        assert 'could not compile' not in output and assertion in output, output
        print(f'CONTROL PASS: {name} compiled and failed {assertion}', flush=True)
finally:
    source.write_bytes(original)
assert source.read_bytes() == original, 'exact Contract source restoration'
for target, name, _ in cases:
    code, output = run(target, name, f'{name}-restored.log')
    assert code == 0 and '1 passed; 0 failed' in output, output
    print(f'RESTORED PASS: {name}; exact source bytes', flush=True)
