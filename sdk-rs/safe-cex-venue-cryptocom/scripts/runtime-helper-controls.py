"""Compiling helper corruption controls with exact source restoration."""
from pathlib import Path
import subprocess
ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'safe-cex-venue-cryptocom/src/runtime.rs'
CONTROLS = {
    'get': ('(h.value.get)(value, key)', '{ let _ = (value, key); Ok((h.value.undefined)()) }'),
    'optional': ('if (h.value.nullish)(&value)', 'if !(h.value.nullish)(&value)'),
    'method': ('let callee = get(h, &receiver, key)?;\n    (h.value.invoke)(callee, receiver, args()?)', 'let args = args()?;\n    let callee = get(h, &receiver, key)?;\n    (h.value.invoke)(callee, receiver, args)'),
    'own': ('method(h, receiver.clone(), key, args)', '{ let _ = args; method(h, receiver.clone(), key, || Ok(vec![])) }'),
    'global': ('(h.value.invoke)(callee, (h.value.undefined)(), args()?)', '(h.value.invoke)(callee, (h.value.boolean)(false), args()?)'),
    'text': ('prefix\n            .encode_utf16()', 'prefix\n            .encode_utf16().take(0)'),
    'loaded': ('(h.value.boolean)(true)', '(h.value.boolean)(false)'),
    'update': ('"update"', '"emit"'),
}
def run(test):
    result = subprocess.run(['cargo','test','--locked','--jobs','1','-q','-p','safe-cex-venue-cryptocom','--lib',test,'--','--exact'],cwd=ROOT,capture_output=True,text=True)
    return result.returncode,result.stdout+result.stderr
for name,(before,after) in CONTROLS.items():
    test=f'runtime::accepted_runtime_helpers::{name}_native_contract'
    code,output=run(test);assert code==0 and 'test result: ok. 1 passed' in output,output
    original=SOURCE.read_bytes();start=original.index(f'fn {name}<'.encode());end=original.index(b'\n}\n',start)+3;region=original[start:end]
    assert region.count(before.encode())==1
    try:
        SOURCE.write_bytes(original[:start]+region.replace(before.encode(),after.encode())+original[end:])
        code,output=run(test)
        assert code==101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output,output
        assert 'runtime helper' in output,output
    finally:SOURCE.write_bytes(original)
    assert SOURCE.read_bytes()==original
    code,output=run(test);assert code==0 and 'test result: ok. 1 passed' in output,output
    print(f'{name}: compiled named native runtime failure (exit 101); exact restoration PASS',flush=True)
