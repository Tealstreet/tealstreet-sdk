"""One compiled production mutation for each of eight new order execution rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'order_execution', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/order_execution.rs'
original = path.read_text()
controls = [('ioc-price', '    Ok(result)\n}\npub async fn build_main_order_payload', '    Ok(h.number(number(h, result)? + 1.0))\n}\npub async fn build_main_order_payload', 'ioc_native_execution_price'), ('payload-stp', '("stp", h.literal("EXPIRE_TAKER"))', '("stp", h.literal("WRONG"))', 'payload_native_stp'), ('batch-correlation', 'call(h, "Map.get", vec![map.clone(), client])', 'call(h, "Map.get", vec![map.clone(), cat(h, vec![client, h.literal("WRONG")])?])', 'batch_native_client_correlation'), ('bracket-child-ids', 'call(h, "push", vec![ids.clone(), h.get(&data, "id")?])?;', 'call(h, "push", vec![ids.clone(), h.literal("WRONG")])?;', 'brackets_native_child_ids'), ('amend-stable-id', '("id", h.get(&order, "id")?)', '("id", h.literal("WRONG"))', 'amend_native_stable_id'), ('constructor-rate-limit', '("maxRPS", h.number(10.0))', '("maxRPS", h.number(11.0))', 'constructor_native_rate_limit'), ('book-registration', 'call(h, "Set.add", vec![callbacks.clone(), callback.clone()])?;', 'call(h, "Set.delete", vec![callbacks.clone(), callback.clone()])?;', 'book_listener_native_callback_set'), ('history-class-descriptor', 'define(&parent, "originalError", undefined(), true, true, true)?;', 'define(&parent, "originalError", undefined(), false, true, true)?;', 'history_class_native_field_descriptor')]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'order-execution-{label}.log').write_text(output)
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
    (LOGS / 'order-execution-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'order-execution-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 8
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
