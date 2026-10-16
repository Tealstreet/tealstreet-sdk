"""Compile controlled regressions, require named failures, restore and retest."""
from pathlib import Path
import json
import os
import subprocess

sdk = Path(__file__).resolve().parents[2]
root = sdk.parent
source = sdk / 'safe-cex-venue-lighter/src/signer_runtime.rs'
logs = sdk / 'target/lighter-signer-mutations'
logs.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
original = source.read_bytes()
controls = [
    ('constructor', 'signer_initialize', '            account_index,\n            api_key_index,', '            account_index: api_key_index,\n            api_key_index: account_index,', 'signer::native constructor account index'),
    ('initialization', 'signer_initialize', '"wasm not initialized".encode_utf16()', '"wasm ready".encode_utf16()', 'signer::native outcome "init-race":0'),
    ('submission', 'signer_submissions', '(r.delay)(&r.host, 150)', '(r.delay)(&r.host, 151)', 'signer::native requests "createOrder-nonce"'),
    ('lock', 'signer_submission_lock', 'let prev = self.submission_lock.replace(next);', 'let previous = self.submission_lock.replace(next); drop(previous); let prev = NativePromise::settled(r.promise_runtime.clone(), Ok(()));', 'signer::native blocked requests "queue-ok"'),
    ('reset-receiver', 'signer_submissions', 'let client = self.transaction_client.borrow().clone();\n        let method = (r.method)(&r.host, client, "resetNonce")?;', 'let client = self.api_client.clone();\n        let method = (r.method)(&r.host, client, "resetNonce")?;', 'signer::native reset receiver "createOrder-nonce"'),
]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results = []
for name, test, before, after, assertion in controls:
    assert original.decode().count(before) == 1, name
    command = [env.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'test', '--manifest-path', str(sdk / 'Cargo.toml'), '-p', 'safe-cex-venue-lighter', '--jobs', '2', '--test', 'live_signer_runtime', '--', test, '--exact']
    try:
        source.write_text(original.decode().replace(before, after))
        failed = subprocess.run(command, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (logs / f'{name}-broken.log').write_text(failed.stdout)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (logs / f'{name}-restored.log').write_text(restored.stdout)
    result = dict(control=name, sha=sha, test=test, assertion=assertion, broken=failed.returncode, restored=restored.returncode, caught=assertion in failed.stdout and 'assertion `left == right` failed' in failed.stdout and 'test result: FAILED' in failed.stdout and 'error[E' not in failed.stdout and failed.returncode == 101)
    results.append(result)
    (logs / 'results.json').write_text(json.dumps(results, indent=2))
    print(json.dumps(result), flush=True)
    assert result['caught'], f'Mutation survivor or invalid failure: {name}; see {logs}'
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, name
assert source.read_bytes() == original
