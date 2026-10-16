"""Paradex-only TOML/note/declaration census, not behavioral parity evidence.
No subprocesses. Run the real scoped parity checker separately.
"""
from collections import Counter
from pathlib import Path
import json
import re
import tomllib

SDK = Path(__file__).resolve().parents[2]
rows = [(path.name, row)
        for path in sorted((SDK / 'parity/manifest/exchanges/paradex').glob('*.toml'))
        for row in tomllib.loads(path.read_text())['symbol']]
def public_target_exists(source, target):
    parts = target.split('::')
    if len(parts) == 1:
        return re.search(r'\bpub\s+(?:async\s+)?(?:fn|struct|enum|type|const|trait)\s+'
                         + re.escape(parts[0]) + r'\b', source) is not None
    if len(parts) != 2:
        return False
    owner = re.search(r'(?m)^pub\s+trait\s+' + re.escape(parts[0]) + r'\b[^\{]*\{', source)
    if owner is None:
        return False
    depth, end = 1, owner.end()
    while end < len(source) and depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    if depth:
        return False
    return re.search(r'(?m)^\s*(?:async\s+)?(?:fn|type|const)\s+'
                     + re.escape(parts[1]) + r'\b', source[owner.end():end-1]) is not None


texts = {}
report = []
for filename, row in rows:
    if row['status'] not in ('ported', 'divergent'):
        continue
    targets = row['rs'] if isinstance(row['rs'], list) else [row['rs']]
    assert targets and all(targets), (filename, row['ts'], 'missing Rust target')
    if row['status'] != 'divergent':
        continue
    note = row['note']
    bucket = re.match(r'^(FORCED|CHOSEN|MIXED|CORRECTED|FINDING)\b', note)
    assert bucket, (filename, row['ts'], 'canonical opening bucket required')
    if bucket[1] == 'CHOSEN':
        assert 'Why mirroring would be worse:' in note, (row['ts'], 'missing why')
        if 'UTF-8' in note:
            assert 'sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary' in note, row['ts']
    elif bucket[1] == 'FINDING':
        assert 'Owner:' in note and 'Why divergent:' in note, row['ts']
    else:
        assert re.search(r'\bwhy\b', note, re.IGNORECASE), (row['ts'], 'missing why')
    for target in targets:
        file, symbol = target.split('::', 1)
        if file not in texts:
            texts[file] = (SDK / file).read_text()
        # Presence only; this is not a Rust parser or a proof that the item runs.
        assert public_target_exists(texts[file], symbol), target
    report.append({'manifest': filename, 'symbol': row['ts'],
                   'bucket': bucket[1], 'target_references': len(targets)})
print(json.dumps({'total': len(rows), 'triple': dict(Counter(r['status'] for _, r in rows)),
                  'buckets': dict(Counter(r['bucket'] for r in report)),
                  'target_references': sum(r['target_references'] for r in report),
                  'missing_targets': 0, 'rows': report}, indent=2))
