#!/usr/bin/env python3
import pathlib
import subprocess

workspace = pathlib.Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-bingx/src/public_stream.rs'
original = source.read_bytes()
old = b'.map(|(_, handlers)| handlers.clone());'
new = b'.map(|(_, handlers)| Rc::new(RefCell::new(handlers.borrow().clone())));'
assert original.count(old) == 1, 'production dispatch capture anchor changed'
command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-bingx', '--lib', 'native_callback_registry_keeps_live_lists_and_original_errors', '--', '--nocapture']

def run():
    return subprocess.run(command, cwd=workspace, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

try:
    source.write_bytes(original.replace(old, new))
    failed = run()
finally:
    source.write_bytes(original)
assert source.read_bytes() == original
restored = run()
print(failed.stdout)
print(restored.stdout)
assert failed.returncode != 0 and 'error[E' not in failed.stdout and 'could not compile' not in failed.stdout, 'control must compile before native refusal'
assert 'native_callback_registry_keeps_live_lists_and_original_errors' in failed.stdout and 'test result: FAILED' in failed.stdout
assert 'actual dispatch retains the captured list' in failed.stdout, 'control must reach the callback-registry assertion'
assert restored.returncode == 0 and '1 passed' in restored.stdout
print('BingX captured-list fault: compiled named RED; exact source restored GREEN')
