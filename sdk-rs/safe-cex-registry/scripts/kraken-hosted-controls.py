"""Require native Kraken witnesses to reject compiling signer/account omissions."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HOST = ROOT / 'safe-cex-registry/src/native_host.rs'
SOCKET = ROOT / 'safe-cex-venue-kraken/src/binding/sockets.rs'


def run():
    r = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-q', '-p',
        'safe-cex-registry', '--test', 'native_host_kraken_signer'],
        cwd=ROOT, capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def passing():
    code, output = run()
    assert code == 0 and 'test result: ok. 3 passed' in output, output


passing()
original = HOST.read_bytes()
start = original.index(b'    fn kraken(&self)')
end = original.index(b'    fn binance', start)
part = original[start:end]
assert part.count(b'self.remote.clone()') == 1
try:
    HOST.write_bytes(original[:start] + part.replace(b'self.remote.clone()', b'None') + original[end:])
    code, output = run()
    assert code == 101 and 'test result: FAILED' in output and 'krakenspot_native_host_signs' in output and 'kraken_native_host_signs' in output, output
finally:
    HOST.write_bytes(original)
passing()
print('Kraken + Spot hosted HTTP: compiling omitted signer fails101; restored3 pass')

original = SOCKET.read_bytes()
old = b'.and_then(|options| options.get("remoteSigner"))'
assert original.count(old) == 1
try:
    SOCKET.write_bytes(original.replace(old, b'.and_then(|options| options.get("missingRemoteSigner"))'))
    code, output = run()
    assert code == 101 and 'kraken_native_private_socket_uses_the_hosted_account_for_challenge_signing' in output and 'test result: FAILED' in output, output
finally:
    SOCKET.write_bytes(original)
passing()
print('Kraken native WS: compiling stale/missing signer options fails101; restored3 pass')
