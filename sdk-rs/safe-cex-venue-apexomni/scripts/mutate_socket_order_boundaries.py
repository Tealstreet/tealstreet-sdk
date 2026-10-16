"""Require the socket-order native assertions to reject four native and one source regression."""
from pathlib import Path
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parents[3]
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
out = root / 'sdk-rs/target/apexomni-socket-order-controls' / sha
out.mkdir(parents=True, exist_ok=True)
path = root / 'sdk-rs/safe-cex-venue-apexomni/src/ws_private.rs'
original = path.read_bytes()
source = original.decode()
start = source.index('    pub fn handle_order_topic(')
end = source.index('    pub fn ',start+10)
body = source[start:end]
cases = [
    ('full-canceled-scan', 'has_canceled = true;\n                break;', 'has_canceled = true;',
     'APEX-WS-ORDER canceled-null native error precedence'),
    ('coerced-blacklist-key', '''            if let J::String(id) = raw_id
                && self
                    .parent
                    .recently_cancelled_orders
                    .borrow()
                    .contains_key(id)
            {
                continue;
            }''', '''            let coerced = text(raw_id)?;
            if self.parent.recently_cancelled_orders.borrow().contains_key(&coerced) { continue; }''',
     'APEX-WS-ORDER cached-numeric native whole mapped-order refusal'),
    ('unchecked-some-property', 'property(data, "some")?;', '',
     'APEX-WS-ORDER null-root native error precedence'),
    ('symmetric-clock-swap', '''let fresh = self.host.date_now_ms() - numeric(get(row, "updatedAt"))? <= 5000.;
            let notify = self.host.now_ms() - self.last_connect_time.get() > 5000. && fresh;''', '''let fresh = self.host.now_ms() - numeric(get(row, "updatedAt"))? <= 5000.;
            let notify = self.host.date_now_ms() - self.last_connect_time.get() > 5000. && fresh;''',
     'APEX-WS-ORDER cancel-stale native operations'),
]
assert len(cases) == 4
receipt = {'sha': sha, 'attempts': [], 'survivors': [], 'invalid_controls': []}


def run(name, expected, marker=None, env_extra=None):
    command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(root/'sdk-rs/Cargo.toml'),
               '--locked', '--jobs', '2', '-p', 'safe-cex-venue-apexomni', '--test', 'exchange',
               'websocket_tests::canceled_prefix_effects_precede_later_order_faults', '--', '--exact', '--nocapture']
    start = time.monotonic()
    result = subprocess.run(command, cwd=root, env={**os.environ, **(env_extra or {})}, capture_output=True, text=True)
    text = result.stdout + result.stderr
    (out/(name+'.log')).write_text(text)
    valid = result.returncode == expected and (('test result: FAILED.' in text and marker in text)
                                             if expected == 101 else '1 passed' in text)
    receipt['attempts'].append({'name': name, 'exit': result.returncode, 'expected_exit': expected,
                               'assertion': marker, 'valid': valid, 'seconds': round(time.monotonic()-start, 2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control': name, 'call_site': 'ApexOmniPrivateWebsocket::handle_order_topic'})
    elif not valid:
        receipt['invalid_controls'].append(name)
    (out/'controls.json').write_text(json.dumps(receipt, indent=2)+'\n')
    print(name, result.returncode, 'valid', valid, flush=True)
    return valid


for name, before, after, marker in cases:
    assert body.count(before) == 1, (name, before)
    try:
        path.write_text(source[:start] + body.replace(before, after) + source[end:])
        run(name+'-mutation', 101, marker)
    finally:
        path.write_bytes(original)
    assert run(name+'-restored', 0), name
run('source-predicate-mutation', 101, 'APEX-WS-ORDER canceled-null native error precedence',
    {'SAFE_CEX_APEX_SOCKET_ORDER_CONTROL': 'predicate'})
assert run('source-predicate-restored', 0)
assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=root).returncode == 0
assert len(receipt['attempts']) == 10, receipt
assert not receipt['survivors'] and not receipt['invalid_controls'], receipt
