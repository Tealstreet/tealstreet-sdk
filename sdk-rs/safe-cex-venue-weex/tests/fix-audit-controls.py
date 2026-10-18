#!/usr/bin/env python3
"""Run alone: reinstate each original adapter, require named compiled RED, restore."""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
root = sdk.parent
file = sdk / 'safe-cex-venue-weex/src/binding/values.rs'
original = file.read_bytes()
old = subprocess.check_output(['git', 'show', 'a8699f238f:sdk-rs/safe-cex-venue-weex/src/binding/values.rs'], cwd=root).decode()
log_dir = Path(tempfile.gettempdir())
env = dict(os.environ, CARGO_INCREMENTAL="0", SAFE_CEX_TYPESCRIPT_PATH=str(root / 'node_modules/typescript'))

def segment(text, name):
    a = text.index('pub(super) fn ' + name + '(')
    b = text.index('\npub(super) fn ', a + 1)
    return a, b, text[a:b]

def run(label, failures, test_filter='fix_audit_unread'):
    log = log_dir / ('port-txflow-weex-' + label + '.log')
    with log.open('w') as stream:
        result = subprocess.run(['cargo', 'test', '--locked', '-p', 'safe-cex-venue-weex', '--test',
                                 'exchange_binding', test_filter, '--jobs', '1'],
                                cwd=sdk, env=env, stdout=stream, stderr=subprocess.STDOUT)
    text = log.read_text()
    assert 'Finished `test`' in text and 'Running tests/exchange_binding.rs' in text, text
    assert 'could not compile' not in text and 'error[E' not in text, text
    assert result.returncode == (101 if failures else 0), text
    assert text.count(' ... FAILED') == len(failures), text
    for name in failures:
        assert 'test ' + name + ' ... FAILED' in text, text
    if failures:
        assert 'WEEX-AUDIT unread ' in text and 'typed value recursion limit exceeded' in text, text
    print(label + ': ' + str(log), flush=True)

try:
    for name, failures in [
        ('candle', ['fix_audit_unread_deep_candle_property']),
        ('book', ['fix_audit_unread_deep_book_property', 'fix_audit_unread_deep_tuple_tail']),
    ]:
        text = original.decode()
        a, b, _ = segment(text, name)
        if name == "book":
            a, _, _ = segment(text, "book_side")
        _, _, body = segment(old, name)
        file.write_text(text[:a] + body + text[b:])
        try:
            run('original-' + name + '-red', failures)
        finally:
            file.write_bytes(original)
            assert file.read_bytes() == original
        run('restored-' + name + '-green', [])
    run('final-audit-green', [], 'fix_audit')
finally:
    file.write_bytes(original)
    assert file.read_bytes() == original
print('RESTORED sha256=' + hashlib.sha256(original).hexdigest(), flush=True)
