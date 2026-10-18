from pathlib import Path
import subprocess

package = Path(__file__).resolve().parents[1]
source = package / 'src/binding/signer.rs'
original = source.read_bytes()
text = original.decode()
name = 'native_contract_handle_preserves_payload_identity_and_releases_after_last_alias'
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-venue-apexomni', '--lib', name, '--jobs', '1']
faults = [('payload field', 'size: text(5)?,', 'size: text(6)?,'),
          ('opaque referent', 'Ok(builder)', 'Ok(Rc::new(()))')]
try:
    for label, before, after in faults:
        assert text.count(before) == 1
        source.write_text(text.replace(before, after, 1))
        result = subprocess.run(command, cwd=package.parent, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101, (label, output)
        assert 'error[E' not in output, output
        assert name in output and 'test result: FAILED' in output, output
        print(label + ': actual NativeSigner compiled RED')
        source.write_bytes(original)
        assert source.read_bytes() == original
finally:
    source.write_bytes(original)
result = subprocess.run(command, cwd=package.parent, capture_output=True, text=True)
assert result.returncode == 0, result.stdout + result.stderr
assert source.read_bytes() == original
print('exact restoration: native handle GREEN')
