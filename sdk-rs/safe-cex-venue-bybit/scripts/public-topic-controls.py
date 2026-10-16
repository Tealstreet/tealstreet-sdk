#!/usr/bin/env python3
"""Run six compiled public consumer faults, then restore exact production bytes."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
venue = root / 'sdk-rs/safe-cex-venue-bybit'
protocol = venue / 'src/ws_public/topics.rs'
topics = venue / 'src/public_ticker_topics.rs'
lifecycle = venue / 'src/public_ws_lifecycle.rs'
original = {p: p.read_bytes() for p in [protocol, topics, lifecycle]}
command = ['cargo', 'test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-bybit', '--test', 'public_topic_binding']
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
    ('source-option-filter', protocol, '.filter(|m| m.option.is_none())', '.filter(|m| m.option.is_some())',
     'primary_public_topics_stay_category_wide_and_follow_market_refreshes'),
]

if len(sys.argv) > 1:
    requested = set(sys.argv[1:])
    assert requested <= {row[0] for row in controls}, requested
    controls = [row for row in controls if row[0] in requested]

def run(label):
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path('/tmp/bybit38-control-' + label + '.txt').write_text(result.stdout)
    return result

baseline = run('baseline')
assert baseline.returncode == 0 and '4 passed; 0 failed' in baseline.stdout, baseline.stdout
try:
    for label, path, before, after, test in controls:
        source = original[path].decode()
        assert source.count(before) == 1, (label, source.count(before))
        path.write_text(source.replace(before, after))
        result = run(label)
        assert result.returncode == 101 and 'panicked at' in result.stdout and test + ' ... FAILED' in result.stdout and 'error[E' not in result.stdout, (label, result.returncode, "see /tmp/bybit38-control-" + label + ".txt")
        path.write_bytes(original[path])
        print(label + ': compiled named assertion failed', flush=True)
finally:
    for path, data in original.items():
        path.write_bytes(data)
restored = run('restored')
assert restored.returncode == 0 and '4 passed; 0 failed' in restored.stdout, restored.stdout
assert all(path.read_bytes() == data for path, data in original.items())
print(str(len(controls)) + ' controls caught; exact restoration; 4 concrete tests including 24 source compositions pass.', flush=True)
