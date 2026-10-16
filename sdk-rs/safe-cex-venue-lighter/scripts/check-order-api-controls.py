#!/usr/bin/env python3
"""Compile order-record identity and consuming-array controls serially."""
import argparse
import importlib.util
import os
from pathlib import Path
import signal
import subprocess
import sys

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
SPEC = importlib.util.spec_from_file_location('batch_controls', Path(__file__).with_name('check-batch-controls.py'))
BATCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BATCH)
HELPERS = BATCH.HELPERS
MODEL = CRATE / 'src/sdk_order_api.rs'
TEST = CRATE / 'tests/live_sdk_order_api.rs'
NUMERIC = ROOT / 'sdk-rs/safe-cex-num/src/numeric_ingress.rs'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exclusive-worktree', action='store_true')
    args = parser.parse_args()
    HELPERS.require(args.exclusive_worktree, 'Requires exclusive worktree, no concurrent Cargo or edits')
    original = {p: p.read_bytes() for p in [MODEL, TEST, NUMERIC]}
    expected = dict(original)
    head = HELPERS.git('rev-parse', 'HEAD')
    HELPERS.require(original[NUMERIC] == HELPERS.git('show', f'HEAD:{NUMERIC.relative_to(ROOT)}'), 'Shared numeric source must match HEAD')
    logs = ROOT / '.aimux/order-api-controls'
    logs.mkdir(parents=True, exist_ok=True)
    lock = ROOT / '.aimux/consuming-collections.lock'

    def check():
        HELPERS.require(HELPERS.git('rev-parse', 'HEAD') == head, 'HEAD changed')
        for p, data in expected.items():
            HELPERS.require(p.read_bytes() == data, f'Concurrent edit: {p}')

    def write(p, text):
        check()
        expected[p] = text.encode()
        p.write_bytes(expected[p])

    def restore():
        check()
        for p, data in original.items():
            if p.read_bytes() != data:
                p.write_bytes(data)
            expected[p] = data

    def run(label, test, failure=None):
        check()
        log = logs / f'{label}.log'
        command = ['cargo', 'test', '--manifest-path', str(ROOT/'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-lighter', '--test', 'live_sdk_order_api', '--locked', '--jobs', '1', test, '--', '--exact', '--include-ignored', '--nocapture']
        with log.open('wb') as output:
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
        text = log.read_text()
        HELPERS.require('could not compile' not in text and 'error[E' not in text, f'Did not compile: {log}')
        HELPERS.require(process.returncode == (101 if failure else 0), f'Unexpected exit {process.returncode}: {log}')
        HELPERS.require(('0 passed; 1 failed; 0 ignored' if failure else '1 passed; 0 failed; 0 ignored') in text, f'Wrong result counts: {log}')
        HELPERS.require('trailing array elements' not in text, f'Non-consuming rejection: {log}')
        if failure:
            HELPERS.require(failure in text, f'Missing named failure {failure}: {log}')
        print(f'{label}: exit {process.returncode}: {log}', flush=True)
        check()

    signal.signal(signal.SIGTERM, lambda _s, _f: sys.exit('Terminated; restoring controls'))
    fd = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.write(fd, f'pid={os.getpid()}\n'.encode())
    os.close(fd)
    try:
        run('baseline', 'order_api_collection_identity')
        write(NUMERIC, HELPERS.consuming_array(original[NUMERIC].decode()))
        run('consuming', 'order_api_collection_identity', 'ORDER-API orders native length')
        text = original[TEST].decode()
        for name in ['native_length', 'comparison_count']:
            text = BATCH.remove_guard(text, name)
        write(TEST, text)
        run('consuming-guards-removed', 'order_api_collection_identity', 'ORDER-API orders native collection identity')
        restore()
        for name, left, right, left_wire, right_wire in [
            ('timestamp', 'created_at', 'updated_at', 'createdAt', 'updatedAt'),
            ('boolean', 'post_only', 'reduce_only', 'postOnly', 'reduceOnly'),
        ]:
            write(MODEL, BATCH.symmetric_names(original[MODEL].decode(), 'ApiOrder', left, right, left_wire, right_wire))
            run(name+'-swap', 'order_api_native_identity', f'ApiOrder.{left_wire} native identity')
            restore()
        run('restored', 'order_api_collection_identity')
    finally:
        try:
            restore()
        finally:
            lock.unlink()


if __name__ == '__main__':
    main()
