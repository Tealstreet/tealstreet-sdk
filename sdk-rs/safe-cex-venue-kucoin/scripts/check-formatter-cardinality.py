import os
from pathlib import Path
import re
import subprocess


CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
PRODUCTION = CRATE / 'src/formatters.rs'
TEST = CRATE / 'tests/formatters.rs'
TARGETS = ('live_futures_payloads_and_client_id_preflight',
           'inherited_market_payloads_preserve_errors_and_id_calls')


def run(target, mode, expected, assertion='', environment=None):
    command = ['cargo', 'test', '--locked', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
               '-p', 'safe-cex-venue-kucoin', '--jobs', '1', '--test', 'formatters', target,
               '--', '--exact']
    result = subprocess.run(command, cwd=ROOT, env=environment, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode != expected or (expected == 101 and assertion not in result.stdout):
        raise RuntimeError(f'{target} {mode}: unexpected {result.returncode}\n{result.stdout}')
    print(f'{target} {mode}: {result.returncode}' + (f' at {assertion}' if expected else ''), flush=True)


def without_source_guards(source):
    while match := re.search(r'KUCOIN-CARDINALITY[^"\n]*source[^"\n]*', source):
        start = source.rfind('assert_eq!(', 0, match.start())
        if start < 0:
            raise RuntimeError('source guard has no assertion')
        end = source.index(');', match.end()) + 2
        source = source[:start] + source[end:]
    return source


def main():
    production, test = PRODUCTION.read_bytes(), TEST.read_bytes()
    for target in TARGETS:
        run(target, 'baseline', 0)
        for mode in ('empty', 'partial', 'extra'):
            env = os.environ.copy()
            env.update(NODE_OPTIONS=f'--require={CRATE / "scripts/capture-cardinality-control.cjs"}',
                       KUCOIN_CARDINALITY_SCRIPT='formatters.cjs', KUCOIN_CARDINALITY_CAPTURE=mode)
            run(target, mode, 101, 'KUCOIN-CARDINALITY formatter source case count', env)
        run(target, 'restored', 0)
    old = 'Ok(vec![J::Object(payload)])'
    if production.decode().count(old) != 1:
        raise RuntimeError('singleton production vector target changed')
    try:
        for mode in ('empty', 'extra', 'bypass-empty', 'bypass-extra'):
            new = ('Ok(vec![J::Object(payload.clone()), J::Object(payload)])'
                   if mode.endswith('extra') else 'Ok(vec![J::Object(payload)].into_iter().take(0).collect())')
            PRODUCTION.write_text(production.decode().replace(old, new))
            TEST.write_text(without_source_guards(test.decode()) if mode.startswith('bypass') else test.decode())
            run(TARGETS[0], mode, 101, 'KUCOIN-CARDINALITY formatter native payload rows')
    finally:
        PRODUCTION.write_bytes(production)
        TEST.write_bytes(test)
    run(TARGETS[0], 'restored-production', 0)


if __name__ == '__main__':
    main()
