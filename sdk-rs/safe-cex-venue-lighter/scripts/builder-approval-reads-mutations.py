"""Compile controlled regressions, require named failures, restore and retest."""
from pathlib import Path
import json
import os
import subprocess

sdk = Path(__file__).resolve().parents[2]
root = sdk.parent
source = sdk / 'safe-cex-venue-lighter/src/builder_approval_reads.rs'
logs = sdk / 'target/lighter-approval-mutations'
logs.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
original = source.read_bytes()
controls = [
    ('caps', 'approval_reads_caps', '(r.get)(h, params, "feeCaps")?', '(r.get)(h, params, "feeValue")?', 'approval_reads::native result'),
    ('wasm', 'approval_reads_clients', 'if !(r.wasm_ready)(h)? {', 'if (r.wasm_ready)(h)? {', 'approval_reads::native init receipts "wasm-false"'),
    ('deadline', 'approval_reads_limits_tier', '.floor() + 60.0', '.floor() + 61.0', 'approval_reads::native requests "limits-ready"'),
    ('warning', 'approval_reads_limits_tier', '(r.warn_tier_lookup_failure)(h, &error);', 'drop(error);', 'approval_reads::native tier fallback warning "tier-best-effort"'),
    ('snapshot', 'approval_reads_accounts', '"max_perps_maker_fee"', '"max_perps_taker_fee"', 'approval_reads::native result "snapshot-full"'),
]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results = []
for name, test, before, after, assertion in controls:
    assert original.decode().count(before) == 1, name
    command = [env.get('CARGO', '/opt/homebrew/opt/rustup/bin/cargo'), 'test', '--manifest-path', str(sdk / 'Cargo.toml'), '-p', 'safe-cex-venue-lighter', '--jobs', '2', '--test', 'live_builder_approval_reads', '--', test, '--exact']
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
