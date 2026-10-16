#!/usr/bin/env python3
"""Sequential cardinality controls; one Node corpus per scoped Rust test process."""
import os
from pathlib import Path
import subprocess
import sys
ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom'
WIRE = CRATE / 'src/wire.rs'
ORACLE = CRATE / 'tests/types_oracle.cjs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-cryptocom', '--test', 'types', '--jobs', '1', '--locked']
def run():
    return subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
def passed(result):
    assert result.returncode == 0 and '3 passed; 0 failed' in result.stdout, result.stdout
def control(name, file, mutate, assertions, supplements=()):
    selected = [arg for arg in sys.argv[1:] if not arg.startswith('--')]
    if selected and name not in selected:
        return
    originals = [(path, path.read_bytes(), mutation) for path, mutation in [(file, mutate), *supplements]]
    try:
        for path, original, mutation in originals:
            changed = mutation(original.decode())
            assert changed != original.decode(), name
            path.write_text(changed)
        result = run()
        if '--audit-before' in sys.argv and name.startswith('source_'):
            passed(result)
            print(f'FALSE GREEN observed {name}: 3 tests passed', flush=True)
        else:
            found = next((a for a in assertions if a in result.stdout), None)
            assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and found, (name, result.stdout)
            print(f'FAIL observed {name}: {found}; exit=101', flush=True)
    finally:
        for path, original, _ in originals:
            path.write_bytes(original)
    passed(run())
    print(f'PASS restored {name}', flush=True)
def decode(record, body):
    def mutate(text):
        start = text.index(f"impl<'de> Deserialize<'de> for {record} {{")
        end = text.index('\n}', start)
        section = text[start:end]
        end_call = section.rfind(')') + 1
        changed = section[:end_call] + f'.map(|mut record| {{ {body} record }})' + section[end_call:]
        return text[:start] + changed + text[end:]
    return mutate
def constant_collection(name, subset):
    def mutate(text):
        start = text.index('= &[', text.index(f'pub const {name}:')) + 3
        stack = ['[']; quoted = False; escaped = False; first_end = None
        for i in range(start + 1, len(text)):
            c = text[i]
            if quoted:
                if escaped: escaped = False
                elif c == chr(92): escaped = True
                elif c == '"': quoted = False
                continue
            if c == '"': quoted = True
            elif c in '[({': stack.append(c)
            elif c in '])}':
                stack.pop()
                if not stack:
                    assert first_end is not None
                    return text[:start+1] + (text[start+1:first_end] if subset else '') + text[i:]
            elif c == ',' and len(stack) == 1 and first_end is None: first_end = i
        raise AssertionError(name)
    return mutate
passed(run())
for name in ['BASE_URLS','WSS_URLS','PUBLIC_ENDPOINTS','GRAFANA_ENDPOINTS_EVERY','GRAFANA_ENDPOINTS_PERIODIC']:
    for subset in [False,True]:
        control(f'{name}_{"subset" if subset else "empty"}', CRATE/'src/types.rs', constant_collection(name,subset), [f'constant_value:{name}', 'expected_native_count'])
for record, field, assertion in [
    ('CryptocomBalanceSummary','position_balances','decoded_field:CryptocomBalanceSummary:position_balances'),
    ('CryptocomBook','data','decoded_field:CryptocomBook:data'),
    ('CryptocomBookData','bids','decoded_field:CryptocomBook:data'),
    ('CryptocomBookData','asks','decoded_field:CryptocomBook:data'),
]:
    for shape, body in [('empty',f'record.{field}.clear();'),('subset',f'if record.{field}.len() > 1 {{ record.{field}.pop(); }}')]:
        control(f'{record}_{field}_{shape}', WIRE, decode(record,body), [assertion, 'expected_native_count'])
for name, injection in [('source_intervals_empty','intervals.length=0;'),('source_account_empty','accounts.cases.length=0;'),('source_flat_subset',"wire.CryptocomTradeFill.cases.length=1;")]:
    control(name,ORACLE,lambda text, injection=injection:text.replace('process.stdout.write(JSON.stringify(',injection+'\nprocess.stdout.write(JSON.stringify('), ['source_case_count','case_inventory'])

