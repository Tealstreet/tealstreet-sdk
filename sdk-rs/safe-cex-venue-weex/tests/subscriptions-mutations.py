"""One compiling production mutation for each newly ported subscription function."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parent.parent
path = crate / 'src/subscriptions.rs'
original = path.read_bytes()
controls = [
    (b'state.operation_timestamps.len() >= 240', b'state.operation_timestamps.len() > 240', 'subscriptions::operation/budget240::result'),
    (b'    if track {', b'    if !track {', 'subscriptions::batch/disconnected_track::state_topics'),
    (b'        state.ticker_topics = next_topics;', b'        state.ticker_topics.clear();', 'subscriptions::reconcile/disconnected_replace::state_tickers'),
    (b'    state.ticker_topics = ordered_set((runtime.desired_topics)(host, state)?);', b'    state.ticker_topics.clear();', 'subscriptions::ticker/basic::state_tickers'),
    (b'    if !topics.is_empty() {', b'    if topics.is_empty() {', 'subscriptions::resubscribe/basic::state_timestamps'),
    (b'    add(&mut state.topics, topic.clone());\n    reconcile_ticker_topics', b'    reconcile_ticker_topics', 'subscriptions::subscribe/basic::state_tickers'),
    (b'    state.topics.retain(|item| item != topic);', b'    state.topics.retain(|_| true);', 'subscriptions::unsubscribe/basic::state_topics'),
    (b'    if state.is_disposed {', b'    if !state.is_disposed {', 'subscriptions::changed/disposed::state_tickers'),
]
command = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'), '-p', 'safe-cex-venue-weex', '--jobs', '2', '--test', 'subscriptions']
try:
    for before, after, label in controls:
        assert original.count(before) == 1, before
        path.write_bytes(original.replace(before, after))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assert result.returncode == 101 and label in output and 'assertion `left == right` failed' in output, output
        print(f'COMPILED exit101 {label}', flush=True)
        path.write_bytes(original)
        subprocess.run(command, check=True)
        print(f'RESTORED exit0 {label}', flush=True)
finally:
    path.write_bytes(original)
print('8 controls caught and individually restored', flush=True)
