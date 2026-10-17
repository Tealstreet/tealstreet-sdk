"""Fault the real Store bridge, require compiled named RED, restore exact bytes."""
from pathlib import Path
import subprocess

sdk = Path(__file__).resolve().parents[2]
source = sdk / 'safe-cex-exchange/src/js_runtime/store.rs'
original = source.read_bytes()
logs = sdk / 'target/okx-terminal-store-controls'
logs.mkdir(parents=True, exist_ok=True)
controls = [
    ('missing-bridge', '"markOrdersTerminal",', '"missingMarkOrdersTerminal",',
     'safe-cex-venue-okx', 'exchange_binding',
     'private_orders_commit_terminal_states_before_final_fill_and_reject_stale_snapshots',
     'OKX private terminal frame commits native status filled'),
    ('missing-terminal-memory', 'st.mark_orders_terminal(&refs, status).map_err(mutation)?;',
     'st.update_orders_status(&refs, status).map_err(mutation)?;',
     'safe-cex-venue-okx', 'exchange_binding',
     'private_orders_commit_terminal_states_before_final_fill_and_reject_stale_snapshots',
     'OKX private terminal memory refuses stale open REST snapshot filled'),
    ('empty-store-shortcut', '// Remember IDs even before any matching order reaches the Store.',
     'if st.orders().is_empty() { return Ok(Js::Undefined); }',
     'safe-cex-exchange', 'js_store_terminal',
     'terminal_memory_precedes_orders_and_filters_only_matching_stale_ids',
     'JS terminal bridge remembers an ID before any order exists'),
    ('discard-subscriber-error', 'st.mark_orders_terminal(&refs, status).map_err(mutation)?;',
     'let _ = st.mark_orders_terminal(&refs, status);',
     'safe-cex-exchange', 'js_store_terminal',
     'terminal_subscriber_error_preserves_committed_status_response_and_origin',
     'JS terminal bridge must propagate subscriber failure'),
    ('lossy-terminal-id', 'keys.push(to_typed::<String>(&order.get("id")?)?);',
     'keys.push(String::from_utf16_lossy(&order.get("id")?.to_js_string()?));',
     'safe-cex-exchange', 'js_store_terminal',
     'terminal_typed_ids_refuse_lone_surrogates_before_mutation',
     'terminal typed ID must refuse rather than replace a surrogate'),
]


def run(crate, target, test, log):
    command = ['cargo', 'test', '--locked', '--jobs', '1', '-p', crate]
    if crate == 'safe-cex-venue-okx':
        command += ['--all-features']
    command += ['--test', target, test, '--', '--exact']
    result = subprocess.run(command, cwd=sdk, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    (logs / log).write_text(result.stdout)
    return result


for label, site, replacement, crate, target, test, assertion in controls:
    assert source.read_bytes() == original, 'source changed between controls'
    text = original.decode()
    assert text.count(site) == 1, f'{label}: expected one production site'
    try:
        source.write_text(text.replace(site, replacement))
        red = run(crate, target, test, f'{label}-red.log')
        assert (red.returncode == 101 and f'test {test} ... FAILED' in red.stdout
                and assertion in red.stdout and 'error[E' not in red.stdout), f'{label}: expected named compiled RED\n{red.stdout}'
    finally:
        source.write_bytes(original)
    green = run(crate, target, test, f'{label}-restored.log')
    assert green.returncode == 0 and '1 passed; 0 failed' in green.stdout, f'{label}: restored failure\n{green.stdout}'
    print(f'{label}: compiled named RED 101; byte-restored PASS', flush=True)
assert source.read_bytes() == original
print(f'{len(controls)} controls passed', flush=True)
