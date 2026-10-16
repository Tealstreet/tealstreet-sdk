"""Serial empty/subset controls: one Node invocation per target corpus."""
from pathlib import Path
import os
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')

def run(target):
    return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-orderly', '--test', target, '--jobs', '1', '--locked'], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

def replace(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new)

def table(text, entries):
    start = text.index('pub const GRAFANA_ENDPOINTS_EVERY:')
    end = text.index('\n];', start) + 3
    return text[:start] + f'pub const GRAFANA_ENDPOINTS_EVERY: [&str; {len(entries)}] = [{", ".join(entries)}];' + text[end:]

wire = 'BalanceResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))'
controls = [
    ('wire-empty', 'wire_parity', 'src/wire.rs', lambda s: replace(s, wire, wire + '.map(|mut value| { value.holding.clear(); value })'), 'wire/BALANCE_RESPONSE/full/field census'),
    ('wire-subset', 'wire_parity', 'src/wire.rs', lambda s: replace(s, wire, wire + '.map(|mut value| { value.holding.pop(); value })'), 'wire/BALANCE_RESPONSE/full/field census'),
    ('table-empty', 'source_parity', 'src/types.rs', lambda s: table(s, []), 'constant/GRAFANA_ENDPOINTS_EVERY'),
    ('table-subset', 'source_parity', 'src/types.rs', lambda s: table(s, ['ENDPOINTS[8].1']), 'constant/GRAFANA_ENDPOINTS_EVERY'),
    ('log-empty', 'errors_parity', 'src/errors.rs', lambda s: replace(s, 'BTreeMap::from([("suppressConsoleLog".into(), true)])', 'BTreeMap::new()'), 'order-log/native-key-count/2'),
]
if not os.environ.get('ORDERLY_CARDINALITY_PRODUCTION_ONLY'):
    for key in ['rows', 'algo']:
        for action in ['clear()', 'pop()']:
            controls.append((f'oracle-{key}-{action}', 'order_utility_parity', 'tests/order_utility_parity.rs',
                lambda s, key=key, action=action: replace(s, 'let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();',
                    'let mut corpus: Value = serde_json::from_slice(&output.stdout).unwrap();\n' + f'    corpus["{key}"].as_array_mut().unwrap().{action};'),
                f'ORDERLY-CARDINALITY order_utility/{key} source total'))
controls.extend([
    ('layered-wire-empty', 'wire_parity', 'src/wire.rs', lambda s: replace(s, wire, wire + '.map(|mut value| { value.holding.clear(); value })'), 'ORDERLY-CARDINALITY wire/native array/BALANCE_RESPONSE/full//holding'),
    ('layered-wire-subset', 'wire_parity', 'src/wire.rs', lambda s: replace(s, wire, wire + '.map(|mut value| { value.holding.pop(); value })'), 'ORDERLY-CARDINALITY wire/native array/BALANCE_RESPONSE/full//holding'),
])
for field in ['account_mode', 'order_type', 'side']:
    for action in (['clear()', 'pop()'] if field == 'side' else ['clear()']):
        controls.append((f'literal-{field}-{action}', 'wire_parity', 'tests/wire_parity.rs',
            lambda s, field=field, action=action: replace(s, 'let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();',
                'let mut corpus: Value = serde_json::from_slice(&output.stdout).unwrap();\n' +
                f'    for row in corpus["literals"].as_array_mut().unwrap() {{ if row["field"] == "/{field}" {{ row["values"].as_array_mut().unwrap().{action}; }} }}'),
            'ORDERLY-CARDINALITY literal/' + ('ACCOUNT_RESPONSE' if field == 'account_mode' else 'EDIT_ORDER_REQUEST') + f'/{field}/count'))
# Source guard removal is deliberate fault injection, never the production witness.
def bypass_source_totals(text):
    return re.sub(r'    assert_eq!\([\s\S]*?\n    \);',
        lambda m: '' if 'source total' in m.group() else m.group(), text)

