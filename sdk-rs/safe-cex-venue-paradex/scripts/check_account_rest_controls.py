"""One compiled production mutation for each of eight new account REST rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'account_rest', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/account_rest.rs'
original = path.read_text()
controls = [
 ('request-message','let full = cat(h, vec![method, timestamp, path, body])?;','let full = cat(h, vec![method, timestamp, path, body, h.literal("WRONG")])?;','request_native_message'),
 ('chain-id-hex','cat(h, vec![h.literal("0x"), call(h, "radix16", vec![integer])?])','cat(h, vec![h.literal("0y"), call(h, "radix16", vec![integer])?])','chain_native_hex'),
 ('jwt-expiration','let expiry = timestamp + 86400.0;','let expiry = timestamp + 86401.0;','jwt_native_expiry_header'),
 ('income-sort','sorted(h,records,"timestamp")','sorted(h,records,"WRONG")','income_native_sorted_first'),
 ('orders-sort','sorted(h,orders,"createdTime")','sorted(h,orders,"WRONG")','history_orders_native_sorted_first'),
 ('fills-timestamp','("timestamp", ts),','("timestamp", h.number(number(h,ts)?+1.0)),','history_fills_native_seconds'),
 ('leverage-cache','call(h, "leverage.set", vec![symbol.clone(), lev.clone()])?;','call(h, "leverage.set", vec![symbol.clone(), h.number(number(h,lev.clone())?+1.0)])?;','leverage_native_cached_value'),
 ('slippage-payload','("max_slippage", slippage.clone())','("max_slippage", h.literal("WRONG"))','slippage_native_payload'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'account-rest-{label}.log').write_text(output)
    return result.returncode, output


LOGS.mkdir(parents=True, exist_ok=True)
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
baseline, output = run('baseline')
assert baseline == 0, output
report = dict(head=head, baseline_exit=baseline, controls=[], survivors=[])
for name, needle, replacement, assertion in controls:
    assert original.count(needle) == 1, (name, 'mutation site changed')
    try:
        path.write_text(original.replace(needle, replacement))
        mutated, output = run(name)
    finally:
        path.write_text(original)
    record = dict(control=name, assertion=assertion, mutation_exit=mutated,
                  compiled_runtime_failure=mutated == 101 and 'test result: FAILED' in output)
    if mutated == 0:
        report['survivors'].append(name)
    report['controls'].append(record)
    (LOGS / 'account-rest-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'account-rest-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 8
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
