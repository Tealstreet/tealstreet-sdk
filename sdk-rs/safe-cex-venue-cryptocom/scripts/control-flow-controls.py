"""Individual native body controls for fifteen reviewed control-flow/adapter owners."""
import os
import sys
from pathlib import Path
import subprocess
SDK=Path(__file__).resolve().parents[2]
CRATE=SDK/'safe-cex-venue-cryptocom'
CONTROLS=[]
TARGETS={
    'order_writes':(['--test','order_writes'],'order_writes_match_actual_typescript_native'),
    'rest_reads':(['--test','rest_reads'],'rest_reads_match_actual_typescript_native'),
}
def body(module,name,before,after,assertions,target=None,test=None):
    if target is None:target,test=TARGETS[module]
    CONTROLS.append((module+'::'+name,CRATE/('src/'+module+'.rs'),('fn '+name).encode(),b'\n}\n',before.encode(),after.encode(),target,test,assertions))
body('order_writes','log<','h.template(prefix, h.error_value(error))?','{ let _ = prefix; h.template("changed: ", h.error_value(error))? }',['CRYPTOCOM-WRITE-NATIVE-LOG'])
body('order_writes','rethrow<','Err(error)','{ let _ = error; Ok(h.undefined()) }',['CRYPTOCOM-WRITE-NATIVE-ERROR'])
body('order_writes','caught<','rethrow(h, prefix, error)','{ let _ = prefix; rethrow(h, "caught: ", error) }',['CRYPTOCOM-WRITE-NATIVE-LOG'])
body('order_writes','finish<','Ok(h.array(vec![id]))','{ let _ = id; Ok(h.array(vec![])) }',['CRYPTOCOM-WRITE-NATIVE-ARRAY'])
body('order_writes','construct<','h.template(prefix, value)?','{ let _ = prefix; h.template("changed: ", value)? }',['CRYPTOCOM-WRITE-NATIVE-CONSTRUCTED-MESSAGE'])
body('order_writes','update_value<','if present { "update" } else { "order" }','if !present { "update" } else { "order" }',['CRYPTOCOM-WRITE-NATIVE-NUMBER','CRYPTOCOM-WRITE-NATIVE-TEXT','CRYPTOCOM-WRITE-NATIVE-KIND','CRYPTOCOM-WRITE-NATIVE-ERROR'])
body('order_writes','listen<','vec![arg, callback]','vec![callback, arg]',['CRYPTOCOM-WRITE-NATIVE-CALLBACK-IDENTITY'])
body('rest_reads','begin<','h.endpoint(endpoint)?','{ let _ = endpoint; h.endpoint("TICKERS")? }',['CRYPTOCOM-READ-NATIVE-ENDPOINT'])
body('rest_reads','catch<','h.error_template(prefix, h.error_value(&error))?','{ let _ = prefix; h.error_template("changed: ", h.error_value(&error))? }',['CRYPTOCOM-READ-NATIVE-LOG'])
body('rest_reads','caught_begin<','catch(h, e, prefix, swallow)','catch(h, e, prefix, !swallow)',['CRYPTOCOM-READ-NATIVE-ERROR'])
body('rest_reads','parsed<','let prepared = h.prepare_parse_float()?;\n    h.call_parse_float(prepared, h.get(value, key)?)','let value = h.get(value, key)?;\n    let prepared = h.prepare_parse_float()?;\n    h.call_parse_float(prepared, value)',['parsed native evaluation order'],['--lib'],'rest_reads::accepted_control_flow::parsed_native_contract')
body('rest_reads','nullish_field<','if !h.nullish(&v) {','if true {',['CRYPTOCOM-READ-NATIVE-NUMBER','CRYPTOCOM-READ-NATIVE-TEXT','CRYPTOCOM-READ-NATIVE-ERROR'])
body('rest_reads','zero_position<','("leverage", h.number(1.0))','("leverage", h.number(2.0))',['CRYPTOCOM-READ-NATIVE-NUMBER'])
body('class_mappings','imported<','h.get(&(h.ops.value.global)(owner)?, key)','{ let _ = key; h.get(&(h.ops.value.global)(owner)?, "__missing") }',['imported native value identity'],['--lib'],'class_mappings::accepted_control_flow::imported_native_contract')
body('class_admin','optional<','(h.value.get)(&value, key)','{ let _ = key; Ok(value) }',['class optional native child identity'],['--lib'],'class_admin::accepted_control_flow::optional_native_contract')
assert len(CONTROLS)==15
CONTROLS.append(('parsed fixture walk',CRATE/'src/rest_reads.rs',b'fn parsed_native_contract()',b'\n    }\n',b'for (failure, output) in cases {',b'for (failure, output) in cases.into_iter().take(6) {',['--lib'],'rest_reads::accepted_control_flow::parsed_native_contract',['parsed native executed cases']))
REVIEW_WALKS = [
    ('optional nullish fixture walk',CRATE/'src/class_admin.rs',b'fn optional_native_contract()',b'\n    }\n',b'for value in [Js::Undefined, Js::Null] {',b'for value in [Js::Undefined, Js::Null].into_iter().take(1) {',['--lib'],'class_admin::accepted_control_flow::optional_native_contract',['class optional nullish executed cases']),
    ('optional falsey fixture walk',CRATE/'src/class_admin.rs',b'fn optional_native_contract()',b'\n    }\n',b'for value in [Js::Bool(false), Js::Number(-0.), Js::str("")] {',b'for value in [Js::Bool(false), Js::Number(-0.), Js::str("")].into_iter().take(2) {',['--lib'],'class_admin::accepted_control_flow::optional_native_contract',['class optional falsey executed cases']),
    ('imported abrupt fixture walk',CRATE/'src/class_mappings.rs',b'fn imported_native_contract()',b'\n    }\n',b'for stage in 0..2 {',b'for stage in 0..1 {',['--lib'],'class_mappings::accepted_control_flow::imported_native_contract',['imported native abrupt executed cases']),
]
CONTROLS.extend(REVIEW_WALKS)
assert len(CONTROLS)==19

