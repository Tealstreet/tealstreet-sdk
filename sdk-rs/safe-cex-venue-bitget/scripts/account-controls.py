"""Account field and corpus controls; every mutation is restored byte-for-byte."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[3]
production = root / 'sdk-rs/safe-cex-venue-bitget/src/account_read.rs'
oracle = root / 'sdk-rs/safe-cex-venue-bitget/scripts/account-capture.cjs'
selected = set(sys.argv[1:])
controls = [
    ('parent-user', production, 'if js_truthy(parent) { parent } else { user }', 'if js_truthy(parent) { user } else { parent }', 'ACCOUNT native user identity'),
    ('sub-affiliate', production, 'sub_id: Some(sub_id),', 'sub_id: Some(if true { affiliate_id.clone() } else { sub_id }),', 'ACCOUNT native sub identity'),
    ('affiliate-sub', production, 'sub_id: Some(sub_id),\n        affiliate_id: Some(affiliate_id),', 'sub_id: Some(sub_id.clone()),\n        affiliate_id: Some(if true { sub_id } else { affiliate_id }),', 'ACCOUNT native affiliate identity'),
    ('missing-null', production, 'sub_id: Some(sub_id),', 'sub_id: sub_id.map(Some),', 'ACCOUNT native sub identity'),
    ('shape-error', production, "Cannot read properties of null (reading 'data')", "Cannot read properties of undefined (reading 'data')", 'ACCOUNT native shape error'),
    ('endpoint', production, 'entry.name == "ACCOUNT_SPOT"', 'entry.name == "ORDER_FILLS"', 'ACCOUNT native request'),
    ('read-allocation', production, 'read(url).await?', 'read(url).await.map_err(|_| { let mut error = ExchangeError::new("original read"); error.name = "ReadFailure".into(); E::from(error) })?', 'ACCOUNT original read identity'),
    ('skip-await', production, 'let response = read(url).await?;', 'let _pending = read(url); let response = Some(serde_json::json!({"data":{}}));', 'ACCOUNT read remains pending'),
    ('read-identity', production, 'read(url).await?', 'read(url).await.map_err(|_| ExchangeError::new("flattened"))?', 'ACCOUNT native read metadata'),
    ('empty-source', oracle, 'const boundaries = [];', 'cases.splice(0);\n  const boundaries = [];', 'ACCOUNT required native input'),
    ('subset-source', oracle, 'const boundaries = [];', 'cases.splice(1);\n  const boundaries = [];', 'ACCOUNT required native input'),
    ('extra-source', oracle, 'const boundaries = [];', 'cases.push(cases[0]);\n  const boundaries = [];', 'ACCOUNT source consumed count'),
    ('actual-input-value', oracle, 'const cases = [];', 'values[0] = null;\n  const cases = [];', 'ACCOUNT input content'),
    ('same-length-source', oracle, 'const boundaries = [];', 'cases.fill(cases[0]);\n  const boundaries = [];', 'ACCOUNT input content'),
]
assert not (selected - {item[0] for item in controls}), 'unknown control selector'
env = dict(os.environ)
env['CARGO_BUILD_JOBS'] = '1'
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
command = ['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','-p','safe-cex-venue-bitget','--test','account_read','--jobs','1','--','--include-ignored']
for name, file, before, after, assertion in controls:
    if selected and name not in selected:
        continue
    original = file.read_bytes()
    assert original.decode().count(before) == 1, name
    try:
        file.write_text(original.decode().replace(before, after, 1))
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True)
        output = result.stdout + result.stderr
        log = Path(tempfile.gettempdir()) / f'bitget-account-{name}.log'
        log.write_text(output)
        assert result.returncode == 101 and assertion in output, f'{name}: {result.returncode}; {log}'
        assert 'could not compile' not in output, name
        print(name, result.returncode, assertion, flush=True)
    finally:
        file.write_bytes(original)
restored = subprocess.run(command,cwd=root,env=env)
assert restored.returncode == 0, 'ACCOUNT restored target'
print('restored', restored.returncode, 'ACCOUNT target: 2 passed, 0 ignored', flush=True)
