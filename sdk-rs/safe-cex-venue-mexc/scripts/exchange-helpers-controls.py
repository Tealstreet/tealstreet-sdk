"""Substitute helper mistakes and require each to fail by its named source."""
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
helpers = crate / 'src/exchange_helpers.rs'
symbol_config = crate / 'src/symbol_config.rs'
command = [
    'cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked',
    '-p', 'safe-cex-venue-mexc', '--test', 'exchange_helpers', '--jobs', '2',
    '--', '--exact', 'mexc_exchange_helpers_live_typescript', '--nocapture',
]
controls = [
    ('extractRows', helpers,
     '.and_then(|v| v.get("data"))\n            .and_then(|v| js_object_or_array_entry(v, key))',
     '.and_then(|v| v.get("data"))\n            .and_then(|v| v.get(key))'),
    ('isPlanOrder', helpers,
     'is_stop_like_order(order) && !is_stop_order(order)',
     'is_stop_like_order(order)'),
    ('planOrderTriggerType', helpers,
     '(opts.r#type == OrderType::TakeProfit) == closes_long',
     '(opts.r#type == OrderType::TakeProfit) != closes_long'),
    ('findOpenPosition', helpers,
     'position.contracts > 0.0', 'position.contracts >= 0.0'),
    ('formatChangeLeveragePayload', symbol_config,
     'MarginType::Isolated => 1,', 'MarginType::Isolated => 2,'),
    ('findOpenPosition', helpers,
     '''    positions.iter().find(|position| {
        position.symbol == symbol && position.side == side && position.contracts > 0.0
    })''',
     '''    positions.iter().find(|position| {
        position.symbol == symbol && position.side == side && position.contracts > 0.0
    }).or_else(|| positions.last())'''),
]

for name, target, before, after in controls:
    original = target.read_text()
    assert original.count(before) == 1, f'{name}: mutation anchor should occur once'
    try:
        target.write_text(original.replace(before, after))
        result = subprocess.run(
            command, cwd=crate.parents[1], text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        assert result.returncode == 101, result.stdout
        assert 'mexc_exchange_helpers_live_typescript ... FAILED' in result.stdout, result.stdout
        assert name in result.stdout and 'assertion `left == right` failed' in result.stdout, result.stdout
        print(f'Rust {name}: named assertion fails at 101', flush=True)
    finally:
        target.write_text(original)

subprocess.run(command, cwd=crate.parents[1], check=True)
