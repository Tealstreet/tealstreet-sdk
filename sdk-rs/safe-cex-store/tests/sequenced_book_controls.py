"""Run sequential negative controls, restoring owned Rust source even on failure."""
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-store/src/sequenced_delta_order_book.rs'
original = source.read_text()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-store', '--test', 'sequenced_book', '--jobs', '2', '--', '--nocapture']
controls = [
    ('rust-amount', 'level[1], true', 'level[1] + 1.0, true', 'book.'),
    ('initial-ready', 'sequence_state: SequenceState::Uninitialized', 'sequence_state: SequenceState::Ready', 'ready'),
    ('snapshot-uninitialized', 'self.sequence_state = SequenceState::Ready;', 'self.sequence_state = SequenceState::Uninitialized;', 'ready'),
    ('snapshot-recovering', 'self.sequence_state = SequenceState::Ready;', 'self.sequence_state = SequenceState::Recovering;', 'ready'),
    ('recovery-uninitialized', 'self.sequence_state = SequenceState::Recovering;', 'self.sequence_state = SequenceState::Uninitialized;', 'recovering'),
    ('recovery-ready', 'self.sequence_state = SequenceState::Recovering;', 'self.sequence_state = SequenceState::Ready;', 'ready'),
    ('reset-ready', 'self.sequence_state = SequenceState::Uninitialized;', 'self.sequence_state = SequenceState::Ready;', 'ready'),
    ('reset-recovering', 'self.sequence_state = SequenceState::Uninitialized;', 'self.sequence_state = SequenceState::Recovering;', 'recovering'),
    ('delta-recovering', 'frame: SequencedBookFrameKind::Delta,', 'frame: { self.sequence_state = SequenceState::Recovering; SequencedBookFrameKind::Delta },', 'ready'),
    ('duplicate-as-stale', 'reason: SequencedBookIgnoreReason::Duplicate,', 'reason: SequencedBookIgnoreReason::Stale,', 'reason'),
    ('stale-as-duplicate', 'reason: SequencedBookIgnoreReason::Stale,', 'reason: SequencedBookIgnoreReason::Duplicate,', 'reason'),
    ('snapshot-fresh-identity', 'book.bids = next_book.bids;\n            book.asks = next_book.asks;', 'book.bids.clear();\n            drop(book);\n            self.order_book = Arc::new(Mutex::new(next_book));', 'book.'),
]

def run(name, env, expected_failure=None):
    output = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if expected_failure:
        assert output.returncode == 101, (name, output.returncode, output.stdout)
        assert 'sequenced_book_live_typescript_every_value_and_transition ... FAILED' in output.stdout, (name, output.stdout)
        assert 'assertion `left == right` failed:' in output.stdout and expected_failure in output.stdout, (name, output.stdout)
    else:
        assert output.returncode == 0, (name, output.stdout)
    print(f'{name}: exit {output.returncode}', flush=True)

env = os.environ.copy()
try:
    run('baseline', env)
    for name, before, after, failure in controls:
        assert original.count(before) == 1, (name, 'mutation anchor must be unique')
        source.write_text(original.replace(before, after))
        run(name, env, failure)
        source.write_text(original)
    run('typescript-amount', {**env, 'SEQUENCED_BOOK_TS_CONTROL':'amount'}, 'book.')
finally:
    source.write_text(original)
run('restored', env)
