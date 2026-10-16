"""One compiled production mutation for each of eight new authentication initialization rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'auth_initialization', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/auth_initialization.rs'
original = path.read_text()
controls = [
 ('history-error-name','name(&parent, "ParadexPrivateHistoryAuthNotReadyError")?;','name(&parent, "WrongHistoryAuthError")?;','history_error_native_name'),
 ('history-predicate','    instance()\n}','    Ok(!instance()?)\n}','history_predicate_native_brand'),
 ('wait-delay','schedule(resolve, ms)','schedule(resolve, ms + 1.0)','wait_native_delay'),
 ('sdk-environment','"prod"','"WRONG"','sdk_native_environment'),
 ('exchange-manager','h.write_state("jwtManager", manager)?;','h.write_state("jwtManager", if h.truthy(&manager) { h.null() } else { manager })?;','exchange_init_native_manager'),
 ('history-retry-delay','[500.0, 1500.0]','[501.0, 1500.0]','history_auth_native_delay_values'),
 ('validation-success','("error", h.literal(""))','("error", h.literal("WRONG"))','validation_native_error_field'),
 ('request-auth-header','"Authorization", authorization','"WrongAuthorization", authorization','request_auth_native_headers'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'auth-initialization-{label}.log').write_text(output)
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
    (LOGS / 'auth-initialization-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'auth-initialization-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 8
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
