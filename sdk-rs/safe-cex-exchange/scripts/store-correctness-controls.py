"""Compile independent Store, terminal-memory and splice faults; restore exact bytes."""
from pathlib import Path
import os
import signal
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
    ('terminal-registration', 'src/js_runtime/native_store/graph.rs',
     '            "markOrdersTerminal",', '            "missingMarkOrdersTerminal",',
     'js_store_terminal', 'terminal_memory_precedes_orders_and_filters_only_matching_stale_ids'),
    ('terminal-memory', 'src/js_runtime/native_store/graph.rs',
     '.call_method("set", vec![id, Js::Number(now)])?;',
     '.call_method("has", vec![id, Js::Number(now)])?;',
     'js_store_terminal', 'terminal_memory_precedes_orders_and_filters_only_matching_stale_ids'),
    ('terminal-empty-store', 'src/js_runtime/native_store/graph.rs',
     '    pub fn mark_terminal(&self, selectors: &Js, status: &Js) -> Result<(), Thrown> {',
     '    pub fn mark_terminal(&self, selectors: &Js, status: &Js) -> Result<(), Thrown> {\n'
     '        if length(&self.get("orders")?)? == 0 { return Ok(()); }',
     'js_store_terminal', 'terminal_memory_precedes_orders_and_filters_only_matching_stale_ids'),
    ('terminal-subscriber-error', 'src/js_runtime/native_store/graph.rs',
     '"markOrdersTerminal" => self.mark_terminal(&a, &b)?,',
     '"markOrdersTerminal" => { let _ = self.mark_terminal(&a, &b); },',
     'js_store_terminal', 'terminal_subscriber_error_preserves_committed_status_response_and_origin'),
    ('terminal-id-replacement', 'src/js_runtime/native_store/graph.rs',
     '.call_method("set", vec![id, Js::Number(now)])?;',
     '.call_method("set", vec![Js::str(&String::from_utf16_lossy(&id.to_js_string()?)), Js::Number(now)])?;',
     'js_store_terminal', 'terminal_raw_ids_preserve_surrogates_without_replacement_aliasing'),
]
terminal_assertions = {
    'terminal-registration': 'JS terminal bridge exposes callable markOrdersTerminal',
    'terminal-memory': 'JS terminal bridge remembers an ID before any order exists',
    'terminal-empty-store': 'JS terminal bridge remembers an ID before any order exists',
    'terminal-subscriber-error': 'JS terminal bridge must propagate subscriber failure',
    'terminal-id-replacement': 'raw terminal memory preserves replacement-character row identity',
}

def run(label, target, test, red=False):
    result = subprocess.run(['cargo', 'test', '--locked', '-p', 'safe-cex-exchange', '--test', target,
        '--jobs', '1', test, '--', '--exact'], cwd=sdk, env=env,
        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (label + '.log')).write_text(result.stdout)
    assert 'Finished `test`' in result.stdout and 'error[E' not in result.stdout, label + ': must compile'
    if red:
        assert result.returncode == 101 and f'test {test} ... FAILED' in result.stdout and 'panicked at' in result.stdout, label
        assertion = terminal_assertions.get(label.removesuffix('-red'))
        assert assertion is None or assertion in result.stdout, label + ': wrong failure'
    else:
        assert result.returncode == 0 and '1 passed; 0 failed' in result.stdout, label
    phase = 'compiled named RED' if red else 'baseline GREEN' if label.endswith('-baseline') else 'exact-restored GREEN'
    print(label + ': ' + phase, flush=True)

selected = set(sys.argv[1:])
assert not selected.difference(row[0] for row in faults), "unknown control"

def stop(signum, _frame):
    raise SystemExit(128 + signum)

signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
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
