"""One compiled native control for each newly mapped runtime body."""
from pathlib import Path
import json
import os
import subprocess

root = Path(__file__).resolve().parents[3]
out = root / 'sdk-rs/target/weex-runtime-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
controls = [
    ('qs-array-format', 'api_binding.rs', '"repeat"', '"indices"', 'runtime_serialize', 'runtime::native serialize_repeat'),
    ('api-base-url', 'api_binding.rs', '("baseURL", (r.text)(url))', '("baseURL", (r.text)(&format!("{url}/wrong")))', 'runtime_api', 'runtime::native api_timeout'),
    ('constructor-public-child', 'lifecycle.rs', '(r.construct)(h, "WeexPublicWebsocket", instance.clone())', '(r.construct)(h, "WeexPrivateWebsocket", instance.clone())', 'runtime_constructor', 'runtime::native constructor_fields'),
    ('validation-disabled', 'lifecycle.rs', 'if (r.strict_false)', 'if !(r.strict_false)', 'runtime_validation', 'runtime::native validate_disabled'),
    ('account-user-prefix', 'lifecycle.rs', 'format!("weex:{hex}")', 'format!("wrong:{hex}")', 'runtime_account', 'runtime::native account_hash'),
    ('start-loaded-market', 'lifecycle.rs', '&["markets"]', '&["tickers"]', 'runtime_start', 'runtime::native start_public'),
    ('tick-position-value', 'lifecycle.rs', '(r.call)(h, add, vec![positions])?;\n            update(h, r, vec![("balance", balance)], &["balance", "positions"])', '(r.call)(h, add, vec![balance])?;\n            update(h, r, vec![("balance", positions)], &["balance", "positions"])', 'runtime_tick', 'runtime::native tick_success'),
    ('income-page-cap', 'income_reads.rs', 'math_max(limit, 1.0), 100.0', 'math_max(limit, 1.0), 99.0', 'runtime_income_page', 'runtime::native income_symbol'),
    ('income-page-delay', 'income_reads.rs', '(r.sleep)(h, 100.0)', '(r.sleep)(h, 101.0)', 'runtime_income_range', 'runtime::native range_short'),
]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results = []
for name, file, before, after, test, assertion in controls:
    source = root / 'sdk-rs/safe-cex-venue-weex/src' / file
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, (name, text.count(before))
    command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'runtime', '--', test, '--exact']
    try:
        source.write_text(text.replace(before, after))
        broken = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / (name + '-broken.log')).write_text(broken.stdout)
    finally:
        source.write_bytes(original)
    restored = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / (name + '-restored.log')).write_text(restored.stdout)
    result = {'control': name, 'test': test, 'assertion': assertion, 'brokenExit': broken.returncode, 'restoredExit': restored.returncode, 'namedFailure': assertion in broken.stdout and 'assertion `left == right` failed' in broken.stdout}
    results.append(result)
    (out / 'mutation-results.json').write_text(json.dumps({'sha': sha, 'controls': results}, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    assert restored.returncode == 0 and '1 passed; 0 failed' in restored.stdout, (name, restored.stdout)
    assert broken.returncode == 101 and result['namedFailure'], (name, result)
