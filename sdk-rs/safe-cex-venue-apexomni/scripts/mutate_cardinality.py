"""Batched collection controls; one Node invocation per complete corpus.
Sources are restored even if a control fails. Run from the repository root.
"""
from pathlib import Path
import os
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
VENUE = ROOT / 'sdk-rs/safe-cex-venue-apexomni'
LOGS = ROOT / 'sdk-rs/target/apexomni-controls'
LOGS.mkdir(parents=True, exist_ok=True)
CARGO = os.environ.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo')
BASE = [CARGO, 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
        '-p', 'safe-cex-venue-apexomni', '--jobs', '1']

def run(label, target, test, code, names):
    result = subprocess.run(BASE + ['--test', target, test, '--', '--test-threads=1', '--nocapture'],
                            cwd=ROOT, text=True, capture_output=True)
    output = result.stdout + result.stderr
    (LOGS / (label + '.log')).write_text(output)
    assert result.returncode == code, (label, result.returncode, output)
    for name in names:
        assert name in output, (label, name, output)
    if code == 101:
        assert 'test result: FAILED.' in output, (label, 'must compile and execute tests')
    print(label, result.returncode, len(names), 'named assertions', flush=True)

# 13 independently caught native field paths, including opaque nonempty arrays.
# Fields::deserialize has consumed all input before each assignment is mutated.
AXES = {
    'public_records.rs': [('DepthObject.a','a','vec'), ('DepthObject.b','b','vec'),
                          ('PerpetualCurrencyObject.usdc','usdc','vec'), ('PerpetualCurrencyObject.usdt','usdt','vec')],
    'private_records.rs': [('AccountObject.wallets','wallets','vec'), ('AccountObject.openPositions','open_positions','vec'),
                           ('AccountObject.accounts','accounts','optional_vec')],
    'config_records.rs': [('SymbolObject.currency','currency','vec'), ('SymbolObject.perpetualContract','perpetual_contract','vec'),
                          ('MultiChainObject.chains','chains','value'), ('UserObject.userData','user_data','optional_value'),
                          ('OrderObject.openTpParam','open_tp_param','value'), ('OrderObject.openSlParam','open_sl_param','value')],
}
original = {VENUE/'src'/file: (VENUE/'src'/file).read_text() for file in AXES}
try:
    for mode in ['empty', 'subset', 'extra']:
        names = []
        for file, axes in AXES.items():
            path = VENUE/'src'/file
            text = original[path]
            for name, field, kind in axes:
                # Consume the original field so a compile-time unused-field error cannot count.
                collection = 'v' if 'vec' in kind else 'v.as_array().unwrap().clone()'
                edit = 'v.truncate(0);' if mode == 'empty' else ('v.truncate(1);' if mode == 'subset' else 'v.push(v[0].clone());')
                body = '{ let mut v = '+collection+'; '+edit+' '+('v' if 'vec' in kind else 'serde_json::Value::Array(v)')+' }'
                if 'vec' not in kind:
                    body = 'if v.is_array() '+body+' else { v }'
                rhs = f'fields.{field}.map(|v| {body})' if kind.startswith('optional') else '{ let v = fields.'+field+'; '+body+' }'
                needle = f'{field}: fields.{field},'
                # currency is also a String field in MultiChainObject: select SymbolObject's decoder.
                if field == 'currency':
                    offset = text.index("impl<'de> serde::Deserialize<'de> for SymbolObject")
                    text = text[:offset]+text[offset:].replace(needle, f'{field}: {rhs},', 1)
                else:
                    assert text.count(needle) == 1, (file, field)
                    text = text.replace(needle, f'{field}: {rhs},')
                names.append(f'APEX-CARDINALITY {name} expected/native length')
            path.write_text(text)
        run('cardinality-native-'+mode, 'records', 'collection_cardinality_native_supplement', 101, names)
finally:
    for path, text in original.items(): path.write_text(text)
run('cardinality-native-restored', 'records', 'collection_cardinality', 0, ['test result: ok.'])

