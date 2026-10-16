"""Remove one concrete object guard at a time; require native runtime rejection.

Run from the repository root. Production source is restored after every control;
compiler failures never count as a passing mutation. Logs remain outside Git.
"""

from pathlib import Path
import json, os, re, subprocess

source = Path('sdk-rs/safe-cex-venue-grvt/src/records.rs')
tests = Path('sdk-rs/safe-cex-venue-grvt/tests/accepted_records.rs')
original = source.read_bytes()
text = original.decode()
selected = re.findall(r'fn (\w+_requires_object_shape)\(\)\s*\{\s*check::<(\w+)>', tests.read_text())
assert len(selected) == 25, selected
logs = Path(os.environ.get('GRVT_ACCEPTED_CONTROL_LOGS', '/tmp/grvt-map-controls'))
logs.mkdir(exist_ok=True)
env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
command = ['cargo', '+1.99.0', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-grvt', '--test', 'accepted_records']
receipts = []
try:
    for test, owner in selected:
        marker = "impl<'de> Deserialize<'de> for " + owner + ' {'
        begin = text.index(marker)
        first = text.index('{', begin)
        depth = 1
        end = first + 1
        while depth:
            if text[end] == '{': depth += 1
            elif text[end] == '}': depth -= 1
            end += 1
        body = text[begin:end]
        changed, count = re.subn(r'safe_cex_types::strict_decode::object\(\s*(deserializer|decoder),?\s*\)', r'\1', body)
        assert count == 1, (owner, count)
        candidate = (text[:begin] + changed + text[end:]).encode()
        try:
            source.write_bytes(candidate)
            with (logs / (owner + '.log')).open('w') as output:
                result = subprocess.run(command + [test, '--', '--exact'], env=env, stdout=output, stderr=subprocess.STDOUT)
            log = (logs / (owner + '.log')).read_text()
            assert result.returncode == 101, (owner, result.returncode, log[-2500:])
            named = 'GRVT MAP ' + owner + ' accepted complete positional array via text'
            assert named in log and '0 passed; 1 failed' in log, (owner, log[-3000:])
            receipts.append({'owner':owner, 'test':test, 'failure':named, 'exit':result.returncode, 'nativeIdentityBeforeFailure':True})
            print(f'{len(receipts)}/25 {owner}: compiled complete array admitted with native identity, named failure', flush=True)
        finally:
            source.write_bytes(original)
            assert source.read_bytes() == original
    with (logs / 'restored-green.log').open('w') as output:
        result = subprocess.run(command, env=env, stdout=output, stderr=subprocess.STDOUT)
    restored = (logs / 'restored-green.log').read_text()
    assert result.returncode == 0 and '25 passed; 0 failed' in restored, restored[-5000:]
    (logs / 'receipts.json').write_text(json.dumps(receipts, indent=2)+'\n')
    print('25 individual guard-only mutations fail after native field validation; exact source restored and all25 pass.', flush=True)
finally:
    source.write_bytes(original)
    assert source.read_bytes() == original
