"""Compile independent Array storage faults, require named failures, restore bytes."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
controls = [
    ("readonly-growth", "value/array.rs", "&& !array.length_writable.get()", "&& array.length_writable.get()", "readonly_array_length_refuses_empty_push_but_define_coerces_twice"),
    ("named-properties", "value.rs", "fn get_key_receiver(&self, key: &[u16], receiver: &Js) -> Result<Js, Thrown> {", "fn get_key_receiver(&self, key: &[u16], receiver: &Js) -> Result<Js, Thrown> {\n        if matches!(self, Js::Array(_)) && array_index(key).is_none() && key != utf16(\"length\").as_ref() { return Ok(Js::Undefined); }", "array_named_descriptors_keep_holes_aliases_bits_and_key_order"),
    ("inherited-receiver", "value.rs", "=> return setter.call(self.clone(), vec![value]).map(|_| ()),", "=> return setter.call(parent.clone(), vec![value]).map(|_| ()),", "inherited_array_index_setter_observes_original_receiver_without_growth"),
    ("iterator-override", "collections.rs", "fn iterator_method_receiver(value: &Js, receiver: &Js) -> Result<Js, Thrown> {", "fn iterator_method_receiver(value: &Js, receiver: &Js) -> Result<Js, Thrown> {\n        if matches!(value, Js::Array(_)) { return Ok(array_iterator_method()); }", "array_symbol_iterator_accessor_retains_receiver_alias_and_negative_zero"),
]
selected = set(sys.argv[1:])
assert not selected.difference(row[0] for row in controls), "unknown control"

def run(label, test, red=False):
    command=["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-exchange", "--test", "js_array_storage", test, "--", "--exact", "--nocapture"]
    result=subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    Path('/tmp/js-array-control-'+label+'.log').write_text(result.stdout)
    if red:
        assert result.returncode and 'panicked at' in result.stdout and test in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
    else:
        assert result.returncode==0 and '1 passed; 0 failed' in result.stdout, result.stdout
    print(label, 'compiled named RED' if red else 'restored GREEN', flush=True)

for label, relative, needle, fault, test in controls:
    if selected and label not in selected: continue
    path=root/'safe-cex-exchange/src/js_runtime'/relative
    original=path.read_bytes();source=original.decode()
    assert source.count(needle)==1,(label,'production anchor drift')
    run(label+'-baseline',test)
    try:
        path.write_text(source.replace(needle,fault))
        run(label+'-red',test,True)
    finally:
        path.write_bytes(original)
    run(label+'-restored',test)
