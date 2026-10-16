"""Compile independent caller mutations, require named failures, then restore."""
from pathlib import Path
import os
import subprocess

source = Path('sdk-rs/safe-cex-venue-woo/src/historical_fills.rs')
original = source.read_text()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--offline', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'historical_fills']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=str(Path.cwd() / 'node_modules/typescript'))
controls = [
    ('public', 'if venue.is_public_data_only() || opts.symbol.is_empty()', 'if opts.symbol.is_empty()', 1, 'FILLS native query sequence'),
    ('millisecond-sort', '        limit,\n        true,', '        limit,\n        false,', 1, 'FILLS native field identities'),
    ('field-swap', 'qty: row.qty,', 'qty: row.price,', 2, 'FILLS native field identities'),
    ('early-filter', 'if let Some(row) = parse_transaction_history_row(row)', 'if row.get("symbol") == Some(&json!("PERP_ETH_USDT")) { continue; }\n        if let Some(row) = parse_transaction_history_row(row)', 1, 'FILLS native reporting order'),
    ('early-refusal', '    let mut rows = rows;\n    select(', '    let mut rows = rows;\n    for row in &rows { asset(row)?; }\n    select(', 1, 'FILLS discarded asset or later source error must not refuse'),
    ('empty', '    rows.into_iter()', '    rows.into_iter().filter(|_| false)', 2, 'FILLS native output cardinality'),
    ('subset', '    rows.into_iter()', '    rows.into_iter().take(1)', 2, 'FILLS native output cardinality'),
    ('extra', '    executions(rows)', '    executions(rows).map(|mut rows| { if let Some(row) = rows.first().cloned() { rows.push(row); } rows })', 1, 'FILLS native output cardinality'),
    ('logger-swallow', '            report(venue, error)?;', '            if report(venue, error).is_err() { return Ok(Vec::new()); }', 3, 'FILLS native reporting order'),
]
try:
    for name, before, after, count, assertion in controls:
        assert original.count(before) == count, (name, original.count(before))
        source.write_text(original.replace(before, after, 1))
        run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-historical-control-' + name + '.log').write_text(run.stdout)
        assert run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout, (name, run.stdout)
        print(name, '101', assertion, flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
assert run.returncode == 0, run.stdout
print('restored target passed', flush=True)
