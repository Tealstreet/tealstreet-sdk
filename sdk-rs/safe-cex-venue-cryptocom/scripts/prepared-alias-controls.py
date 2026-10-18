from pathlib import Path
import subprocess

package = Path(__file__).resolve().parents[1]
source = package / 'src/native_bindings.rs'
original = source.read_bytes()
text = original.decode()
name = 'production_prepared_pair_captures_method_once_and_keeps_receiver_and_native_payload'
command = ['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-venue-cryptocom', '--lib', name, '--jobs', '1']
faults = [('prepared member order', 'Ok((self.get(&receiver, key)?, receiver))', 'Ok((receiver.clone(), self.get(&receiver, key)?))'),
          ('original receiver', '(self.ops.value.invoke)(prepared.0, prepared.1, args)', '(self.ops.value.invoke)(prepared.0.clone(), prepared.0, args)')]
try:
    for label, before, after in faults:
        assert text.count(before) == 1
        source.write_text(text.replace(before, after, 1))
        result = subprocess.run(command, cwd=package.parent, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101, (label, output)
        assert 'error[E' not in output, output
        assert name in output and 'test result: FAILED' in output, output
        print(label + ': actual BoundHost compiled RED')
        source.write_bytes(original)
        assert source.read_bytes() == original
finally:
    source.write_bytes(original)
result = subprocess.run(command, cwd=package.parent, capture_output=True, text=True)
assert result.returncode == 0, result.stdout + result.stderr
assert source.read_bytes() == original
print('exact restoration: prepared pair GREEN')
