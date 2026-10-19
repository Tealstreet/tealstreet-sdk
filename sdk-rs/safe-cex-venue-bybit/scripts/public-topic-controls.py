#!/usr/bin/env python3
"""Compile public consumer faults and require exact restored GREEN after each."""
from pathlib import Path
import subprocess
import os
import json
import hashlib
import sys

root = Path(__file__).resolve().parents[3]
sdk = root / 'sdk-rs'
venue = sdk / 'safe-cex-venue-bybit'
env = dict(os.environ, CARGO_INCREMENTAL='0')
protocol = venue / 'src/ws_public/topics.rs'
topics = venue / 'src/public_ticker_topics.rs'
lifecycle = venue / 'src/public_ws_lifecycle.rs'
original = {p: p.read_bytes() for p in [protocol, topics, lifecycle]}
command = ['cargo', 'test', '--locked', '--jobs', '2', '-p',
           'safe-cex-venue-bybit', '--all-features', '--test', 'public_topic_binding', '--', '--include-ignored']
controls = [
    ('initial-sync', protocol, '|| sync(&parent, &self.current),', '|| Ok(()),',
     'production_public_callbacks_match_twenty_four_actual_source_compositions'),
    ('constructor-active-listener', protocol, '"activeTickerSymbols:changed",\n                Rc::new',
     '"ignored-active-event",\n                Rc::new', 'spot_public_topics_use_active_selection_and_keep_prior_subscriptions'),
    ('live-active-send', protocol, '|frame| socket.send(frame),', '|_frame| Ok(()),',
     'spot_public_topics_use_active_selection_and_keep_prior_subscriptions'),
    ('market-listener', protocol, 'Some(parent.base.on(event, handler))', 'Some(parent.base.on({ let _ = event; "ignored-market-event" }, handler))',
     'primary_public_topics_stay_category_wide_and_follow_market_refreshes'),
    ('retained-topic-set', topics, 'let merged = current\n        .into_iter()',
     'let merged = { let _ = current; Vec::<String>::new() }\n        .into_iter()',
     'spot_public_topics_use_active_selection_and_keep_prior_subscriptions'),
    ('source-option-filter', protocol, 'if value.get("option")?.truthy() {', 'if !value.get("option")?.truthy() {',
     'primary_public_topics_stay_category_wide_and_follow_market_refreshes'),
    ('subscribe-delay', lifecycle, 'Ok(Some((frames, sleep(10))))', 'Ok(Some((frames, sleep(0))))',
     'disposal_drops_pending_public_subscription_tails_and_weak_venue_references'),
    ('market-delay', topics, 'Ok(Some((chunks, sleep(5))))', 'Ok(Some((chunks, sleep(0))))',
     'market_additions_wait_five_ms_between_real_socket_chunks'),
]

if len(sys.argv) > 1:
    requested = set(sys.argv[1:])
    assert requested <= {row[0] for row in controls}, requested
    controls = [row for row in controls if row[0] in requested]

def run(label):
    result = subprocess.run(command, cwd=sdk, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    Path('/tmp/bybit38-control-' + label + '.txt').write_text(result.stdout)
    return result

baseline = run('baseline')
assert baseline.returncode == 0 and '7 passed; 0 failed' in baseline.stdout, baseline.stdout
records = []
try:
    for label, path, before, after, test in controls:
        source = original[path].decode()
        assert source.count(before) == 1, (label, source.count(before))
        path.write_text(source.replace(before, after))
        result = run(label)
        assert result.returncode == 101 and 'panicked at' in result.stdout and test + ' ... FAILED' in result.stdout and 'error[E' not in result.stdout, (label, result.returncode, "see /tmp/bybit38-control-" + label + ".txt")
        path.write_bytes(original[path])
        assert all(p.read_bytes() == data for p, data in original.items())
        restored = run(label + '-restored')
        assert restored.returncode == 0 and '7 passed; 0 failed' in restored.stdout, restored.stdout
        records.append(dict(control=label, named_test=test, compiled_runtime_red=True, exact_restore=True, green=True, red_sha256=hashlib.sha256(result.stdout.encode()).hexdigest(), green_sha256=hashlib.sha256(restored.stdout.encode()).hexdigest()))
        print(label + ': compiled named RED -> exact restored GREEN', flush=True)
finally:
    for path, data in original.items():
        path.write_bytes(data)
restored = run('restored')
assert restored.returncode == 0 and '7 passed; 0 failed' in restored.stdout, restored.stdout
assert all(path.read_bytes() == data for path, data in original.items())
print(str(len(controls)) + ' controls caught; exact restoration; 5 concrete tests plus2 support tests, including24 source compositions, pass.', flush=True)

(venue / 'evidence/delegated-public-controls.json').write_text(json.dumps(dict(executed=len(records), controls=records, source_sha256={str(p.relative_to(venue)):hashlib.sha256(data).hexdigest() for p,data in original.items()}), indent=2)+'\n')
