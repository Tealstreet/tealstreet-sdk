"""Compile independent Store/property/await-free splice faults; restore exact bytes."""
from pathlib import Path
import os
import subprocess
import sys

sdk = Path(__file__).resolve().parents[2]
logs = Path(os.environ.get('STORE_CORRECTNESS_LOG_DIR', '/tmp/store-correctness-controls'))
logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, CARGO_INCREMENTAL='0')
faults = [
    ('early-key', 'src/js_runtime/native_store/graph.rs',
     'set_id_key(index, &value.get("id")?, self.get(key)?.get("length")?)?;',
     'let id = id_key(&value.get("id")?)?; set_id_key(index, &id, self.get(key)?.get("length")?)?;',
     'js_store_symbol_keys', 'indexed_upsert_preserves_source_assignment_coercion_order'),
    ('symbol-key', 'src/js_runtime/native_store/graph.rs',
     'if matches!(primitive, Js::SymbolIterator) {',
     'if false && matches!(primitive, Js::SymbolIterator) {',
     'js_store_symbol_keys', 'symbol_ids_remain_distinct_after_computed_property_coercion'),
    ('missing-splice', 'src/js_runtime/value/array.rs',
     '        "splice",\n', '',
     'js_store_id_matching', 'singular_store_ids_match_actual_source_property_access_and_lookup_rules'),
    ('late-length', 'src/js_runtime/builtins.rs',
     'this.splice_array(len, start, count, args.into_iter().skip(2).collect())?;',
     'this.splice_array(array_length(&this)?, start, count, args.into_iter().skip(2).collect())?;',
     'js_array_splice', 'installed_splice_captures_length_before_coercion_and_deleted_getters'),
]

def run(label, target, test, red=False):
    result = subprocess.run(['cargo', 'test', '--locked', '-p', 'safe-cex-exchange', '--test', target,
        '--jobs', '1', test, '--', '--exact'], cwd=sdk, env=env,
        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (label + '.log')).write_text(result.stdout)
    assert 'Finished `test`' in result.stdout and 'error[E' not in result.stdout, label + ': must compile'
    if red:
        assert result.returncode == 101 and f'test {test} ... FAILED' in result.stdout and 'panicked at' in result.stdout, label
    else:
        assert result.returncode == 0 and '1 passed; 0 failed' in result.stdout, label
    print(label + (': compiled named RED' if red else ': exact-restored GREEN'), flush=True)

selected = set(sys.argv[1:])
assert not selected.difference(row[0] for row in faults), "unknown control"
for label, file, needle, fault, target, test in faults:
    if selected and label not in selected:
        continue
    path = sdk / 'safe-cex-exchange' / file
    original = path.read_bytes()
    assert original.decode().count(needle) == 1, label
    run(label + '-baseline', target, test)
    try:
        path.write_text(original.decode().replace(needle, fault))
        run(label + '-red', target, test, True)
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    run(label + '-restored', target, test)
