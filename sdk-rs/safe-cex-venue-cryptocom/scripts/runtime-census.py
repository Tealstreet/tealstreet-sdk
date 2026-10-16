"""Bounded tomllib census of the six maintained Crypto.com manifests."""
from collections import Counter
import json
from pathlib import Path
import subprocess
import tomllib

root = Path(__file__).resolve().parents[3]
base = '6957adc80f485c945c18bb0d83c4965f5fe3cd95'
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
names = ['api', 'exchange', 'signing', 'types', 'ws-private', 'ws-public']
records = []
for sha in [base, head]:
    total = Counter()
    rows = {}
    for name in names:
        path = f'sdk-rs/parity/manifest/exchanges/cryptocom/cryptocom.{name}.ts.toml'
        content = subprocess.check_output(['git', 'show', f'{sha}:{path}'], cwd=root, text=True)
        symbols = tomllib.loads(content)['symbol']
        for row in symbols:
            total[row['status']] += 1
            rows[(name, row['ts'])] = row['status']
    records.append({'sha': sha, 'triple_order': ['ported', 'divergent', 'pending'],
                    'triple': [total[k] for k in ['ported', 'divergent', 'pending']]})
    if sha == base:
        original = rows
    else:
        moved = [{'file': key[0], 'symbol': key[1], 'before': original[key], 'after': value}
                 for key, value in rows.items() if original[key] != value]
print(json.dumps({'floor_files': 6, 'stop': 'index5 cryptocom.ws-public.ts.toml',
                  'revisions': records, 'rows_moved': len(moved), 'changes': moved}, indent=2))
