#!/usr/bin/env python3
"""Compile production regressions with each boundary reverted, then restore exactly."""
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / 'safe-cex-venue-coinbase'


def run(test, name):
    return subprocess.run(
        ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-coinbase', '--test', test,
         name, '--', '--exact', '--nocapture'], cwd=ROOT,
        env=os.environ.copy(), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )


cases = [
    ('src/portfolios_balances.rs',
     'pub type Portfolio = Rc<RefCell<PortfolioState>>;',
     'pub type Portfolio = Rc<RefCell<crate::types::ListPortfoliosResponsePortfoliosItem>>;',
     'exchange_binding', 'portfolios_do_not_require_unobserved_name_or_deleted_fields',
     'unused portfolio metadata must not reject selection'),
    ('src/binding/lifecycle.rs',
     '&[serde_json::json!(message), serde_json::json!({"forceToast": true})]',
     '&[serde_json::json!(message)]',
     'private_binding_frames', 'real_private_write_timeout_emits_forced_toast_once_until_recovery',
     'production warning must retain the second emitter argument'),
]

for relative, before, after, test, name, label in cases:
    source = CRATE / relative
    original = source.read_bytes()
    text = original.decode()
    # rustfmt may split the emitter argument array across lines.
    if before not in text and relative.endswith('lifecycle.rs'):
        before = '''&[
                            serde_json::json!(message),
                            serde_json::json!({"forceToast": true}),
                        ]'''
    assert text.count(before) == 1, (relative, before)
    baseline = run(test, name)
    assert baseline.returncode == 0, baseline.stdout
    try:
        source.write_text(text.replace(before, after))
        result = run(test, name)
        print(result.stdout, flush=True)
        assert result.returncode == 101 and 'test result: FAILED' in result.stdout and label in result.stdout, result.stdout
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    restored = run(test, name)
    assert restored.returncode == 0, restored.stdout
    print(f'CONTROL {relative}: compiled named failure / exact-restored PASS', flush=True)
