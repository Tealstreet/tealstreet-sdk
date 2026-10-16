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

executed = []

def control(name, file, mutate, assertions, supplements=()):
    selected = sys.argv[1:]
    if selected and name not in selected:
        return
    originals = [(path, path.read_bytes(), mutation) for path, mutation in [(file, mutate), *supplements]]
    try:
        for path, original, mutation in originals:
            changed = mutation(original.decode())
            assert changed != original.decode(), name
            path.write_text(changed)
        result = run()
        found = next((a for a in assertions if a in result.stdout), None)
        assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and found, (name, result.stdout)
        print(f'FAIL observed {name}: {found}; exit=101', flush=True)
    finally:
        for path, original, _ in originals:
            path.write_bytes(original)
    passed(run())
    print(f'PASS restored {name}', flush=True)
    executed.append(name)

def decode(record, body):
    def mutate(text):
        start = text.index(f"impl<'de> Deserialize<'de> for {record} {{")
        end = text.index('\n}', start)
        section = text[start:end]
        end_call = section.rfind(')') + 1
        changed = section[:end_call] + f'.map(|mut record| {{ {body} record }})' + section[end_call:]
        return text[:start] + changed + text[end:]
    return mutate

def remove_assertion(text, label, replacement=''):
    marker = text.index('"'+label+'"')
    start = text.rfind('assert_eq!(',0,marker)
    end = text.index(');',marker)+2
    assert start >= 0
    return text[:start]+replacement+text[end:]

def swap(record, left, right):
    def mutate(source):
        start = source.index(f'struct Wire{record} {{')
        end = source.index('\n}', start)
        section = source[start:end]
        old_left = f'rename = "{left}"'
        old_right = f'rename = "{right}"'
        assert section.count(old_left) == section.count(old_right) == 1
        changed = section.replace(old_left, 'SWAP_SENTINEL').replace(old_right, old_left).replace('SWAP_SENTINEL', old_right)
        return source[:start] + changed + source[end:]
    return mutate

TYPES = CRATE/'tests/types.rs'
TS = ROOT/'packages/safe-cex/src/exchanges/cryptocom/cryptocom.types.ts'
def within_order(text, before, after):
    start=text.index('export interface CryptocomOrder {');end=text.index('\n}',start)
    section=text[start:end];assert before in section
    return text[:start]+section.replace(before,after)+text[end:]
def swap_enum(name,left,right):
    def mutation(text):
        for prefix in ['pub enum ', 'enum Wire']:
            start=text.index(prefix+name+' {');end=text.index('\n}',start)
            section=text[start:end].replace('rename = "'+left+'"','rename = "__SWAP__"').replace('rename = "'+right+'"','rename = "'+left+'"').replace('rename = "__SWAP__"','rename = "'+right+'"')
            text=text[:start]+section+text[end:]
        return text
    return mutation
passed(run())
control('order_timestamp_swap',WIRE,swap('CryptocomOrder','create_time','update_time'),['wire_ingress:CryptocomOrder:numeric ingress changed Number bits at $.create_time'])
control('order_price_string_swap',WIRE,swap('CryptocomOrder','limit_price','trigger_price'),['decoded_field:CryptocomOrder:limit_price'])
for name,left,right,field in [
    ('CryptocomOrderType','LIMIT','MARKET','order_type'),
    ('CryptocomTimeInForce','GOOD_TILL_CANCEL','FILL_OR_KILL','time_in_force'),
    ('CryptocomRefPriceType','MARK_PRICE','INDEX_PRICE','ref_price_type'),
    ('CryptocomOrderStatus','ACTIVE','FILLED','status'),
]:
    control('order_symmetric_'+name,WIRE,swap_enum(name,left,right),['decoded_field:CryptocomOrder:'+field])
control('order_optional_null',WIRE,lambda text:text.replace('T::deserialize(deserializer).map(Some)','Option::<T>::deserialize(deserializer)'),['order_null_refusal:account_id'])
def remove_guard(text):
    return text.replace('WireCryptocomOrder::deserialize(safe_cex_types::strict_decode::object(deserializer))','WireCryptocomOrder::deserialize(deserializer)')
control('order_object_guard',WIRE,remove_guard,['order_populated_array_refusal'])
for name,old,new,assertion in [
    ('order_source_optional','account_id?: string;', 'account_id: string;', 'order_optional_census'),
    ('order_source_literal',"'MARK_PRICE' | 'INDEX_PRICE' | 'LAST_PRICE'", "'MARK_PRICE' | 'INDEX_PRICE'", 'order_literal_census:ref_price_type'),
    ('order_source_array','exec_inst?: string[];', 'exec_inst?: number[];', 'array_element_reference:CryptocomOrder:exec_inst'),
]:
    control(name,TS,lambda text,old=old,new=new:within_order(text,old,new),[assertion])
for mode,body in [('empty','items.clear();'),('subset','if items.len()>1 { items.pop(); }')]:
    control('order_consuming_'+mode,WIRE,decode('CryptocomOrder','if let Some(items) = record.exec_inst.as_mut() { '+body+' }'),['expected_native_count:order:exec_inst'])
consume=decode('CryptocomOrder','if let Some(items) = record.exec_inst.as_mut() { items.clear(); }')
control('order_consuming_without_lengths',WIRE,consume,['decoded_field:CryptocomOrder:exec_inst'],
    [(TYPES,lambda text:remove_assertion(remove_assertion(text,'expected_native_count:order:exec_inst','let _ = expected_count;'),'projected_count:order:exec_inst'))])
control('order_consuming_without_value',WIRE,consume,['expected_native_count:order:exec_inst'],
    [(TYPES,lambda text:remove_assertion(text,'decoded_field:CryptocomOrder:{field}:{}','let _ = (&actual[field], expected);'))])
def inject(text,statement):
    return text.replace('process.stdout.write(JSON.stringify(',statement+'\nprocess.stdout.write(JSON.stringify(')
control('order_joint_truncation',WIRE,consume,['expected_native_count:order:exec_inst'],
    [(ORACLE,lambda text:inject(text,'for (const row of wire.CryptocomOrder.cases) if (row.observed.exec_inst.type === "object") row.observed.exec_inst.value=[];'))])
control('order_empty_source_cases',ORACLE,lambda text:inject(text,'wire.CryptocomOrder.cases.length=0;'),['source_case_count:CryptocomOrder'])
control('order_subset_source_cases',ORACLE,lambda text:inject(text,'wire.CryptocomOrder.cases.pop();'),['source_case_count:CryptocomOrder'])
def skip_comparisons(text):
    start=text.index('fn check_order(')
    return text[:start]+text[start:].replace('.iter().enumerate()', '.iter().take(0).enumerate()',1)
control('order_zero_comparisons',TYPES,skip_comparisons,['comparisons_executed:order'])
def skip_fields(text):
    start=text.index('fn check_order(')
    return text[:start]+text[start:].replace('in row["observed"].as_object().unwrap() {','in row["observed"].as_object().unwrap().iter().take(0) {',1)
control('order_zero_field_comparisons',TYPES,skip_fields,['comparisons_executed:order:fields'])
if len(sys.argv) > 1:
    assert set(executed) == set(sys.argv[1:]), f'unknown_order_controls:{set(sys.argv[1:])-set(executed)}'
else:
    assert len(executed) == len(set(executed)) == 20, 'order_control_count'
print(f'{len(executed)} order controls failed by name and passed after restoration',flush=True)
