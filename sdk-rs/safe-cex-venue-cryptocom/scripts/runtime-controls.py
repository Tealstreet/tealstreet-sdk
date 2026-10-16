"""Sequential compiling controls; each mutation is restored before another Cargo."""
import difflib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path(__file__).resolve().parents[3]
os.chdir(root)
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
run_kind = 'review-controls-3' if sys.argv[1:] == ['--review-controls'] else 'controls-7'
evidence = Path('sdk-rs/safe-cex-venue-cryptocom/evidence/runtime-' + sha[:10]) / run_kind
evidence.mkdir(parents=True, exist_ok=True)
source = Path('sdk-rs/safe-cex-venue-cryptocom/src/runtime.rs')
witness = Path('sdk-rs/safe-cex-venue-cryptocom/tests/runtime.rs')
oracle = Path('sdk-rs/safe-cex-venue-cryptocom/tests/runtime_oracle.cjs')
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-cryptocom', '--test', 'runtime', '--jobs', '1', '--locked']
variants = [
 ('api-timeout', source, '(h.value.number)(30000.)', '(h.value.number)(30001.)', 'runtime_bodies_match_maintained_typescript_native', 'CRYPTOCOM-RUNTIME-NATIVE-API-TIMEOUT'),
 ('constructor-rate', source, '(limit_h.value.number)(50.)', '(limit_h.value.number)(49.)', 'runtime_bodies_match_maintained_typescript_native', 'CRYPTOCOM-RUNTIME-NATIVE-CTOR-RATE'),
 ('class-name', source, '"CryptocomExchange"', '"WrongExchange"', 'runtime_bodies_match_maintained_typescript_native', 'CRYPTOCOM-RUNTIME-NATIVE-CLASS-NAME'),
 ('start-delay', source, '(timer_h.value.number)(1000.)', '(timer_h.value.number)(1001.)', 'runtime_bodies_match_maintained_typescript_native', 'CRYPTOCOM-RUNTIME-NATIVE-TIMER'),
 ('tick-period', source, '(h.value.number)(10_000.)', '(h.value.number)(9_999.)', 'runtime_bodies_match_maintained_typescript_native', 'CRYPTOCOM-RUNTIME-NATIVE-TIMER'),
 ('oracle-noop', oracle, None, 'process.exit(0);\n', 'runtime_oracle_rejects_typescript_mutation', 'NATIVE-ORACLE-MUTATION-MUST-FAIL'),
 ('promise-ownership', witness, 'self.tasks.borrow_mut().push(promise.clone());', 'let _ = promise.clone();', 'runtime_promise_ownership_survives_observer_drop', 'NATIVE-PROMISE-OWNERSHIP'),
]
review_variants = [
 ('interceptor-receiver', source, '(h.value.invoke)(callee, receiver, args()?)', '(h.value.invoke)(callee, if key == "use" { (h.value.undefined)() } else { receiver }, args()?)', 'runtime_bodies_match_maintained_typescript_native', 'NATIVE-INTERCEPTOR-RECEIVER'),
 ('socket-receiver', source, '(h.value.invoke)(callee, receiver, args()?)', '(h.value.invoke)(callee, if key == "connectAndSubscribe" { (h.value.undefined)() } else { receiver }, args()?)', 'runtime_bodies_match_maintained_typescript_native', 'NATIVE-SOCKET-RECEIVER'),
 ('constructor-callable', witness, 'port::create_api(h.ops(), args[0].clone(), args[1].clone())', 'let _ = args; Ok(h.limited.clone())', 'runtime_bodies_match_maintained_typescript_native', 'NATIVE-CONSTRUCTOR-API-RESULT'),
]
if sys.argv[1:] == ['--review-controls']:
    variants = review_variants
    planned_floor = 3
else:
    assert not sys.argv[1:], 'Only --review-controls is supported'
    planned_floor = 7

def sanitize(text):
    text = text.replace(str(root), '<worktree>')
    compiler = os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
    if compiler:
        text = text.replace(str(Path(compiler).parent), '<isolated-dependencies>')
    return text

def run(name, test):
    argv = command + ([test] if test else []) + ['--', '--test-threads=1', '--nocapture']
    started = time.time()
    print('RUN', name, sha, flush=True)
    result = subprocess.run(argv, capture_output=True, text=True)
    content = sanitize(result.stdout + result.stderr)
    (evidence / (name + '.log')).write_text(content)
    record = {'sha': sha, 'command': argv, 'exit': result.returncode, 'seconds': round(time.time() - started, 2), 'log': name + '.log', 'env': ['SAFE_CEX_TYPESCRIPT_PATH supplied externally', 'CARGO_BUILD_JOBS=1']}
    print(name, 'EXIT', result.returncode, flush=True)
    return record, content

records = []
baseline, text = run('baseline', None)
(evidence / 'baseline.json').write_text(json.dumps(baseline, indent=2) + '\n')
assert baseline['exit'] == 0 and '4 passed' in text and 'RUNTIME native floor70 stops index69' in text
for name, path, old, replacement, test, assertion in variants:
    original = path.read_bytes()
    text = original.decode()
    if old is not None:
        assert text.count(old) == 1, (name, text.count(old))
        mutant = text.replace(old, replacement)
    else:
        mutant = replacement
    patch = ''.join(difflib.unified_diff(text.splitlines(True), mutant.splitlines(True), fromfile=str(path), tofile=str(path)))
    (evidence / (name + '.patch')).write_text(patch)
    try:
        path.write_text(mutant)
        changed, content = run(name + '-mutated', test)
    finally:
        path.write_bytes(original)
    restored, restored_text = run(name + '-restored', test)
    runtime = changed['exit'] == 101 and 'running 1 test' in content and assertion in content and 'could not compile' not in content
    record = {'name': name, 'sha': sha, 'patch': name + '.patch', 'named_assertion': assertion, 'mutation': changed, 'restoration': restored, 'named_runtime_failure': runtime}
    records.append(record)
    (evidence / 'controls.json').write_text(json.dumps({'sha': sha, 'planned_floor': planned_floor, 'executed_floor': len(records), 'stop': name, 'controls': records}, indent=2) + '\n')
    assert runtime and restored['exit'] == 0, record
argv = ['git', 'diff', '--exit-code', sha, '--', str(source), str(witness), str(oracle)]
result = subprocess.run(argv, capture_output=True, text=True)
assert result.returncode == 0
(evidence / 'controls-survival.json').write_text(json.dumps({'sha': sha, 'command': argv, 'exit': result.returncode}, indent=2) + '\n')
