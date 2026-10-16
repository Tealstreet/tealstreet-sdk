"""Inventory equal scalar siblings, including enum strings.
Null/containers need schema types; candidates are not parity findings.
Use --json for exact pointers and field groups.
"""
from pathlib import Path
from collections import defaultdict
import json
import sys

root = Path(__file__).resolve().parents[1] / 'tests' / 'fixtures'
files = []
for path in sorted(root.glob('*.json')):
    findings = []
    objects = [0]
    def walk(value, pointer):
        if isinstance(value, dict):
            objects[0] += 1
            groups = defaultdict(list)
            for key, item in value.items():
                if item is not None and isinstance(item, (str, bool, int, float)):
                    kind = 'boolean' if isinstance(item, bool) else 'string' if isinstance(item, str) else 'number'
                    groups[(kind, json.dumps(item))].append(key)
            for (kind, spelling), names in groups.items():
                if len(names) > 1:
                    findings.append({'path': pointer, 'type': kind, 'fields': names, 'value': json.loads(spelling)})
            for key, item in value.items():
                walk(item, pointer + '/' + key.replace('~', '~0').replace('/', '~1'))
        elif isinstance(value, list):
            for index, item in enumerate(value):
                walk(item, pointer + '/' + str(index))
    walk(json.loads(path.read_text()), '')
    files.append({'file': path.name, 'records': objects[0], 'findings': findings})
if '--json' in sys.argv:
    print(json.dumps(files, indent=2))
else:
    for entry in files:
        print(entry['file'], 'records=' + str(entry['records']), 'candidate_groups=' + str(len(entry['findings'])))
    print('FILES', len(files), 'RECORDS', sum(e['records'] for e in files),
          'CANDIDATE_GROUPS', sum(len(e['findings']) for e in files))
