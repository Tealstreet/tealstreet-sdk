#!/usr/bin/env python3
"""Sequential runtime/closure controls for the one-row proof; always restore bytes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
RUST = ROOT / 'sdk-rs/safe-cex-venue-polymarket/src/spot_contracts.rs'
TS = ROOT / 'packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts'
COMMAND = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p',
           'safe-cex-venue-polymarket', '--locked', '--jobs', '2', '--test',
           'name_complete', '--', '--exact', 'name_complete_current_runtime', '--nocapture']


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'typescript_environment_required'
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    assert subprocess.run(['git', 'diff', '--quiet', 'HEAD'], cwd=ROOT).returncode == 0, 'clean_input_head_required'
    original = {path: path.read_bytes() for path in [RUST, TS]}
    for path, raw in original.items():
        assert subprocess.check_output(['git', 'show', f'{sha}:{path.relative_to(ROOT)}'], cwd=ROOT) == raw, 'input_bytes_match_head'
    measurements = []

    def run(label):
        start = time.monotonic()
        result = subprocess.run(COMMAND, cwd=ROOT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        published = result.stdout.replace(str(ROOT), '<worktree>')
        published = re.sub(r'/(?:Users|home)/[^\s\"\']+', '<machine-path>', published)
        log = args.output / (label + '.log')
        log.write_text(published)
        measurements.append({'name': label, 'exit': result.returncode,
                             'seconds': round(time.monotonic() - start, 3),
                             'log': log.name,
                             'published_log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()})
        print(f'{label}: exit {result.returncode}', flush=True)
        return result

    # Two independently observable return faults and two proof-closure drift guards.
    controls = [
        ('native-return', RUST,
         'pub fn name() -> ExchangeName {\n    ExchangeName::Polymarket\n}',
         'pub fn name() -> ExchangeName {\n    ExchangeName::Kraken\n}',
         'name_complete_native_value', 'runtime native variant mismatch'),
        ('source-return', TS,
         "get name(): ExchangeName {\n    return 'polymarket';\n  }",
         "get name(): ExchangeName {\n    return 'kraken';\n  }",
         'name_complete_source_value', 'runtime original getter value mismatch'),
        ('native-hidden-read', RUST,
         'pub fn name() -> ExchangeName {\n    ExchangeName::Polymarket\n}',
         'pub fn name() -> ExchangeName {\n    let _environment = std::env::var_os("POLYMARKET_COMPLETE_CONTROL");\n    ExchangeName::Polymarket\n}',
         'name_complete_rust_body_schema', 'closure guard; added environment read is rejected before invocation'),
        ('source-hidden-read', TS,
         "get name(): ExchangeName {\n    return 'polymarket';\n  }",
         "get name(): ExchangeName {\n    (globalThis as any).__polymarket_name_control;\n    return 'polymarket';\n  }",
         'name_complete_source_body_schema', 'closure guard; added global property read is rejected before module execution'),
    ]
    results = []
    try:
        baseline = run('baseline')
        assert baseline.returncode == 0, 'baseline_pass_required'
        for label, path, before, after, marker, kind in controls:
            assert path.read_bytes() == original[path], 'pre_control_original_bytes'
            text = original[path].decode()
            assert text.count(before) == 1, f'unique_control_anchor:{label}'
            try:
                path.write_text(text.replace(before, after))
                broken = run(label + '-mutant')
                caught = broken.returncode == 101 and marker in broken.stdout and 'test result: FAILED' in broken.stdout
                results.append({'name': label, 'kind': kind, 'marker': marker,
                                'mutant_exit': broken.returncode, 'caught': caught})
            finally:
                path.write_bytes(original[path])
            restored = run(label + '-restored')
            results[-1]['restored_exit'] = restored.returncode
            assert caught, f'SURVIVOR_OR_INVALID_FAILURE:{label}'
            assert restored.returncode == 0, f'restoration_failed:{label}'
    finally:
        for path, raw in original.items():
            path.write_bytes(raw)
        report = {'input_sha': sha, 'command': COMMAND, 'controls': results,
                  'measurements': measurements, 'independent_control_floor': 4,
                  'stop': 'source-hidden-read',
                  'restored_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in original},
                  'baseline_clean_head': True,
                  'interpretation': 'Two runtime return faults plus two syntactic proof-closure guards. The guards are not runtime effect-observation proofs or exhaustive mutation coverage.'}
        (args.output / 'controls.json').write_text(json.dumps(report, indent=2) + '\n')
    assert len(results) == 4, 'independent_four_control_floor'


if __name__ == '__main__':
    main()
