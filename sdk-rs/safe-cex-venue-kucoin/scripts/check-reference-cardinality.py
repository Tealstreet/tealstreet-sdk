import os
from pathlib import Path
import subprocess


CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
BALANCE = 'whole_object_balance_write_replaces_only_the_dictionary_prototype'
QUERY = 'live_fill_query_numbers_survive_before_json_projection'
MAPPER = 'live_fill_mapper_bits_clock_and_error_order'
TARGETS = (('balance_reference', BALANCE, 'balance-write.cjs', 'results',
            'KUCOIN-CARDINALITY balance source total'),
           ('spot_fills_reference', QUERY, 'spot-fills.cjs', '',
            'KUCOIN-CARDINALITY spot_fills_reference source case count'),
           ('spot_fills_reference', MAPPER, 'spot-fills.cjs', '',
            'KUCOIN-CARDINALITY spot_fills_reference source case count'))


def run(target, mode, path=None, assertion=None):
    binary, test, script, default_path, default_assertion = target
    env = os.environ.copy()
    altered = mode != 'restored'
    if altered:
        env.update(NODE_OPTIONS=f'--require={CRATE / "scripts/capture-cardinality-control.cjs"}',
                   KUCOIN_CARDINALITY_SCRIPT=script, KUCOIN_CARDINALITY_CAPTURE=mode,
                   KUCOIN_CARDINALITY_PATH=default_path if path is None else path)
    command = ['cargo', 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
               '-p', 'safe-cex-venue-kucoin', '--jobs', '1', '--test', binary, test, '--', '--exact']
    result = subprocess.run(command, cwd=ROOT, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    expected = 101 if altered else 0
    assertion = assertion or default_assertion
    if result.returncode != expected or (altered and assertion not in result.stdout):
        raise RuntimeError(f'{test} {mode}: unexpected {result.returncode}\n{result.stdout}')
    print(f'{test} {path or default_path or "outer"} {mode}: {result.returncode}'
          + (f' at {assertion}' if altered else ''), flush=True)


def main():
    for target in TARGETS:
        for mode in ('empty', 'partial', 'extra'):
            run(target, mode)
        run(target, 'restored')
    for mode in ('empty', 'extra'):
        run(TARGETS[1], mode, '0.calls', 'KUCOIN-CARDINALITY fills query call count')
        for field in ('fills', 'bits', 'keys'):
            run(TARGETS[2], mode, f'0.{field}', f'KUCOIN-CARDINALITY fill mapper {field} rows')
    run(TARGETS[1], 'restored')
    run(TARGETS[2], 'restored')


if __name__ == '__main__':
    main()
