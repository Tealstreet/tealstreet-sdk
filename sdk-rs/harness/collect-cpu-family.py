"""Independently build the authenticated base; retain actual family byte seals."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--authority', required=True)
parser.add_argument('--temporary', required=True)
args = parser.parse_args()
authority = json.loads(Path(args.authority).read_text())
root = Path.cwd().resolve()
temporary = Path(args.temporary).resolve()
base = temporary / 'base-checkout'
out = root / 'sdk-rs/harness/out/cpu-family'


def git(*args, cwd=root):
    return subprocess.check_output(['git', *args], cwd=cwd, text=True).strip()


def insist(ok, message):
    if not ok:
        raise RuntimeError(message)


def candidate_state():
    diff = subprocess.check_output(['git', 'diff', '--no-ext-diff', '--binary', 'HEAD'], cwd=root)
    return (git('status', '--porcelain', '--untracked-files=all'), hashlib.sha256(diff).hexdigest())


insist(os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ.get('GITHUB_RUN_ATTEMPT') == '1'
       and os.environ.get('GITHUB_RUN_ID') == authority['runId'], 'original Actions attempt required')
insist(git('rev-parse', 'HEAD') == authority['candidate']
       and git('rev-parse', 'HEAD^{tree}') == authority['candidateTree'], 'candidate source differs')
before = candidate_state()
insist(not out.exists() and not base.exists(), 'refuse overwriting proof or base checkout')
out.mkdir(parents=True)
shutil.copytree(root / 'sdk-rs/hosts/wasm/pkg/ci-replay', out / 'candidate')
try:
    subprocess.run(['git', 'init', '-q', '-b', 'master', str(base)], check=True)
    subprocess.run(['git', 'fetch', '--no-tags', '--depth=1', root.as_uri(), authority['baseline']], cwd=base, check=True)
    subprocess.run(['git', 'checkout', '--detach', authority['baseline']], cwd=base, check=True)
    insist(git('rev-parse', 'HEAD', cwd=base) == authority['baseline']
           and git('rev-parse', 'HEAD^{tree}', cwd=base) == git('rev-parse', authority['baseline'] + '^{tree}')
           and not git('status', '--porcelain', '--untracked-files=all', cwd=base), 'base checkout is not clean authenticated parent')
    env = {**os.environ, 'CARGO_TARGET_DIR': str(base / 'sdk-rs/target')}
    subprocess.run(['bash', 'sdk-rs/harness/build.sh', '--venue', authority['suite']], cwd=base, env=env, check=True)
    subprocess.run(['node', 'sdk-rs/harness/ci-wasm.mjs', '--venue', authority['suite'],
                    '--archive', str(out / 'baseline')], cwd=base, env=env, check=True)
finally:
    insist(git('rev-parse', 'HEAD') == authority['candidate']
           and git('rev-parse', 'HEAD^{tree}') == authority['candidateTree']
           and candidate_state() == before, 'candidate source or working changes were modified')
(out / 'proof.json').write_text(json.dumps(authority, indent=2) + '\n')
