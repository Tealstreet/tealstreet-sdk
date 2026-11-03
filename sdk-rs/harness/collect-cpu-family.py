"""Independently build the authenticated base; retain actual family byte seals."""
import argparse
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
root = Path.cwd()
temporary = Path(args.temporary)
out = root / 'sdk-rs/harness/out/cpu-family'


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def insist(ok, message):
    if not ok:
        raise RuntimeError(message)


insist(os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ.get('GITHUB_RUN_ATTEMPT') == '1'
       and os.environ.get('GITHUB_RUN_ID') == authority['runId'], 'original Actions attempt required')
insist(git('rev-parse', 'HEAD') == authority['candidate']
       and git('rev-parse', 'HEAD^{tree}') == authority['candidateTree'], 'candidate source differs')
insist(not git('status', '--porcelain'), 'dirty build inputs')
insist(not out.exists(), 'refuse overwriting proof')
out.mkdir(parents=True)
shutil.copytree(root / 'sdk-rs/hosts/wasm/pkg/ci-replay', out / 'candidate')
for name in ('node', 'web'):
    shutil.copytree(root / 'sdk-rs/hosts/wasm/pkg' / name, temporary / name)
try:
    subprocess.run(['git', 'checkout', '--detach', authority['baseline']], check=True)
    subprocess.run(['cargo', 'clean', '--manifest-path', 'sdk-rs/Cargo.toml'], check=True)
    subprocess.run(['bash', 'sdk-rs/harness/build.sh', '--venue', authority['suite']], check=True)
    subprocess.run(['node', 'sdk-rs/harness/ci-wasm.mjs', '--venue', authority['suite'],
                    '--archive', str(out / 'baseline')], check=True)
finally:
    subprocess.run(['git', 'checkout', '--detach', authority['candidate']], check=True)
    for name in ('node', 'web'):
        path = root / 'sdk-rs/hosts/wasm/pkg' / name
        shutil.rmtree(path)
        shutil.copytree(temporary / name, path)
insist(git('rev-parse', 'HEAD^{tree}') == authority['candidateTree']
       and not git('status', '--porcelain'), 'candidate source not restored')
(out / 'proof.json').write_text(json.dumps(authority, indent=2) + '\n')
