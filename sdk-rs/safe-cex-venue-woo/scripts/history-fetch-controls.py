"""Compile independent orchestration mutations and require named failures."""
from pathlib import Path
import os
import subprocess

source = Path('sdk-rs/safe-cex-venue-woo/src/history_fetch.rs')
original = source.read_text()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--offline', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'history_fetch']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH=str(Path.cwd() / 'node_modules/typescript'))
controls = [
    ('direct', 'return request_rows(adapter, query, None).await;', 'return fetch_legacy_transaction_history_rows(adapter, query).await;', 'HISTORY native request sequence'),
    ('legacy', 'let rows = fetch_legacy_transaction_history_rows(adapter, query).await?;', 'let _unpolled = fetch_legacy_transaction_history_rows(adapter, query); let rows = Vec::<Value>::new();', 'HISTORY native request sequence'),
    ('precedence', 'let rows = fetch_legacy_transaction_history_rows(adapter, query).await?;', 'if let Some(error) = last_error { return Err(error); }\n    let rows = fetch_legacy_transaction_history_rows(adapter, query).await?;', 'HISTORY native request sequence'),
    ('empty', 'Ok(crate::response::parse_transaction_rows_from_response(&data).to_vec())', 'Ok({ let _rows = crate::response::parse_transaction_rows_from_response(&data); Vec::new() })', 'HISTORY native row cardinality'),
    ('subset', 'Ok(crate::response::parse_transaction_rows_from_response(&data).to_vec())', 'Ok(crate::response::parse_transaction_rows_from_response(&data).iter().take(2).cloned().collect())', 'HISTORY native row cardinality'),
    ('extra', 'Ok(crate::response::parse_transaction_rows_from_response(&data).to_vec())', 'Ok(crate::response::parse_transaction_rows_from_response(&data).iter().cloned().chain(std::iter::once(Value::Null)).collect())', 'HISTORY native row cardinality'),
]
try:
    for name, before, after, assertion in controls:
        assert original.count(before) == 1, name
        source.write_text(original.replace(before, after))
        run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-history-control-' + name + '.log').write_text(run.stdout)
        assert run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout, (name, run.stdout)
        print(name, '101', assertion, flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
assert run.returncode == 0, run.stdout
print('restored target passed', flush=True)
