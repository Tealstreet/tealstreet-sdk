"""Inject consuming Array::next_element_seed faults; never change shared production permanently."""
from pathlib import Path
import json
import os
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
shared = crate.parent / 'safe-cex-num/src/numeric_ingress.rs'
harness = crate / 'tests/auth_types.rs'
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR']); logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
original_shared = shared.read_text()
original_harness = harness.read_text()
start = original_shared.index("impl<'de> SeqAccess<'de> for Array {")
point = original_shared.index('        self.0\n            .next()', start)
controls = [(side, mode, False) for side in ['bids', 'asks', 'hashes'] for mode in ['empty', 'subset']]
controls.append(('bids', 'empty', True))
summary = {'sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=crate, text=True).strip(), 'controls': []}

def run(name, field):
    target = 'order_response_record_actual_typescript' if field == 'hashes' else 'order_book_record_actual_typescript'
    command = ['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','auth_types','--locked','--jobs','1','--',target,'--nocapture']
    r = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs/(name+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout)
    assert r.stdout.count('polymarket auth oracle: one Node invocation for the entire corpus') == 1, r.stdout
    return r

assert run('consuming-baseline', 'bids').returncode == 0
for field, mode, bypass in controls:
    name = f'consume-{field}-{mode}' + ('-bypass-lengths' if bypass else '')
    # Select the existing actual-TS sample side by its distinct first price.
    # This keeps asks independently reachable instead of always failing bids first.
    if field == 'hashes':
        predicate = 'matches!(&v.0, Node::Text(_))'
    else:
        price = '0.123400' if field == 'bids' else '0.234500'
        predicate = 'matches!(&v.0, Node::Object(fields) if fields.iter().any(|(key, value)| key == "price" && matches!(&value.0, Node::Text(price) if price == "'+price+'")))'
    condition = 'self.0.as_slice().first().is_some_and(|v| '+predicate+')'
    if mode == 'empty':
        body = 'self.0.by_ref().for_each(drop);\n            return Ok(None);'
    else:
        body = 'let first = self.0.next();\n            self.0.by_ref().for_each(drop);\n            return first.map(|value| seed.deserialize(JsNumberDeserializer(value.0))).transpose();'
    injected = f'        if {condition} {{\n            {body}\n        }}\n'
    assertion = ('native_order_response.transactions_hashes.length' if field == 'hashes' else f'native_book.{field}.length')
    try:
        shared.write_text(original_shared[:point]+injected+original_shared[point:])
        if bypass:
            changed = re.sub(r'fn assert_book_source_count\(source: &Value, expected: usize\) \{[\s\S]*?\n\}', 'fn assert_book_source_count(_source: &Value, _expected: usize) {}', original_harness, count=1)
            old = '        assert_eq!(levels.len(), expected.len(), "native_book.{name}.length");\n'
            assert changed.count(old) == 1
            harness.write_text(changed.replace(old, ''))
            assertion = 'native_book.level_comparison_count'
        r = run(name+'-mutation', field)
        assert r.returncode == 101 and assertion in r.stdout, r.stdout
    finally:
        shared.write_text(original_shared)
        harness.write_text(original_harness)
    restored = run(name+'-restored', field)
    assert restored.returncode == 0, restored.stdout
    summary['controls'].append({'name':name,'mutation_exit':r.returncode,'assertion':assertion,'restored_exit':restored.returncode})
    (logs/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(f'{name}: 101 {assertion}; restored 0',flush=True)
