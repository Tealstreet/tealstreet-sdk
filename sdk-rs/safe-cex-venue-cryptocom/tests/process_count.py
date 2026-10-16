#!/usr/bin/env python3
"""Measure process count for the complete types test target, including concurrent tests."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import sys
target = sys.argv[1] if len(sys.argv) > 1 else "types"
assert target in {"types", "signing", "auth"}
expected_tests = 3 if target == "types" else 1

ROOT = Path(__file__).resolve().parents[3]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
node = shutil.which('node')
assert node, 'node is required'
with tempfile.TemporaryDirectory(prefix='cryptocom-process-count-', dir=ROOT / 'sdk-rs/target') as temp:
    directory = Path(temp)
    log = directory / 'invocations'
    wrapper = directory / 'node'
    wrapper.write_text('#!/bin/sh\n' + 'printf "node\\n" >> ' + shlex.quote(str(log)) + '\nexec ' + shlex.quote(node) + ' "$@"\n')
    wrapper.chmod(0o755)
    env = dict(os.environ, PATH=str(directory) + os.pathsep + os.environ['PATH'])
    result = subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
                             '-p', 'safe-cex-venue-cryptocom', '--test', target, '--jobs', '1', '--locked'],
                            cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert result.returncode == 0 and f'{expected_tests} passed; 0 failed' in result.stdout, result.stdout
    count = len(log.read_text().splitlines())
    assert count == 1, f'one_node_process_per_corpus: expected 1, observed {count}'
    print(f'one_node_process_per_corpus: {count}; {expected_tests} tests passed')
