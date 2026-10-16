"""Five production controls and one native record-construction control."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'host_helpers', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=
           '/Users/sam/cs/tealstreet-next/node_modules/typescript')
files = ['src/batch_predicate.rs', 'src/subscription_delegates.rs',
         'src/sdk_state.rs', 'tests/host_helpers.rs']
original = {file: (CRATE / file).read_text() for file in files}
controls = []
controls.append(('predicate', 'production', 'src/batch_predicate.rs',
                 original['src/batch_predicate.rs'].replace('        && opts.trigger_price.is_none()\n', ''), 'batch_native_value'))
for name, fn, arg, generics in [('book','listen_order_book','symbol','S'),('ohlcv','listen_ohlcv','opts','O')]:
    source = original['src/subscription_delegates.rs']
    start = source.index(f'pub fn {fn}<')
    end = source.index('\n}', start) + 2
    body = source[start:end].replace(f'<{generics}, C, R, E>', f'<{generics}: Clone, C: Clone, R, E>')
    body = body.replace('listen: impl FnOnce', 'mut listen: impl FnMut')
    body = body.replace(f'    listen({arg}, callback)', f'    listen({arg}.clone(), callback.clone())?;\n    listen({arg}, callback)')
    controls.append((name, 'production', 'src/subscription_delegates.rs', source[:start] + body + source[end:], name + '_native_calls'))
source = original['src/sdk_state.rs']
address = source.replace('get_account_address<A, T, E>', 'get_account_address<A: Clone, T, E>')
address = address.replace('    read_address(read_account()?)', '    let account = read_account()?;\n    read_address(account.clone())?;\n    read_address(account)')
controls.append(('address', 'production', 'src/sdk_state.rs', address, 'address_native_value'))
controls.append(('cache', 'production', 'src/sdk_state.rs', source.replace('cache.token_expiry = 0.0;', 'cache.token_expiry = 1.0;'), 'clear_native_expiry'))
source = original['tests/host_helpers.rs']
record = source.replace('config: config.clone(),\n        provider: provider.clone(),', 'config: provider.clone(),\n        provider: config.clone(),')
controls.append(('record-fields', 'native-construction', 'tests/host_helpers.rs', record, 'sdk_native_config_identity'))


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'host-helpers-{label}.log').write_text(output)
    return result.returncode, output


head = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, kind, file, changed, assertion in controls:
    assert changed != original[file], (name, 'control did not change source')
    try:
        (CRATE / file).write_text(changed)
        mutated, output = run(name)
    finally:
        (CRATE / file).write_text(original[file])
    assert mutated == 101 and 'test result: FAILED' in output, (name, output)
    assert assertion in output, (name, output)
    restored, output = run(name + '-restored')
    assert restored == 0, (name, output)
    row = dict(control=name, kind=kind, assertion=assertion, mutation_exit=mutated, restored_exit=restored)
    report['controls'].append(row)
    (LOGS / 'host-helpers-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
assert len(report['controls']) == 6
assert subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip() == head
