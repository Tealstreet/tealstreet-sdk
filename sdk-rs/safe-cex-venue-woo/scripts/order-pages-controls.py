"""Compile pagination mutations and require native named assertion failures."""
from pathlib import Path
import os
import subprocess
import hashlib
import json

source = Path('sdk-rs/safe-cex-venue-woo/src/order_pages.rs')
original = source.read_text()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--offline', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'order_pages']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('method', 'method: "GET".into()', 'method: "POST".into()', 'PAGES native request method'),
    ('early-disposal', '    loop {', '    loop { if base.is_disposed() { return Ok(result); }', 'PAGES native request cardinality'),
    ('empty', 'result.extend(rows.iter().cloned());', 'result.extend(rows.iter().filter(|_| false).cloned());', 'PAGES native output cardinality'),
    ('subset', 'result.extend(rows.iter().cloned());', 'result.extend(rows.iter().take(1).cloned());', 'PAGES native output cardinality'),
    ('extra', 'result.extend(rows.iter().cloned());', 'result.extend(rows.iter().cloned()); result.extend(rows.first().cloned());', 'PAGES native output cardinality'),
    ('reverse', 'result.extend(rows.iter().cloned());', 'result.extend(rows.iter().rev().cloned());', 'PAGES native row identity'),
    ('lost-prefix', '        if base.is_disposed() {\n            return Ok(result);', '        if base.is_disposed() {\n            return Ok(Vec::new());', 'PAGES native output cardinality'),
    ('early-refusal', '        let meta = data', '        if rows.and_then(Value::as_array).is_none() { return Err(refusal("early rows refusal")); }\n        let meta = data', 'PAGES metadata coercion wins first over typed rows'),
]
evidence = {
    'sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'source_sha256': hashlib.sha256(original.encode()).hexdigest(),
    'command': command,
    'iterations': [],
}
record = Path('/tmp/woo-pages-controls-evidence.json')
try:
    for name, before, after, assertion in controls:
        assert original.count(before) == 1, (name, original.count(before))
        source.write_text(original.replace(before, after))
        run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-pages-control-' + name + '.log').write_text(run.stdout)
        caught = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        evidence['iterations'].append({'name': name, 'exit': run.returncode, 'assertion': assertion, 'caught': caught})
        record.write_text(json.dumps(evidence, indent=2) + '\n')
        assert caught, (name, run.stdout)
        print(name, '101', assertion, flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
run = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
Path('/tmp/woo-pages-control-restored.log').write_text(run.stdout)
evidence['restored_exit'] = run.returncode
record.write_text(json.dumps(evidence, indent=2) + '\n')
assert run.returncode == 0, run.stdout
print('restored target passed', flush=True)
