from pathlib import Path
import subprocess

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-venue-polymarket/src/binding/mod.rs'
original = source.read_bytes()
anchor = b'Ok(rt.spawn(body(args)))'
assert original.count(anchor) == 1
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-polymarket', '--lib', 'native_pin_task_settlement_preserves_arguments_and_original_values', '--jobs', '1']
try:
    source.write_bytes(original.replace(anchor, b'Ok(rt.spawn(body(args.into_iter().take(0).collect())))'))
    failed = subprocess.run(command, cwd=sdk, capture_output=True, text=True)
    Path('/tmp/polymarket-task-forwarding-control.log').write_text(failed.stdout + failed.stderr)
    assert failed.returncode == 101, failed.stdout + failed.stderr
    assert 'error[E' not in failed.stderr and 'could not compile' not in failed.stderr
    assert 'PinTask native argument forwarding' in failed.stdout
finally:
    source.write_bytes(original)
restored = subprocess.run(command, cwd=sdk, capture_output=True, text=True)
Path('/tmp/polymarket-task-forwarding-restored.log').write_text(restored.stdout + restored.stderr)
assert restored.returncode == 0, restored.stdout + restored.stderr
assert '1 passed' in restored.stdout
print('Production PinTask argument-forwarding control compiled RED; exact restoration GREEN.')