def run(target,test):
    result=subprocess.run(['cargo','test','--locked','--jobs','1','-q','-p','safe-cex-venue-cryptocom',*target,test,'--','--exact'],cwd=SDK,capture_output=True,text=True)
    return result.returncode,result.stdout+result.stderr

def passing(target,test):
    code,output=run(target,test)
    assert code==0 and 'test result: ok. 1 passed' in output,output

# Check every location before any mutation; a stale site is not a runtime proof.
for name,file,marker,ending,before,after,target,test,assertions in CONTROLS:
    original=file.read_bytes();start=original.index(marker);end=original.index(ending,start)+len(ending)
    assert original[start:end].count(before)==1,(name,'ambiguous/missing mutation site',before)

if __name__=='__main__':
    assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'),'compiler_path_required'
    assert sys.argv[1:] in ([], ['--review-walks-only']), 'unknown control scope'
    selected = REVIEW_WALKS if sys.argv[1:] else CONTROLS
    for name,file,marker,ending,before,after,target,test,assertions in selected:
        passing(target,test)
        original=file.read_bytes();start=original.index(marker);end=original.index(ending,start)+len(ending);region=original[start:end]
        try:
            file.write_bytes(original[:start]+region.replace(before,after)+original[end:])
            code,output=run(target,test)
            header=f'---- {test} stdout ----'
            assert code==101 and 'test result: FAILED' in output and header in output,output
            failure=output.split(header,1)[1]
            matched=[assertion for assertion in assertions if assertion in failure]
            assert matched,output
        finally:
            file.write_bytes(original)
        assert file.read_bytes()==original
        passing(target,test)
        print(f'{name}: compiled named runtime failure (exit 101); assertion={matched[0]}; exact restoration PASS',flush=True)
    print(f'All {len(selected)} selected control-flow body/counter controls passed.',flush=True)
