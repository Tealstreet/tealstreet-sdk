import os
from pathlib import Path
import subprocess


CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
TARGETS = ('pagination_guard_and_partial_results', 'concurrency_return_and_sibling_effects',
           'discovery_preserves_nonstring_symbols', 'later_pages_rejections_and_info_alias',
           'field_identity_and_raw_responses')


def run(target, path, mode, assertion):
    env = os.environ.copy()
    if mode != 'restored':
        env.update(NODE_OPTIONS=f'--require={CRATE / "scripts/capture-cardinality-control.cjs"}',
                   KUCOIN_CARDINALITY_SCRIPT='spot-orders.cjs', KUCOIN_CARDINALITY_CAPTURE=mode,
                   KUCOIN_CARDINALITY_PATH=path)
    command = ['cargo', 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
               '-p', 'safe-cex-venue-kucoin', '--jobs', '1', '--test', 'spot_orders_reference',
               'live_spot_order_' + target, '--', '--exact']
    result = subprocess.run(command, cwd=ROOT, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    expected = 0 if mode == 'restored' else 101
    if result.returncode != expected or (expected and assertion not in result.stdout):
        raise RuntimeError(f'{target} {path} {mode}: {result.returncode}\n{result.stdout}')
    print(f'{target} {path} {mode}: {result.returncode}'
          + (f' at {assertion}' if expected else ''), flush=True)


def main():
    for target in TARGETS:
        for mode in ('empty', 'partial', 'extra'):
            run(target, 'cases', mode, 'KUCOIN-CARDINALITY orders reference source case count')
        run(target, '', 'restored', '')
    nested = [
        (TARGETS[0], 'cases.6.atReturn.calls', 'KUCOIN-CARDINALITY pagination capped calls'),
        (TARGETS[0], 'cases.6.atReturn.wire', 'KUCOIN-CARDINALITY orders atReturn case 6 wire'),
        (TARGETS[0], 'cases.6.atReturn.bits', 'KUCOIN-CARDINALITY orders atReturn case 6 bits'),
        (TARGETS[0], 'cases.6.atReturn.types', 'KUCOIN-CARDINALITY orders atReturn case 6 types'),
        (TARGETS[0], 'cases.6.atReturn.bits.0', 'KUCOIN-CARDINALITY orders atReturn case 6 numeric fields'),
        (TARGETS[0], 'cases.6.after.wire', 'KUCOIN-CARDINALITY orders after case 6 wire'),
        (TARGETS[2], 'cases.0.atReturn.calls', 'live Spot orders discovery and duplicate fanout'),
    ]
    for target, path, assertion in nested:
        for mode in ('empty', 'partial', 'extra'):
            run(target, path, mode, assertion)
    run(TARGETS[0], '', 'restored', '')
    run(TARGETS[2], '', 'restored', '')


if __name__ == '__main__':
    main()
