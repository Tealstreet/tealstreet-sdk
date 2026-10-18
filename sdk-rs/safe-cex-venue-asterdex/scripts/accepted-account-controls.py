#!/usr/bin/env python3
"""Corrupt all eleven real function bodies; require named runtime failures, restore."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
logs = workspace / 'target/asterdex-account-function-controls'
logs.mkdir(parents=True, exist_ok=True)
result = workspace / 'target/asterdex-account-function-results.json'
runner = ['node', str(crate / 'scripts/accepted-account-contracts.mjs')]
changes = {
    'src/account_fetch.rs': [
        ('serde_json::from_value(value).map_err(|error| {',
         'serde_json::from_value({ let _ = value; serde_json::Value::Null }).map_err(|error| {'),
        ('error.name = name.into();', 'error.name = format!("control:{name}");'),
    ],
    'src/account_helpers.rs': [
        ('remaining: amount - filled,', 'remaining: amount - filled + 1.0,'),
        ('remaining: (amount - filled).max(0.0),', 'remaining: (amount - filled).max(0.0) + 1.0,'),
        ('effects.push(AccountUpdateEffect::EmitAccount(data));', 'let _ = AccountUpdateEffect::EmitAccount(data);'),
        ('asset: asset.into(),', 'asset: format!("control:{asset}"),'),
        ('.map_or(0.0, |p| p.unrealized_pnl);', '.map_or(1.0, |p| p.unrealized_pnl);'),
    ],
    'src/api_helpers.rs': [
        ('result.map_err(safe_cex_exchange::runtime::ExchangeError::new)',
         'result.map(|_| String::new()).map_err(safe_cex_exchange::runtime::ExchangeError::new)'),
        ('.map(|r| r.signature)', '.map(|r| { let _ = r; String::new() })'),
    ],
    'src/types.rs': [
        ('T::deserialize(d).map(Some)', 'T::deserialize(d).map(|value| { let _ = value; None })'),
        ('serde_json::Value::Object(fields) => Ok(fields),',
         'serde_json::Value::Object(fields) => Ok({ let _ = fields; serde_json::Map::new() }),'),
    ],
}
originals = {crate / relative: (crate / relative).read_bytes() for relative in changes}
receipt = {}
try:
    for relative, replacements in changes.items():
        file = crate / relative
        source = originals[file].decode()
        for before, after in replacements:
            assert source.count(before) == 1, (relative, before, source.count(before))
            source = source.replace(before, after)
        file.write_text(source)
    if result.exists():
        result.unlink()
    with (logs / 'compiled-production-red.log').open('w') as output:
        red = subprocess.run(runner, cwd=workspace, env=os.environ, stdout=output, stderr=subprocess.STDOUT)
    assert red.returncode != 0, 'production faults unexpectedly passed'
    assert result.exists(), 'no runtime receipts: compiler failures cannot prove a control'
    rows = json.loads(result.read_text())
    assert len(rows) == 11 and all(row['failures'] for row in rows), rows
    assert all(not row.get('classificationOnly') for row in rows), 'all eleven owners must execute'
    receipt['namedRuntimeFailures'] = {row['name']: row['failures'] for row in rows}
    print('Production corruption: all 11 compiled native owners fail by name.', flush=True)
finally:
    for file, original in originals.items():
        file.write_bytes(original)
        assert file.read_bytes() == original
    receipt['restoredSha256'] = {str(file.relative_to(crate)): hashlib.sha256(raw).hexdigest()
                               for file, raw in originals.items()}
    (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
with (logs / 'exact-restoration-green.log').open('w') as output:
    green = subprocess.run(runner, cwd=workspace, env=os.environ, stdout=output, stderr=subprocess.STDOUT)
assert green.returncode == 0, 'exact restoration must pass all eleven native owners'
print('Exact restoration: all 11 native owners PASS.', flush=True)
