"""Compiling body/case-count controls for the seventeen reviewed helper owners."""
from pathlib import Path
import subprocess

SDK = Path(__file__).resolve().parents[2]
CRATE = SDK / 'safe-cex-venue-cryptocom'
CONTROLS = []

def body(module, name, before, after, test, assertion):
    CONTROLS.append((module+'::'+name, CRATE / ('src/'+module+'.rs'),
                     ('fn '+name).encode(), b'\n}\n', before.encode(), after.encode(),
                     ['--lib'], test, assertion))

for name, before, after, assertion in [
    ('map_order_type','return CryptocomOrderType::Market;','return CryptocomOrderType::Limit;','map_order_type native variant'),
    ('map_time_in_force','return CryptocomTimeInForce::FillOrKill;','return CryptocomTimeInForce::GoodTillCancel;','map_time_in_force native variant'),
    ('map_stop_trigger_type','return CryptocomRefPriceType::Index;','return CryptocomRefPriceType::Mark;','map_stop_trigger_type native variant'),
]:
    CONTROLS.append((name,CRATE/'src/order_mappings.rs',('pub fn '+name+'(').encode(),b'\n}\n',before.encode(),after.encode(),['--test','accepted_mapping_variants'],'native_'+name,assertion))
for module in ['order_mappings','private_order_mappings']:
    body(module,'equals(',
         'value.is_some_and(|value| value.iter().copied().eq(literal.encode_utf16()))',
         '!value.is_some_and(|value| value.iter().copied().eq(literal.encode_utf16()))',
         module+'::accepted_equals::native_strict_comparison','strict comparison native result')
for name, before, after, assertion in [
    ('get<','(h.get)(object, key)','{ let _ = key; (h.get)(object, "__missing") }','socket get native identity'),
    ('method<','(h.invoke)(callee, receiver, args()?)','(h.invoke)(callee, (h.undefined)(), args()?)','socket method native receiver'),
    ('global<','(h.invoke)(callee, (h.undefined)(), args()?)','(h.invoke)(callee, (h.literal)("wrong"), args()?)','socket global undefined receiver'),
    ('own<','method(h, receiver.clone(), name, args)','{ let _ = (h, name, args); Ok(receiver.clone()) }','socket own native identity'),
    ('math<','method(h, (h.global)("Math")?, name, args)','{ let _ = (name, args); Ok((h.undefined)()) }','socket math native identity'),
    ('parsed_field<','parse(h, || get(h, data, key))','{ let _ = key; parse(h, || get(h, data, "__missing")) }','socket parsed_field native input'),
    ('parsed_or<','Ok((h.number)(default))','Ok((h.number)(default + 1.0))','socket parsed_or native value'),
    ('concat<','prefix.encode_utf16().chain((h.text)(value)?).collect()','{ let _ = prefix; (h.text)(value)? }','socket concat native UTF16'),
    ('caught<','emit(h, receiver, || concat(h, prefix, &(h.error_value)(error)))?;','let _ = (h, receiver, prefix, error);','socket caught native evaluation order'),
]:
    body('socket_handlers',name,before,after,'socket_handlers::accepted_helpers::'+name.rstrip('<')+'_native_contract',assertion)
body('api_error','append(', 'text.extend(literal.encode_utf16());', 'text.clear(); text.extend(literal.encode_utf16());', 'api_error::accepted_helpers::append_native_contract','api_error append native UTF16')
body('api_error','optional_get<', 'h.get(&value, key)', '{ let _ = key; Ok(value) }', 'api_error::accepted_helpers::optional_get_native_contract','api_error optional_get native identity')
body('api_error','http_message<', 'append(&mut message, "): ");', 'append(&mut message, "");', 'api_error::accepted_helpers::http_message_native_contract','api_error http_message native UTF16')
assert len(CONTROLS)==17
for label, file, marker, ending, before, after, target, test, assertion in [
    ('mapping walk',CRATE/'tests/accepted_mapping_variants.rs',b'fn native_map_order_type()',b'\n}\n',b'for case in cases {',b'for case in cases.into_iter().take(17) {',['--test','accepted_mapping_variants'],'native_map_order_type','map_order_type executed fixture count'),
    ('comparison walk',CRATE/'src/order_mappings.rs',b'fn native_strict_comparison()',b'\n    }\n',b'for case in cases {',b'for case in cases.into_iter().take(23) {',['--lib'],'order_mappings::accepted_equals::native_strict_comparison','strict comparison executed fixture count'),
    ('fallback walk',CRATE/'src/socket_handlers.rs',b'fn parsed_or_native_contract()',b'\n    }\n',b'for (result, truthy) in cases {',b'for (result, truthy) in cases.into_iter().take(10) {',['--lib'],'socket_handlers::accepted_helpers::parsed_or_native_contract','socket parsed_or executed cases'),
]:
    CONTROLS.append((label,file,marker,ending,before,after,target,test,assertion))
assert len(CONTROLS)==20

def run(target, test):
    result=subprocess.run(['cargo','test','--locked','--jobs','1','-q','-p','safe-cex-venue-cryptocom',*target,test,'--','--exact'],cwd=SDK,capture_output=True,text=True)
    return result.returncode,result.stdout+result.stderr

def passed(target,test):
    code,output=run(target,test)
    assert code==0 and 'test result: ok. 1 passed' in output,output

# Refuse stale/ambiguous mutation locations before changing anything.
for name,file,marker,ending,before,after,target,test,assertion in CONTROLS:
    original=file.read_bytes();start=original.index(marker);end=original.index(ending,start)+len(ending)
    assert original[start:end].count(before)==1,(name,'mutation site',before)

for name,file,marker,ending,before,after,target,test,assertion in CONTROLS:
    passed(target,test)
    original=file.read_bytes();start=original.index(marker);end=original.index(ending,start)+len(ending);region=original[start:end]
    try:
        file.write_bytes(original[:start]+region.replace(before,after)+original[end:])
        code,output=run(target,test)
        assert code==101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output,output
        assert assertion in output,output
    finally:
        file.write_bytes(original)
    assert file.read_bytes()==original
    passed(target,test)
    print(f'{name}: compiled named runtime failure (exit 101); exact restoration PASS',flush=True)
print('All 26 helper body/case-count controls passed.',flush=True)
