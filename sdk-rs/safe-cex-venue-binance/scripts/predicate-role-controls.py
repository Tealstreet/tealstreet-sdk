#!/usr/bin/env python3
"""Prove native witnesses for three individually reviewed Boolean predicates."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-venue-binance/src/order_logic.rs'
original = source.read_bytes()
cases = [
    ('is_binance_position_mode_already_set_error',
     'position_mode_error_is_a_strict_boolean_recognizer',
     b'\n    let _ = error;\n    false\n',
     ['assertion failed:', 'is_binance_position_mode_already_set_error']),
    ('detect_balance_issue',
     'deprecated_balance_predicate_ignores_account_data',
     b'\n    true\n',
     ['assertion failed:', '!detect_balance_issue(None)']),
    ('is_post_only_rejection_error',
     'post_only_predicate_preserves_code_and_message_precedence',
     b'\n    let _ = error;\n    Ok(false)\n',
     ['left: false', 'right: true']),
]


def run(test):
    result = subprocess.run(
        ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-binance', '--jobs', '1',
         '--test', 'accepted_predicate_roles', test, '--', '--exact'],
        cwd=workspace, capture_output=True, text=True, timeout=600,
    )
    return result, result.stdout + result.stderr


try:
    for owner, test, replacement, expected in cases:
        marker = ('pub fn ' + owner + '(').encode()
        assert original.count(marker) == 1, owner
        start = original.index(b'{', original.index(marker))
        end = start + 1
        depth = 1
        # Reviewed source bodies have no braces in comments or string literals.
        while depth:
            depth += (original[end] == ord('{')) - (original[end] == ord('}'))
            end += 1
        source.write_bytes(original[:start + 1] + replacement + original[end - 1:])
        result, output = run(test)
        assert (result.returncode == 101
                and f'test {test} ... FAILED' in output
                and all(needle in output for needle in expected)
                and 'error[E' not in output), output[-6000:]
        print(f'CONTROL PASS: {owner} compiled; incorrect Boolean result fails its named native witness', flush=True)
        source.write_bytes(original)
        restored, output = run(test)
        assert restored.returncode == 0 and '1 passed' in output, output[-6000:]
        print(f'RESTORED PASS: {owner}', flush=True)
finally:
    source.write_bytes(original)
