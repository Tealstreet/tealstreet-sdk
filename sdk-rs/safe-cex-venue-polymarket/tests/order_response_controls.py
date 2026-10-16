"""Named order-response controls; restore every changed source."""
from pathlib import Path
import os
import subprocess
crate = Path(__file__).resolve().parents[1]
rust = crate / 'src/types.rs'
source = crate.parents[1] / 'packages/safe-cex/src/exchanges/polymarket/polymarket.types.ts'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR']); logs.mkdir(parents=True, exist_ok=True)
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(crate.parent/'Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'auth_types', '--locked', '--jobs', '1', '--', 'order_response_record_actual_typescript', '--nocapture']
def run(name):
    r = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs/(name+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout)
    assert r.stdout.count('polymarket auth oracle: one Node invocation for the entire corpus') == 1, r.stdout
    return r

def region(text, change):
    start = text.index('pub struct PolymarketOrderResponse {')
    return text[:start] + change(text[start:])

def swap(text, first, second):
    return region(text, lambda s: s.replace('"'+first+'"', '"SWAP"').replace('"'+second+'"', '"'+first+'"').replace('"SWAP"','"'+second+'"'))

controls = [
 ('response-id-swap', rust, lambda s: swap(s,'errorMsg','orderID'), 'native_order_response.error_msg'),
 ('response-amount-swap', rust, lambda s: swap(s,'takingAmount','makingAmount'), 'native_order_response.taking_amount'),
 ('response-success-inverted', rust, lambda s: region(s,lambda s:s.replace('success: fields.success,','success: !fields.success,')), 'native_order_response.success'),
 ('response-hashes-reversed', rust, lambda s: region(s,lambda s:s.replace('transactions_hashes: fields.transactions_hashes,','transactions_hashes: fields.transactions_hashes.map(|v| v.into_iter().rev().collect()),')), 'native_order_response.transactions_hashes'),
 ('response-status-null', rust, lambda s: region(s,lambda s:s.replace('rename = "status", default, deserialize_with = "present"','rename = "status", default')), 'present_null:PolymarketOrderResponse.status'),
 ('response-hashes-null', rust, lambda s: s.replace('Vec::<String>::deserialize(d).map(Some)', 'Option::<Vec<String>>::deserialize(d)'), 'present_null:PolymarketOrderResponse.transactionsHashes'),
 ('response-actual-boolean-narrowing', source, lambda s: s.replace('success: boolean;', 'success: true;'), 'unsupported field success'),
]
assert run('response-baseline').returncode == 0
for name,file,change,assertion in controls:
    original=file.read_text(); changed=change(original); assert changed!=original,name
    try:
        file.write_text(changed); r=run(name+'-mutation')
        assert r.returncode==101 and assertion in r.stdout,r.stdout
        print(name+': FAIL '+assertion,flush=True)
    finally: file.write_text(original)
    assert run(name+'-restored').returncode==0
    print(name+': RESTORED PASS',flush=True)
