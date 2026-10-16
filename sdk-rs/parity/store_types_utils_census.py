"""Read the assigned manifests at an explicit commit using standard tomllib.

This is a nine-file census, not proof of implementation parity.
Run with Python >=3.11 from the worktree root. No files are written.
"""
import collections
import json
import subprocess
import sys
import tomllib

sha = subprocess.check_output(['git', 'rev-parse', sys.argv[1]], text=True).strip()
# Expected pending counts are independently supplied by the lane assignment.
expectations = [
    ('accessToken.ts', 12),
    ('config/config.ts', 11),
    ('index.ts', 8),
    ('exchangeMetadata.ts', 8),
    ('createExchangeFromAccount.ts', 7),
    ('telemetry.ts', 5),
    ('utils/venueRestDiagnostics.ts', 26),
    ('utils/wsDiagnostics.ts', 9),
    ('instrument-class/resolve.ts', 1),
]
statuses = ('ported', 'divergent', 'pending')
total = collections.Counter()
files = []
for name, expected_pending in expectations:
    path = 'sdk-rs/parity/manifest/' + name + '.toml'
    manifest = tomllib.loads(subprocess.check_output(['git', 'show', sha + ':' + path], text=True))
    counts = collections.Counter(row['status'] for row in manifest['symbol'])
    assert set(counts) <= set(statuses), (name, counts)
    assert counts['pending'] == expected_pending, (name, counts, expected_pending)
    total.update(counts)
    files.append({'file': name, 'triple': [counts[status] for status in statuses],
                  'symbols': len(manifest['symbol'])})
triple = [total[status] for status in statuses]
assert triple == [8, 0, 87], triple
assert sum(total.values()) == 95, total
print(json.dumps({'sha': sha, 'order': list(statuses), 'files': files, 'triple': triple,
                  'symbols': sum(total.values()), 'stopped_after_file': 9,
                  'scope': 'nine assigned files; 87 pending rows plus 8 existing ported rows'}, indent=2))
