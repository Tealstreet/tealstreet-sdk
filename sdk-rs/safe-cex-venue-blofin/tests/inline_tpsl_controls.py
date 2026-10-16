"""One row mutation and four production boundary controls, sequential whole-corpus fail/restore tests."""
import os
import pathlib
import subprocess
import tomllib
ROOT = pathlib.Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-blofin/src/inline_tpsl.rs'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-blofin', '--locked', '--jobs', '1', '--test', 'inline_tpsl', '--', '--nocapture']
MUTATIONS = {
    'BlofinExchange#addInlineTpSlFallbackOrders': ('("tpTriggerPrice", OrderType::TakeProfit, "__take_profit")', '("slTriggerPrice", OrderType::TakeProfit, "__take_profit")', 'inline_take_profit_price'),
}
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_required'
rows = tomllib.loads((ROOT / 'sdk-rs/parity/manifest/exchanges/blofin/blofin.exchange.ts.toml').read_text())['symbol']
assert {r['ts'] for r in rows if r['status'] != 'pending' and any('/inline_tpsl.rs::' in target for target in (r['rs'] if isinstance(r['rs'], list) else [r['rs']]))} == set(MUTATIONS), 'inline_tpsl_row_mutation_census'
MUTATIONS.update({
    'stop_loss_field_control': ('("slTriggerPrice", OrderType::StopLoss, "__stop_loss")', '("tpTriggerPrice", OrderType::StopLoss, "__stop_loss")', 'inline_stop_loss_price'),
    'native_quantity_control': ('remaining_native: Some(native),', 'remaining_native: Some({let _ = native; remaining}),', 'inline_fallback_native_quantity'),
    'dedup_control': ('if !protective.contains(&key) {', 'if true {', 'inline_order_count'),
    'trim_control': ('let parent = js_trim(&parent);', 'let parent = { let _ = js_trim(&parent); parent.trim() };', 'inline_order_count'),
})
original = SOURCE.read_text()
logs = ROOT / 'sdk-rs/target/blofin-inline_tpsl-controls'
logs.mkdir(exist_ok=True)
for name, (old, new, assertion) in MUTATIONS.items():
    assert original.count(old) == 1, name + '_anchor'
    try:
        SOURCE.write_text(original.replace(old, new))
        result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (logs / (name + '.fail.log')).write_text(result.stdout)
        assert result.returncode == 101 and 'panicked at' in result.stdout and assertion in result.stdout, result.stdout
        print(f'{name}: FAIL {assertion} exit=101', flush=True)
    finally:
        SOURCE.write_text(original)
    result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / (name + '.restored.log')).write_text(result.stdout)
    assert result.returncode == 0, result.stdout
    print(f'{name}: RESTORED exit=0', flush=True)
