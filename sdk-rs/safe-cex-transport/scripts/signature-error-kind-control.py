#!/usr/bin/env python3
"""The original message-only wrapper must fail, then exact restoration must pass."""
from pathlib import Path
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-transport/src/signature_rejection.rs'
ORIGINAL = SOURCE.read_bytes()
KEEPS_KIND = 'RestFailure::from_transport(crate::TransportError::JavaScript(error), request.clone())'
DROPS_KIND = 'RestFailure::pre_transmit(error.message.into(), request.clone())'
TESTS = [
    'rejected_error_keeps_predicate_type_error_and_failed_request',
    'resolved_resend_keeps_predicate_type_error_and_effective_config',
    'retry_engine_keeps_predicate_type_error_for_read_and_write',
]


def run(restored):
    env = os.environ.copy()
    env['SAFE_CEX_TYPESCRIPT_PATH'] = str(ROOT / 'node_modules/typescript')
    done = subprocess.run(
        ['cargo', 'test', '--locked', '--jobs', '1', '-p', 'safe-cex-transport',
         '--test', 'signature_error_kind', '--', '--include-ignored'],
        cwd=ROOT / 'sdk-rs', env=env, capture_output=True, text=True,
    )
    output = done.stdout + done.stderr
    name = 'restored' if restored else 'disconnected'
    log = Path('/tmp/transport-signature-error-kind-' + name + '.log')
    log.write_text(output)
    assert 'could not compile' not in output, log
    if restored:
        assert done.returncode == 0 and '4 passed; 0 failed' in output, log
    else:
        assert done.returncode == 101 and '1 passed; 3 failed' in output, log
        assert all(f'test {test} ... FAILED' in output for test in TESTS), log
        assert 'signature predicate TypeError must retain its native JavaScript error kind: None' in output, log
        assert 'test maintained_source_throws_type_error_before_stamping_or_resending ... ok' in output, log
    print(name + ': ' + ('4 native/source PASS' if restored else '3 compiled named failures; source witness PASS'), flush=True)


source = ORIGINAL.decode()
assert source.count(KEEPS_KIND) == 1
try:
    SOURCE.write_text(source.replace(KEEPS_KIND, DROPS_KIND))
    run(False)
finally:
    SOURCE.write_bytes(ORIGINAL)
run(True)
assert SOURCE.read_bytes() == ORIGINAL
print('Original error-kind loss proved; exact source restoration passed', flush=True)
