"""Serial owner-specific guard controls; run only on accepted review branch."""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/response_types.rs'
original = source.read_bytes()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=str(root / 'node_modules/typescript'))
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-weex', '--test', 'accepted_responses', '--jobs', '1']

def run(label):
    result = subprocess.run(command, cwd=root / 'sdk-rs', env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log = root / 'sdk-rs/target/weex-accepted-response-controls' / (label+'.log')
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_bytes(result.stdout)
    return result.returncode, result.stdout.decode(errors='replace'), log

try:
    for owner, marker in [('OrderResultWire','accepted_order_result::object_guard'), ('ExchangeInfoWire','accepted_exchange_info::object_guard')]:
        before = f'{owner}::deserialize(safe_cex_types::strict_decode::object(decoder))'.encode()
        after = f'{owner}::deserialize(decoder)'.encode()
        assert original.count(before) == 1
        source.write_bytes(original.replace(before, after))
        code, text, log = run(owner+'-guard-removed')
        assert code == 101 and marker in text and 'could not compile' not in text, (code, log)
        print(owner, 'compiled guard-removal failure:', marker, flush=True)
        source.write_bytes(original)
        code, _, log = run(owner+'-guard-restored')
        assert code == 0, log
        print(owner, 'restored native target passed', flush=True)
finally:
    source.write_bytes(original)