# Deliberate source omissions; bypass only the source count guard for layer-two controls.
for target, script, keys, test in [
    ('utils','utils.cjs',['precision','native','random','prefix'],'utility_rows_match_actual_typescript'),
    ('signer_utils','signer-utils.cjs',['timestamps','asserts','cases'],'signer_rows_match_actual_typescript'),
    ('signer_utils','signer-utils.cjs',['validators'],'validator_rows_match_actual_typescript'),
]:
    js = VENUE/'scripts'/script
    rust = VENUE/'tests'/(target+'.rs')
    old_js, old_rust = js.read_text(), rust.read_text()
    try:
        for bypass in [False, True]:
            if bypass:
                pattern = r'    assert_eq!\(rows.len\(\), count, "APEX-CARDINALITY source \{key\} total"\);'
                edited, n = re.subn(pattern, '    let _ = count;', old_rust)
                assert n == 1, 'source-only guard bypass must apply exactly once'
                rust.write_text(edited)
            for mode in ['empty', 'subset']:
                # Preserve the positive predicate row while shortening its corpus.
                edits = ''.join(f'{key}.length = '+('0' if mode == 'empty' else f'{key}.length-1')+';\n' for key in keys)
                needle = 'console.log(JSON.stringify('
                assert old_js.count(needle) == 1
                js.write_text(old_js.replace(needle, edits+needle))
                names = [f'APEX-CARDINALITY {"native" if bypass else "source"} {key} {"comparisons" if bypass else "total"}' for key in keys]
                # Empty predicates fail the mandatory positive lookup before iteration; subset reaches count.
                if bypass and mode == 'empty' and 'cases' in keys:
                    names.remove('APEX-CARDINALITY native cases comparisons')
                    names += [f'APEX-CARDINALITY {name} mandatory positive case' for name in ['assertTypeArray','assertTypeBoolean','assertTypeNumber','assertTypeString','assertSubAccountId']]
                run(f'cardinality-source-{keys[0]}-{mode}-bypass{bypass}', target, test, 101, names)
    finally:
        js.write_text(old_js)
        rust.write_text(old_rust)
    run('cardinality-source-'+keys[0]+'-restored',target,test,0,['test result: ok.'])

# Recursive collection paths, independently from outer cardinality.
inner_axes = {
 'public_records.rs': [('DepthObject.a','a'),('DepthObject.b','b')],
 'config_records.rs': [('MultiChainObject.chains','chains'),('UserObject.userData','user_data'),
                       ('OrderObject.openTpParam','open_tp_param'),('OrderObject.openSlParam','open_sl_param')],
}
original = {VENUE/'src'/file: (VENUE/'src'/file).read_text() for file in inner_axes}
try:
 for mode in ['empty','subset','extra']:
  names=[]
  for file,axes in inner_axes.items():
   path=VENUE/'src'/file;text=original[path]
   for name,field in axes:
    edit='v.truncate(0);' if mode=='empty' else ('v.truncate(1);' if mode=='subset' else 'v.push(v[0].clone());')
    if file=='public_records.rs':
     rhs=f'fields.{field}.into_iter().map(|mut v| {{ {edit} v }}).collect()'
    else:
     body='if v.is_array() { serde_json::Value::Array(v.as_array().unwrap().iter().map(|child| { let mut v = child.as_array().unwrap().clone(); '+edit+' serde_json::Value::Array(v) }).collect()) } else { v }'
     rhs=f'fields.{field}.map(|v| {body})' if field=='user_data' else '{ let v = fields.'+field+'; '+body+' }'
    needle=f'{field}: fields.{field},';assert text.count(needle)==1
    text=text.replace(needle,f'{field}: {rhs},');names.append(f'APEX-CARDINALITY {name}[0] expected/native length')
   path.write_text(text)
  run('cardinality-inner-'+mode,'records','collection_cardinality_native_supplement',101,names)
finally:
 for path,text in original.items():path.write_text(text)
run('cardinality-inner-restored','records','collection_cardinality',0,['test result: ok.'])

# Raw and native numeric subcorpora have fixed execution totals as a second layer.
js=VENUE/'scripts/records.cjs';rust=VENUE/'tests/records.rs'
old_js,old_rust=js.read_text(),rust.read_text()
try:
 for bypass in [False,True]:
  if bypass:
   needle='    numeric_corpus_cardinality(&corpus);';assert old_rust.count(needle)==1
   rust.write_text(old_rust.replace(needle,''))
  for key,test in [('$numbers','raw_numbers_preserve_sign_and_use_javascript_json_spelling'),
                   ('$nativeNumbers','timestamp_native_numbers_canonicalize_nan_only')]:
   for mode in ['empty','subset']:
    edit=f'records.{key}.length = '+('0' if mode=='empty' else f'records.{key}.length-1')+';\n'
    js.write_text(old_js.replace('process.stdout.write(JSON.stringify(records));',edit+'process.stdout.write(JSON.stringify(records));'))
    label='APEX-CARDINALITY native numeric comparisons' if bypass else f'APEX-CARDINALITY source {key} total'
    run(f'cardinality-numeric-{key}-{mode}-bypass{bypass}','records',test,101,[label])
finally:
 js.write_text(old_js);rust.write_text(old_rust)
run('cardinality-numeric-restored','records','numbers',0,['test result: ok.'])
