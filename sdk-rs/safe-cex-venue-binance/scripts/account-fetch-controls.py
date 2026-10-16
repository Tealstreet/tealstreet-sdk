#!/usr/bin/env python3
"""Serial scoped gates, with one compiling native-value control per new row."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument('--gate-sha', required=True)
parser.add_argument('--base-sha', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
os.chdir(root)
crate = Path('sdk-rs/safe-cex-venue-binance')
sha = subprocess.check_output(['git', 'rev-parse', args.gate_sha], text=True).strip()
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == sha
logs = Path('sdk-rs/target/binance-account-fetch')/sha
logs.mkdir(parents=True, exist_ok=True)
parent = subprocess.check_output(['git', 'rev-parse', args.base_sha], text=True).strip()
subprocess.run(['git', 'merge-base', '--is-ancestor', parent, sha], check=True)
candidate_parent = subprocess.check_output(['git', 'rev-parse', sha+'^'], text=True).strip()
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
inputs = [crate/'src/account_fetch.rs', crate/'src/lib.rs', crate/'tests/account_fetch.rs', crate/'scripts/account-fetch.mjs', Path(__file__).relative_to(root)]
inputs += list(Path('sdk-rs/parity/manifest/exchanges/binance').glob('*.exchange.ts.toml'))
inputs += list(Path('packages/safe-cex/src/exchanges/binance').glob('*.exchange.ts'))
inputs += list(Path('packages/safe-cex/src/exchanges/binance').glob('*.types.ts'))
originals = {path: path.read_bytes() for path in inputs}
for path, content in originals.items():
    assert content == subprocess.check_output(['git', 'show', f'{sha}:{path}']), path
records = []
def run(label, command, expected_exit, assertion=None):
    result = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs/(label+'.log')).write_bytes(result.stdout)
    text = result.stdout.decode(errors='replace')
    record = dict(label=label, command=command, gate_sha=sha, exit=result.returncode,
                  log=str(logs/(label+'.log')), log_sha256=hashlib.sha256(result.stdout).hexdigest(), output=text)
    if assertion:
        record['named_native_failure'] = assertion
    records.append(record)
    print(f'{label}: exit {result.returncode}', flush=True)
    assert result.returncode == expected_exit, text
    if assertion:
        assert 'Finished `test` profile' in text and assertion in text and 'test result: FAILED.' in text, text
        assert '0 passed; 1 failed;' in text, text
    elif label in ('baseline', 'final-restore'):
        assert 'test result: ok. 6 passed; 0 failed; 0 ignored;' in text, text
    elif label == 'extractor-check':
        assert 'Would update 0 manifests; 0 added, 0 orphaned symbols.' in text, text
    return text
cargo = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-binance', '--test', 'account_fetch', '--jobs', '2']
run('baseline', cargo+['--', '--include-ignored'], 0)
source = crate/'src/account_fetch.rs'
controls = [
    ('futures-second-row', 'Some(Value::Array(items)) => items.into_iter().next(),', 'Some(Value::Array(items)) => items.into_iter().nth(1),', 'futures_account_matches_source', 'ACCOUNT FETCH binance case 0 reject false native decoded value', 'second-alias versus first-alias'),
    ('pm-second-row', 'account_identity(items.into_iter().next())', 'account_identity(items.into_iter().nth(1))', 'pm_account_matches_source', 'ACCOUNT FETCH binancepm case 0 reject false native decoded value', 'second-alias versus first-alias'),
    ('spot-discard-value', 'FetchedPositions::Present { positions } => Ok(positions),', 'FetchedPositions::Present { positions: _ } => Ok(None),', 'spot_positions_matches_source', 'ACCOUNT FETCH binancespot case 0 reject false native decoded value', 'undefined versus two-row positions array with BTC quantity 3.6 and ETH quantity 7'),
]
for label, before, after, test, assertion, pair in controls:
    restored = originals[source].decode()
    assert restored.count(before) == 1
    try:
        source.write_text(restored.replace(before, after))
        run(label, cargo+['--', '--include-ignored', test, '--exact'], 101, assertion)
        records[-1].update(call_site=str(source), mutation_before=before, mutation_after=after, native_pair=pair)
    finally:
        source.write_bytes(originals[source])
    assert source.read_bytes() == originals[source]
run('final-restore', cargo+['--', '--include-ignored'], 0)
run('scoped-cargo-check', ['cargo','check','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-binance','--all-targets','--jobs','2'], 0)
run('extractor-check', ['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract:check'], 0)
run('scoped-rustfmt', ['rustfmt','--edition','2024','--check',str(crate/'src/account_fetch.rs'),str(crate/'tests/account_fetch.rs')], 0)
run('git-diff-check', ['git','diff','--check'], 0)
for path, content in originals.items():
    assert path.read_bytes() == content, f'Unrestored gate input {path}'

def census(revision=None):
    rows = {}
    for path in Path('sdk-rs/parity/manifest/exchanges/binance').rglob('*.toml'):
        content = subprocess.check_output(['git','show',f'{revision}:{path}']).decode() if revision else path.read_text()
        for row in tomllib.loads(content)['symbol']:
            key = (str(path),row['ts'],row['kind'])
            assert key not in rows, key
            rows[key] = row['status']
    return rows
before, after = census(parent), census()
assert before.keys() == after.keys()
changes = [dict(manifest=k[0],symbol=k[1],kind=k[2],before=before[k],after=after[k]) for k in before if before[k]!=after[k]]
assert len(changes) == 3 and all(c['before']=='pending' and c['after']=='ported' for c in changes)
counts = lambda rows: {status: list(rows.values()).count(status) for status in ('ported','divergent','pending')}
evidence = dict(gate_sha=sha, parent_built_on=parent, candidate_parent=candidate_parent, unreviewed_work_in_base=False,
    environment=dict(node=subprocess.check_output(['node','--version'],text=True).strip(),os=platform.system(),architecture=platform.machine(),typescript_path=env['SAFE_CEX_TYPESCRIPT_PATH']),
    moved_pending_to_ported=3, moved_pending_to_divergent=0, census_before=counts(before), census_after=counts(after), changed_rows=changes,
    scope='Two account readers at configured decoded JSON/undefined, plus generic native Spot positions forwarding with explicit undefined/null/present decoded container; normalized ExchangeError callback boundaries; executable getters/custom iterators, primitive promise rejection values, synchronous callback throws and concrete class/client/fetch wiring are not covered.',
    enumeration_floor='120 authored source scenarios:48 futures+48 PM+24 spot. Completed all selected scenarios; three selected compiling controls, one per new row. Additional native Position assertions preserve nonfinite fields, signed zero and Rc identity. No exhaustive input or whole-venue claim.',
    controls_surviving=[], records=records,
    restored_inputs_sha256={str(path):hashlib.sha256(content).hexdigest() for path,content in originals.items()},
    preliminary_uncommitted_runs=[dict(exit=101,passed=4,failed=1,reason='allocation test fixture cloned its input through json! before the reader; corrected fixture to direct moves'),dict(exit=0,passed=5,failed=0)],
    earlier_candidate_runner=dict(gate_sha='80601d2d65dd5a01fd4ba37fc4a33858c0fa064f',runner_exit=1,completed_gate_exits={'baseline':0,'futures-second-row':101,'pm-second-row':101,'spot-wrong-property':101,'final-restore':0,'scoped-cargo-check':0,'extractor-check':0},reason='Runner expected Updated instead of actual Would update from extractor check mode; exact zero-update output was inspected, parser corrected in the final candidate. These earlier results are not substituted for final-candidate results.'),
    manifest_writer=dict(command=['node','/Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js','parity:extract'],exit=0,output='Updated 3 manifests; 0 added, 0 orphaned symbols.',scope='uncommitted manifest edits before candidate commit'),
    not_run=['full crate test suite','workspace compilation','host clippy','wasm clippy','CI rerun','whole-class/client integration','whole-venue audit','cross-engine error-message verification'])
(crate/'ACCOUNT_FETCH_GATE_EXITS.json').write_text(json.dumps(evidence,indent=2)+'\n')
print('Evidence recorded at '+str(crate/'ACCOUNT_FETCH_GATE_EXITS.json'), flush=True)
