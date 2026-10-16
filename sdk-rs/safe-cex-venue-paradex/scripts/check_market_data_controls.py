"""One compiled production mutation for each of eight new market data rows."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-paradex'
LOGS = ROOT / '.aimux/paradex'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-paradex', '--test', 'market_data', '--jobs', '2']
ENV = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
path = CRATE / 'src/market_data.rs'
original = path.read_text()
controls = [
 ('start-public-connect','call(h, "connectPublic", vec![])?;','call(h, "connectPrivate", vec![])?;','start_native_public_connection'),
 ('tick-delay','h.schedule_tick(10_000.0)?;','h.schedule_tick(10_001.0)?;','tick_native_schedule'),
 ('market-leverage','max = (1.0 / imf).floor();\n                }\n            } else','max = (1.0 / imf).floor() + 1.0;\n                }\n            } else','markets_native_max_leverage'),
 ('ticker-clock','fields.push(("timestamp", call(h, "virtualClock", vec![])?));','fields.push(("timestamp", h.number(num(h,call(h,"virtualClock",vec![])?)?+1.0)));','tickers_native_timestamp'),
 ('documented-candle-time','fields.push((key, h.get(&numbers, &i.to_string())?));','fields.push((key, if i==0 {h.number(num(h,h.get(&numbers,&i.to_string())?)?+1.0)}else{h.get(&numbers,&i.to_string())?}));','ohlcv_native_timestamp'),
 ('position-precedence','for rows in [fake, actual]','for rows in [actual, fake]','positions_native_actual_precedence'),
 ('rest-book-total','("total", h.number(num(h, p)? * num(h, s)?)),','("total", h.number(num(h, p)? * num(h, s)? + if tuple {1.0} else {0.0})),','book_native_bid_total'),
 ('book-broadcast','h.broadcast(callbacks, book)?;','h.broadcast(callbacks, if h.truthy(&book) {h.object(vec![("bids",h.array(vec![h.object(vec![("total",h.number(0.0))])]))])}else{book})?;','book_ingress_native_broadcast'),
]


def run(label):
    result = subprocess.run(COMMAND, cwd=ROOT, env=ENV, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (LOGS / f'market-data-{label}.log').write_text(output)
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
    (LOGS / 'market-data-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    mutated_output = output
    restored, output = run(name + '-restored')
    record['restored_exit'] = restored
    (LOGS / 'market-data-controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert restored == 0, (name, output)
    assert record['compiled_runtime_failure'], (name, mutated_output)
    assert assertion in mutated_output, (name, mutated_output)
    assert path.read_text() == original
    print(json.dumps(record), flush=True)
assert len(report['controls']) == 8
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
