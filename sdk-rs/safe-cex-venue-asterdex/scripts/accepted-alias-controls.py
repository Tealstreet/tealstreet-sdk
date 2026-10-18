#!/usr/bin/env python3
"""Require eight compiled alias-root failures, then normal native restoration."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-alias-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-alias-contracts.mjs']
assert len(os.sys.argv) == 1, 'alias controls take no scope-changing arguments'


def run(label, control=False):
    result = subprocess.run([*COMMAND, *(['--root-control'] if control else [])],
                            cwd=ROOT, env=os.environ, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output and 'could not compile' not in output, f'{label}: compiler error is not evidence; {LOGS}'
    receipts = [json.loads(line[len('ASTER_ALIAS_ROWS '):]) for line in output.splitlines()
                if line.startswith('ASTER_ALIAS_ROWS ')]
    assert len(receipts) == 1, f'{label}: actual native receipts required; {LOGS}'
    receipt = receipts[0]
    rows = receipt['rows']
    assert len(rows) == 8 and len(set(receipt['names'])) == 8, LOGS
    assert sorted(receipt['names']) == sorted(row['name'] for row in rows), LOGS
    failures = [row for row in rows if row['failures']]
    if control:
        assert result.returncode == 1 and len(failures) == 8, f'{label}: eight named native failures required; {LOGS}'
        for row in failures:
            assert any('identity mismatch' in message for message in row['failures']), f'{row["name"]}: exact identity assertion required; {LOGS}'
    else:
        assert result.returncode == 0 and not failures, f'{label}: native baseline failure; {LOGS}'
        assert sum(bool(row.get('classificationOnly')) for row in rows) == 7, LOGS
    print(f'{label}: exit {result.returncode}; {len(failures)} failed native aliases', flush=True)


run('baseline')
run('compiled-root-identity', control=True)
run('normal-root-restored')
print(f'All eight compiled native identity controls caught; normal alias probes pass again; logs {LOGS}')
