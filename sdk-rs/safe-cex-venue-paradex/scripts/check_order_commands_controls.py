"""One compiled production mutation for each of nine new order command rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'order_commands', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/order_commands.rs'
original = path.read_text()
controls = [
 ('client-id-prefix','u("ts_")','u("xs_")','client_id_native_value'),
 ('single-main-id','let mut ids = vec![main.clone()];','let mut ids = vec![];','place_native_ids'),
 ('mixed-batch-size','if batch.len() == 10 {','if batch.len() == 9 {','mixed_native_chunks'),
 ('main-response-id','        h.get(&data, "id")\n    }','        h.get(&data, "client_id")\n    }','main_native_id'),
 ('cancel-one-endpoint','h.replace_endpoint("/orders/{id}", "{id}", &id)?','h.replace_endpoint("/wrong/{id}", "{id}", &id)?','cancel_one_native_endpoint'),
 ('cancel-pair-status','same(h, &status, "QUEUED_FOR_CANCELLATION")','same(h, &status, "WRONG")','batch_cancel_native_success'),
 ('cancel-many-stride','offset += 2.0;','offset += 3.0;','cancel_many_native_chunks'),
 ('cancel-all-endpoint','h.delete(u("/orders"), None)?.await?;','h.delete(u("/wrong"), None)?.await?;','cancel_all_native_endpoint'),
 ('cancel-symbol-market','("market", symbol.clone())','("symbol", symbol.clone())','cancel_symbol_native_config'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'order-commands-{label}.log').write_text(output)
    return result.returncode, output


LOGS.mkdir(parents=True, exist_ok=True)
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, needle, replacement, assertion in controls:
    assert original.count(needle) == 1, (name, 'mutation site changed')
    try:
        path.write_text(original.replace(needle, replacement))
        mutated, output = run(name)
    finally:
        path.write_text(original)
    record = dict(control=name, assertion=assertion, mutation_exit=mutated,
                  compiled_runtime_failure=mutated == 101 and 'test result: FAILED' in output)
    if mutated == 0:
        report['survivors'].append(name)
    report['controls'].append(record)
    (LOGS / 'order-commands-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'order-commands-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 9
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
