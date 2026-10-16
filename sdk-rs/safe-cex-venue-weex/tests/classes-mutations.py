"""One compiled, restored native-value control for each assembled WEEX class."""
from pathlib import Path
import json
import os
import subprocess

root = Path(__file__).resolve().parents[3]
out = root / 'sdk-rs/target/weex-classes-gates'
out.mkdir(parents=True, exist_ok=True)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
controls = [
    ('exchange-mode', 'lifecycle.rs', '(r.text)("oneway")', '(r.text)("hedge")', 'classes_exchange', 'classes::native exchange mode'),
    ('public-first-id', 'public_feed.rs', '(rt.define)(host, &instance, "subscriptionId", (rt.number)(1.0))?', '(rt.define)(host, &instance, "subscriptionId", (rt.number)(2.0))?', 'classes_public', 'classes::native socket initial Public'),
    ('private-first-id', 'classes.rs', 'host.define(&receiver, "subscriptionId", host.number(1.0))?', 'host.define(&receiver, "subscriptionId", host.number(2.0))?', 'classes_private', 'classes::native socket initial Private'),
]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
results = []
for name, file, before, after, test, assertion in controls:
    source = root / 'sdk-rs/safe-cex-venue-weex/src' / file
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, (name, text.count(before))
    command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'classes', '--', test, '--exact']
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
