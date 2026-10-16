"""One compiled production control for each new SDK/private-auth row."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'private_auth', '--test', 'signing', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('signer-constructor', 'auth_operations.rs', 'write_account(account)?;\n    write_options(options)\n}', 'write_options(options)?;\n    write_account(account)\n}', 'signer_constructor_native_write_order'),
    ('jwt-constructor', 'auth_operations.rs', 'write_base_url(if testnet {', 'write_base_url(if !testnet {', 'jwt_constructor_native_url'),
    ('jwt-expiry', 'auth_operations.rs', 'write_expiry(now + 300_000.0)?;', 'write_expiry(now + 300_001.0)?;', 'token_native_expiry'),
    ('private-open', 'auth_operations.rs', 'write_authenticated(false)?;', 'write_authenticated(true)?;', 'private_open_native_authenticated'),
    ('authenticate-id', 'auth_operations.rs', 'write_id(id + 1.0)?;', 'write_id(id + 2.0)?;', 'authenticate_native_counter'),
    ('quantum-scale', 'signing_operations.rs', 'multiply(number, 1e8)?', 'multiply(number, 1e7)?', 'quantum_native_scale'),
    ('order-side', 'signing_operations.rs', 'let side = if read_buy()? { "1" } else { "2" };', 'let side = if read_buy()? { "2" } else { "1" };', 'order_native_side'),
    ('modify-side', 'signing_operations.rs', 'let side = if read_buy()? { "1" } else { "2" };', 'let side = if read_buy()? { "2" } else { "1" };', 'modify_native_side'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'private-auth-{label}.log').write_text(output)
    return result.returncode, output


LOGS.mkdir(parents=True, exist_ok=True)
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, filename, needle, replacement, assertion in controls:
    path = CRATE / 'src' / filename
    original = path.read_text()
    if name in ['order-side', 'modify-side']:
        split = original.index('pub async fn sign_modify_order')
        prefix, suffix = original[:split], original[split:]
        segment = prefix if name == 'order-side' else suffix
        assert segment.count(needle) == 1, (name, 'mutation site changed')
        segment = segment.replace(needle, replacement)
        mutated_source = segment + suffix if name == 'order-side' else prefix + segment
    else:
        assert original.count(needle) == 1, (name, 'mutation site changed')
        mutated_source = original.replace(needle, replacement)
    try:
        path.write_text(mutated_source)
        mutated, output = run(name)
    finally:
        path.write_text(original)
    record = dict(control=name, assertion=assertion, mutation_exit=mutated,
                  compiled_runtime_failure=mutated == 101 and 'test result: FAILED' in output)
    if mutated == 0:
        report['survivors'].append(name)
    report['controls'].append(record)
    (LOGS / 'private-auth-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutation_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'private-auth-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutation_output)
    assert assertion in mutation_output, (name, mutation_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 8
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
