"""One compiled production mutation for each of nine new runtime assembly rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'runtime_assembly', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/runtime_assembly.rs'
original = path.read_text()
controls = [('request-signer-field', 'undefined_fields(h, &owner, &["sdkAccount", "options"])?;', 'undefined_fields(h, &owner, &["WRONG", "options"])?;', 'signer_native_initial_field'), ('jwt-expiry', 'field(h, &owner, "tokenExpiry", h.number(0.0))?;', 'field(h, &owner, "tokenExpiry", h.number(1.0))?;', 'jwt_class_native_expiry'), ('exchange-leverage-map', 'maps(h, &owner, &["leverageSettings"])?;', 'maps(h, &owner, &["WRONG"])?;', 'exchange_class_native_leverage_map'), ('private-position-throttle', 'field(h, &owner, "positionFetchThrottleMs", h.number(500.0))?;', 'field(h, &owner, "positionFetchThrottleMs", h.number(501.0))?;', 'private_class_native_position_throttle'), ('public-recovery-map', '"ohlcvLastRecoveryAt",', '"WRONG",', 'public_class_native_recovery_map'), ('api-timeout', 'h.number(20_000.0)', 'h.number(20_001.0)', 'api_native_timeout'), ('position-pending-reset', 'h.write(&owner, "hasPendingPositionEvents", h.boolean(false))?;', 'h.write(&owner, "hasPendingPositionEvents", h.boolean(true))?;', 'positions_native_pending_reset'), ('orders-loaded', 'h.write(&loaded, "orders", h.boolean(true))?;', 'h.write(&loaded, "orders", h.boolean(false))?;', 'orders_native_loaded'), ('ohlcv-delay', 'vec![poll, h.number(3000.0)]', 'vec![poll, h.number(3001.0)]', 'ohlcv_native_poll_delay')]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'runtime-assembly-{label}.log').write_text(output)
    return result.returncode, output


LOGS.mkdir(parents=True, exist_ok=True)
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, needle, replacement, assertion in controls:
    assert original.count(needle) == 1, (name, 'mutation site changed')
    try:
        path.write_text(original.replace(needle, replacement))
        mutated, output = run(name)
    finally:
        path.write_text(original)
    record = dict(control=name, assertion=assertion, mutation_exit=mutated,
                  compiled_runtime_failure=mutated == 101 and 'test result: FAILED' in output)
    if mutated == 0:
        report['survivors'].append(name)
    report['controls'].append(record)
    (LOGS / 'runtime-assembly-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'runtime-assembly-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 9
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
