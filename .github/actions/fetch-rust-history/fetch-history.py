"""Complete the exact Actions checkout without fetching every branch or tag."""
import json
import os
from pathlib import Path
import re
import subprocess


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def commit(value):
    if not isinstance(value, str) or not re.fullmatch(r'[a-f0-9]{40}', value):
        raise ValueError('Missing or invalid immutable workflow commit')
    return value


checkout = commit(os.environ.get('GITHUB_SHA'))
if git('rev-parse', 'HEAD') != checkout:
    raise ValueError('Checkout differs from tested workflow commit')
specs = [checkout, '+refs/heads/master:refs/remotes/origin/master']
required = [checkout]
if os.environ.get('GITHUB_EVENT_NAME') == 'pull_request':
    event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
    pull = event['pull_request']
    base, head = commit(pull['base']['sha']), commit(pull['head']['sha'])
    base_ref = pull['base']['ref']
    if not isinstance(base_ref, str):
        raise ValueError('Missing pull request base ref')
    subprocess.run(['git', 'check-ref-format', f'refs/heads/{base_ref}'], check=True)
    specs += [base, head, f'+refs/heads/{base_ref}:refs/remotes/origin/{base_ref}']
    required += [base, head]
shallow = git('rev-parse', '--is-shallow-repository') == 'true'
subprocess.run(['git', 'fetch', '--no-tags', '--no-recurse-submodules', '--no-auto-gc',
                *(['--unshallow'] if shallow else []), 'origin', *dict.fromkeys(specs)], check=True)
if git('rev-parse', '--is-shallow-repository') != 'false' or git('rev-parse', 'HEAD') != checkout:
    raise ValueError('Targeted history fetch changed checkout or left shallow ancestry')
for sha in required:
    git('rev-parse', '--verify', f'{sha}^{{commit}}')
git('rev-parse', '--verify', 'refs/remotes/origin/master^{commit}')
print('Complete history retained for tested checkout, immutable PR commits and landing refs only.')
