"""Run from the checkout root, after every other Cargo command has finished.

Requires Python 3.11+, Rust on PATH and SAFE_CEX_TYPESCRIPT_PATH supplied externally.
The deliberate mutation changes Serde names in both directions, leaving independent
native field access intact. Outputs belong under the ignored target directory.
"""
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def clean_tree():
    assert subprocess.run(['git', 'diff', '--quiet', 'HEAD', '--']).returncode == 0


clean_tree()
root = Path.cwd()
sha = git('rev-parse', 'HEAD')
out = Path('sdk-rs/target/bitrue-wire-control') / sha[:10]
out.mkdir(parents=True, exist_ok=True)
source = Path('sdk-rs/safe-cex-venue-bitrue/src/wire.rs')
original = source.read_bytes()
text = original.decode()
start = text.index('struct FetchKlineResponseItemWire {')
end = text.index('\n}', start)
item = text[start:end]
assert item.count('rename = "high"') == item.count('rename = "low"') == 1
swapped = (item.replace('rename = "high"', 'rename = "SWAP_TEMP"')
           .replace('rename = "low"', 'rename = "high"')
           .replace('rename = "SWAP_TEMP"', 'rename = "low"'))
mutant = text[:start] + swapped + text[end:]
patch = out / 'wire-high-low-symmetric.patch'
patch.write_text(''.join(difflib.unified_diff(
    text.splitlines(True), mutant.splitlines(True),
    fromfile=str(source), tofile=str(source))))
env = os.environ.copy()
assert env.get('SAFE_CEX_TYPESCRIPT_PATH'), 'Supply TypeScript externally'
env.update(RUSTFLAGS='-Dwarnings', CARGO_BUILD_JOBS='2')
node_version = subprocess.check_output(['node', '--version'], env=env, text=True).strip()
command = ['cargo', 'test', '--locked', '-p', 'safe-cex-venue-bitrue',
           '--test', 'wire', '--jobs', '1', '--', '--test-threads=1', '--nocapture']


def run(name):
    assert git('rev-parse', 'HEAD') == sha
    print('RUN', name, sha, flush=True)
    began = time.monotonic()
    result = subprocess.run(command, cwd='sdk-rs', env=env,
                            capture_output=True, text=True)
    output = (result.stdout + result.stderr).replace(str(root), '<worktree>')
    output = output.replace(env['SAFE_CEX_TYPESCRIPT_PATH'], '<external-typescript>')
    output = re.sub(r'/Users/[^\s\)\"\']+', '<external-path>', output)
    log = out / (name + '.log')
    log.write_text(output)
    record = {'base_sha': sha, 'command': command, 'cwd': 'sdk-rs',
              'exit': result.returncode, 'seconds': round(time.monotonic() - began, 2),
              'log': log.name, 'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()}
    print(name, 'EXIT', result.returncode, flush=True)
    return record, output


try:
    source.write_text(mutant)
    broken, broken_output = run('wire-high-low-mutated')
finally:
    source.write_bytes(original)
restored, _ = run('wire-high-low-restored')
marker = 'native type parity: FETCH_KLINE_RESPONSE mode-0 public raw'
discriminates = (broken['exit'] == 101 and 'running 1 test' in broken_output
                 and marker in broken_output and 'could not compile' not in broken_output)
record = {
    'base_sha': sha, 'node': node_version,
    'control': 'swap high/low Serde names in both serialization and deserialization',
    'patch': patch.name, 'patch_sha256': hashlib.sha256(patch.read_bytes()).hexdigest(),
    'assertion': marker, 'mutated': broken, 'restored': restored,
    'runtime_discrimination': discriminates,
    'exact_source_restored': source.read_bytes() == original,
    'stop': 'iteration0 FetchKlineResponseItemWire high/low; no other wire fields certified',
}
(out / 'wire-symmetric-control.json').write_text(json.dumps(record, indent=2) + '\n')
assert discriminates and restored['exit'] == 0 and record['exact_source_restored'], record
clean_tree()
print('COMPLETE symmetric wire swap runtime101 -> restored0', flush=True)
