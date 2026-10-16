#!/usr/bin/env python3
"""Run batch identity and consuming-decoder controls in an exclusive worktree."""
import argparse
import importlib.util
import os
from pathlib import Path
import signal
import subprocess
import sys

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
NUMERIC = ROOT / 'sdk-rs/safe-cex-num/src/numeric_ingress.rs'
MODEL = CRATE / 'src/sdk_batches.rs'
TEST = CRATE / 'tests/live_sdk_batches.rs'
SPEC = importlib.util.spec_from_file_location('collection_controls', Path(__file__).with_name('check-consuming-collections.py'))
HELPERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPERS)


def remove_guard(text, name):
    start = text.index(f'fn {name}(')
    body = text.index('{', start)
    depth = 1
    end = body + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[:body] + '{ let _ = (actual, expected, label); }' + text[end:]


def symmetric_names(text, record, left, right, left_wire, right_wire):
    for name in [record, record + 'Wire']:
        start = text.index(f'struct {name} {{')
        end = text.index('\n}', start)
        block = text[start:end]
        for field, wire in [(left, right_wire), (right, left_wire)]:
            prefix = '    pub ' if name == record else '    '
            old = prefix + field + ':'
            block = HELPERS.replace_once(block, old, f'    #[serde(rename = "{wire}")]\n' + old,
                                         f'{name}.{field} symmetric rename')
        text = text[:start] + block + text[end:]
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exclusive-worktree', action='store_true')
    args = parser.parse_args()
    HELPERS.require(args.exclusive_worktree, 'Requires --exclusive-worktree; no concurrent edits or Cargo')
    logs = ROOT / '.aimux/batch-controls'
    logs.mkdir(parents=True, exist_ok=True)
    lock = ROOT / '.aimux/consuming-collections.lock'
    original = {p:p.read_bytes() for p in [NUMERIC, MODEL, TEST]}
    expected = dict(original)
    head = HELPERS.git('rev-parse', 'HEAD')
    HELPERS.require(original[NUMERIC] == HELPERS.git('show', f'HEAD:{NUMERIC.relative_to(ROOT)}'),
                    'Refusing to mutate modified shared numeric source')

    def check():
        HELPERS.require(HELPERS.git('rev-parse', 'HEAD') == head, 'HEAD changed during controls')
        for path, data in expected.items():
            HELPERS.require(path.read_bytes() == data, f'Concurrent edit detected: {path}')

    def write(path, text):
        check()
        expected[path] = text.encode()
        path.write_bytes(expected[path])

    def restore():
        for path, data in original.items():
            if path.read_bytes() != expected[path]:
                backup = logs / (path.name + '.original')
                backup.write_bytes(data)
                raise RuntimeError(f'Concurrent edit preserved at {path}; original at {backup}')
            path.write_bytes(data)
            expected[path] = data

    def run(phase, selected, failures, names):
        check()
        logfile = logs / (phase + '.log')
        command = ['cargo', 'test', '--manifest-path', str(ROOT/'sdk-rs/Cargo.toml'), '-p',
                   'safe-cex-venue-lighter', '--locked', '--jobs', '1', '--test',
                   'live_sdk_batches', selected, '--', '--include-ignored', '--nocapture', '--test-threads=1']
        with logfile.open('wb') as output:
            process = subprocess.Popen(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                process.wait()
            except BaseException:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
                raise
        check()
        text = logfile.read_text()
        HELPERS.require('could not compile' not in text and 'error[E' not in text, f'Control did not compile: {logfile}')
        HELPERS.require(process.returncode == (101 if failures else 0), f'Unexpected exit {process.returncode}: {logfile}')
        if failures:
            HELPERS.require(f'0 passed; {failures} failed; 0 ignored' in text, f'Wrong failure count: {logfile}')
        else:
            HELPERS.require('5 passed; 0 failed; 0 ignored' in text, f'Wrong baseline count: {logfile}')
        for name in names:
            HELPERS.require(name in text, f'Missing assertion {name}: {logfile}')
        HELPERS.require('trailing array elements' not in text, f'Non-consuming error is not proof: {logfile}')
        print(f'{phase}: exit {process.returncode}; {logfile}', flush=True)

    fields = ['BatchTxReq.transactions', 'BatchCreateOrdersParams.orders', 'BatchTxResponse.txHashes',
              'BatchTxResponse.successIndices', 'BatchTxResponse.failedIndices']
    signal.signal(signal.SIGTERM, lambda _s, _f: sys.exit('Terminated; restoring batch controls'))
    fd = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.write(fd, f'pid={os.getpid()}\n'.encode())
    os.close(fd)
    try:
        run('baseline', 'sdk_batch_collection', 0, [])
        write(NUMERIC, HELPERS.consuming_array(original[NUMERIC].decode()))
        run('consuming', 'sdk_batch_collection', 5, ['BATCH-CARDINALITY '+field for field in fields])
        guards_removed = remove_guard(remove_guard(original[TEST].decode(), 'native_length'), 'comparison_count')
        write(TEST, guards_removed)
        run('consuming-guards-removed', 'sdk_batch_collection', 5, ['BATCH-CARDINALITY '+field for field in fields])
        restore()
        for record in ['BatchTxReq', 'BatchCreateOrdersParams']:
            write(MODEL, symmetric_names(original[MODEL].decode(), record, 'account_index',
                                        'api_key_index', 'accountIndex', 'apiKeyIndex'))
            case = 'all-four' if record == 'BatchTxReq' else 'both-orders'
            run(record + '-symmetric-index-swap', 'sdk_batches_source_shapes_and_retained_identity', 1,
                [f'{record} "{case}" bare raw.accountIndex native identity'])
            restore()
        write(MODEL, symmetric_names(original[MODEL].decode(), 'BatchTxResponse', 'success_indices',
                                    'failed_indices', 'successIndices', 'failedIndices'))
        run('response-symmetric-array-swap', 'sdk_batch_collection_success_indices', 1,
            ['BATCH-CARDINALITY BatchTxResponse.successIndices bare raw native length'])
        restore()
        run('restored', 'sdk_batch_collection', 0, [])
    finally:
        try:
            restore()
        finally:
            lock.unlink()


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, FileExistsError) as error:
        print(f'Batch controls refused/failed: {error}', file=sys.stderr)
        sys.exit(1)