controls.extend([
    ('wire-extra', 'wire_parity', 'src/wire.rs', lambda s: replace(s, wire, wire + '.map(|mut value| { value.holding.push(value.holding[0].clone()); value })'), 'wire/BALANCE_RESPONSE/full/field census'),
    ('table-extra', 'source_parity', 'src/types.rs', lambda s: table(s, [f'ENDPOINTS[{i}].1' for i in [8,5,7,9,10,11,14,15,16,8]]), 'constant/GRAFANA_ENDPOINTS_EVERY'),
    ('log-extra', 'errors_parity', 'src/errors.rs', lambda s: replace(s, 'BTreeMap::from([("suppressConsoleLog".into(), true)])', 'BTreeMap::from([("suppressConsoleLog".into(), true), ("extra".into(), false)])'), 'order-log/native-key-count/2'),
    ('layered-wire-extra', 'wire_parity', 'src/wire.rs', lambda s: replace(s, wire, wire + '.map(|mut value| { value.holding.push(value.holding[0].clone()); value })'), 'ORDERLY-CARDINALITY wire/native array/BALANCE_RESPONSE/full//holding'),
])
for target, key in [('source', 'symbols'), ('encryption', 'hashes'), ('errors', 'corpus'), ('margin', 'mapped'), ('current_margin', 'rows'), ('order_utility', 'rows'), ('wire', 'cases')]:
    var = 'source' if target == 'source' else 'corpus'
    for action in ['clear()', 'pop()']:
        def mutate(s, var=var, key=key, action=action):
            s = bypass_source_totals(s)
            return replace(s, f'let {var}: Value = serde_json::from_slice(&output.stdout).unwrap();',
                f'let mut {var}: Value = serde_json::from_slice(&output.stdout).unwrap();\n    {var}["{key}"].as_array_mut().unwrap().{action};')
        assertion = 'ORDERLY-COMPARISONS wire/per-type groups' if target == 'wire' else f'ORDERLY-COMPARISONS {target}/{key}'
        controls.append((f'bypass-{target}-{action}', target + '_parity', f'tests/{target}_parity.rs', mutate, assertion))
for action in ['clear()', 'pop()']:
    controls.append((f'unguarded-wire-{action}', 'wire_parity', 'src/wire.rs',
        lambda s, action=action: replace(s, wire, wire + f'.map(|mut value| {{ value.holding.{action}; value }})'),
        'wire/BALANCE_RESPONSE/full/native/holding/ingress0'))
start = int(os.environ.get('ORDERLY_CARDINALITY_START', '0'))
for label, target, relative, mutate, assertion in controls[start:]:
    path = crate / relative
    saved = path.read_bytes()
    witness = crate / 'tests/wire_parity.rs'
    witness_saved = witness.read_bytes()
    try:
        path.write_text(mutate(saved.decode()))
        if label.startswith('layered-'):
            # Bypass the older field census: independent native array length must fail.
            witness.write_text(replace(witness_saved.decode(), 'view.keys().collect::<Vec<_>>().len(),', 'expected.len(),'))
        if label.startswith('unguarded-'):
            changed = replace(witness_saved.decode(), 'view.keys().collect::<Vec<_>>().len(),', 'expected.len(),')
            changed = replace(changed, 'view.get(*path).unwrap()[1].as_u64().unwrap() as usize,', 'source_length,')
            witness.write_text(changed)
        result = run(target)
        assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
        print(f'{label}: compiled; exit 101; {assertion}', flush=True)
    finally:
        path.write_bytes(saved)
        if label.startswith(('layered-', 'unguarded-')):
            witness.write_bytes(witness_saved)
    restored = run(target)
    assert restored.returncode == 0, restored.stdout
    print(f'{label}: restored exit 0', flush=True)
print(f'{len(controls) - start} controls rejected and restored (start {start})')
