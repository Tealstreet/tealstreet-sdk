"""One native-value fault per newly ported spot contract, serial Cargo."""
from pathlib import Path
import json, os, subprocess
crate = Path(__file__).resolve().parents[1]
rust = crate / 'src/spot_contracts.rs'
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR'])
logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
controls = [
    ('venue-name', 'spot_name_native', 'ExchangeName::Polymarket', 'ExchangeName::Binance'),
    ('position-mode', 'spot_mode_native', 'PositionMode::OneWay', 'PositionMode::Hedged'),
    ('max-leverage', 'spot_leverage_native', '    1.0\n', '    2.0\n'),
    ('leverage-warning', 'spot_log_native_message:setLeverage', 'Leverage is not supported on Polymarket (always 1x)', 'Leverage is supported on Polymarket'),
    ('position-warning', 'spot_log_native_message:changePositionMode', 'Position mode changes not supported on Polymarket', 'Position mode changed on Polymarket'),
    ('margin-warning', 'spot_log_native_message:changeMarginType', 'Margin type changes not supported on Polymarket', 'Margin type changed on Polymarket'),
]
# Concrete finding: an await-only oracle let deferred async warning calls pass.
# Reinstall that earlier implementation; the before-poll native guard must fail.
source = rust.read_text()
start = source.index('pub fn set_leverage<E>(')
end = source.index('/// Source `PolymarketExchange.changePositionMode`', start)
eager = source[start:end]
deferred = eager.replace('pub fn set_leverage', 'pub async fn set_leverage').replace('Ready<Result<(), E>>', 'Result<(), E>').replace('    ready(log(', '    log(').replace('    ))\n}', '    )?;\n    Ok(())\n}')
assert eager != deferred
controls.append(('eager-prefix', 'spot_warning_native_eager:setLeverage', eager, deferred))
assert len(controls) == 7, 'spot_control_floor'
assert controls[-1][0] == 'eager-prefix', 'spot_control_last_iteration'
assert not subprocess.check_output(['git', 'status', '--porcelain', '--', str(crate), str(crate.parent/'parity/manifest/exchanges/polymarket')], cwd=crate, text=True).strip(), 'commit lane files before binding gates to HEAD'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent/'Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'spot_contracts', '--locked', '--jobs', '2', '--', '--nocapture']
def run(name):
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs/(name+'.log')).write_text(f'exit_code={result.returncode}\n'+result.stdout)
    assert result.stdout.count('polymarket spot oracle: one Node invocation for the entire corpus') == 1, result.stdout
    return result
baseline = run('baseline')
assert baseline.returncode == 0, baseline.stdout
summary = {'sha': subprocess.check_output(['git','rev-parse','HEAD'], cwd=crate, text=True).strip(), 'baseline_exit': baseline.returncode, 'floor': '7 controls, stopped after eager-prefix', 'controls': []}
original = rust.read_text()
for name, assertion, old, new in controls:
    assert original.count(old) == 1, old
    try:
        rust.write_text(original.replace(old, new))
        mutated = run(name+'-mutation')
        caught = mutated.returncode == 101 and assertion in mutated.stdout
    finally:
        rust.write_text(original)
    restored = run(name+'-restored')
    summary['controls'].append({'name': name, 'assertion': assertion, 'mutation_exit': mutated.returncode, 'restored_exit': restored.returncode, 'caught': caught})
    (logs/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    assert restored.returncode == 0, restored.stdout
    assert caught, f'FINDING survivor or wrong failure at {name}:\n{mutated.stdout}'
    print(f'{name}: 101 {assertion}; restored 0', flush=True)