for name, injection, assertion in [
    ('duplicate_flat_case', 'wire.CryptocomTradeFill.cases[1]=wire.CryptocomTradeFill.cases[0];', 'case_inventory:CryptocomTradeFill'),
    ('summary_surrogates_empty', 'summary.surrogates.length=0;', 'case_inventory:surrogates:summary'),
    ('book_surrogates_subset', 'book.surrogates.length=1;', 'case_inventory:surrogates:book'),
    ('summary_expected_empty', 'summary.cases[2].observed.position_balances=[];', 'projected_count:summary'),
    ('book_expected_subset', 'book.cases[2].observed.data.pop();', 'projected_count:book'),
    ('book_expected_bids_empty', 'book.cases[2].observed.data[1].bids=[];', 'projected_count:book:bids'),
    ('book_expected_asks_subset', 'book.cases[2].observed.data[1].asks.pop();', 'projected_count:book:asks'),
]:
    control(name, ORACLE, lambda text, injection=injection:text.replace('process.stdout.write(JSON.stringify(',injection+'\nprocess.stdout.write(JSON.stringify('), [assertion])

# Changing the source population while preserving case count must also fail.
for name, before, after, assertion in [
    ('summary_population_shift', 'for (const count of [0, 1, 2])', 'for (const count of [2, 3, 4])', 'source_population:summary'),
    ('book_population_shift', 'for (const count of [0,1,2])', 'for (const count of [2,3,4])', 'source_population:book'),
]:
    control(name, ORACLE, lambda text,before=before,after=after:text.replace(before,after), [assertion])

def remove_assertion(text, label, replacement=''):
    marker = text.index('"'+label+'"')
    start = text.rfind('assert_eq!(',0,marker)
    end = text.index(');',marker)+2
    assert start >= 0
    return text[:start]+replacement+text[end:]

# Every mutant below first deserializes the entire input successfully, then
# clears the decoded collection. No trailing-element error can detect it.
# Isolate both layers: disabling lengths leaves full-value equality; disabling
# full-value equality leaves the independent native cardinality assertion.
for record, field, length_label, value_label in [
    ('CryptocomBalanceSummary','position_balances','expected_native_count:summary','decoded_field:CryptocomBalanceSummary:{field}'),
    ('CryptocomBook','data','expected_native_count:book','decoded_field:CryptocomBook:{field}'),
    ('CryptocomBookData','bids','expected_native_count:book:bids','decoded_field:CryptocomBook:{field}'),
    ('CryptocomBookData','asks','expected_native_count:book:asks','decoded_field:CryptocomBook:{field}'),
]:
    control(f'{record}_{field}_without_length', WIRE, decode(record,f'record.{field}.clear();'), [value_label.split('{')[0]],
        [(CRATE/'tests/types.rs', lambda text,label=length_label:remove_assertion(remove_assertion(text,label, 'let _ = '+{'expected_native_count:summary':'expected_count', 'expected_native_count:book':'expected.len()', 'expected_native_count:book:bids':'bid_count', 'expected_native_count:book:asks':'ask_count'}[label]+';'),label.replace('expected_native_count:', 'projected_count:')))])
    control(f'{record}_{field}_without_value', WIRE, decode(record,f'record.{field}.clear();'), [length_label],
        [(CRATE/'tests/types.rs', lambda text,label=value_label:remove_assertion(text,label,'let _ = (&actual[field], expected);'))])

# A projected output shortened together with native must not define its own
# expected length: immutable raw-input population expectations still detect it.
for record, field, injection, assertion in [
    ('CryptocomBalanceSummary', 'position_balances', 'for (const row of summary.cases) row.observed.position_balances=[];', 'expected_native_count:summary'),
    ('CryptocomBook', 'data', 'for (const row of book.cases) row.observed.data=[];', 'expected_native_count:book'),
    ('CryptocomBookData', 'bids', 'for (const row of book.cases) for (const data of row.observed.data) data.bids=[];', 'expected_native_count:book:bids'),
    ('CryptocomBookData', 'asks', 'for (const row of book.cases) for (const data of row.observed.data) data.asks=[];', 'expected_native_count:book:asks'),
]:
    control(f'{record}_{field}_joint_truncation', WIRE, decode(record,f'record.{field}.clear();'), [assertion],
        [(ORACLE, lambda text,injection=injection:text.replace('process.stdout.write(JSON.stringify(',injection+'\nprocess.stdout.write(JSON.stringify('))])
